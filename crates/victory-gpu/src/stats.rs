//! The machine's load, looked at once a second on a thread of its own, for
//! the box in the corner of the picture (`crate::fpsbox`): how busy the
//! processors and the card are and how much memory is left, so that whoever
//! watches the picture sees which of them holds the frame.
//!
//! - Processors: the share of all logical processors that was busy, system
//!   wide (`GetSystemTimes`); the processor time of this process, in
//!   processors (`GetProcessTimes`: 5.2 means it kept 5.2 of them busy);
//!   and its busiest thread, in per cent of one processor
//!   (`GetThreadTimes`). A thread near 100 % with the card far from it is a
//!   frame held by that thread.
//! - The card: Windows' own counters (`\GPU Engine(*)\Utilization
//!   Percentage`, the ones the Task Manager shows), which name the process
//!   and the engine of every figure. For the host D3D12 adapter, across
//!   all processes and this process: the busiest physical engine. They
//!   work for every maker of cards and load no library of the maker's.
//! - Memory: this process's committed bytes and the PC's free physical
//!   memory. (The card's memory is asked of the host by the box itself.)
//!
//! Nothing here touches the title or the card. `VICTORY_FPS_COUNTER=0` or
//! `=fps` never starts the thread.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One look.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Stats {
    /// Per cent of all logical processors busy, system wide.
    pub(crate) cpu_all: f32,
    /// This process's processor time, in processors.
    pub(crate) cores: f32,
    /// This process's busiest thread, in per cent of one processor.
    pub(crate) top_thread: f32,
    /// The card's busiest engine, per cent, all processes; none if the
    /// host adapter has no usable counter sample.
    pub(crate) gpu_all: Option<f32>,
    /// The same for this process alone.
    pub(crate) gpu_ours: Option<f32>,
    /// This process's committed memory, MB.
    pub(crate) ram_ours_mb: u32,
    /// The PC's free physical memory, MB.
    pub(crate) ram_free_mb: u32,
}

/// The last look, and its number (1 for the first).
static NOW: Mutex<(u64, Option<Stats>)> = Mutex::new((0, None));

/// The last look and its number; none before the first.
pub(crate) fn now() -> Option<(u64, Stats)> {
    let now = NOW.lock().unwrap_or_else(|e| e.into_inner());
    now.1.map(|s| (now.0, s))
}

/// Starts the looks, once, bound to the exact adapter backing the host D3D12 device.
pub(crate) fn start(device: &windows::Win32::Graphics::Direct3D12::ID3D12Device) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: GetAdapterLuid is a read-only query on the live host device.
        let luid = unsafe { device.GetAdapterLuid() };
        let adapter = AdapterLuid { high: luid.HighPart as u32, low: luid.LowPart };
        tracing::info!(
            high = format_args!("{:#010x}", adapter.high),
            low = format_args!("{:#010x}", adapter.low),
            "the corner box's GPU counters are bound to the host D3D12 adapter LUID"
        );
        let spawned = std::thread::Builder::new()
            .name("victory-stats".into())
            .spawn(move || run(adapter));
        if let Err(e) = spawned {
            tracing::warn!(%e, "the load figures of the corner box have no thread: the box shows the frame rate only");
        }
    });
}

/// A 100 ns count from the two halves Windows gives.
fn ticks(t: &windows_sys::Win32::Foundation::FILETIME) -> u64 {
    u64::from(t.dwHighDateTime) << 32 | u64::from(t.dwLowDateTime)
}

/// System wide: (idle, busy and idle together), in 100 ns of processor time.
fn system_times() -> Option<(u64, u64)> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::GetSystemTimes;
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: three structures the call fills.
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        return None;
    }
    // The kernel's time has the idle time in it.
    Some((ticks(&idle), ticks(&kernel) + ticks(&user)))
}

/// This process's processor time, in 100 ns.
fn process_time() -> u64 {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let mut t = [FILETIME::default(); 4];
    let [created, exited, kernel, user] = &mut t;
    // SAFETY: our own process and four structures the call fills.
    if unsafe { GetProcessTimes(GetCurrentProcess(), created, exited, kernel, user) } == 0 {
        return 0;
    }
    ticks(&t[2]) + ticks(&t[3])
}

