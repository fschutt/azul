//! The user's session as the platform reports it - the screen locked or
//! unlocked, the machine about to sleep or awake again - pushed into
//! `azul_layout::managers::device_events::push_session_change`, which every
//! window's next pass delivers as a `SessionChange` event at its root.
//!
//! - macOS: NSWorkspace's sleep, wake and session notifications, and the
//!   distributed `com.apple.screenIsLocked` / `com.apple.screenIsUnlocked`.
//! - Windows: `WM_POWERBROADCAST` and `WM_WTSSESSION_CHANGE` in the window
//!   procedure; a window asks for the latter once
//!   ([`windows::register_window`]).
//! - Linux: logind's `PrepareForSleep` and the session's `Lock` / `Unlock`
//!   signals on the system bus.
//!
//! Elsewhere nothing reports it, and no `SessionChange` arrives.

use azul_core::events::SessionChange;

/// Start the platform's observers, once per process.
pub fn ensure_started() {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    #[cfg(target_os = "macos")]
    apple::start();
    #[cfg(target_os = "linux")]
    linux::start();
}

/// Report `change` to every window.
fn report(change: SessionChange) {
    azul_layout::managers::device_events::push_session_change(change);
}

#[cfg(target_os = "macos")]
mod apple {
    use azul_core::events::SessionChange;
    use block2::RcBlock;
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    use objc2_foundation::NSString;

    pub(super) fn start() {
        // SAFETY: AppKit's notification names are process-lifetime constants,
        // and the observers below live as long as the process.
        unsafe {
            let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
            let workspace_center = workspace.notificationCenter();
            let center: *const AnyObject =
                (&*workspace_center as *const objc2_foundation::NSNotificationCenter).cast();
            observe(center, objc2_app_kit::NSWorkspaceWillSleepNotification, SessionChange::WillSleep);
            observe(center, objc2_app_kit::NSWorkspaceDidWakeNotification, SessionChange::Woke);
            // Fast user switching: the session is switched away from and back.
            observe(
                center,
                objc2_app_kit::NSWorkspaceSessionDidResignActiveNotification,
                SessionChange::Locked,
            );
            observe(
                center,
                objc2_app_kit::NSWorkspaceSessionDidBecomeActiveNotification,
                SessionChange::Unlocked,
            );
            let Some(distributed) = std::ffi::CString::new("NSDistributedNotificationCenter")
                .ok()
                .as_deref()
                .and_then(AnyClass::get)
            else {
                return;
            };
            let distributed: *mut AnyObject = msg_send![distributed, defaultCenter];
            if distributed.is_null() {
                return;
            }
            let locked = NSString::from_str("com.apple.screenIsLocked");
            let unlocked = NSString::from_str("com.apple.screenIsUnlocked");
            observe(distributed, &locked, SessionChange::Locked);
            observe(distributed, &unlocked, SessionChange::Unlocked);
        }
    }

    /// Report `change` on every `name` notification of `center`.
    ///
    /// # Safety
    /// `center` is a live notification center or null.
    unsafe fn observe(center: *const AnyObject, name: &NSString, change: SessionChange) {
        if center.is_null() {
            return;
        }
        let handler = RcBlock::new(move |_note: *mut AnyObject| super::report(change));
        let nil: *mut AnyObject = core::ptr::null_mut();
        let _: *mut AnyObject = unsafe {
            msg_send![
                center,
                addObserverForName: name,
                object: nil,
                queue: nil,
                usingBlock: &*handler
            ]
        };
        // The center keeps a copy of the block; this one stays for the process.
        core::mem::forget(handler);
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use azul_core::events::SessionChange;

    /// logind on the system bus.
    const LOGIND: &str = "org.freedesktop.login1";
    /// logind's manager: `PrepareForSleep`.
    const MANAGER: (&str, &str) = ("/org/freedesktop/login1", "org.freedesktop.login1.Manager");
    /// The caller's own session: `Lock`, `Unlock`.
    const SESSION: (&str, &str) = (
        "/org/freedesktop/login1/session/auto",
        "org.freedesktop.login1.Session",
    );

    pub(super) fn start() {
        // `PrepareForSleep(true)` before the machine sleeps, `(false)` after it woke.
        spawn(MANAGER, "PrepareForSleep", |message| {
            message.body().deserialize::<bool>().ok().map(|going| {
                if going {
                    SessionChange::WillSleep
                } else {
                    SessionChange::Woke
                }
            })
        });
        spawn(SESSION, "Lock", |_| Some(SessionChange::Locked));
        spawn(SESSION, "Unlock", |_| Some(SessionChange::Unlocked));
    }

    /// Watch `signal` of logind's object `(path, interface)` on a thread of
    /// its own (the iterator blocks), reporting what `change` reads off each.
    fn spawn(
        (path, interface): (&'static str, &'static str),
        signal: &'static str,
        change: fn(&zbus::Message) -> Option<SessionChange>,
    ) {
        let _ = std::thread::Builder::new()
            .name(format!("azul-session-{signal}"))
            .spawn(move || {
                let Ok(conn) = zbus::blocking::Connection::system() else {
                    return;
                };
                let Ok(proxy) = zbus::blocking::Proxy::new(&conn, LOGIND, path, interface) else {
                    return;
                };
                let Ok(signals) = proxy.receive_signal(signal) else {
                    return;
                };
                for message in signals {
                    if let Some(change) = change(&message) {
                        super::report(change);
                    }
                }
            });
    }
}

/// The window procedure's half (the messages arrive at each top-level window).
#[cfg(target_os = "windows")]
pub mod windows {
    use azul_core::events::SessionChange;

