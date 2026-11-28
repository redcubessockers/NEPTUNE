//! The title's life cycle. A console title is never told it is closing:
//! quitting it or launching a new game first suspends it (its suspend
//! routines run, and it has a second to save) and then ends the process.
//! Closing the title's window does the same here; a new launch then starts
//! `victory` again, which waits for this process to be gone.
#![allow(non_snake_case)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(clippy::too_many_arguments)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
use windows_sys::Win32::UI::WindowsAndMessaging::{self as wm, HMENU};
use windows_sys::core::{PCSTR, PCWSTR, w};

use crate::{HleModule, hle_export};

type Routine = extern "system" fn(quiesced: u8, context: *mut c_void);

/// Registration id, routine and context of each suspend routine.
static ROUTINES: Mutex<Vec<(usize, Routine, usize)>> = Mutex::new(Vec::new());

fn routines() -> MutexGuard<'static, Vec<(usize, Routine, usize)>> {
    ROUTINES.lock().unwrap_or_else(|e| e.into_inner())
}

/// How long the console lets suspend routines run before it ends the title.
const SUSPEND_DEADLINE: Duration = Duration::from_secs(1);

const ERROR_INVALID_PARAMETER: u32 = 87;

static PROCESS_FINISH: OnceLock<fn()> = OnceLock::new();

/// Installs the process-level finalizer owned by the executable layer.
/// The first registration wins so title code cannot replace it later.
pub fn set_process_finish(finish: fn()) -> bool {
    PROCESS_FINISH.set(finish).is_ok()
}

pub extern "system" fn RegisterAppStateChangeNotification(
    routine: Option<Routine>,
    context: *mut c_void,
    registration: *mut usize,
) -> u32 {
    let Some(routine) = routine else {
        return ERROR_INVALID_PARAMETER;
    };
    if registration.is_null() {
        return ERROR_INVALID_PARAMETER;
    }
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    routines().push((id, routine, context as usize));
    // SAFETY: checked non-null; the title passes room for its registration.
    unsafe { registration.write(id) };
    0
}

pub extern "system" fn UnregisterAppStateChangeNotification(registration: usize) {
    routines().retain(|&(id, ..)| id != registration);
}

/// The window procedure a title window had before ours.
const TITLE_PROC: PCWSTR = w!("victory.wndproc");

/// Original procedures of ANSI classes registered through the console API.
/// A class hook covers windows made through dynamically-resolved host calls,
/// not only calls through our CreateWindowEx import wrapper.
const CLASS_LIMIT: usize = 256;
static CLASS_PROCS: Mutex<Vec<(u16, isize)>> = Mutex::new(Vec::new());

/// ABI layout of Win32 WNDCLASSEXA. windows-sys gates its identical type
/// behind the unrelated GDI feature solely because HBRUSH appears in it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct WindowClassExA {
    cb_size: u32,
    style: u32,
    wnd_proc: wm::WNDPROC,
    cls_extra: i32,
    wnd_extra: i32,
    instance: HINSTANCE,
    icon: *mut c_void,
    cursor: *mut c_void,
    background: *mut c_void,
    menu_name: PCSTR,
    class_name: PCSTR,
    small_icon: *mut c_void,
}

#[link(name = "user32")]
unsafe extern "system" {
    #[link_name = "RegisterClassExA"]
    fn host_RegisterClassExA(class: *const WindowClassExA) -> u16;
}