/// The processor time of each thread of this process, in 100 ns, by thread
/// id. (The snapshot lists every thread of the system: a few thousand
/// entries, about a millisecond.)
fn thread_times() -> HashMap<u32, u64> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcessId, GetThreadTimes, OpenThread, THREAD_QUERY_LIMITED_INFORMATION};
    let mut out = HashMap::new();
    // SAFETY: a snapshot handle of our own, closed below; sized structures
    // the calls fill; thread handles opened and closed here.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
            return out;
        }
        let pid = GetCurrentProcessId();
        let mut entry = THREADENTRY32 { dwSize: size_of::<THREADENTRY32>() as u32, ..Default::default() };
        let mut more = Thread32First(snapshot, &mut entry) != 0;
        while more {
            if entry.th32OwnerProcessID == pid {
                let thread = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, entry.th32ThreadID);
                if !thread.is_null() {
                    let mut t = [FILETIME::default(); 4];
                    let [created, exited, kernel, user] = &mut t;
                    if GetThreadTimes(thread, created, exited, kernel, user) != 0 {
                        out.insert(entry.th32ThreadID, ticks(&t[2]) + ticks(&t[3]));
                    }
                    CloseHandle(thread);
                }
            }
            more = Thread32Next(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    out
}

/// This process's committed memory and the PC's free physical memory, MB.
fn memory() -> (u32, u32) {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    let mut c = PROCESS_MEMORY_COUNTERS { cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
    // SAFETY: plain data the call fills in.
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    // SAFETY: our own process and sized structures.
    unsafe {
        K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb);
        GlobalMemoryStatusEx(&mut m);
    }
    ((c.PagefileUsage >> 20) as u32, (m.ullAvailPhys >> 20) as u32)
}

/// Windows' counters of the card's engines: one figure for each process
/// and engine.
struct Engines {
    query: windows_sys::Win32::System::Performance::PDH_HQUERY,
    counter: windows_sys::Win32::System::Performance::PDH_HCOUNTER,
    /// The answer's memory, kept from look to look (8-byte units: the
    /// items hold pointers and doubles).
    buffer: Vec<u64>,
}

/// Identity of one physical GPU engine, independent of which process used it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct EngineKey {
    luid_high: u32,
    luid_low: u32,
    phys: u32,
    engine: u32,
}

/// The DXGI adapter identity carried into Windows' GPU Engine counters.
/// High is stored as raw bits because PDH prints the signed LONG in hex.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AdapterLuid {
    high: u32,
    low: u32,
}

/// One PDH process/engine sample.
#[derive(Debug, PartialEq)]
struct Engine {
    pid: u32,
    key: EngineKey,
    kind: String,
    percent: f64,
}

/// Exact fields in a Windows GPU Engine counter instance. Never infer an
/// engine's identity from its type: one adapter can have many Copy engines.
fn parse_instance(name: &str) -> Option<(u32, EngineKey, &str)> {
    let (pid, rest) = name.strip_prefix("pid_")?.split_once("_luid_")?;
    let (high, rest) = rest.split_once('_')?;
    let (low, rest) = rest.split_once("_phys_")?;
    let (phys, rest) = rest.split_once("_eng_")?;
    let (engine, kind) = rest.split_once("_engtype_")?;
    if kind.is_empty() {
        return None;
    }
    let hex = |word: &str| u32::from_str_radix(word.strip_prefix("0x")?, 16).ok();
    Some((
        pid.parse().ok()?,
        EngineKey {
            luid_high: hex(high)?,
            luid_low: hex(low)?,
            phys: phys.parse().ok()?,
            engine: engine.parse().ok()?,
        },
        kind,
    ))
}

