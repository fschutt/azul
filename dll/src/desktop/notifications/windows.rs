//! Windows native notifications  -  a `Shell_NotifyIconW` balloon (`NIF_INFO`).
//!
//! # Why the balloon, not a WinRT toast
//!
//! A WinRT `ToastNotification` is only shown for an app whose
//! AppUserModelID the shell can resolve: a Start-menu shortcut carrying it
//! (what an installer creates), a registered COM activator, or an MSIX
//! package identity. An unpackaged binary - `target\release\AzWidgets.exe` -
//! has none of them, and `ToastNotifier::Show` then fails SILENTLY: no banner,
//! no error. The balloon needs no registration at all, and on Windows 10 and
//! 11 the shell renders it as a toast, attributed to the executable, that also
//! lands in the Action Center. (`scripts/MULTIMONITOR_AND_TRAY_RESEARCH`
//! reached the same verdict for the tray: ship `NIF_INFO`, keep an AUMID hook
//! for apps with an installer - still open.)
//!
//! What it cannot do, and `PlatformCapability::notifications()` says so:
//!
//! * **No buttons.** A balloon has none; a notification's actions are dropped here.
//! * **One at a time.** A balloon belongs to a notify icon, and an icon shows one. Posting a
//!   notification while another is up REPLACES it, and the replaced one reports `Dismissed`.
//!
//! # The notify icon
//!
//! A balloon hangs off a notification-area icon, and the tray has no Windows
//! backend yet, so this module adds its own - on a hidden TOP-LEVEL window,
//! not `HWND_MESSAGE` (message-only windows miss broadcasts; see
//! `tray/windows.rs`) - for as long as a balloon is up, and removes it as soon
//! as the balloon ends, so no stray icon is left behind.
//!
//! # Events
//!
//! With `NOTIFYICON_VERSION_4`, `LOWORD(lParam)` of the icon's callback
//! message is the event: `NIN_BALLOONUSERCLICK` (a click), `NIN_BALLOONTIMEOUT`
//! (timed out, or closed with its X), `NIN_BALLOONHIDE` (hidden by the
//! system), mapped by `wire::balloon_event`. They arrive in [`notify_wndproc`]
//! during the run loop's thread-queue drain; the notification pump right after
//! the drain delivers them.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock, PoisonError,
};

use azul_core::notification::{Notification, NotificationEvent, NotificationSound};
use azul_css::AzString;
use azul_layout::managers::notification::{queue_notification_event, wire};

use crate::desktop::shell2::windows::dlopen::{
    encode_wide, Win32Libraries, HICON, HINSTANCE, HWND, LPARAM, LRESULT, WNDCLASSW, WPARAM,
};

/// The icon's callback message (`WM_APP` range).
const WM_AZ_NOTIFY: u32 = 0x8000 + 0x0A11;
/// The icon's id. Kept small: v4 reports it in the 16-bit `HIWORD(lParam)`.
const ICON_UID: u32 = 0x0A11;

const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIM_SETVERSION: u32 = 4;
const NIF_MESSAGE: u32 = 0x01;
const NIF_ICON: u32 = 0x02;
const NIF_TIP: u32 = 0x04;
const NIF_INFO: u32 = 0x10;
const NOTIFYICON_VERSION_4: u32 = 4;
const NIIF_INFO: u32 = 0x01;
const NIIF_USER: u32 = 0x04;
const NIIF_NOSOUND: u32 = 0x10;
const NIIF_LARGE_ICON: u32 = 0x20;
const NIIF_RESPECT_QUIET_TIME: u32 = 0x80;

const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_OVERLAPPED: u32 = 0;
const IDI_APPLICATION: usize = 32512;
const IMAGE_ICON: u32 = 1;
const LR_DEFAULTSIZE: u32 = 0x0040;
const LR_LOADFROMFILE: u32 = 0x0010;

