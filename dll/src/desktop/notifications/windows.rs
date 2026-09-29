//! Windows native notifications  -  a WinRT toast, with a `Shell_NotifyIconW`
//! balloon (`NIF_INFO`) as the fallback.
//!
//! # The toast (the [`toast`] module)
//!
//! A WinRT `ToastNotification` is only shown for an app whose
//! AppUserModelID the shell can resolve; for an unpackaged binary -
//! `target\release\AzWidgets.exe` - without one, `ToastNotifier::Show` fails
//! SILENTLY. No installer is needed to have one: registering the AUMID under
//! `HKCU\Software\Classes\AppUserModelId\<aumid>` (a `DisplayName` is
//! enough) is what the Community Toolkit's `ToastNotificationManagerCompat`,
//! Firefox and the Windows App SDK do at runtime, and it needs no admin
//! rights. [`toast::Toaster::new`] writes that key the first time the service
//! starts, then creates the notifier with `CreateToastNotifierWithId`.
//!
//! What toasts add over the balloon: buttons (`<action>`s), several
//! notifications at once, entries that stay in the Action Center after the
//! banner times out, the app's payload in the activation arguments, and a
//! `Failed` event. Clicks are reported while the process runs
//! (`ToastNotification.Activated`, on a thread-pool thread, which queues the
//! event and posts a wake-up to the hidden window). A click on an Action
//! Center entry AFTER the app exited needs a COM activator
//! (`INotificationActivationCallback`) - not implemented; see the report.
//!
//! # The balloon (the fallback)
//!
//! Used when the toast cannot start: the registry write failed, the WinRT
//! toast factory is missing (Windows before 10), or the notifier cannot be
//! created. On Windows 10 and 11 the shell renders a balloon as a toast
//! attributed to the executable; on Windows 11 a timed-out one is not kept in
//! the Action Center.
//!
//! What the balloon cannot do, and `PlatformCapability::notifications()` says so:
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
/// Posted to the hidden window by the toast event handlers, which run on a
/// thread-pool thread: its only job is to wake the run loop's `WaitMessage`,
/// so the notification pump runs and delivers the event they queued.
const WM_AZ_TOAST_WAKE: u32 = 0x8000 + 0x0A12;
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
    if msg == WM_AZ_TOAST_WAKE {
        // Arriving was the point: the pump after the drain reads the mailbox.
        return 0;
    }
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

