//! Windows global hotkeys: `RegisterHotKey` + `WM_HOTKEY`.
//!
//! # Which window, which thread
//!
//! `RegisterHotKey` posts `WM_HOTKEY` to the THREAD that registered it - to
//! the window named in the call, or, with a null `HWND`, to the thread queue
//! with no window at all. A null-window message is retrieved by the run
//! loop's thread-queue drain and then handed to `DispatchMessageW`, which
//! has nowhere to send it; so each backend gets a window of its own: a
//! MESSAGE-ONLY window (`HWND_MESSAGE` parent), created lazily on the
//! event-loop thread at the first grab. Its window procedure parks the press
//! in the backend's [`HotkeySink`]; the run loop's hotkey pump runs the
//! callback. Every grab happens on the event-loop thread (the manager syncs
//! there), which is the thread `WM_HOTKEY` must come back to.
//!
//! Message-only is right here, unlike for the tray: it receives no
//! broadcasts, and hotkeys are not broadcasts. It also never appears
//! anywhere.
//!
//! Since the message is posted to the loop's own queue, `WaitMessage` wakes
//! for it - no polling.
//!
//! # Ids
//!
//! Win32 hotkey ids must be `0x0000..=0xBFFF` and unique per window; the
//! manager's ids are not bounded, so the smallest free Win32 id is allocated
//! per grab and mapped both ways.
//!
//! `MOD_NOREPEAT` suppresses the auto-repeat of a held combination, so one
//! press is one callback. A combination another process holds fails with
//! `ERROR_HOTKEY_ALREADY_REGISTERED`: that is `TakenByAnotherApp`.
//!
//! user32 / kernel32 are loaded with `libloading` so this file needs no
//! `windows`-crate feature and no change to the shared Win32 dlopen table.

use std::{
    collections::BTreeMap,
    ffi::c_void,
    sync::{Arc, Mutex, OnceLock, PoisonError},
};

use azul_core::global_hotkey::{GlobalHotkey, GlobalHotkeyError, GlobalHotkeyId};
use azul_layout::managers::global_hotkey::{BackendGrant, GlobalHotkeyBackend, HotkeySink};

type Hwnd = *mut c_void;
type WndProc = unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize;

/// `WNDCLASSEXW`.
#[repr(C)]
struct WndClassExW {
    cb_size: u32,
    style: u32,
    lpfn_wnd_proc: Option<WndProc>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: *mut c_void,
    h_icon: *mut c_void,
    h_cursor: *mut c_void,
    hbr_background: *mut c_void,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: *mut c_void,
}

const WM_HOTKEY: u32 = 0x0312;
const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;
const MOD_NOREPEAT: u32 = 0x4000;
const ERROR_HOTKEY_ALREADY_REGISTERED: u32 = 1409;
const ERROR_CLASS_ALREADY_EXISTS: u32 = 1410;
/// The highest id an application may register (`0xC000..` is for DLLs).
const MAX_WIN32_ID: i32 = 0xBFFF;

/// What the capability probe and the backend call themselves.
pub(super) const NAME: &str = "Win32 RegisterHotKey (message-only window)";

struct User32 {
    register_hot_key: unsafe extern "system" fn(Hwnd, i32, u32, u32) -> i32,
    unregister_hot_key: unsafe extern "system" fn(Hwnd, i32) -> i32,
    register_class_ex_w: unsafe extern "system" fn(*const WndClassExW) -> u16,
    #[allow(clippy::type_complexity)]
    create_window_ex_w: unsafe extern "system" fn(
        u32,
        *const u16,
        *const u16,
        u32,
        i32,
        i32,
        i32,
        i32,
        Hwnd,
        *mut c_void,
        *mut c_void,
        *mut c_void,
    ) -> Hwnd,
    destroy_window: unsafe extern "system" fn(Hwnd) -> i32,
    def_window_proc_w: unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize,
    get_module_handle_w: unsafe extern "system" fn(*const u16) -> *mut c_void,
    get_last_error: unsafe extern "system" fn() -> u32,
}

fn user32() -> Option<&'static User32> {
    static USER32: OnceLock<Option<User32>> = OnceLock::new();
    USER32
        .get_or_init(|| unsafe {
            let user = libloading::Library::new("user32.dll").ok()?;
            let kernel = libloading::Library::new("kernel32.dll").ok()?;
            let fns = User32 {
                register_hot_key: *user
                    .get::<unsafe extern "system" fn(Hwnd, i32, u32, u32) -> i32>(
                        b"RegisterHotKey\0",
                    )
                    .ok()?,
                unregister_hot_key: *user
                    .get::<unsafe extern "system" fn(Hwnd, i32) -> i32>(b"UnregisterHotKey\0")
                    .ok()?,
                register_class_ex_w: *user
                    .get::<unsafe extern "system" fn(*const WndClassExW) -> u16>(
                        b"RegisterClassExW\0",
                    )
                    .ok()?,
                create_window_ex_w: *user
                    .get::<unsafe extern "system" fn(
                        u32,
                        *const u16,
                        *const u16,
                        u32,
                        i32,
                        i32,
                        i32,
                        i32,
                        Hwnd,
                        *mut c_void,
                        *mut c_void,
                        *mut c_void,
                    ) -> Hwnd>(b"CreateWindowExW\0")
                    .ok()?,
                destroy_window: *user
                    .get::<unsafe extern "system" fn(Hwnd) -> i32>(b"DestroyWindow\0")
                    .ok()?,
                def_window_proc_w: *user
                    .get::<unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize>(
                        b"DefWindowProcW\0",
                    )
                    .ok()?,
                get_module_handle_w: *kernel
                    .get::<unsafe extern "system" fn(*const u16) -> *mut c_void>(
                        b"GetModuleHandleW\0",
                    )
                    .ok()?,
                get_last_error: *kernel
                    .get::<unsafe extern "system" fn() -> u32>(b"GetLastError\0")
                    .ok()?,
            };
            // Both are loaded for the life of the process anyway; leaking the
            // handles keeps the fn pointers valid.
            std::mem::forget(user);
            std::mem::forget(kernel);
            Some(fns)
        })
        .as_ref()
}