pub extern "system" fn RegisterClassExA(class: *const WindowClassExA) -> u16 {
    if class.is_null() {
        // SAFETY: preserve the host API's validation of a null argument.
        return unsafe { host_RegisterClassExA(class) };
    }
    // SAFETY: the title passes a readable WNDCLASSEXA for this call.
    let Some(original) = (unsafe { (*class).wnd_proc }) else {
        // SAFETY: preserve the host API's validation of a missing procedure.
        return unsafe { host_RegisterClassExA(class) };
    };
    let mut classes = CLASS_PROCS.lock().unwrap_or_else(|e| e.into_inner());
    if classes.len() >= CLASS_LIMIT {
        tracing::warn!(limit = CLASS_LIMIT, "title window-class hook limit reached");
        // SAFETY: the title's original class description.
        return unsafe { host_RegisterClassExA(class) };
    }
    // SAFETY: WNDCLASSEXA is a plain Win32 description with no Rust-owned
    // fields. The host copies it during RegisterClassExA.
    let mut hooked = unsafe { class.read() };
    hooked.wnd_proc = Some(window_proc_a);
    // SAFETY: a local class description whose pointers are the title's.
    let atom = unsafe { host_RegisterClassExA(&hooked) };
    if atom != 0 {
        let original = original as *const () as isize;
        if let Some(entry) = classes.iter_mut().find(|entry| entry.0 == atom) {
            entry.1 = original;
        } else {
            classes.push((atom, original));
        }
        tracing::info!(atom, "title window class hooked");
    }
    atom
}

pub extern "system" fn CreateWindowExW(
    ex_style: u32,
    class: PCWSTR,
    name: PCWSTR,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    parent: HWND,
    menu: HMENU,
    instance: HINSTANCE,
    param: *const c_void,
) -> HWND {
    let (x, y, width, height) = fit(parent, x, y, width, height);
    // SAFETY: the title's own arguments, forwarded (the size fitted).
    let hwnd = unsafe {
        wm::CreateWindowExW(
            ex_style, class, name, style, x, y, width, height, parent, menu, instance, param,
        )
    };
    hook(hwnd, false);
    hwnd
}

/// See [`CreateWindowExW`].
pub extern "system" fn CreateWindowExA(
    ex_style: u32,
    class: PCSTR,
    name: PCSTR,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    parent: HWND,
    menu: HMENU,
    instance: HINSTANCE,
    param: *const c_void,
) -> HWND {
    let (x, y, width, height) = fit(parent, x, y, width, height);
    // SAFETY: the title's own arguments, forwarded (the size fitted).
    let hwnd = unsafe {
        wm::CreateWindowExA(
            ex_style, class, name, style, x, y, width, height, parent, menu, instance, param,
        )
    };
    hook(hwnd, true);
    hwnd
}

/// A console title sizes its window to its render target (
/// the title: 3840x2160 on a Series X), which can exceed the host's screen.
/// A top-level window larger than the work area is shrunk to fit, keeping
/// its proportions, and centred; what it shows is scaled to it (the swap
/// chain stretches).
fn fit(parent: HWND, x: i32, y: i32, width: i32, height: i32) -> (i32, i32, i32, i32) {
    let mut area = RECT::default();
    // SAFETY: `area` receives the primary monitor's work area.
    let known =
        unsafe { wm::SystemParametersInfoW(wm::SPI_GETWORKAREA, 0, (&raw mut area).cast(), 0) };
    let fitted = fit_into(&area, width, height);
    if !parent.is_null() || known == 0 || fitted == (width, height) {
        return (x, y, width, height);
    }
    tracing::info!(width, height, to = ?fitted, "title window fitted to the screen");
    let (w, h) = fitted;
    let left = area.left + (area.right - area.left - w) / 2;
    let top = area.top + (area.bottom - area.top - h) / 2;
    (left, top, w, h)
}

/// `width` x `height` scaled down to fit `area`, or unchanged if it fits
/// (or is `CW_USEDEFAULT`).
fn fit_into(area: &RECT, width: i32, height: i32) -> (i32, i32) {
    let (aw, ah) = (area.right - area.left, area.bottom - area.top);
    if width <= 0 || height <= 0 || aw <= 0 || ah <= 0 || (width <= aw && height <= ah) {
        return (width, height);
    }
    let scale = f64::min(
        f64::from(aw) / f64::from(width),
        f64::from(ah) / f64::from(height),
    );
    (
        (f64::from(width) * scale) as i32,
        (f64::from(height) * scale) as i32,
    )
}

