//! The operating system asks the app to quit: a logout, a restart, a shutdown.
//!
//! A quit is the title bar's close request sent to every window, so an app that holds unsaved
//! work asks its question ("Save changes?", with Save, Don't Save and Cancel) instead of losing
//! it. macOS asks through `applicationShouldTerminate:`. Windows sends every top-level window
//! `WM_QUERYENDSESSION` and then `WM_ENDSESSION`. On Linux a session ends by sending the
//! process `SIGTERM` (systemd, a session manager) or `SIGHUP` (the session's terminal went
//! away): the run loop turns either into the same close request for every window, and the
//! process ends once every window has closed. A window that vetoes keeps the app running; the
//! session manager then shows it as one that holds the logout, or ends it later anyway.

use azul_core::window::WindowType;

/// Whether a window of `window_type` is asked before the session ends. A menu or a tooltip
/// holds no work of its own and closes with its owner.
#[must_use]
pub const fn asks_before_session_end(window_type: WindowType) -> bool {
    !matches!(window_type, WindowType::Menu | WindowType::Tooltip)
}

#[cfg(unix)]
mod signals {
    use core::sync::atomic::{AtomicBool, Ordering};

    /// Raised by the signal handler, taken by the run loop. A signal handler may do nothing
    /// but this: it runs between any two instructions of the UI thread.
    static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

    extern "C" fn on_quit_signal(_signal: libc::c_int) {
        QUIT_REQUESTED.store(true, Ordering::SeqCst);
    }

    /// Turns `SIGTERM` and `SIGHUP` into a quit request for the run loop. A signal the app
    /// already handles itself (its disposition is not the default) is left alone. The signal
    /// also wakes the loop: it interrupts the `poll(2)` the loop parks in.
    pub fn install_quit_signal_handlers() {
        for signal in [libc::SIGTERM, libc::SIGHUP] {
            // SAFETY: plain sigaction calls with zeroed, then filled structs; the handler only
            // stores to an atomic, which is async-signal-safe.
            unsafe {
                let mut old: libc::sigaction = core::mem::zeroed();
                if libc::sigaction(signal, core::ptr::null(), &mut old) != 0
                    || old.sa_sigaction != libc::SIG_DFL
                {
                    continue;
                }
                let mut action: libc::sigaction = core::mem::zeroed();
                action.sa_sigaction = on_quit_signal as extern "C" fn(libc::c_int) as usize;
                libc::sigemptyset(&mut action.sa_mask);
                let _ = libc::sigaction(signal, &action, core::ptr::null_mut());
            }
        }
    }

    /// Whether a quit was requested since the last call.
    pub fn take_quit_request() -> bool {
        QUIT_REQUESTED.swap(false, Ordering::SeqCst)
    }

    /// Raises the request as the handler does (for tests).
    #[cfg(test)]
    pub fn request_quit() {
        on_quit_signal(libc::SIGTERM);
    }
}

#[cfg(unix)]
pub use signals::{install_quit_signal_handlers, take_quit_request};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_or_a_tooltip_is_not_asked_before_the_session_ends_but_every_other_window_is() {
        assert!(asks_before_session_end(WindowType::Normal));
        assert!(asks_before_session_end(WindowType::Dialog));
        assert!(!asks_before_session_end(WindowType::Menu));
        assert!(!asks_before_session_end(WindowType::Tooltip));
    }

    #[cfg(unix)]
    #[test]
    fn a_quit_signal_is_taken_once_by_the_run_loop() {
        let _ = take_quit_request();
        signals::request_quit();
        assert!(take_quit_request(), "the loop sees the request");
        assert!(!take_quit_request(), "and only once");
    }
}