/// `NOTIFYICONDATAW` (Vista+ layout), declared here as `tray/windows.rs`
/// recommends: winapi's `shellapi` feature is not enabled for one struct.
/// 976 bytes on x64. `guidItem` is a GUID (alignment 4), hence `[u32; 4]`.
#[repr(C)]
struct NotifyIconDataW {
    cb_size: u32,
    h_wnd: HWND,
    u_id: u32,
    u_flags: u32,
    u_callback_message: u32,
    h_icon: HICON,
    sz_tip: [u16; 128],
    dw_state: u32,
    dw_state_mask: u32,
    sz_info: [u16; 256],
    /// `uTimeout` / `uVersion` union.
    u_timeout_or_version: u32,
    sz_info_title: [u16; 64],
    dw_info_flags: u32,
    guid_item: [u32; 4],
    h_balloon_icon: HICON,
}

type ShellNotifyIconW = unsafe extern "system" fn(u32, *mut NotifyIconDataW) -> i32;
type LoadIconW = unsafe extern "system" fn(HINSTANCE, *const u16) -> HICON;
/// Returns a `HANDLE`; with `IMAGE_ICON` that handle is an `HICON`.
type LoadImageW = unsafe extern "system" fn(HINSTANCE, *const u16, u32, i32, i32, u32) -> HICON;

/// The three entry points `Win32Libraries` does not load. Resolved here, not
/// added to its shell32 group, because that group is all-or-nothing and
/// guards drag-and-drop.
struct Shell {
    _shell32: libloading::Library,
    _user32: libloading::Library,
    notify: ShellNotifyIconW,
    load_icon: LoadIconW,
    load_image: LoadImageW,
}

fn shell() -> Option<&'static Shell> {
    static SHELL: OnceLock<Option<Shell>> = OnceLock::new();
    SHELL
        .get_or_init(|| unsafe {
            let shell32 = libloading::Library::new("shell32.dll").ok()?;
            let user32 = libloading::Library::new("user32.dll").ok()?;
            let notify: ShellNotifyIconW = *shell32.get::<ShellNotifyIconW>(b"Shell_NotifyIconW\0").ok()?;
            let load_icon: LoadIconW = *user32.get::<LoadIconW>(b"LoadIconW\0").ok()?;
            let load_image: LoadImageW = *user32.get::<LoadImageW>(b"LoadImageW\0").ok()?;
            Some(Shell {
                _shell32: shell32,
                _user32: user32,
                notify,
                load_icon,
                load_image,
            })
        })
        .as_ref()
}

/// The app id of the notification the balloon shows. Read by the window
/// procedure (which has no other state), cleared when the balloon ends.
static SHOWING: Mutex<Option<String>> = Mutex::new(None);
/// Set by the window procedure when the balloon ended; the next pump then
/// removes the notify icon (not from inside the procedure).
static ENDED: AtomicBool = AtomicBool::new(false);

/// The hidden window's procedure. Never panics - it runs under
/// `DispatchMessageW`, where an unwind would abort the process.
unsafe extern "system" fn notify_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_AZ_NOTIFY {
        let code = (lparam as usize & 0xFFFF) as u32;
        let mut showing = SHOWING.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(id) = showing.as_deref() {
            if let Some(event) = wire::balloon_event(id, code) {
                queue_notification_event(event);
                *showing = None;
                ENDED.store(true, Ordering::Release);
            }
        }
        return 0;
    }
    match Win32Libraries::shared() {
        Some(libs) => unsafe { (libs.user32.DefWindowProcW)(hwnd, msg, wparam, lparam) },
        None => 0,
    }
}

/// `s` in a fixed UTF-16 field, NUL-terminated and never split mid-character.
fn fixed<const N: usize>(s: &str) -> [u16; N] {
    let units = wire::utf16_truncated(s, N);
    let mut out = [0u16; N];
    out[..units.len()].copy_from_slice(&units);
    out
}

/// `(available, reason)` for `PlatformCapability::notifications()`.
pub(super) fn probe() -> (bool, String) {
    if Win32Libraries::shared().is_none() {
        return (false, "user32.dll / gdi32.dll could not be loaded".to_string());
    }
    if shell().is_none() {
        return (
            false,
            "shell32.dll / user32.dll lack Shell_NotifyIconW / LoadIconW / LoadImageW".to_string(),
        );
    }
    (
        true,
        "shown as a toast attributed to the executable; no buttons (a notification's actions are \
         dropped) and one notification at a time (a new one replaces the one showing)"
            .to_string(),
    )
}