/// Puts our procedure in front of the title's.
fn hook(hwnd: HWND, ansi: bool) {
    if hwnd.is_null() {
        return;
    }
    // SAFETY: a window this thread just created. The title's procedure is
    // stored before ours can run.
    unsafe {
        // A class WndProc installed by RegisterClassExA intercepts messages
        // already. GetWindowLongPtrW may return an ANSI/Unicode thunk token
        // (0xffff....), not the Rust function address. Subclassing it again
        // would save that token as TITLE_PROC and dispatch back into our own
        // window_proc_a indefinitely. Keep exactly one intercept layer.
        let atom = wm::GetClassLongPtrW(hwnd, wm::GCW_ATOM) as u16;
        if CLASS_PROCS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .any(|entry| entry.0 == atom)
        {
            return;
        }
        let title = wm::GetWindowLongPtrW(hwnd, wm::GWLP_WNDPROC);
        let ours = if ansi { window_proc_a } else { window_proc_w } as *const () as isize;
        if title == ours {
            return;
        }
        if wm::SetPropW(hwnd, TITLE_PROC, title as *mut c_void) == 0 {
            tracing::warn!(?hwnd, "could not retain the title window procedure");
            return;
        }
        if wm::SetWindowLongPtrW(hwnd, wm::GWLP_WNDPROC, ours) == 0 {
            wm::RemovePropW(hwnd, TITLE_PROC);
            tracing::warn!(?hwnd, "could not hook the title window procedure");
        }
    }
}

extern "system" fn window_proc_a(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    window_proc(hwnd, message, wparam, lparam, true)
}

extern "system" fn window_proc_w(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    window_proc(hwnd, message, wparam, lparam, false)
}

fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    ansi: bool,
) -> LRESULT {
    // A user close (close button, Alt+F4, window or taskbar menu); a
    // programmatic close sends WM_CLOSE directly. A console title sees
    // neither kind of request.
    if close_requested(message, wparam) {
        end_title("window closed", Box::new(|| {}));
        return 0;
    }
    // Alt or F10 puts a window into menu mode: the system runs a modal menu
    // loop on the thread that owns the window, which is the title's. The
    // title then stops (no frames, no input) until a key ends the loop, and
    // nothing shows it, since the window has no menu bar;
    // the title aborts itself about half a minute later. A console has no window
    // menu. The mouse still opens it from the title bar.
    if message == wm::WM_SYSCOMMAND && wparam & 0xFFF0 == wm::SC_KEYMENU as usize {
        return 0;
    }
    // SAFETY: the procedure `hook` stored, called the way the system would.
    unsafe {
        let mut title = wm::GetPropW(hwnd, TITLE_PROC);
        if title.is_null() && ansi {
            let atom = wm::GetClassLongPtrW(hwnd, wm::GCW_ATOM) as u16;
            let address = CLASS_PROCS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .find_map(|&(registered, address)| (registered == atom).then_some(address))
                .unwrap_or(0);
            title = address as *mut c_void;
            if !title.is_null() {
                wm::SetPropW(hwnd, TITLE_PROC, title);
            }
        }
        if message == wm::WM_NCDESTROY {
            wm::RemovePropW(hwnd, TITLE_PROC);
        }
        if title.is_null() {
            return if ansi {
                wm::DefWindowProcA(hwnd, message, wparam, lparam)
            } else {
                wm::DefWindowProcW(hwnd, message, wparam, lparam)
            };
        }
        let title = std::mem::transmute::<*mut c_void, wm::WNDPROC>(title);
        if ansi {
            wm::CallWindowProcA(title, hwnd, message, wparam, lparam)
        } else {
            wm::CallWindowProcW(title, hwnd, message, wparam, lparam)
        }
    }
}

fn close_requested(message: u32, wparam: WPARAM) -> bool {
    message == wm::WM_CLOSE
        || (message == wm::WM_SYSCOMMAND && wparam & 0xFFF0 == wm::SC_CLOSE as usize)
}

