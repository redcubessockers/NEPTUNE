//! The pass that uploads textures picked by index, on a thread of its own
//! (`VICTORY_HEAP_WORKER=1`; off by default; needs `VICTORY_LATE_WAIT`).
//!
//! [`crate::draw::State::heap_textures`] runs on whichever thread of the
//! title records the frame's first shader that picks textures by index: it
//! asks what of each due texture is mapped, makes the host texture, makes a
//! staging buffer and records the copy, up to 250 ms of every frame while a
//! backlog waits, and the threads waiting for that list wait with it. The
//! detiling of what it recorded then runs on the thread that submits. With
//! the switch the pass runs here, and the recording thread only wakes it.
//!
//! What keeps the picture as it was:
//! - the uploads are recorded into host lists of ours ([`Batch`]), which the
//!   next submission of the title's runs ahead of its own lists
//!   ([`take_ready`]): texture states are settled by submission order, as
//!   between any two lists;
//! - a texture's host slot is given its view at that submission, not before
//!   ([`Publish`]): until then shaders see what the slot showed, never a
//!   texture without its contents. (A host texture may be placed in memory
//!   another texture had: `crate::placed`.) A slot the title wrote again
//!   meanwhile is left to the write's own turn in the queue;
//! - the detiling done here is checked at submission as any texture's is:
//!   memory the title wrote in between is read again;
//! - a title texture the pass works on is held ([`crate::resource::hold`]),
//!   and so is the title's heap;
//! - the uploads before the title's decoration bake, which must be there
//!   for that draw, still run on the recording thread; the worker makes way
//!   for them ([`urgent`]).
// shortcut: what a submission shows for a texture is up to a frame later
// than when the recording thread did it all; with a backlog it is earlier,
// since the pass has the whole frame instead of a slice of it.

use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering::Relaxed};
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};

use crate::descriptor::Heap;
use crate::host::HostList;
use crate::object::This;

/// Whether the pass runs on its own thread now.
pub(crate) fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("VICTORY_HEAP_WORKER").is_some_and(|v| v == "1")) && crate::host::late()
}

/// For the frame line, since the last one: nanoseconds the worker was at
/// work, of them detiling, lists given to submissions, textures published,
/// slots the title had written again, and textures in those lists whose
/// detiling was not done here.
pub(crate) static STATS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

/// One upload list holds at most this much texture memory, or this many
/// milliseconds of work, so that its textures show soon.
const LIST_BYTES: u64 = 48 << 20;
const LIST_MS: u128 = 40;
const OUTSTANDING_LISTS: usize = 4;

fn outstanding_full(count: usize) -> bool {
    count >= OUTSTANDING_LISTS
}

static OUTSTANDING: AtomicUsize = AtomicUsize::new(0);
pub(crate) static CAPACITY_STOPS: AtomicU64 = AtomicU64::new(0);

/// One list admitted to READY or waiting for descriptor publication. The
/// token follows the list's publication metadata and releases its place on
/// every drop path.
struct Outstanding;

impl Outstanding {
    fn new() -> Option<Self> {
        OUTSTANDING
            .try_update(Relaxed, Relaxed, |count| (!outstanding_full(count)).then_some(count + 1))
            .map(|_| Outstanding)
            .map_err(|_| CAPACITY_STOPS.fetch_add(1, Relaxed))
            .ok()
    }
}

impl Drop for Outstanding {
    fn drop(&mut self) {
        let before = OUTSTANDING.fetch_sub(1, Relaxed);
        debug_assert!(before > 0);
    }
}

/// A view to give a host slot once its texture's upload is in the queue:
/// the title's heap, the slot, the record's handle, and the record's key as
/// it was when the upload was recorded.
pub(crate) struct Publish {
    index: u32,
    handle: u64,
    key: u64,
}

/// Holds a reference to a title heap.
struct HeldHeap(This);
// SAFETY: a reference counted title object, of which only the count and the
// parts behind locks are touched.
unsafe impl Send for HeldHeap {}

impl HeldHeap {
    fn new(heap: &Heap) -> Self {
        let this = std::ptr::from_ref(heap).cast();
        crate::object::add_ref(this);
        HeldHeap(this)
    }

    fn heap(&self) -> &Heap {
        crate::descriptor::heap(self.0)
    }
}

impl Drop for HeldHeap {
    fn drop(&mut self) {
        crate::object::release_boxed::<Heap>(self.0);
    }
}

/// A closed upload list, and what is published when it is submitted.
struct Ready {
    list: HostList,
    publish: Vec<Publish>,
    _heap: HeldHeap,
    _outstanding: Outstanding,
}
// SAFETY: D3D12 objects are free-threaded; the list is closed and touched
// by nobody until it is taken out.
unsafe impl Send for Ready {}