pub(super) struct PlatformNotifier {
    hwnd: HWND,
    icon_added: bool,
    /// A `.ico` loaded for the balloon, destroyed when replaced.
    balloon_icon: HICON,
    warned_actions: bool,
}

impl PlatformNotifier {
    pub(super) fn new() -> Result<Self, String> {
        let libs = Win32Libraries::shared().ok_or("user32.dll / gdi32.dll could not be loaded")?;
        shell().ok_or("shell32.dll has no Shell_NotifyIconW")?;
        let hinstance: HINSTANCE = libs
            .kernel32
            .as_ref()
            .map(|k| unsafe { (k.GetModuleHandleW)(core::ptr::null()) })
            .unwrap_or(core::ptr::null_mut());
        let class_name = encode_wide("AzulNotificationWindow");
        let title = encode_wide("Azul notifications");
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(notify_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: core::ptr::null_mut(),
            hCursor: core::ptr::null_mut(),
            hbrBackground: core::ptr::null_mut(),
            lpszMenuName: core::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        // 0 when the class is already registered (a second service after a
        // shutdown); CreateWindowExW below is what tells.
        unsafe { (libs.user32.RegisterClassW)(&class) };
        // A real top-level window that is never shown - see the module docs.
        let hwnd = unsafe {
            (libs.user32.CreateWindowExW)(
                WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                hinstance,
                core::ptr::null_mut(),
            )
        };
        if hwnd.is_null() {
            return Err("CreateWindowExW failed for the hidden notification window".to_string());
        }
        crate::plog_info!("[notifications] Shell_NotifyIconW balloon backend ready");
        Ok(Self {
            hwnd,
            icon_added: false,
            balloon_icon: core::ptr::null_mut(),
            warned_actions: false,
        })
    }

    fn base_data(&self) -> NotifyIconDataW {
        let mut nid: NotifyIconDataW = unsafe { core::mem::zeroed() };
        nid.cb_size = core::mem::size_of::<NotifyIconDataW>() as u32;
        nid.h_wnd = self.hwnd;
        nid.u_id = ICON_UID;
        nid.u_callback_message = WM_AZ_NOTIFY;
        nid
    }

    /// The executable's own icon (resource 1, the id a Windows resource
    /// compiler gives the first icon), else the stock application icon.
    /// Shared icons from `LoadIconW` are never destroyed.
    fn app_icon(shell: &Shell) -> HICON {
        let libs = Win32Libraries::shared();
        let hinstance: HINSTANCE = libs
            .and_then(|l| l.kernel32.as_ref())
            .map(|k| unsafe { (k.GetModuleHandleW)(core::ptr::null()) })
            .unwrap_or(core::ptr::null_mut());
        let own = unsafe { (shell.load_icon)(hinstance, 1usize as *const u16) };
        if !own.is_null() {
            return own;
        }
        unsafe { (shell.load_icon)(core::ptr::null_mut(), IDI_APPLICATION as *const u16) }
    }

    fn release_balloon_icon(&mut self) {
        if self.balloon_icon.is_null() {
            return;
        }
        if let Some(libs) = Win32Libraries::shared() {
            unsafe { (libs.user32.DestroyIcon)(self.balloon_icon) };
        }
        self.balloon_icon = core::ptr::null_mut();
    }

    fn remove_icon(&mut self) {
        if self.icon_added {
            if let Some(shell) = shell() {
                let mut nid = self.base_data();
                unsafe { (shell.notify)(NIM_DELETE, &mut nid) };
            }
            self.icon_added = false;
        }
        self.release_balloon_icon();
    }

    pub(super) fn post(&mut self, notification: &Notification) -> Result<(), String> {
        let shell = shell().ok_or("shell32.dll has no Shell_NotifyIconW")?;
        let id = notification.id.as_str().to_string();

        // One balloon per icon: whatever is showing ends now.
        let previous = SHOWING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(id.clone());
        if let Some(previous) = previous {
            if previous != id {
                queue_notification_event(NotificationEvent::dismissed_because(
                    AzString::from(previous),
                    AzString::from_const_str(
                        "replaced by a newer notification (Windows shows one at a time)",
                    ),
                ));
            }
        }
        ENDED.store(false, Ordering::Release);

        if !self.icon_added {
            let mut add = self.base_data();
            add.u_flags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            add.h_icon = Self::app_icon(shell);
            add.sz_tip = fixed::<128>(notification.title.as_str());
            if unsafe { (shell.notify)(NIM_ADD, &mut add) } == 0 {
                *SHOWING.lock().unwrap_or_else(PoisonError::into_inner) = None;
                return Err(
                    "Shell_NotifyIconW(NIM_ADD) failed - is the shell's notification area \
                     (explorer.exe) running?"
                        .to_string(),
                );
            }
            // After EVERY add: without v4 there is no NIN_BALLOONUSERCLICK
            // in LOWORD(lParam) as `wire::balloon_event` reads it.
            add.u_timeout_or_version = NOTIFYICON_VERSION_4;
            unsafe { (shell.notify)(NIM_SETVERSION, &mut add) };
            self.icon_added = true;
        }

        let mut info = self.base_data();
        info.u_flags = NIF_INFO;
        info.sz_info_title = fixed::<64>(notification.title.as_str());
        // An EMPTY szInfo removes the balloon instead of showing one.
        let body = notification.body.as_str();
        info.sz_info = fixed::<256>(if body.is_empty() { " " } else { body });
        info.dw_info_flags = NIIF_INFO | NIIF_RESPECT_QUIET_TIME;
        if matches!(notification.sound, NotificationSound::Silent) {
            info.dw_info_flags |= NIIF_NOSOUND;
        }
        self.release_balloon_icon();
        if let Some(icon) = notification.icon.as_ref() {
            let path = icon.as_str();
            if path.to_ascii_lowercase().ends_with(".ico") {
                let wide = encode_wide(path);
                let loaded = unsafe {
                    (shell.load_image)(
                        core::ptr::null_mut(),
                        wide.as_ptr(),
                        IMAGE_ICON,
                        0,
                        0,
                        LR_LOADFROMFILE | LR_DEFAULTSIZE,
                    )
                };
                if loaded.is_null() {
                    crate::plog_warn!("[notifications] icon {path:?} could not be loaded");
                } else {
                    self.balloon_icon = loaded;
                    info.h_balloon_icon = self.balloon_icon;
                    info.dw_info_flags = (info.dw_info_flags & !NIIF_INFO) | NIIF_USER | NIIF_LARGE_ICON;
                }
            } else {
                crate::plog_warn!(
                    "[notifications] icon {path:?} not shown: a Windows balloon takes a .ico file"
                );
            }
        }
        if unsafe { (shell.notify)(NIM_MODIFY, &mut info) } == 0 {
            *SHOWING.lock().unwrap_or_else(PoisonError::into_inner) = None;
            self.remove_icon();
            return Err("Shell_NotifyIconW(NIM_MODIFY, NIF_INFO) failed".to_string());
        }
        if !notification.actions.as_ref().is_empty() && !self.warned_actions {
            self.warned_actions = true;
            crate::plog_warn!(
                "[notifications] Windows balloons have no buttons: the actions of {id:?} (and \
                 of any later notification) are not shown"
            );
        }
        Ok(())
    }

    pub(super) fn withdraw(&mut self, id: &str) {
        let showing_this = {
            let mut showing = SHOWING.lock().unwrap_or_else(PoisonError::into_inner);
            if showing.as_deref() == Some(id) {
                *showing = None;
                true
            } else {
                false
            }
        };
        if showing_this {
            // Deleting the icon takes its balloon with it. The NIN_BALLOONHIDE
            // that follows finds nothing showing and reports nothing.
            self.remove_icon();
        }
    }

    /// Remove the icon once the balloon it carried has ended.
    pub(super) fn pump(&mut self) {
        if ENDED.swap(false, Ordering::AcqRel) {
            self.remove_icon();
        }
    }

    /// Before exit: the shell keeps a dead process's icon until the mouse
    /// passes over it.
    pub(super) fn shutdown(&mut self) {
        *SHOWING.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.remove_icon();
        if let Some(libs) = Win32Libraries::shared() {
            unsafe { (libs.user32.DestroyWindow)(self.hwnd) };
        }
        self.hwnd = core::ptr::null_mut();
    }
}