    /// `WM_POWERBROADCAST`.
    pub const WM_POWERBROADCAST: u32 = 0x0218;
    /// `WM_WTSSESSION_CHANGE`.
    pub const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
    const PBT_APMSUSPEND: usize = 0x0004;
    const PBT_APMRESUMESUSPEND: usize = 0x0007;
    const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
    const WTS_SESSION_LOCK: usize = 0x0007;
    const WTS_SESSION_UNLOCK: usize = 0x0008;
    /// `NOTIFY_FOR_THIS_SESSION`.
    const NOTIFY_FOR_THIS_SESSION: u32 = 0;

    /// Ask for `WM_WTSSESSION_CHANGE` at `hwnd` (`WTSRegisterSessionNotification`,
    /// loaded from wtsapi32 at run time).
    pub fn register_window(hwnd: isize) {
        type Register = unsafe extern "system" fn(isize, u32) -> i32;
        static WTSAPI: std::sync::OnceLock<Option<Register>> = std::sync::OnceLock::new();
        let register = WTSAPI.get_or_init(|| unsafe {
            let lib = libloading::Library::new("wtsapi32.dll").ok()?;
            let f: Register = *lib.get(b"WTSRegisterSessionNotification\0").ok()?;
            // The function stays valid while the library is loaded: for the process.
            core::mem::forget(lib);
            Some(f)
        });
        if let Some(register) = register {
            // SAFETY: a live window handle.
            let _ = unsafe { register(hwnd, NOTIFY_FOR_THIS_SESSION) };
        }
    }

    /// The session change a power or session message reports, if any.
    #[must_use]
    pub const fn change_of(msg: u32, wparam: usize) -> Option<SessionChange> {
        match (msg, wparam) {
            (WM_POWERBROADCAST, PBT_APMSUSPEND) => Some(SessionChange::WillSleep),
            (WM_POWERBROADCAST, PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC) => {
                Some(SessionChange::Woke)
            }
            (WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK) => Some(SessionChange::Locked),
            (WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK) => Some(SessionChange::Unlocked),
            _ => None,
        }
    }

    /// Report the change `msg` stands for.
    pub fn handle(msg: u32, wparam: usize) {
        if let Some(change) = change_of(msg, wparam) {
            super::report(change);
        }
    }

    #[cfg(test)]
    mod tests {
        use azul_core::events::SessionChange;

        use super::*;

        #[test]
        fn the_power_and_session_messages_name_their_change() {
            assert_eq!(change_of(WM_POWERBROADCAST, 0x4), Some(SessionChange::WillSleep));
            assert_eq!(change_of(WM_POWERBROADCAST, 0x12), Some(SessionChange::Woke));
            assert_eq!(change_of(WM_WTSSESSION_CHANGE, 0x7), Some(SessionChange::Locked));
            assert_eq!(change_of(WM_WTSSESSION_CHANGE, 0x8), Some(SessionChange::Unlocked));
            assert_eq!(change_of(WM_POWERBROADCAST, 0xA), None, "a power status change");
        }
    }
}
