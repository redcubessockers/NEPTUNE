use super::*;
use std::cell::Cell;
use std::hint::black_box;
use std::time::Instant;

fn flight(fence: u64) -> Late {
    Late::Run(Flight {
        number: fence, fence, lists: Vec::new(), order: Vec::new(), card: None,
        back: Vec::new(), texture_back_ranges: Vec::new(),
    })
}
fn fence_of(l: &Late) -> Option<u64> {
    match l { Late::Run(f) => Some(f.fence), _ => None }
}
// Independent baseline: materialize all fence values, then find the first blocked flight.
#[inline(never)]
fn old(line: &[Late], completed: u64) -> (usize, Option<u64>) {
    let fences: Vec<_> = line.iter().map(fence_of).collect();
    if completed == u64::MAX { return (0, None); }
    let n = fences.iter().position(|f| f.is_some_and(|v| v > completed)).unwrap_or(fences.len());
    (n, fences.get(n).copied().flatten())
}

#[test]
fn neptune_relay_prefix_early_stop() {
    let line = [Some(5), None, Some(6), None, Some(7)];
    let reads = Cell::new(0);
    let got = landing_prefix(line.iter().copied().inspect(|_| reads.set(reads.get()+1)), 4);
    assert_eq!(got, (0, Some(5)));
    assert_eq!(reads.get(), 1, "blocked front must not scan or allocate for tail");
    reads.set(0);
    assert_eq!(landing_prefix(line.iter().copied().inspect(|_| reads.set(reads.get()+1)), u64::MAX), (0,None));
    assert_eq!(reads.get(), 0, "removed device must not scan");
}

#[test]
fn neptune_relay_prefix_exhaustive() {
    // Enumerate all lines of up to seven entries from two fences and a signal.
    let mut cases = 0;
    for len in 0..=7u32 {
        for mut code in 0..3usize.pow(len) {
            let mut line = Vec::new();
            for _ in 0..len {
                line.push(match code % 3 { 0 => Late::Signal(0, 0), 1 => flight(1), _ => flight(2) });
                code /= 3;
            }
            for completed in [0, 1, 2, 3, u64::MAX-1, u64::MAX] {
                assert_eq!(relay_prefix(&line, completed), old(&line, completed), "len={len} completed={completed}");
                cases += 1;
            }
        }
    }
    println!("RELAY_PREFIX_CASES={cases}");
}

#[test]
#[ignore = "requires physical NVIDIA D3D12 device"]
fn neptune_physical_relay_prefix() {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    const BYTES: u64 = 8192;
    // SAFETY: private device, queue, two copy lists, and buffers retained
    // through both completion signals. Release the gate before assertions.
    unsafe {
        let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).unwrap();
        let mut device: Option<ID3D12Device> = None;
        D3D12CreateDevice(None, D3D_FEATURE_LEVEL_12_0, &mut device).unwrap();
        let dev = device.unwrap();
        let adapter: IDXGIAdapter1 = factory.EnumAdapterByLuid(dev.GetAdapterLuid()).unwrap();
        let desc = adapter.GetDesc1().unwrap();
        assert_eq!(desc.Flags & 2, 0, "software adapter");
        assert_eq!(desc.VendorId, 0x10de, "physical NVIDIA required");
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
        let expected: Vec<u8> = (0..BYTES as usize).map(|i| ((i*131) ^ (i>>4) ^ (i>>12)) as u8).collect();
        let mut data = std::ptr::null_mut();
        upload.Map(0, None, Some(&mut data)).unwrap();
        std::ptr::copy_nonoverlapping(expected.as_ptr(), data.cast(), expected.len());
        upload.Unmap(0, None);
        let make_list = |offset: u64| {
            let alloc: ID3D12CommandAllocator = dev.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
            let list: ID3D12GraphicsCommandList = dev.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, None).unwrap();
            list.CopyBufferRegion(&readback, offset, &upload, offset, BYTES/2);
            list.Close().unwrap();
            (alloc, list.cast::<ID3D12CommandList>().unwrap())
        };
        let (_a1, l1) = make_list(0);
        let (_a2, l2) = make_list(BYTES/2);
        let event = CreateEventW(None, false, false, None).unwrap();
        let abort = || {
            let _ = gate.Signal(2);
            std::process::abort()
        };
        let wait = |target| {
            if fence.SetEventOnCompletion(target, event).is_err() { abort(); }
            if WaitForSingleObject(event, 10_000) != WAIT_OBJECT_0 { abort(); }
            let done = fence.GetCompletedValue();
            if done == u64::MAX || done < target { abort(); }
            done
        };
        let entries = [flight(1), Late::Signal(0, 0), flight(2), Late::Signal(0, 0)];
        if queue.Wait(&gate, 1).is_err() { abort(); }
        queue.ExecuteCommandLists(&[Some(l1.clone())]);
        if queue.Signal(&fence, 1).is_err() || queue.Wait(&gate, 2).is_err() { abort(); }
        queue.ExecuteCommandLists(&[Some(l2.clone())]);
        if queue.Signal(&fence, 2).is_err() { abort(); }
        let before = fence.GetCompletedValue();
        let p0 = relay_prefix(&entries, before);
        if gate.Signal(1).is_err() { abort(); }
        let middle = wait(1);
        let p1 = relay_prefix(&entries, middle);
        if gate.Signal(2).is_err() { abort(); }
        let after = wait(2);
        let p2 = relay_prefix(&entries, after);
        CloseHandle(event).unwrap();
        dev.GetDeviceRemovedReason().unwrap();
        assert_eq!((before,middle,after), (0,1,2));
        assert_eq!(p0, (0,Some(1)));
        assert_eq!(p1, (2,Some(2)));
        assert_eq!(p2, (4,None));
        assert_eq!(relay_prefix(&entries, u64::MAX), (0,None));
        let mut data = std::ptr::null_mut();
        readback.Map(0, None, Some(&mut data)).unwrap();
        assert_eq!(std::slice::from_raw_parts(data.cast::<u8>(), BYTES as usize), expected.as_slice());
        readback.Unmap(0, None);
        println!("ADAPTER={:04x}:{:04x} FENCES={before},{middle},{after} PREFIX={},{},{} BYTES={BYTES} MISMATCHES=0",desc.VendorId,desc.DeviceId,p0.0,p1.0,p2.0);
    }
}