/// What the window procedure needs to turn a `WM_HOTKEY` into a press: the
/// backend's sink and its Win32-id -> OS-id map. Shared (`Arc`) between the
/// backend and the per-window routing table below.
struct Routing {
    sink: HotkeySink,
    /// Win32 hotkey id -> the manager's OS id.
    by_win32_id: Mutex<BTreeMap<i32, u32>>,
}

/// The routing of every live message-only window, by `HWND`. A window
/// procedure gets no user pointer of its own without `SetWindowLongPtrW`
/// (which 32-bit user32 does not export), so it looks its backend up here.
/// Keyed per window, so two backends (two `App`s) never share a sink.
static ROUTES: Mutex<BTreeMap<usize, Arc<Routing>>> = Mutex::new(BTreeMap::new());

fn routes() -> std::sync::MutexGuard<'static, BTreeMap<usize, Arc<Routing>>> {
    ROUTES.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The message-only window's procedure. Runs inside the run loop's
/// `DispatchMessageW`, on the event-loop thread. Parks the press; never
/// unwinds.
unsafe extern "system" fn wnd_proc(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize {
    if msg == WM_HOTKEY {
        // The routing is cloned out and the table released before the sink
        // is touched: the two locks are never held together.
        let routing = routes().get(&(hwnd as usize)).cloned();
        if let Some(routing) = routing {
            let win32_id = wparam as i32;
            let id = routing
                .by_win32_id
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&win32_id)
                .copied();
            if let Some(id) = id {
                routing.sink.fired(GlobalHotkeyId { id });
            }
        }
        return 0;
    }
    match user32() {
        Some(u) => unsafe { (u.def_window_proc_w)(hwnd, msg, wparam, lparam) },
        None => 0,
    }
}

fn last_error(u: &User32) -> u32 {
    unsafe { (u.get_last_error)() }
}

/// The Win32 virtual-key code for `key`: the inverse of the table the
/// window's own `WM_KEYDOWN` uses. The OEM punctuation keys need the
/// layout's character to resolve and are not invertible - they come back
/// `None` (`KeyNotMappable`).
fn vk_of(key: azul_core::window::VirtualKeyCode) -> Option<u32> {
    (1..0xFF)
        .find(|vk| {
            crate::desktop::shell2::common::event::win32_vkey_to_virtual_key(*vk, None) == Some(key)
        })
        .and_then(|vk| u32::try_from(vk).ok())
}

fn win32_modifiers(hotkey: &GlobalHotkey) -> u32 {
    let m = hotkey.modifiers;
    let mut bits = MOD_NOREPEAT;
    if m.alt {
        bits |= MOD_ALT;
    }
    if m.ctrl {
        bits |= MOD_CONTROL;
    }
    if m.shift {
        bits |= MOD_SHIFT;
    }
    if m.meta {
        bits |= MOD_WIN;
    }
    bits
}

/// Is user32 loadable? No side effect beyond the (cached) dlopen.
pub(super) fn probe() -> Result<(), String> {
    if user32().is_some() {
        Ok(())
    } else {
        Err(String::from("user32.dll could not be loaded"))
    }
}

/// One app's Win32 hotkeys. `HWND` kept as `usize` so the backend is `Send`;
/// it is only used on the event-loop thread.
pub(super) struct Win32Backend {
    sink: HotkeySink,
    /// The message-only window and its routing, once the first grab made it.
    window: Option<(usize, Arc<Routing>)>,
    /// The manager's OS id -> Win32 hotkey id.
    by_os_id: BTreeMap<u32, i32>,
}

impl Win32Backend {
    pub(super) fn new(sink: HotkeySink) -> Self {
        Self {
            sink,
            window: None,
            by_os_id: BTreeMap::new(),
        }
    }