/// `(available, reason)` of the balloon.
fn balloon_probe() -> (bool, String) {
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

const BALLOON_BACKEND: &str = "Shell_NotifyIconW balloon (NIF_INFO)";

/// `(available, backend, reason)` for `PlatformCapability::notifications()`:
/// the toast where it can work, else the balloon, saying why.
pub(super) fn probe() -> (bool, String, String) {
    match toast::probe() {
        toast::ToastProbe::Usable { aumid, reason } => (
            true,
            format!("WinRT toast (AppUserModelID {aumid})"),
            reason,
        ),
        toast::ToastProbe::Disabled { aumid, reason } => (
            false,
            format!("WinRT toast (AppUserModelID {aumid})"),
            reason,
        ),
        toast::ToastProbe::Unavailable(why) => {
            let (available, reason) = balloon_probe();
            (
                available,
                BALLOON_BACKEND.to_string(),
                format!("{reason} (WinRT toasts are unavailable: {why})"),
            )
        }
    }
}

/// The notification "permission": Windows has no prompt, but the user (or a
/// policy) can turn an app's notifications off, which the toast notifier
/// reports.
pub(super) fn permission_state() -> azul_layout::managers::permission::PermissionState {
    use azul_layout::managers::permission::{PermissionQuality, PermissionState};
    match toast::probe() {
        toast::ToastProbe::Usable { .. } => PermissionState::Granted(PermissionQuality::Full),
        toast::ToastProbe::Disabled { .. } => PermissionState::Denied,
        toast::ToastProbe::Unavailable(_) => {
            if balloon_probe().0 {
                PermissionState::Granted(PermissionQuality::Full)
            } else {
                PermissionState::Restricted
            }
        }
    }
}

pub(super) struct PlatformNotifier {
    hwnd: HWND,
    icon_added: bool,
    /// A `.ico` loaded for the balloon, destroyed when replaced.
    balloon_icon: HICON,
    warned_actions: bool,
    /// The toast backend; `None` = the balloon is used.
    toaster: Option<toast::Toaster>,
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
        // The window stays either way: it is the balloon's notify-icon owner
        // and the toast handlers' wake-up target.
        let toaster = match toast::Toaster::new(hwnd as isize) {
            Ok(toaster) => {
                crate::plog_info!(
                    "[notifications] WinRT toast backend ready (AppUserModelID {})",
                    toaster.aumid()
                );
                Some(toaster)
            }
            Err(why) => {
                crate::plog_warn!(
                    "[notifications] WinRT toasts unavailable, using the Shell_NotifyIconW \
                     balloon instead: {why}"
                );
                None
            }
        };
        if toaster.is_none() {
            crate::plog_info!("[notifications] Shell_NotifyIconW balloon backend ready");
        }
        Ok(Self {
            hwnd,
            icon_added: false,
            balloon_icon: core::ptr::null_mut(),
            warned_actions: false,
            toaster,
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
        if let Some(toaster) = self.toaster.as_mut() {
            return toaster.post(notification);
        }
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
        if let Some(toaster) = self.toaster.as_mut() {
            toaster.withdraw(id);
            return;
        }
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

/// The WinRT toast backend: an AUMID registered under HKCU, a
/// `ToastNotifier` created for it, and the toast's own events.
pub(super) mod toast {
    use std::collections::BTreeMap;

    use azul_core::notification::{Notification, NotificationEvent};
    use azul_css::AzString;
    use azul_layout::managers::notification::{queue_notification_event, wire};
    use windows::{
        core::{IInspectable, Interface, HSTRING},
        Data::Xml::Dom::XmlDocument,
        Foundation::TypedEventHandler,
        UI::Notifications::{
            NotificationSetting, ToastActivatedEventArgs, ToastDismissedEventArgs,
            ToastFailedEventArgs, ToastNotification, ToastNotificationManager, ToastNotifier,
        },
    };

    use super::WM_AZ_TOAST_WAKE;
    use crate::desktop::shell2::windows::dlopen::{encode_wide, Win32Libraries, HWND};

    /// Every toast of this app is in one group, so a withdraw can name it in
    /// the Action Center history.
    const GROUP: &str = "azul";
    /// A toast `Tag` longer than this is refused; such an id is withdrawn
    /// through the live toast object only.
    const MAX_TAG: usize = 64;

    /// `(AUMID, display name)`, both from the app's one identity
    /// (`desktop::app_identity`): an unpackaged exe declares nothing, so the
    /// AUMID is the id derived from its name (`com.azul.azwidgets`, the
    /// bundle id the same app gets on macOS). The display name is what the
    /// toast and Settings > Notifications show.
    pub(super) fn app_identity() -> (String, String) {
        let app = crate::desktop::app_identity::current();
        (app.windows_aumid(), app.display_name())
    }

    type RegCreateKeyExW = unsafe extern "system" fn(
        isize,
        *const u16,
        u32,
        *mut u16,
        u32,
        u32,
        *const core::ffi::c_void,
        *mut isize,
        *mut u32,
    ) -> i32;
    type RegSetValueExW =
        unsafe extern "system" fn(isize, *const u16, u32, u32, *const u8, u32) -> i32;
    type RegCloseKey = unsafe extern "system" fn(isize) -> i32;

    /// Write `HKCU\Software\Classes\AppUserModelId\<aumid>` with its
    /// `DisplayName`: the registration an unpackaged app needs before its
    /// toasts are shown. Idempotent; no admin rights (HKCU).
    fn register_aumid(aumid: &str, display_name: &str) -> Result<(), String> {
        const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
        const KEY_WRITE: u32 = 0x0002_0006;
        const REG_SZ: u32 = 1;
        let key_path = wire::aumid_registry_key(aumid);
        unsafe {
            let lib = libloading::Library::new("advapi32.dll")
                .map_err(|e| format!("advapi32.dll could not be loaded: {e}"))?;
            let create: RegCreateKeyExW = *lib
                .get::<RegCreateKeyExW>(b"RegCreateKeyExW\0")
                .map_err(|e| format!("RegCreateKeyExW: {e}"))?;
            let set: RegSetValueExW = *lib
                .get::<RegSetValueExW>(b"RegSetValueExW\0")
                .map_err(|e| format!("RegSetValueExW: {e}"))?;
            let close: RegCloseKey = *lib
                .get::<RegCloseKey>(b"RegCloseKey\0")
                .map_err(|e| format!("RegCloseKey: {e}"))?;

            let path = encode_wide(&key_path);
            let mut key: isize = 0;
            let status = create(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                core::ptr::null_mut(),
                0,
                KEY_WRITE,
                core::ptr::null(),
                &mut key,
                core::ptr::null_mut(),
            );
            if status != 0 {
                return Err(format!(
                    "RegCreateKeyExW(HKCU\\{key_path}) failed with error {status}"
                ));
            }
            let name = encode_wide("DisplayName");
            let value = encode_wide(display_name);
            let bytes = u32::try_from(value.len() * 2).unwrap_or(u32::MAX);
            let status = set(
                key,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast::<u8>(),
                bytes,
            );
            close(key);
            if status != 0 {
                return Err(format!(
                    "RegSetValueExW(HKCU\\{key_path}\\DisplayName) failed with error {status}"
                ));
            }
        }
        Ok(())
    }

    /// Wake the run loop from a toast event handler (a thread-pool thread).
    fn wake(hwnd: isize) {
        if hwnd == 0 {
            return;
        }
        if let Some(libs) = Win32Libraries::shared() {
            unsafe { (libs.user32.PostMessageW)(hwnd as HWND, WM_AZ_TOAST_WAKE, 0, 0) };
        }
    }

    /// Why the notifier's setting keeps toasts from showing, or `None`.
    fn setting_refusal(setting: NotificationSetting) -> Option<String> {
        let why = match setting {
            NotificationSetting::Enabled => return None,
            NotificationSetting::DisabledForApplication => {
                "notifications are turned off for this app (Settings > System > Notifications)"
            }
            NotificationSetting::DisabledForUser => {
                "notifications are turned off for this user (Settings > System > Notifications)"
            }
            NotificationSetting::DisabledByGroupPolicy => {
                "notifications are turned off by a group policy"
            }
            _ => "notifications are disabled for this app (by its manifest)",
        };
        Some(why.to_string())
    }

    /// What [`probe`] found.
    pub(super) enum ToastProbe {
        /// Toasts can be shown. `reason` notes anything the app should know.
        Usable { aumid: String, reason: String },
        /// Toasts work, but the user or a policy turned them off - the
        /// balloon must NOT be used to get around that.
        Disabled { aumid: String, reason: String },
        /// No toasts here: the balloon is the backend.
        Unavailable(String),
    }

    /// Non-destructive: creates a notifier (no registry write, nothing
    /// shown) and reads its setting.
    pub(super) fn probe() -> ToastProbe {
        let (aumid, _) = app_identity();
        let notifier =
            match ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(aumid.as_str()))
            {
                Ok(n) => n,
                Err(e) => {
                    return ToastProbe::Unavailable(format!("CreateToastNotifierWithId: {e}"));
                }
            };
        match notifier.Setting() {
            Ok(setting) => match setting_refusal(setting) {
                None => ToastProbe::Usable {
                    aumid,
                    reason: String::new(),
                },
                Some(reason) => ToastProbe::Disabled { aumid, reason },
            },
            // Before the AUMID is registered (the first post does that) the
            // setting cannot be read yet.
            Err(_) => ToastProbe::Usable {
                aumid,
                reason: "the AppUserModelID is registered (HKCU) when the first notification is \
                         posted"
                    .to_string(),
            },
        }
    }

    pub(super) struct Toaster {
        notifier: ToastNotifier,
        aumid: String,
        /// The hidden window the event handlers post their wake-up to.
        wake_hwnd: isize,
        /// The toasts this process showed, by the app's id, for `Hide`.
        shown: BTreeMap<String, ToastNotification>,
    }

    impl Toaster {
        pub(super) fn new(wake_hwnd: isize) -> Result<Self, String> {
            let (aumid, display_name) = app_identity();
            register_aumid(&aumid, &display_name)?;
            let notifier =
                ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(aumid.as_str()))
                    .map_err(|e| format!("CreateToastNotifierWithId({aumid}): {e}"))?;
            Ok(Self {
                notifier,
                aumid,
                wake_hwnd,
                shown: BTreeMap::new(),
            })
        }

        pub(super) fn aumid(&self) -> &str {
            &self.aumid
        }

        pub(super) fn post(&mut self, notification: &Notification) -> Result<(), String> {
            if let Ok(setting) = self.notifier.Setting() {
                if let Some(why) = setting_refusal(setting) {
                    return Err(why);
                }
            }
            let id = notification.id.as_str().to_string();
            let xml = wire::toast_xml(notification);
            let toast = (|| -> windows::core::Result<ToastNotification> {
                let doc = XmlDocument::new()?;
                doc.LoadXml(&HSTRING::from(xml.as_str()))?;
                let toast = ToastNotification::CreateToastNotification(&doc)?;
                if id.len() <= MAX_TAG {
                    // Tag + group: the same id REPLACES its toast, also in
                    // the Action Center, and a withdraw can find it there.
                    toast.SetTag(&HSTRING::from(id.as_str()))?;
                    toast.SetGroup(&HSTRING::from(GROUP))?;
                }

                let wake_hwnd = self.wake_hwnd;
                toast.Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
                    move |_sender, args| {
                        if let Some(args) = args.as_ref() {
                            if let Ok(activated) = args.cast::<ToastActivatedEventArgs>() {
                                if let Ok(arguments) = activated.Arguments() {
                                    if let Some(event) =
                                        wire::toast_activated_event(&arguments.to_string_lossy())
                                    {
                                        queue_notification_event(event);
                                        wake(wake_hwnd);
                                    }
                                }
                            }
                        }
                        Ok(())
                    },
                ))?;

                let dismissed_id = id.clone();
                toast.Dismissed(&TypedEventHandler::<ToastNotification, ToastDismissedEventArgs>::new(
                    move |_sender, args| {
                        if let Some(args) = args.as_ref() {
                            if let Ok(reason) = args.Reason() {
                                if let Some(event) =
                                    wire::toast_dismissed_event(&dismissed_id, reason.0)
                                {
                                    queue_notification_event(event);
                                    wake(wake_hwnd);
                                }
                            }
                        }
                        Ok(())
                    },
                ))?;

                let failed_id = id.clone();
                toast.Failed(&TypedEventHandler::<ToastNotification, ToastFailedEventArgs>::new(
                    move |_sender, args| {
                        let code = args
                            .as_ref()
                            .and_then(|a| a.ErrorCode().ok())
                            .map_or(0, |hr| hr.0);
                        queue_notification_event(NotificationEvent::failed(
                            AzString::from(failed_id.clone()),
                            AzString::from(format!(
                                "Windows could not show the toast (HRESULT 0x{code:08X})"
                            )),
                        ));
                        wake(wake_hwnd);
                        Ok(())
                    },
                ))?;

                self.notifier.Show(&toast)?;
                Ok(toast)
            })()
            .map_err(|e| format!("the toast could not be shown: {e}"))?;
            if notification.icon.is_some() {
                // `<image placement="appLogoOverride">` takes the path as is.
                crate::plog_debug!("[notifications] toast {id:?} shows its icon as the app logo");
            }
            self.shown.insert(id, toast);
            Ok(())
        }

        /// Off the screen and out of the Action Center.
        pub(super) fn withdraw(&mut self, id: &str) {
            if let Some(toast) = self.shown.remove(id) {
                let _ = self.notifier.Hide(&toast);
            }
            if id.len() <= MAX_TAG {
                if let Ok(history) = ToastNotificationManager::History() {
                    let _ = history.RemoveGroupedTagWithId(
                        &HSTRING::from(id),
                        &HSTRING::from(GROUP),
                        &HSTRING::from(self.aumid.as_str()),
                    );
                }
            }
        }
    }
}