static READY: Mutex<Vec<Ready>> = Mutex::new(Vec::new());

/// Descriptor publications carried by upload lists until the submission
/// containing those lists has finished. Keeping the title heap alive also
/// keeps its host heap and descriptor records valid for the final key check.
pub(crate) struct ReadyPublish {
    publish: Vec<Publish>,
    _heap: HeldHeap,
    _outstanding: Outstanding,
}

/// Publication batches waiting for a completed submission number.
static PENDING: Mutex<Vec<(u64, Vec<ReadyPublish>)>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What a pass on the worker's thread records into: one upload list after
/// the other, made when first needed.
pub(crate) struct Batch {
    heap: This,
    list: Option<HostList>,
    publish: Vec<Publish>,
    outstanding: Option<Outstanding>,
    bytes: u64,
    began: std::time::Instant,
}

impl Batch {
    fn new(heap: &Heap) -> Self {
        Batch {
            heap: std::ptr::from_ref(heap).cast(),
            list: None,
            publish: Vec::new(),
            outstanding: None,
            bytes: 0,
            began: std::time::Instant::now(),
        }
    }

    /// The open upload list.
    pub(crate) fn list(&mut self) -> Option<&HostList> {
        if self.list.is_none() {
            self.outstanding = Outstanding::new();
            if self.outstanding.is_none() {
                return None;
            }
            self.list = crate::list::fresh_list();
            if self.list.is_none() {
                self.outstanding = None;
                return None;
            }
            self.began = std::time::Instant::now();
        }
        self.list.as_ref()
    }

    /// Slot `index` (its record at `handle`, then with `key`) shows its
    /// texture when the open list is submitted. `bytes` were uploaded.
    pub(crate) fn publish(&mut self, index: u32, handle: u64, key: u64) {
        self.publish.push(Publish { index, handle, key });
    }

    /// `bytes` more of texture memory in the open list, which is closed
    /// and handed over when it holds enough.
    pub(crate) fn uploaded(&mut self, bytes: u64) {
        self.bytes += bytes;
        if self.bytes >= LIST_BYTES || self.began.elapsed().as_millis() >= LIST_MS {
            self.flush();
        }
    }

    /// Detiles what the open list uploads, closes it and hands it to the
    /// next submission.
    fn flush(&mut self) {
        self.bytes = 0;
        let publish = std::mem::take(&mut self.publish);
        let Some(list) = self.list.take() else {
            return;
        };
        let outstanding = self.outstanding.take().expect("an open worker list has an outstanding token");
        let started = std::time::Instant::now();
        let left = list.refresh_textures_ahead();
        STATS[1].fetch_add(started.elapsed().as_nanos() as u64, Relaxed);
        STATS[5].fetch_add(left, Relaxed);
        list.close();
        let heap = HeldHeap::new(crate::descriptor::heap(self.heap));
        lock(&READY).push(Ready { list, publish, _heap: heap, _outstanding: outstanding });
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        self.flush();
    }
}

/// The upload lists ready now, for the submission about to be made, which
/// runs them ahead of its own lists; their host slots are carried separately
/// and are given their views only after that submission has finished.
/// Called by the thread that submits.
pub(crate) fn take_ready() -> (Vec<HostList>, Vec<ReadyPublish>) {
    // (Asked without the lock by every submission of a run without it.)
    if JOB.get().is_none() {
        return (Vec::new(), Vec::new());
    }
    let ready = std::mem::take(&mut *lock(&READY));
    STATS[2].fetch_add(ready.len() as u64, Relaxed);
    let mut lists = Vec::with_capacity(ready.len());
    let mut publish = Vec::with_capacity(ready.len());
    for r in ready {
        lists.push(r.list);
        publish.push(ReadyPublish {
            publish: r.publish,
            _heap: r._heap,
            _outstanding: r._outstanding,
        });
    }
    (lists, publish)
}

/// Keep the views until their upload and every potentially reading
/// submission are complete. Only the submission owner drains this queue.
pub(crate) fn publish_after(after: u64, ready: Vec<ReadyPublish>) {
    if !ready.is_empty() {
        lock(&PENDING).push((after, ready));
    }
}

/// Called only under the host submission mutex, before the next submission
/// starts or after settling. A relay completion alone cannot publish: a
/// later flight could already be reading this shared shader-visible heap.
pub(crate) fn publish_idle() {
    let Some(finished) = crate::host::idle_submission() else { return };
    let ready = {
        let mut pending = lock(&PENDING);
        if pending.is_empty() { return; }
        let (ready, wait) = std::mem::take(&mut *pending)
            .into_iter()
            .partition(|(after, _)| *after <= finished);
        *pending = wait;
        ready
    };
    for (_, batch) in ready {
        publish(batch);
    }
}