#[inline(never)]
fn new(line: &[Late], completed: u64) -> (usize, Option<u64>) {
    relay_prefix(line, completed)
}

#[test]
#[ignore = "native paired CPU microbenchmark, run alone"]
fn neptune_relay_prefix_paired_bench() {
    fn run(line: &[Late], completed: u64, loops: usize, newer: bool) -> (u128, usize) {
        let start = Instant::now();
        let mut checksum = 0usize;
        for _ in 0..loops {
            let input = black_box(line);
            let done = black_box(completed);
            let (n, blocked) = if newer { new(input, done) } else { old(input, done) };
            checksum = checksum.wrapping_add(black_box(n).wrapping_add(black_box(blocked.unwrap_or(0)) as usize));
        }
        (start.elapsed().as_nanos(), black_box(checksum))
    }
    for (label, len, completed, loops) in [
        ("blocked8", 8usize, 0u64, 100_000usize),
        ("blocked64", 64, 0, 100_000),
        ("half64", 64, 16, 100_000),
        ("all64", 64, 64, 100_000),
        ("blocked4096", 4096, 0, 5_000),
    ] {
        let line: Vec<_> = (0..len).map(|i| if i%2 == 0 { flight((i/2+1) as u64) } else { Late::Signal(0,0) }).collect();
        let expected = old(&line, completed);
        assert_eq!(relay_prefix(&line, completed), expected);
        for _ in 0..3 { assert_eq!(run(&line, completed, loops, false).1, run(&line, completed, loops, true).1); }
        let mut a = Vec::new(); let mut b = Vec::new();
        for i in 0..9 {
            let (x,y) = if i%2 == 0 {
                (run(&line, completed, loops, false), run(&line, completed, loops, true))
            } else {
                let y = run(&line, completed, loops, true); (run(&line, completed, loops, false), y)
            };
            assert_eq!(x.1,y.1);
            println!("RELAY_BENCH={label} SAMPLE={i} LOOPS={loops} OLD_NS={} NEW_NS={} CHECKSUM={}",x.0,y.0,x.1);
            a.push(x.0); b.push(y.0);
        }
        let wins = a.iter().zip(&b).filter(|(x,y)| x>y).count();
        a.sort_unstable(); b.sort_unstable();
        println!("RELAY_MEDIAN={label} OLD_NS={} NEW_NS={} SPEEDUP={:.3} WINS={wins}/9", a[4], b[4], a[4] as f64 / b[4].max(1) as f64);
    }
}
