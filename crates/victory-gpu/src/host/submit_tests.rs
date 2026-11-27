use super::*;
use std::cell::Cell;

thread_local! {
    // None outside the isolated fixture; counts actual take_ready call sites.
    static OBSERVED: Cell<Option<(usize, bool)>> = const { Cell::new(None) };
}

pub(super) fn observe_ready_lock(host: &Host) {
    OBSERVED.with(|w| {
        if let Some((n, all_locked)) = w.get() {
            let locked = matches!(host.submitted.try_lock(), Err(std::sync::TryLockError::WouldBlock));
            w.set(Some((n + 1, all_locked && locked)));
        }
    });
}

#[test]
#[ignore = "isolated physical NVIDIA test; VICTORY_LATE_WAIT=1"]
fn neptune_late_ready_under_submission_lock() {
    assert!(late(), "late submission must be enabled");
    let host = host().expect("physical host");
    const N: usize = 4096;
    let expected: Vec<u8> = (0..N).map(|i| (i.wrapping_mul(173) ^ (i >> 5)) as u8).collect();
    let (upload, ptr) = host.upload_buffer(N as u64).unwrap();
    let back = host.read_back_buffer(N as u64).unwrap();
    let list = host.new_list().unwrap();
    // SAFETY: resources and mapped memory live through the settled submission.
    unsafe {
        let factory: IDXGIFactory4 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).unwrap();
        let adapter: IDXGIAdapter1 = factory.EnumAdapterByLuid(host.device.GetAdapterLuid()).unwrap();
        let desc = adapter.GetDesc1().unwrap();
        assert_eq!(desc.Flags & 2, 0);
        assert_eq!(desc.VendorId, 0x10de);
        std::ptr::copy_nonoverlapping(expected.as_ptr(), ptr, N);
        upload.Unmap(0, None);
        list.list.CopyBufferRegion(&back, 0, &upload, 0, N as u64);
        println!("ADAPTER={:04x}:{:04x}", desc.VendorId, desc.DeviceId);
    }
    list.close();
    let before = STARTED.load(Ordering::Acquire);
    assert_eq!(before, FINISHED.load(Ordering::Acquire));
    assert!(host.submitted.try_lock().is_ok(), "isolated fixture");
    OBSERVED.with(|w| w.set(Some((0, true))));
    host.execute_late(vec![crate::list::Detached::ours(list)], vec![0]);
    let (calls, locked) = OBSERVED.with(|w| w.replace(None).unwrap());
    settle_for_fence();
    let completed = FINISHED.load(Ordering::Acquire);
    let started = STARTED.load(Ordering::Acquire);
    if completed != started || started <= before { std::process::abort(); }
    let mut mapped = std::ptr::null_mut();
    let mismatch = unsafe {
        host.device.GetDeviceRemovedReason().unwrap();
        back.Map(0, None, Some(&mut mapped)).unwrap();
        let bytes = std::slice::from_raw_parts(mapped.cast::<u8>(), N);
        let mismatch = bytes.iter().zip(&expected).filter(|(a,b)| a != b).count();
        back.Unmap(0, None);
        mismatch
    };
    println!("READY_CALLS={calls} SUBMISSION_LOCKED={locked} STARTED={started} FINISHED={completed} BYTES={N} MISMATCH={mismatch}");
    assert_eq!(mismatch, 0);
    assert_eq!(calls, 1);
    assert!(locked, "take_ready publication boundary outside submission mutex");
}

// Physical two-flight control. The relay landing boundary is invoked manually
// after observing A's real fence, while B is blocked by an independent fence.
// No late helper/relay thread is started. Isolated process only.
pub(crate) fn two_flight_fixture(observe: impl Fn() -> bool) -> (bool, bool) {
    assert!(late());
    assert!(!relay());
    let h = host().unwrap();
    assert_eq!(STARTED.load(Ordering::Acquire), 0);
    assert_eq!(FINISHED.load(Ordering::Acquire), 0);
    let a = h.new_list().unwrap();
    a.close();
    let mut lists = vec![crate::list::Detached::ours(a)];
    let mut order = vec![0];
    let (submitted, sa) = h.submit(&[], true, Some((&mut lists, &mut order))).unwrap();
    let fa = *submitted;
    drop(submitted);
    let wait = |value| {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while unsafe { h.fence.GetCompletedValue() } < value && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(unsafe { h.fence.GetCompletedValue() } >= value);
        unsafe { h.device.GetDeviceRemovedReason().unwrap(); }
    };
    wait(fa);
    let gate: ID3D12Fence = unsafe { h.device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap() };
    unsafe { h.queue.Wait(&gate, 1).unwrap(); }
    struct Open(ID3D12Fence);
    impl Drop for Open { fn drop(&mut self) { unsafe { let _ = self.0.Signal(1); } } }
    let open = Open(gate);
    let b = h.new_list().unwrap();
    b.close();
    let (submitted, sb) = h.submit(&[&b], true, None).unwrap();
    let fb = *submitted;
    drop(submitted);
    assert_eq!(sa.number, 1);
    assert_eq!(sb.number, 2);
    assert!(fa < fb);
    let blocked = unsafe { h.fence.GetCompletedValue() };
    assert!(blocked >= fa && blocked < fb, "B must remain physically gated");
    FINISHED.store(sa.number, Ordering::Release);
    bury(sa.number);
    let early = observe();
    drop(open);
    wait(fb);
    FINISHED.store(sb.number, Ordering::Release);
    bury(sb.number);
    settle_for_fence();
    let final_made = observe();
    println!("FENCE_A={fa} FENCE_B={fb} BLOCKED={blocked} EARLY={early} FINAL_MADE={final_made}");
    drop((lists, b, sa, sb));
    (early, final_made)
}