fn publish(ready: Vec<ReadyPublish>) {
    for r in ready {
        for p in &r.publish {
            let Some(host) = r._heap.heap().host() else {
                continue;
            };
            let shown = host.publish(p.index, p.handle, p.key);
            STATS[if shown { 3 } else { 4 }].fetch_add(1, Relaxed);
        }
    }
}

/// The heap the next pass is over, and what wakes the worker.
struct Job {
    heap: Mutex<Option<HeldHeap>>,
    wake: Condvar,
}

static JOB: OnceLock<Job> = OnceLock::new();
/// Held by whoever runs the pass.
static PASS: Mutex<()> = Mutex::new(());
/// Number of recording threads waiting for [`PASS`]. A successful waiter
/// clears only its own request, never the requests of other waiters.
static URGENT: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static URGENT_ENTERED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A pass over `heap` is due (once a frame). False if there is no worker:
/// the caller runs the pass itself.
pub(crate) fn kick(heap: &Heap) -> bool {
    if !on() {
        return false;
    }
    static STARTED: OnceLock<bool> = OnceLock::new();
    let started = *STARTED.get_or_init(|| {
        JOB.get_or_init(|| Job { heap: Mutex::new(None), wake: Condvar::new() });
        match std::thread::Builder::new().name("victory-heap".into()).spawn(worker) {
            Ok(_) => {
                tracing::info!("textures picked by index are uploaded by a thread of their own (VICTORY_HEAP_WORKER)");
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "no thread for textures picked by index");
                false
            }
        }
    });
    let Some(job) = JOB.get().filter(|_| started) else {
        return false;
    };
    *lock(&job.heap) = Some(HeldHeap::new(heap));
    job.wake.notify_one();
    true
}

/// Takes the pass from the worker for the caller, which cannot wait for a
/// later submission: what the worker has recorded so far is handed over
/// first, so the caller's list runs behind it.
pub(crate) fn urgent() -> MutexGuard<'static, ()> {
    if let Ok(pass) = PASS.try_lock() {
        return pass;
    }
    URGENT.fetch_add(1, Relaxed);
    #[cfg(test)]
    URGENT_ENTERED.fetch_add(1, std::sync::atomic::Ordering::Release);
    let pass = lock(&PASS);
    URGENT.fetch_sub(1, Relaxed);
    pass
}

/// Whether the worker is to stop where it is ([`urgent`]).
pub(crate) fn make_way() -> bool {
    URGENT.load(Relaxed) != 0
}

