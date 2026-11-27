use super::{Retired, enqueue_reap, MAX_REAP_PENDING};
use std::sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}};

struct Witness(Arc<AtomicUsize>);
impl Drop for Witness {
    fn drop(&mut self) { self.0.fetch_add(1, Ordering::Relaxed); }
}
fn batch(n: usize, dropped: &Arc<AtomicUsize>) -> Vec<Retired> {
    (0..n).map(|_| Retired(Box::new(Witness(dropped.clone())))).collect()
}

#[test]
fn neptune_reap_queue_bound_and_ownership() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let pending = Mutex::new(Vec::new());
    assert!(enqueue_reap(&pending, batch(MAX_REAP_PENDING, &dropped)).is_none());
    assert_eq!(pending.lock().unwrap().len(), MAX_REAP_PENDING);
    let overflow = enqueue_reap(&pending, batch(1, &dropped));
    assert!(overflow.is_some(), "saturated reaper must return ownership to caller");
    assert_eq!(pending.lock().unwrap().len(), MAX_REAP_PENDING);
    assert_eq!(dropped.load(Ordering::Relaxed), 0);
    drop(overflow);
    assert_eq!(dropped.load(Ordering::Relaxed), 1);
    let held = std::mem::take(&mut *pending.lock().unwrap());
    let oversized = enqueue_reap(&pending, batch(MAX_REAP_PENDING + 1, &dropped));
    assert!(oversized.is_some(), "oversized batch must stay with caller");
    assert!(pending.lock().unwrap().is_empty());
    assert!(enqueue_reap(&pending, batch(1, &dropped)).is_none());
    assert_eq!(pending.lock().unwrap().len(), 1);
    drop(oversized);
    drop(held);
    drop(pending);
    assert_eq!(dropped.load(Ordering::Relaxed), MAX_REAP_PENDING * 2 + 3);
}

#[test]
fn neptune_reap_queue_concurrent_admission() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let pending = Mutex::new(Vec::new());
    let admitted = AtomicUsize::new(0);
    let refused = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                for _ in 0..256 {
                    let work = batch(7, &dropped);
                    match enqueue_reap(&pending, work) {
                        Some(work) => { refused.fetch_add(work.len(), Ordering::Relaxed); drop(work); }
                        None => { admitted.fetch_add(7, Ordering::Relaxed); }
                    }
                }
            });
        }
    });
    let retained = pending.lock().unwrap().len();
    assert!(retained <= MAX_REAP_PENDING);
    assert_eq!(retained, admitted.load(Ordering::Relaxed));
    assert_eq!(retained + refused.load(Ordering::Relaxed), 8 * 256 * 7);
    assert_eq!(dropped.load(Ordering::Relaxed), refused.load(Ordering::Relaxed));
    drop(pending);
    assert_eq!(dropped.load(Ordering::Relaxed), 8 * 256 * 7);
}

#[cfg(not(feature = "diagnostics"))]
mod immediate {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::hint::black_box;
    use std::time::Instant;

    // Test executable only. Count allocation calls on the measuring thread,
    // not on libtest, GPU driver or other worker threads.
    thread_local! { static COUNT: Cell<Option<usize>> = const { Cell::new(None) }; }
    struct Meter;
    #[global_allocator]
    static ALLOCATOR: Meter = Meter;
    fn tick() {
        let _ = COUNT.try_with(|c| {
            if let Some(n) = c.get() { c.set(Some(n + 1)); }
        });
    }
    unsafe impl GlobalAlloc for Meter {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            tick();
            unsafe { System.alloc(l) }
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            tick();
            unsafe { System.alloc_zeroed(l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            tick();
            unsafe { System.realloc(p, l, n) }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            unsafe { System.dealloc(p, l) }
        }
    }
    fn count(f: impl FnOnce()) -> usize {
        COUNT.with(|c| { assert!(c.get().is_none()); c.set(Some(0)); });
        f();
        COUNT.with(|c| c.replace(None).unwrap())
    }
    fn no_flight() {
        assert!(!super::super::reaper_on(), "run with VICTORY_REAPER=0");
        assert!(!super::super::retire_lean(), "run with VICTORY_RETIRE_LEAN=0");
        assert_eq!(super::super::STARTED.load(Ordering::Acquire),
                   super::super::FINISHED.load(Ordering::Acquire));
    }
    #[inline(never)]
    fn old(w: Witness) {
        use super::super::*;
        // Exact R37C no-flight path (guarded by no_flight).
        let started = STARTED.load(Ordering::Acquire);
        assert!(started <= FINISHED.load(Ordering::Acquire));
        RETIREMENTS[0].fetch_add(1, Ordering::Relaxed);
        if retire_lean() && !reaper_on() { drop(w); return; }
        reap(vec![Retired(Box::new(w))]);
    }
    #[inline(never)]
    fn candidate(w: Witness) { super::super::retire(w); }