    /// Create the message-only window on first use, on the calling
    /// (event-loop) thread - the thread `WM_HOTKEY` will then be posted to.
    fn ensure_window(&mut self, u: &User32) -> Result<(Hwnd, Arc<Routing>), GlobalHotkeyError> {
        if let Some((hwnd, routing)) = self.window.as_ref() {
            return Ok((*hwnd as Hwnd, routing.clone()));
        }
        let class_name: Vec<u16> = "AzulGlobalHotkeyWindow\0".encode_utf16().collect();
        let hwnd = unsafe {
            let instance = (u.get_module_handle_w)(core::ptr::null());
            let class = WndClassExW {
                cb_size: core::mem::size_of::<WndClassExW>() as u32,
                style: 0,
                lpfn_wnd_proc: Some(wnd_proc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: instance,
                h_icon: core::ptr::null_mut(),
                h_cursor: core::ptr::null_mut(),
                hbr_background: core::ptr::null_mut(),
                lpsz_menu_name: core::ptr::null(),
                lpsz_class_name: class_name.as_ptr(),
                h_icon_sm: core::ptr::null_mut(),
            };
            if (u.register_class_ex_w)(&class) == 0 {
                let err = last_error(u);
                if err != ERROR_CLASS_ALREADY_EXISTS {
                    return Err(GlobalHotkeyError::Platform(
                        format!("RegisterClassExW failed (error {err})").into(),
                    ));
                }
            }
            // HWND_MESSAGE = (HWND)-3: a message-only window.
            let hwnd_message = (-3_isize) as Hwnd;
            (u.create_window_ex_w)(
                0,
                class_name.as_ptr(),
                class_name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                hwnd_message,
                core::ptr::null_mut(),
                instance,
                core::ptr::null_mut(),
            )
        };
        if hwnd.is_null() {
            return Err(GlobalHotkeyError::Platform(
                format!(
                    "CreateWindowExW(HWND_MESSAGE) failed (error {})",
                    last_error(u)
                )
                .into(),
            ));
        }
        let routing = Arc::new(Routing {
            sink: self.sink.clone(),
            by_win32_id: Mutex::new(BTreeMap::new()),
        });
        routes().insert(hwnd as usize, routing.clone());
        self.window = Some((hwnd as usize, routing.clone()));
        Ok((hwnd, routing))
    }
}

impl GlobalHotkeyBackend for Win32Backend {
    fn name(&self) -> &'static str {
        NAME
    }

    fn probe(&self) -> Result<(), String> {
        probe()
    }

    fn register(
        &mut self,
        os_id: GlobalHotkeyId,
        hotkey: &GlobalHotkey,
        _description: &str,
    ) -> Result<BackendGrant, GlobalHotkeyError> {
        let Some(u) = user32() else {
            return Err(GlobalHotkeyError::Unavailable(
                "user32.dll could not be loaded".into(),
            ));
        };
        let Some(vk) = vk_of(hotkey.key) else {
            return Err(GlobalHotkeyError::KeyNotMappable);
        };
        let (hwnd, routing) = self.ensure_window(u)?;

        // Reserve the smallest free Win32 id.
        let win32_id = {
            let mut by_win32_id = routing
                .by_win32_id
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let Some(free) = (1..=MAX_WIN32_ID).find(|w| !by_win32_id.contains_key(w)) else {
                return Err(GlobalHotkeyError::Platform(
                    "every Win32 hotkey id (0x0001..=0xBFFF) is in use".into(),
                ));
            };
            by_win32_id.insert(free, os_id.id);
            free
        };

        let ok = unsafe { (u.register_hot_key)(hwnd, win32_id, win32_modifiers(hotkey), vk) };
        if ok != 0 {
            self.by_os_id.insert(os_id.id, win32_id);
            return Ok(BackendGrant::Active);
        }
        let err = last_error(u);
        routing
            .by_win32_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&win32_id);
        if err == ERROR_HOTKEY_ALREADY_REGISTERED {
            Err(GlobalHotkeyError::TakenByAnotherApp)
        } else {
            Err(GlobalHotkeyError::Platform(
                format!("RegisterHotKey failed (error {err})").into(),
            ))
        }
    }

    fn unregister(&mut self, os_id: GlobalHotkeyId) {
        let Some(win32_id) = self.by_os_id.remove(&os_id.id) else {
            return;
        };
        let Some((hwnd, routing)) = self.window.as_ref() else {
            return;
        };
        routing
            .by_win32_id
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&win32_id);
        if let Some(u) = user32() {
            unsafe {
                let _ = (u.unregister_hot_key)(*hwnd as Hwnd, win32_id);
            }
        }
    }

    // `poll` / `needs_loop_polling`: the defaults. `WM_HOTKEY` wakes
    // `WaitMessage` and the run loop dispatches it to `wnd_proc`.
}

impl Drop for Win32Backend {
    /// Dropping the App (or replacing the backend) releases every grab and
    /// destroys the message-only window.
    fn drop(&mut self) {
        let Some((hwnd, _routing)) = self.window.take() else {
            return;
        };
        routes().remove(&hwnd);
        if let Some(u) = user32() {
            unsafe {
                for (_, win32_id) in core::mem::take(&mut self.by_os_id) {
                    let _ = (u.unregister_hot_key)(hwnd as Hwnd, win32_id);
                }
                let _ = (u.destroy_window)(hwnd as Hwnd);
            }
        }
    }
}
