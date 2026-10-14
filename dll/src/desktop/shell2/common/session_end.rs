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