/// Suspends the title as the console does when it ends it, runs `then`
/// (once), then ends the process. Returns at once; later calls do nothing.
pub(crate) fn end_title(why: &'static str, then: Box<dyn FnOnce() + Send>) {
    static ENDING: AtomicBool = AtomicBool::new(false);
    if ENDING.swap(true, Ordering::SeqCst) {
        return;
    }
    let suspend = routines().clone();
    tracing::info!(routines = suspend.len(), "{why}: suspending the title");
    // Whoever comes first (the routines or the deadline) runs `then`; the
    // other waits for it, so the process never ends halfway through.
    let then = std::sync::Arc::new(Mutex::new(Some(then)));
    let finish = move |how: &str| {
        let mut then = then.lock().unwrap_or_else(|e| e.into_inner());
        tracing::info!("{how}; ending the title");
        if let Some(then) = then.take() {
            then();
        }
        end();
    };
    let late = finish.clone();
    // Not on the calling thread: titles hand the suspend to their window
    // thread and wait for it inside the routine.
    std::thread::spawn(move || {
        for (_, routine, context) in suspend {
            routine(1, context as *mut c_void);
        }
        finish("title suspended");
    });
    std::thread::spawn(move || {
        std::thread::sleep(SUSPEND_DEADLINE);
        late("title did not finish suspending in time");
    });
}

/// Set in the environment of a relaunched `victory`: an inherited handle
/// of the process it replaces.
const RELAUNCHED_FROM: &str = "VICTORY_RELAUNCHED_FROM";

/// Starts `victory` again with `args` (after the title has suspended); the
/// new process waits until this one has ended.
pub(crate) fn relaunch(args: Vec<std::ffi::OsString>) {
    use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
    let mut me: HANDLE = std::ptr::null_mut();
    // SAFETY: an inheritable duplicate of our own process handle.
    let duplicated = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            GetCurrentProcess(),
            GetCurrentProcess(),
            &mut me,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } != 0;
    let spawned = std::env::current_exe().and_then(|exe| {
        let mut command = std::process::Command::new(exe);
        command.args(&args);
        if duplicated {
            command.env(RELAUNCHED_FROM, (me as usize).to_string());
        }
        command.spawn()
    });
    match spawned {
        Ok(child) => tracing::info!(pid = child.id(), ?args, "relaunched"),
        Err(e) => tracing::warn!(%e, "relaunch failed; the title just ends"),
    }
}

/// In a relaunched `victory`: waits (30 s at most) for the process it
/// replaces to be gone, so the two never share the title's files.
pub fn wait_for_previous() {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    let Some(handle) = std::env::var(RELAUNCHED_FROM)
        .ok()
        .and_then(|h| h.parse::<usize>().ok())
    else {
        return;
    };
    // SAFETY: removing our own variable before any other thread reads it.
    unsafe { std::env::remove_var(RELAUNCHED_FROM) };
    let handle = handle as HANDLE;
    // SAFETY: the inherited process handle the previous process gave us.
    unsafe {
        WaitForSingleObject(handle, 30_000);
        CloseHandle(handle);
    }
}

fn end() {
    // Compiled host pipelines kept for the next run.
    #[cfg(windows)]
    victory_gpu::pipeline::save_pipeline_cache();
    if let Some(finish) = PROCESS_FINISH.get() {
        finish();
    }
    // SAFETY: ends this process, as the console ends a suspended title.
    unsafe { TerminateProcess(GetCurrentProcess(), 0) };
}