fn worker() {
    let Some(job) = JOB.get() else {
        return;
    };
    loop {
        let heap = {
            let mut next = lock(&job.heap);
            loop {
                match next.take() {
                    Some(heap) => break heap,
                    None => next = job.wake.wait(next).unwrap_or_else(|e| e.into_inner()),
                }
            }
        };
        let started = std::time::Instant::now();
        {
            let _pass = lock(&PASS);
            let heap = heap.heap();
            if let Some(host) = heap.host() {
                let mut batch = Batch::new(heap);
                crate::draw::State::heap_textures(crate::draw::Into::Ahead(&mut batch), heap, host);
            }
        }
        STATS[0].fetch_add(started.elapsed().as_nanos() as u64, Relaxed);
        // Whoever wants the pass now has it first.
        while make_way() {
            std::thread::yield_now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread, time::{Duration, Instant}};

    #[test]
    fn neptune_outstanding_capacity_boundary() {
        assert_eq!(OUTSTANDING.load(Relaxed), 0);
        assert!(!outstanding_full(OUTSTANDING_LISTS - 1));
        assert!(outstanding_full(OUTSTANDING_LISTS));
        assert!(outstanding_full(OUTSTANDING_LISTS + 1));
        let before = CAPACITY_STOPS.load(Relaxed);
        let mut tokens: Vec<Outstanding> = (0..OUTSTANDING_LISTS).map(|_| Outstanding::new().unwrap()).collect();
        assert_eq!(OUTSTANDING.load(Relaxed), OUTSTANDING_LISTS);
        assert!(Outstanding::new().is_none());
        assert_eq!(CAPACITY_STOPS.load(Relaxed), before + 1);
        drop(tokens.pop());
        tokens.push(Outstanding::new().expect("a released place is reusable"));
        drop(tokens);
        println!("OUTSTANDING_LIMIT={OUTSTANDING_LISTS} CAPACITY_STOPS=1 FINAL={}", OUTSTANDING.load(Relaxed));
        assert_eq!(OUTSTANDING.load(Relaxed), 0);
    }

    #[test]
    #[ignore = "isolated global worker state"]
    fn neptune_two_urgent_waiters_preserve_request() {
        use std::sync::atomic::Ordering::Acquire;
        assert!(!make_way());
        let baseline = URGENT_ENTERED.load(Acquire);
        let pass = lock(&PASS);
        let (entered, acquired) = mpsc::channel();
        let mut senders = Vec::new();
        let mut threads = Vec::new();
        for id in 0..2 {
            let entered = entered.clone();
            let (release, wait) = mpsc::channel();
            senders.push(release);
            threads.push(thread::spawn(move || {
                let _pass = urgent();
                entered.send(id).unwrap();
                wait.recv_timeout(Duration::from_secs(5)).unwrap();
            }));
        }
        drop(entered);
        let deadline = Instant::now() + Duration::from_secs(3);
        while URGENT_ENTERED.load(Acquire) != baseline + 2 && Instant::now() < deadline {
            thread::yield_now();
        }
        let registered = URGENT_ENTERED.load(Acquire) - baseline;
        drop(pass);
        let first = acquired.recv_timeout(Duration::from_secs(3)).unwrap();
        let observed = make_way();
        senders[first].send(()).unwrap();
        let second = acquired.recv_timeout(Duration::from_secs(3)).unwrap();
        senders[second].send(()).unwrap();
        for t in threads { t.join().unwrap(); }
        assert_eq!(registered, 2);
        assert_ne!(first, second);
        assert!(!make_way());
        println!("REGISTERED={registered} FIRST={first} SECOND={second} REMAINING_REQUEST={observed}");
        assert!(observed, "first lock acquisition erased another pending urgent request");
    }

    #[test]
    #[ignore = "isolated global worker state"]
    fn neptune_urgent_waiter_boundaries() {
        use std::sync::atomic::Ordering::Acquire;
        assert_eq!(URGENT.load(Acquire), 0);
        let initial = URGENT_ENTERED.load(Acquire);
        { let _pass = urgent(); assert!(!make_way()); }
        assert_eq!(URGENT_ENTERED.load(Acquire), initial, "uncontended fast path");
        let mut handoffs = 0;
        for n in [1usize, 2, 8, 16] {
            for _ in 0..8 {
                let baseline = URGENT_ENTERED.load(Acquire);
                let pass = lock(&PASS);
                let (tx, rx) = mpsc::channel();
                let mut release = Vec::new();
                let mut threads = Vec::new();
                for id in 0..n {
                    let tx = tx.clone();
                    let (send, recv) = mpsc::channel();
                    release.push(send);
                    threads.push(thread::spawn(move || {
                        let _pass = urgent();
                        tx.send(id).unwrap();
                        recv.recv_timeout(Duration::from_secs(5)).unwrap();
                    }));
                }
                drop(tx);
                let deadline = Instant::now() + Duration::from_secs(3);
                while URGENT_ENTERED.load(Acquire) != baseline + n && Instant::now() < deadline {
                    thread::yield_now();
                }
                let registered = URGENT_ENTERED.load(Acquire) - baseline;
                let pending = URGENT.load(Acquire);
                drop(pass);
                let mut seen = vec![false; n];
                let mut mismatches = 0;
                for remaining in (0..n).rev() {
                    let id = rx.recv_timeout(Duration::from_secs(3)).unwrap();
                    assert!(!seen[id]);
                    seen[id] = true;
                    if URGENT.load(Acquire) != remaining || make_way() != (remaining != 0) {
                        mismatches += 1;
                    }
                    release[id].send(()).unwrap();
                    handoffs += 1;
                }
                for t in threads { t.join().unwrap(); }
                assert_eq!(registered, n);
                assert_eq!(pending, n);
                assert_eq!(mismatches, 0);
                assert!(seen.iter().all(|v| *v));
                assert_eq!(URGENT.load(Acquire), 0);
            }
        }
        println!("WAITER_BOUNDARIES=1,2,8,16 ROUNDS=32 HANDOFFS={handoffs} FINAL_PENDING={}", URGENT.load(Acquire));
        assert_eq!(handoffs, 216);
    }
}

#[cfg(test)]
pub(crate) fn outstanding_count() -> usize {
    OUTSTANDING.load(Relaxed)
}

#[cfg(test)]
pub(crate) fn stage_fixture(heap: &Heap, list: HostList, entries: &[(u32, u64, u64)]) {
    assert!(JOB.get().is_none(), "isolated test must not start worker");
    JOB.get_or_init(|| Job { heap: Mutex::new(None), wake: Condvar::new() });
    assert!(lock(&READY).is_empty());
    let mut batch = Batch::new(heap);
    batch.outstanding = Outstanding::new();
    assert!(batch.outstanding.is_some());
    batch.list = Some(list);
    for &(index, handle, key) in entries {
        batch.publish(index, handle, key);
    }
    drop(batch); // production flush, close and enqueue
    assert_eq!(lock(&READY).len(), 1);
    assert_eq!(outstanding_count(), 1);
}