    #[test]
    #[ignore = "isolated process with both retirement switches off"]
    fn neptune_immediate_zero_alloc() {
        no_flight();
        let drops = Arc::new(AtomicUsize::new(0));
        // Calibration proves the counter sees a real heap allocation.
        let calibration = count(|| drop(black_box(Box::new(black_box(17_u64)))));
        assert!(calibration >= 1);
        let original = count(|| old(black_box(Witness(drops.clone()))));
        let actual = count(|| candidate(black_box(Witness(drops.clone()))));
        println!("CALIBRATION={calibration} OLD_ALLOCS={original} NEW_ALLOCS={actual} DROPS={}", drops.load(Ordering::Relaxed));
        assert!(original >= 2, "baseline must expose the Box and Vec allocations");
        assert_eq!(drops.load(Ordering::Relaxed), 2);
        assert_eq!(actual, 0, "completed synchronous retirement must not allocate");
    }

    #[test]
    #[ignore = "paired CPU benchmark, isolated process with switches off"]
    fn neptune_immediate_paired_bench() {
        no_flight();
        const N: usize = 200_000;
        let drops = Arc::new(AtomicUsize::new(0));
        let run = |f: fn(Witness)| {
            let start = Instant::now();
            for _ in 0..N { f(black_box(Witness(drops.clone()))); }
            start.elapsed().as_nanos()
        };
        for _ in 0..3 { black_box(run(old)); black_box(run(candidate)); }
        let mut a = Vec::new();
        let mut b = Vec::new();
        for i in 0..9 {
            let (x, y) = if i % 2 == 0 { (run(old), run(candidate)) }
                else { let y = run(candidate); (run(old), y) };
            a.push(x); b.push(y);
        }
        assert_eq!(drops.load(Ordering::Relaxed), N * 24);
        println!("IMMEDIATE_LOOPS={N} OLD_NS={a:?} NEW_NS={b:?} DROPS={}", drops.load(Ordering::Relaxed));
    }
}

#[test]
#[ignore = "physical NVIDIA D3D12, isolated process VICTORY_REAPER=1"]
fn neptune_physical_reap_runtime() {
    use super::*;
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    struct GpuWitness {
        _resource: ID3D12Resource,
        owner: std::thread::ThreadId,
        local: Arc<AtomicUsize>,
        remote: Arc<AtomicUsize>,
    }
    impl Drop for GpuWitness {
        fn drop(&mut self) {
            let c = if self.owner == std::thread::current().id() { &self.local } else { &self.remote };
            c.fetch_add(1, Ordering::Release);
        }
    }
    const BYTES: u64 = 65536;
    assert!(reaper_on(), "run with VICTORY_REAPER=1");
    assert!(REAP.0.lock().unwrap().is_empty());
    let before = REAPED[0].load(Ordering::Acquire);
    let local = Arc::new(AtomicUsize::new(0));
    let remote = Arc::new(AtomicUsize::new(0));
    let owner = std::thread::current().id();
    let witness = |resource| GpuWitness {
        _resource: resource, owner, local: local.clone(), remote: remote.clone(),
    };
    // Private real queue and fence; no title data. On a GPU wait failure,
    // abort the isolated fixture rather than release potentially live objects.
    unsafe {
        let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).unwrap();
        let mut device: Option<ID3D12Device> = None;
        D3D12CreateDevice(None, D3D_FEATURE_LEVEL_12_0, &mut device).unwrap();
        let dev = device.unwrap();
        let adapter: IDXGIAdapter1 = factory.EnumAdapterByLuid(dev.GetAdapterLuid()).unwrap();
        let desc = adapter.GetDesc1().unwrap();
        assert_eq!(desc.Flags & 2, 0);
        assert_eq!(desc.VendorId, 0x10de);
        let queue: ID3D12CommandQueue = dev.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT, ..Default::default()
        }).unwrap();
        let gate: ID3D12Fence = dev.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
        let fence: ID3D12Fence = dev.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
        let buffer = |kind, state| -> ID3D12Resource {
            let mut out = None;
            dev.CreateCommittedResource(
                &D3D12_HEAP_PROPERTIES { Type: kind, ..Default::default() },
                D3D12_HEAP_FLAG_NONE,
                &D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                    Width: BYTES, Height: 1, DepthOrArraySize: 1, MipLevels: 1,
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR, ..Default::default()
                }, state, None, &mut out
            ).unwrap();
            out.unwrap()
        };
        let upload = buffer(D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ);
        let readback = buffer(D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST);
        let expected: Vec<u8> = (0..BYTES as usize).map(|i| ((i * 173) ^ (i >> 7)) as u8).collect();
        let mut data = std::ptr::null_mut();
        upload.Map(0, None, Some(&mut data)).unwrap();
        std::ptr::copy_nonoverlapping(expected.as_ptr(), data.cast(), expected.len());
        upload.Unmap(0, None);
        let alloc: ID3D12CommandAllocator = dev.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
        let list: ID3D12GraphicsCommandList = dev.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, None).unwrap();
        list.CopyBufferRegion(&readback, 0, &upload, 0, BYTES);
        list.Close().unwrap();
        let commands = [Some(list.cast::<ID3D12CommandList>().unwrap())];
        let event = CreateEventW(None, false, false, None).unwrap();
        let pending = Mutex::new(Vec::new());
        let finished = std::sync::atomic::AtomicU64::new(0);
        queue.Wait(&gate, 1).unwrap();
        queue.ExecuteCommandLists(&commands);
        let signal = queue.Signal(&fence, 1);
        let blocked = fence.GetCompletedValue();
        let returned = defer_retirement(1, &finished, &pending, witness(upload));
        let premature = local.load(Ordering::Acquire) + remote.load(Ordering::Acquire);
        if gate.Signal(1).is_err() || signal.is_err() { std::process::abort(); }
        if fence.SetEventOnCompletion(1, event).is_err() { std::process::abort(); }
        let waited = WaitForSingleObject(event, 10_000);
        let _ = CloseHandle(event);
        if waited != WAIT_OBJECT_0 { std::process::abort(); }
        let completed = fence.GetCompletedValue();
        dev.GetDeviceRemovedReason().unwrap();
        assert_eq!((blocked, completed, premature), (0, 1, 0));
        assert!(returned.is_none());
        finished.store(completed, Ordering::Release);
        let done = std::mem::take(&mut *pending.lock().unwrap());
        assert_eq!(done.len(), 1);
        reap(done.into_iter().map(|(_, x)| x).collect());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while (REAPED[0].load(Ordering::Acquire) == before || remote.load(Ordering::Acquire) == 0)
              && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(REAPED[0].load(Ordering::Acquire), before + 1);
        assert_eq!(remote.load(Ordering::Acquire), 1);
        assert_eq!(local.load(Ordering::Acquire), 0);
        // 4097 references to one real completed resource, NOT 4097 VRAM allocations.
        let oversized = (0..=MAX_REAP_PENDING)
            .map(|_| Retired(Box::new(witness(readback.clone())))).collect();
        reap(oversized);
        assert_eq!(local.load(Ordering::Acquire), MAX_REAP_PENDING + 1);
        assert_eq!(remote.load(Ordering::Acquire), 1);
        assert!(REAP.0.lock().unwrap().is_empty());
        let mut data = std::ptr::null_mut();
        readback.Map(0, None, Some(&mut data)).unwrap();
        let actual = std::slice::from_raw_parts(data.cast::<u8>(), BYTES as usize);
        let mismatches = actual.iter().zip(&expected).filter(|(a,b)| a != b).count();
        readback.Unmap(0, None);
        assert_eq!(mismatches, 0);
        println!("ADAPTER={:04x}:{:04x} FENCE={blocked}->{completed} BYTES={BYTES} MISMATCHES={mismatches} PREMATURE={premature} ASYNC={} SYNC_OVERFLOW={}",
            desc.VendorId, desc.DeviceId, remote.load(Ordering::Acquire), local.load(Ordering::Acquire));
    }
}