pub(crate) fn module() -> HleModule {
    HleModule {
        name: "lifecycle",
        dlls: &[
            "api-ms-win-core-psm-appnotify-*",
            "ext-ms-win-rtcore-ntuser-window-*",
        ],
        exports: vec![
            hle_export!(RegisterAppStateChangeNotification),
            hle_export!(UnregisterAppStateChangeNotification),
            hle_export!(RegisterClassExA),
            hle_export!(CreateWindowExW),
            hle_export!(CreateWindowExA),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "system" fn noop(_: u8, _: *mut c_void) {}

    // R67: a title may subclass its ANSI HWND during WM_CREATE before
    // our CreateWindowExA import wrapper has returned. The class hook then
    // stops being the instance WndProc, so the import hook must detect it.
    static R67_SUBCLASS_SET: AtomicBool = AtomicBool::new(false);

    extern "system" fn r67_instance_proc(
        hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM,
    ) -> LRESULT {
        if message == wm::WM_SYSCOMMAND {
            COMMANDS.with(|c| c.borrow_mut().push(wparam & 0xFFF0));
            return 0;
        }
        unsafe { wm::DefWindowProcA(hwnd, message, wparam, lparam) }
    }

    extern "system" fn r67_class_proc(
        hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM,
    ) -> LRESULT {
        if message == wm::WM_CREATE {
            // SAFETY: HWND is being created on this test thread. A missing
            // subclass is recorded for an assertion *outside* the callback.
            let previous = unsafe {
                wm::SetWindowLongPtrW(
                    hwnd, wm::GWLP_WNDPROC, r67_instance_proc as *const () as isize,
                )
            };
            R67_SUBCLASS_SET.store(previous != 0, Ordering::Relaxed);
        }
        unsafe { wm::DefWindowProcA(hwnd, message, wparam, lparam) }
    }

    #[test]
    fn subclass_during_create_must_keep_lifecycle_interception() {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

        const CLASS: &[u8] = b"neptune.r67.subclass\\0";
        const NAME: &[u8] = b"r67\\0";
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class = WindowClassExA {
            cb_size: std::mem::size_of::<WindowClassExA>() as u32,
            style: 0,
            wnd_proc: Some(r67_class_proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: std::ptr::null_mut(),
            cursor: std::ptr::null_mut(),
            background: std::ptr::null_mut(),
            menu_name: std::ptr::null(),
            class_name: CLASS.as_ptr(),
            small_icon: std::ptr::null_mut(),
        };
        R67_SUBCLASS_SET.store(false, Ordering::Relaxed);
        assert_ne!(RegisterClassExA(&class), 0);
        let hwnd = CreateWindowExA(
            0, CLASS.as_ptr(), NAME.as_ptr(), 0, 0, 0, 10, 10,
            std::ptr::null_mut(), std::ptr::null_mut(), instance, std::ptr::null(),
        );
        assert!(!hwnd.is_null(), "registered ANSI window creation failed");
        assert!(R67_SUBCLASS_SET.load(Ordering::Relaxed),
            "WM_CREATE did not install the title's WndProc");
        COMMANDS.with(|c| c.borrow_mut().clear());
        unsafe {
            wm::SendMessageA(hwnd, wm::WM_SYSCOMMAND, wm::SC_KEYMENU as usize, 0);
        }
        COMMANDS.with(|c| assert!(c.borrow().is_empty(),
            "SC_KEYMENU leaked to a title subclass after class registration"));
        unsafe {
            wm::SendMessageA(hwnd, wm::WM_SYSCOMMAND, wm::SC_MINIMIZE as usize, 0);
        }
        COMMANDS.with(|c| assert_eq!(c.borrow().as_slice(), [wm::SC_MINIMIZE as usize]));
        unsafe {
            wm::DestroyWindow(hwnd);
            assert_ne!(wm::UnregisterClassA(CLASS.as_ptr(), instance), 0);
        }
    }

    #[test]
    fn registrations_come_and_go() {
        let (mut a, mut b) = (0, 0);
        assert_eq!(
            RegisterAppStateChangeNotification(Some(noop), std::ptr::null_mut(), &mut a),
            0
        );
        assert_eq!(
            RegisterAppStateChangeNotification(Some(noop), std::ptr::null_mut(), &mut b),
            0
        );
        UnregisterAppStateChangeNotification(a);
        let left: Vec<_> = routines().iter().map(|r| r.0).collect();
        assert_eq!(left, [b]);
    }

    #[test]
    fn oversized_windows_shrink_to_the_screen() {
        let area = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        };
        assert_eq!(fit_into(&area, 3840, 2160), (1848, 1040));
        assert_eq!(fit_into(&area, 1280, 720), (1280, 720));
        assert_eq!(
            fit_into(&area, wm::CW_USEDEFAULT, 0),
            (wm::CW_USEDEFAULT, 0)
        );
    }

    #[test]
    fn user_and_programmatic_close_requests_end_the_title() {
        assert!(close_requested(wm::WM_CLOSE, 0));
        assert!(close_requested(wm::WM_SYSCOMMAND, wm::SC_CLOSE as usize));
        assert!(close_requested(wm::WM_SYSCOMMAND, wm::SC_CLOSE as usize | 3));
        assert!(!close_requested(wm::WM_SYSCOMMAND, wm::SC_MINIMIZE as usize));
        assert!(!close_requested(wm::WM_DESTROY, 0));
    }

    #[test]
    fn process_finalizer_cannot_be_replaced() {
        assert!(set_process_finish(|| {}));
        assert!(!set_process_finish(|| {}));
    }

    #[test]
    fn hooked_windows_still_reach_the_title_procedure() {
        let null = std::ptr::null_mut();
        let hwnd = CreateWindowExW(
            0,
            w!("STATIC"),
            w!("before"),
            0,
            0,
            0,
            10,
            10,
            null,
            null,
            null,
            std::ptr::null(),
        );
        assert!(!hwnd.is_null());
        let mut text = [0u16; 16];
        // SAFETY: our window; the buffer holds 16 characters.
        unsafe {
            wm::SetWindowTextW(hwnd, w!("after"));
            let n = wm::GetWindowTextW(hwnd, text.as_mut_ptr(), 16);
            assert_eq!(String::from_utf16_lossy(&text[..n as usize]), "after");
            assert!(!wm::GetPropW(hwnd, TITLE_PROC).is_null());
            wm::DestroyWindow(hwnd);
        }
    }

    #[test]
    fn registered_class_hooks_windows_created_outside_import_wrapper() {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

        const CLASS: &[u8] = b"neptune.lifecycle.registered\0";
        const NAME: &[u8] = b"registered\0";
        // SAFETY: null asks for this executable's module handle.
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class = WindowClassExA {
            cb_size: size_of::<WindowClassExA>() as u32,
            style: 0,
            wnd_proc: Some(recording_a),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: std::ptr::null_mut(),
            cursor: std::ptr::null_mut(),
            background: std::ptr::null_mut(),
            menu_name: std::ptr::null(),
            class_name: CLASS.as_ptr(),
            small_icon: std::ptr::null_mut(),
        };
        let atom = RegisterClassExA(&class);
        assert_ne!(atom, 0);
        // Deliberately bypass our CreateWindowExA import wrapper. The class
        // procedure must keep lifecycle interception in front by itself.
        let hwnd = unsafe {
            wm::CreateWindowExA(
                0,
                CLASS.as_ptr(),
                NAME.as_ptr(),
                0,
                0,
                0,
                10,
                10,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());
        unsafe {
            assert!(!wm::GetPropW(hwnd, TITLE_PROC).is_null());
            COMMANDS.with(|commands| commands.borrow_mut().clear());
            wm::SendMessageA(hwnd, wm::WM_SYSCOMMAND, wm::SC_KEYMENU as usize, 0);
            COMMANDS.with(|commands| assert!(commands.borrow().is_empty()));
            wm::DestroyWindow(hwnd);
            wm::UnregisterClassA(CLASS.as_ptr(), instance);
        }
    }

    #[test]
    fn class_hook_and_create_import_do_not_chain_the_same_wndproc_twice() {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

        const CLASS: &[u8] = b"neptune.lifecycle.double.chain\0";
        const TITLE: &[u8] = b"chained\0";
        // SAFETY: null fetches the test executable's HINSTANCE.
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class = WindowClassExA {
            cb_size: std::mem::size_of::<WindowClassExA>() as u32,
            style: 0,
            wnd_proc: Some(recording_a),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: std::ptr::null_mut(),
            cursor: std::ptr::null_mut(),
            background: std::ptr::null_mut(),
            menu_name: std::ptr::null(),
            class_name: CLASS.as_ptr(),
            small_icon: std::ptr::null_mut(),
        };
        assert_ne!(RegisterClassExA(&class), 0);
        // Control: the same registered class must create through user32
        // before attributing any failure to our CreateWindowExA import.
        let native = unsafe {
            wm::CreateWindowExA(
                0, CLASS.as_ptr(), TITLE.as_ptr(), 0, 0, 0, 10, 10,
                std::ptr::null_mut(), std::ptr::null_mut(), instance,
                std::ptr::null(),
            )
        };
        assert!(
            !native.is_null(),
            "direct user32 CreateWindowExA failed, last_error={}",
            unsafe { windows_sys::Win32::Foundation::GetLastError() }
        );
        unsafe { wm::DestroyWindow(native); }
        // Both imports are deliberately routed through the HLE wrappers.
        // The original title procedure must remain intact as a property.
        let hwnd = CreateWindowExA(
            0, CLASS.as_ptr(), TITLE.as_ptr(), 0, 0, 0, 10, 10,
            std::ptr::null_mut(), std::ptr::null_mut(), instance,
            std::ptr::null(),
        );
        assert!(
            !hwnd.is_null(),
            "CreateWindowExA returned NULL, Win32 last_error={}",
            unsafe { windows_sys::Win32::Foundation::GetLastError() }
        );
        unsafe {
            let title = wm::GetPropW(hwnd, TITLE_PROC);
            assert_eq!(title, recording_a as *const () as *mut c_void,
                "double subclass replaced the original WndProc with a thunk");
            COMMANDS.with(|c| c.borrow_mut().clear());
            wm::SendMessageA(hwnd, wm::WM_SYSCOMMAND, wm::SC_MINIMIZE as usize, 0);
            COMMANDS.with(|c| assert_eq!(c.borrow().as_slice(), [wm::SC_MINIMIZE as usize]));
            wm::DestroyWindow(hwnd);
            assert_ne!(wm::UnregisterClassA(CLASS.as_ptr(), instance), 0);
        }
    }

    thread_local! {
        /// The system commands `recording` received, low four bits cleared.
        static COMMANDS: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// Stands in for a title's procedure: notes system commands and acts
    /// on none of them (no menu loop, no minimising).
    extern "system" fn recording(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == wm::WM_SYSCOMMAND {
            COMMANDS.with(|c| c.borrow_mut().push(wparam & 0xFFF0));
            return 0;
        }
        // SAFETY: the system's default procedure, called as the system would.
        unsafe { wm::DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    /// ANSI fixture: using DefWindowProcW for WM_NCCREATE of a class
    /// registered with RegisterClassExA can reject native window creation.
    /// Keep the reference procedure on the same A/W contract as the class.
    extern "system" fn recording_a(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == wm::WM_SYSCOMMAND {
            COMMANDS.with(|c| c.borrow_mut().push(wparam & 0xFFF0));
            return 0;
        }
        unsafe { wm::DefWindowProcA(hwnd, message, wparam, lparam) }
    }

    #[test]
    fn the_keyboard_menu_never_reaches_the_title() {
        let null = std::ptr::null_mut();
        let hwnd = CreateWindowExW(
            0,
            w!("STATIC"),
            w!("menu"),
            0,
            0,
            0,
            10,
            10,
            null,
            null,
            null,
            std::ptr::null(),
        );
        assert!(!hwnd.is_null());
        // SAFETY: our window, owned by this thread, so each message is
        // handled before SendMessageW returns. `recording` stands in for the
        // title's procedure and the original is put back before the window
        // is destroyed.
        unsafe {
            let title = wm::GetPropW(hwnd, TITLE_PROC);
            wm::SetPropW(hwnd, TITLE_PROC, recording as *const () as *mut c_void);
            let command = |c: u32, key: u8| {
                wm::SendMessageW(hwnd, wm::WM_SYSCOMMAND, c as usize, LPARAM::from(key))
            };
            command(wm::SC_KEYMENU, 0); // Alt or F10
            command(wm::SC_KEYMENU, b' '); // Alt+Space
            command(wm::SC_KEYMENU | 1, 0); // the low bits are the system's
            command(wm::SC_MINIMIZE, 0);
            wm::SetPropW(hwnd, TITLE_PROC, title);
            wm::DestroyWindow(hwnd);
        }
        let seen = COMMANDS.with(|c| c.borrow().clone());
        assert_eq!(seen, [wm::SC_MINIMIZE as usize]);
    }
}