/// Sum per-process utilization only for the SAME physical engine. Report
/// the busiest engine per type; never sum different engines or adapters.
fn busiest_filtered(
    engines: &[Engine],
    pid: Option<u32>,
    adapter: Option<AdapterLuid>,
) -> (f32, HashMap<&str, f64>) {
    let mut by_engine: HashMap<EngineKey, (f64, &str)> = HashMap::new();
    for e in engines.iter().filter(|e| {
        pid.is_none_or(|p| p == e.pid)
            && adapter.is_none_or(|a| a.high == e.key.luid_high && a.low == e.key.luid_low)
            && e.percent.is_finite()
            && e.percent >= 0.0
    }) {
        let entry = by_engine.entry(e.key).or_insert((0.0, e.kind.as_str()));
        entry.0 += e.percent;
    }
    let mut by_kind: HashMap<&str, f64> = HashMap::new();
    for (_, (percent, kind)) in by_engine {
        let load = percent.min(100.0);
        let top = by_kind.entry(kind).or_insert(0.0);
        *top = (*top).max(load);
    }
    let top = by_kind.values().copied().fold(0.0, f64::max);
    (top as f32, by_kind)
}

#[cfg(test)]
fn busiest(engines: &[Engine], pid: Option<u32>) -> (f32, HashMap<&str, f64>) {
    busiest_filtered(engines, pid, None)
}

fn busiest_on_adapter(
    engines: &[Engine],
    pid: Option<u32>,
    adapter: AdapterLuid,
) -> (f32, HashMap<&str, f64>) {
    busiest_filtered(engines, pid, Some(adapter))
}

// Keep the PDH per-instance status decision explicit for regression tests.
fn counter_status_valid(status: u32) -> bool {
    // pdhmsg.h: VALID_DATA (0) and NEW_DATA (1) both carry valid values.
    matches!(status, 0 | 1)
}

// Native lifetime witness: counts only successful opens and actual close calls
// on this test thread. No production storage or instrumentation.
#[cfg(test)]
std::thread_local! {
    static PDH_LIFETIME: std::cell::Cell<(u32, u32, u32)> =
        const { std::cell::Cell::new((0, 0, u32::MAX)) };
}

impl Engines {
    fn open() -> Option<Engines> {
        Self::open_path("\\GPU Engine(*)\\Utilization Percentage")
    }

    fn open_path(path: &str) -> Option<Engines> {
        use windows_sys::Win32::System::Performance::{PdhAddEnglishCounterW, PdhCollectQueryData, PdhOpenQueryW};
        let path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let mut query = std::ptr::null_mut();
        // SAFETY: plain calls with a terminated string and valid output
        // locations. The query is owned immediately after a successful open.
        unsafe {
            if PdhOpenQueryW(std::ptr::null(), 0, &mut query) != 0 {
                return None;
            }
            #[cfg(test)]
            PDH_LIFETIME.with(|v| {
                let (opens, closes, status) = v.get();
                v.set((opens + 1, closes, status));
            });
            let mut owner = Engines { query, counter: std::ptr::null_mut(), buffer: Vec::new() };
            if PdhAddEnglishCounterW(owner.query, path.as_ptr(), 0, &mut owner.counter) != 0 {
                return None; // owner closes the query, even on partial initialization
            }
            // The first collection has nothing to compare with: a figure
            // comes from the second on.
            PdhCollectQueryData(owner.query);
            Some(owner)
        }
    }

    /// The figures since the last call.
    fn look(&mut self) -> Option<Vec<Engine>> {
        use windows_sys::Win32::System::Performance::{
            PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        };
        // SAFETY: our own handles; the buffer is as long as the size given,
        // and the items and the names they point at lie inside it until the
        // next call.
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let (mut bytes, mut count) = (0u32, 0u32);
            let status = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, std::ptr::null_mut());
            if status != PDH_MORE_DATA || bytes == 0 {
                return None;
            }
            // Instances come and go between the two calls: some room over.
            bytes += 16 << 10;
            self.buffer.resize((bytes as usize).div_ceil(8), 0);
            let items = self.buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, items) != 0 {
                return None;
            }
            let mut out = Vec::with_capacity(count as usize);
            for item in std::slice::from_raw_parts(items, count as usize) {
                if item.szName.is_null() || !counter_status_valid(item.FmtValue.CStatus) {
                    continue;
                }
                let len = (0..512).take_while(|&i| *item.szName.add(i) != 0).count();
                let name = String::from_utf16_lossy(std::slice::from_raw_parts(item.szName, len));
                if let Some((pid, key, kind)) = parse_instance(&name) {
                    out.push(Engine { pid, key, kind: kind.to_owned(), percent: item.FmtValue.Anonymous.doubleValue });
                }
            }
            Some(out)
        }
    }
}

impl Drop for Engines {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Performance::PdhCloseQuery;
        // SAFETY: this owner holds exactly one successfully opened query.
        // PDH closes every counter associated with it; no separate counter close.
        let status = unsafe { PdhCloseQuery(self.query) };
        #[cfg(test)]
        PDH_LIFETIME.with(|v| {
            let (opens, closes, _) = v.get();
            v.set((opens, closes + 1, status));
        });
        if status != 0 {
            tracing::warn!(status = format_args!("{status:#010x}"), "PDH query close failed");
        }
    }
}

/// Convert a PDH snapshot into the two displayed adapter load figures.
fn adapter_loads(
    looked: Option<&[Engine]>, pid: u32, adapter: AdapterLuid,
) -> (Option<f32>, Option<f32>) {
    let Some(e) = looked else { return (None, None) };
    let (all, kinds) = busiest_on_adapter(e, None, adapter);
    // An empty engine map is not a measured zero. This also covers
    // adapters without PDH counters and snapshots containing only invalid values.
    if kinds.is_empty() {
        return (None, None);
    }
    (Some(all), Some(busiest_on_adapter(e, Some(pid), adapter).0))
}

/// The thread: a look a second, for as long as the process runs.
fn run(adapter: AdapterLuid) {
    let mut engines = Engines::open();
    if engines.is_none() {
        tracing::info!("the system's counters of the card's engines did not open: the corner box shows no card load");
    }
    let processors = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
    // SAFETY: a plain call.
    let pid = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessId() };
    let mut last = (Instant::now(), system_times(), process_time(), thread_times());
    let mut looks = 0u64;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let now = (Instant::now(), system_times(), process_time(), thread_times());
        // 100 ns of wall time since the last look.
        let wall = (now.0 - last.0).as_secs_f64() * 1e7;
        if wall <= 0.0 {
            continue;
        }
        let cpu_all = match (last.1, now.1) {
            (Some((idle0, all0)), Some((idle1, all1))) if all1 > all0 => {
                100.0 * (1.0 - (idle1.saturating_sub(idle0)) as f64 / (all1 - all0) as f64)
            }
            _ => 0.0,
        };
        let cores = now.2.saturating_sub(last.2) as f64 / wall;
        let top_thread = now
            .3
            .iter()
            .map(|(id, &t)| t.saturating_sub(last.3.get(id).copied().unwrap_or(t)))
            .max()
            .unwrap_or(0) as f64
            / wall
            * 100.0;
        let looked = engines.as_mut().and_then(Engines::look);
        let (gpu_all, gpu_ours) = adapter_loads(looked.as_deref(), pid, adapter);
        let (ram_ours_mb, ram_free_mb) = memory();
        let stats = Stats {
            cpu_all: cpu_all.clamp(0.0, 100.0) as f32,
            cores: cores.clamp(0.0, processors) as f32,
            top_thread: top_thread.clamp(0.0, 100.0) as f32,
            gpu_all,
            gpu_ours,
            ram_ours_mb,
            ram_free_mb,
        };
        looks += 1;
        *NOW.lock().unwrap_or_else(|e| e.into_inner()) = (looks, Some(stats));
        // In the log every fifth look, with the card's engines by kind: the
        // box has room for one figure only.
        if looks.is_multiple_of(5) {
            let kinds = |pid: Option<u32>| {
                if gpu_all.is_none() {
                    return "not known".to_owned();
                }
                let Some(e) = &looked else { return "not known".to_owned() };
                let mut kinds: Vec<(&str, f64)> =
                    busiest_on_adapter(e, pid, adapter).1.into_iter().filter(|(_, p)| *p >= 0.5).collect();
                kinds.sort_by(|a, b| b.1.total_cmp(&a.1));
                if kinds.is_empty() {
                    return "nothing at work".to_owned();
                }
                kinds.iter().map(|(k, p)| format!("{k} {p:.0} %")).collect::<Vec<_>>().join(", ")
            };
            tracing::info!(
                "the machine's load over the last second (the corner box's figures): processors {:.0} % busy, all programs; this \
                 process kept {:.1} of {processors:.0} processors busy, its busiest thread {:.0} % of one; the card's engines, all \
                 programs: {}; this process's: {}; this process has {} MB committed, the PC {} MB of memory free",
                stats.cpu_all,
                stats.cores,
                stats.top_thread,
                kinds(None),
                kinds(Some(pid)),
                stats.ram_ours_mb,
                stats.ram_free_mb,
            );
        }
        last = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "native Windows PDH query ownership witness"]
    fn pdh_query_is_closed_when_counter_registration_fails() {
        PDH_LIFETIME.with(|v| v.set((0, 0, u32::MAX)));
        let result = Engines::open_path("\\NEPTUNE_R74_NONEXISTENT_OBJECT_20261128\\NoSuchCounter");
        assert!(result.is_none(), "invalid counter registration must fail");
        let observed = PDH_LIFETIME.with(|v| v.get());
        println!("R74_PDH_FAILED_REGISTRATION opens={} closes={} close_status={:#010x}", observed.0, observed.1, observed.2);
        assert_eq!(observed, (1, 1, 0), "failed registration must close its opened query");
    }

    #[test]
    #[ignore = "native Windows PDH query ownership witness"]
    fn pdh_query_is_closed_when_owner_is_dropped() {
        PDH_LIFETIME.with(|v| v.set((0, 0, u32::MAX)));
        let owner = Engines::open_path("\\Processor(_Total)\\% Processor Time")
            .expect("open native processor counter");
        assert_eq!(PDH_LIFETIME.with(|v| v.get()), (1, 0, u32::MAX));
        drop(owner);
        let observed = PDH_LIFETIME.with(|v| v.get());
        println!("R74_PDH_OWNER_DROP opens={} closes={} close_status={:#010x}", observed.0, observed.1, observed.2);
        assert_eq!(observed, (1, 1, 0), "owner drop must close its query exactly once");
    }

    #[test]
    fn pdh_new_data_is_a_valid_sample() {
        // pdhmsg.h: PDH_CSTATUS_NEW_DATA = 0x00000001.
        assert!(counter_status_valid(1));
    }

    #[test]
    fn pdh_unchanged_data_and_error_statuses() {
        assert!(counter_status_valid(0));
        for status in [2, 0x800007d1, 0x800007d5, 0xc0000bba, u32::MAX] {
            assert!(!counter_status_valid(status), "unexpected accepted status {status:#x}");
        }
    }

    #[test]
    fn distinct_gpu_engines_keep_distinct_instance_identity() {
        let a = parse_instance("pid_7_luid_0x00000000_0x00000001_phys_0_eng_3_engtype_copy");
        let b = parse_instance("pid_7_luid_0x00000000_0x00000001_phys_0_eng_10_engtype_copy");
        assert!(a.is_some() && b.is_some());
        assert_ne!(a, b, "different physical engines must not alias by engine type");
    }

    #[test]
    fn distinct_gpu_adapters_keep_distinct_instance_identity() {
        let a = parse_instance("pid_7_luid_0x00000000_0x00000001_phys_0_eng_3_engtype_copy");
        let b = parse_instance("pid_7_luid_0x00000000_0x00000002_phys_0_eng_3_engtype_copy");
        assert!(a.is_some() && b.is_some());
        assert_ne!(a, b, "different adapter LUIDs must not alias");
    }

    #[test]
    fn malformed_gpu_instance_is_not_counted() {
        assert!(parse_instance("pid_7_engtype_copy").is_none());
        assert!(parse_instance("pid_7_luid_0x00000000_0x00000001_phys_0_eng_3_engtype_").is_none());
    }

    #[test]
    fn an_instance_name_says_the_process_and_the_engine() {
        let first = EngineKey { luid_high: 0, luid_low: 0xe3b5, phys: 0, engine: 0 };
        let second = EngineKey { luid_high: 0, luid_low: 1, phys: 0, engine: 5 };
        assert_eq!(parse_instance("pid_1234_luid_0x00000000_0x0000E3B5_phys_0_eng_0_engtype_3D"), Some((1234, first, "3D")));
        assert_eq!(parse_instance("pid_8_luid_0x0_0x1_phys_0_eng_5_engtype_Compute_0"), Some((8, second, "Compute_0")));
        assert_eq!(parse_instance("_Total"), None);
        assert_eq!(parse_instance("pid_x_engtype_3D"), None);
    }

    #[test]
    fn absent_adapter_is_unknown_not_idle() {
        let selected = AdapterLuid { high: 0, low: 1 };
        let other = Engine {
            pid: 7, key: EngineKey { luid_high: 0, luid_low: 2, phys: 0, engine: 0 },
            kind: "3D".into(), percent: 99.0,
        };
        assert_eq!(adapter_loads(Some(&[other]), 7, selected), (None, None));
    }

    #[test]
    fn unusable_adapter_samples_are_unknown() {
        let selected = AdapterLuid { high: 0, low: 1 };
        let e = |percent| Engine {
            pid: 7, key: EngineKey { luid_high: 0, luid_low: 1, phys: 0, engine: 0 },
            kind: "3D".into(), percent,
        };
        assert_eq!(adapter_loads(Some(&[]), 7, selected), (None, None));
        assert_eq!(adapter_loads(Some(&[e(f64::NAN), e(f64::INFINITY), e(-1.0)]), 7, selected), (None, None));
    }

    #[test]
    fn observed_idle_and_absent_process_remain_zero() {
        let selected = AdapterLuid { high: 0, low: 1 };
        let e = |percent| Engine {
            pid: 7, key: EngineKey { luid_high: 0, luid_low: 1, phys: 0, engine: 0 },
            kind: "3D".into(), percent,
        };
        assert_eq!(adapter_loads(None, 7, selected), (None, None));
        assert_eq!(adapter_loads(Some(&[e(0.0)]), 7, selected), (Some(0.0), Some(0.0)));
        assert_eq!(adapter_loads(Some(&[e(31.0)]), 8, selected), (Some(31.0), Some(0.0)));
        assert_eq!(adapter_loads(Some(&[e(31.0)]), 7, selected), (Some(31.0), Some(31.0)));
    }

    #[test]
    fn selected_host_adapter_does_not_take_another_adapters_engine() {
        let e = |adapter_low, percent| Engine {
            pid: 7,
            key: EngineKey { luid_high: 0, luid_low: adapter_low, phys: 0, engine: 0 },
            kind: "3D".into(),
            percent,
        };
        let selected = AdapterLuid { high: 0, low: 1 };
        let engines = [e(1, 31.0), e(2, 99.0)];
        assert_eq!(
            busiest_on_adapter(&engines, None, selected).0,
            31.0,
            "the host adapter must not inherit a busier engine from another GPU",
        );
    }

    #[test]
    fn the_cards_load_is_its_busiest_kind_of_engine() {
        let e = |pid, engine, kind: &str, percent| Engine {
            pid, key: EngineKey { luid_high: 0, luid_low: 1, phys: 0, engine },
            kind: kind.to_owned(), percent,
        };
        let engines = [
            e(1, 0, "3D", 30.0), e(2, 0, "3D", 25.0),
            e(1, 1, "Copy", 40.0), e(2, 2, "VideoEncode", 70.0),
            e(1, 0, "3D", 5.0),
        ];
        // Same physical 3D engine: 30 + 25 + 5 = 60.
        assert_eq!(busiest(&engines, None).0, 70.0);
        // Process 1 on the 3D engine: 30 + 5 = 35; Copy = 40.
        assert_eq!(busiest(&engines, Some(1)).0, 40.0);
        assert_eq!(busiest(&engines, Some(9)).0, 0.0);
        // Saturation is per physical engine, not per type.
        assert_eq!(busiest(&[e(1, 0, "3D", 80.0), e(2, 0, "3D", 60.0)], None).0, 100.0);
    }

    #[test]
    fn copy_engines_and_adapters_are_not_falsely_summed() {
        let e = |pid, adapter, engine, percent| Engine {
            pid, key: EngineKey { luid_high: 0, luid_low: adapter, phys: 0, engine },
            kind: "Copy".into(), percent,
        };
        let samples = [
            e(1, 1, 3, 75.0), e(2, 1, 3, 10.0), // same engine: 85
            e(1, 1, 10, 65.0), // different engine, same type
            e(3, 2, 3, 60.0), // different GPU, same engine number
        ];
        let (all, kinds) = busiest(&samples, None);
        assert_eq!(all, 85.0, "physical-engine load must not sum across engines");
        assert_eq!(kinds["Copy"], 85.0);
        assert_eq!(busiest(&samples, Some(1)).0, 75.0);
        assert_eq!(busiest(&samples, Some(2)).0, 10.0);
    }

    #[test]
    fn invalid_gpu_utilization_is_not_counted() {
        let e = |percent| Engine {
            pid: 1, key: EngineKey { luid_high: 0, luid_low: 1, phys: 0, engine: 0 },
            kind: "3D".into(), percent,
        };
        assert_eq!(busiest(&[e(f64::NAN), e(f64::INFINITY), e(-3.0)], None).0, 0.0);
        assert_eq!(busiest(&[e(70.0), e(f64::NAN)], None).0, 70.0);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "native Windows D3D12 adapter LUID + GPU Engine PDH snapshot"]
    fn physical_gpu_engine_pdh_snapshot() {
        use std::collections::HashSet;
        use windows::Win32::Graphics::{
            Direct3D::D3D_FEATURE_LEVEL_12_0,
            Direct3D12::{D3D12CreateDevice, ID3D12Device},
        };

        let mut device: Option<ID3D12Device> = None;
        // SAFETY: same default-adapter creation path used by the production host.
        unsafe {
            D3D12CreateDevice(None, D3D_FEATURE_LEVEL_12_0, &mut device)
                .expect("create native default D3D12 device");
        }
        let device = device.expect("D3D12CreateDevice returned no device");
        // SAFETY: read-only identity query on the device just created.
        let luid = unsafe { device.GetAdapterLuid() };
        let selected = AdapterLuid { high: luid.HighPart as u32, low: luid.LowPart };

        let mut pdh = Engines::open().expect("open native Windows GPU Engine counter");
        std::thread::sleep(Duration::from_millis(1100)); // second PDH rate sample
        let samples = pdh.look().expect("read formatted GPU Engine counter array");
        assert!(!samples.is_empty(), "Windows returned no usable GPU engine instances");

        let adapters: HashSet<_> = samples.iter()
            .map(|e| (e.key.luid_high, e.key.luid_low)).collect();
        let engine_keys: HashSet<_> = samples.iter().map(|e| e.key).collect();
        let selected_instances = samples.iter()
            .filter(|e| e.key.luid_high == selected.high && e.key.luid_low == selected.low)
            .count();
        assert!(
            selected_instances > 0,
            "PDH returned no GPU Engine instance for the D3D12 device LUID {:#010x}:{:#010x}",
            selected.high,
            selected.low,
        );

        let (global, _) = busiest(&samples, None);
        let (host, by_type) = busiest_on_adapter(&samples, None, selected);
        assert!(host.is_finite() && (0.0..=100.0).contains(&host));
        assert_eq!(adapter_loads(Some(&samples), std::process::id(), selected).0, Some(host));
        assert!(host <= global + f32::EPSILON, "host-adapter load cannot exceed the global busiest engine");
        println!(
            "R72_DXGI_PDH_LUID={:#010x}:{:#010x} PDH_INSTANCES={} SELECTED_INSTANCES={} ADAPTER_LUIDS={} PHYSICAL_ENGINES={} GLOBAL_BUSY={} HOST_BUSY={} HOST_ENGINE_TYPES={}",
            selected.high, selected.low, samples.len(), selected_instances, adapters.len(),
            engine_keys.len(), global, host, by_type.len(),
        );
    }

    /// The looks that need no card: they answer on any machine.
    #[test]
    fn the_processors_and_the_memory_answer() {
        let (idle, all) = system_times().expect("system times");
        assert!(all >= idle && all > 0);
        // Some work, so that this thread has time to show.
        let mut x = 0u64;
        for i in 0..20_000_000u64 {
            x = x.wrapping_mul(31).wrapping_add(i);
        }
        std::hint::black_box(x);
        assert!(process_time() > 0);
        let threads = thread_times();
        assert!(!threads.is_empty(), "this process has threads");
        let (ours, free) = memory();
        assert!(ours > 0 && free > 0);
    }
}
