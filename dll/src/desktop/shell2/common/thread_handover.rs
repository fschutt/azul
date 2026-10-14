//! A window that closes while others stay open hands the app's background threads to one of
//! them.
//!
//! An app's thread (a Send / Receive, a download, a save) lives in the window whose callback
//! started it, because a write-back needs a window to run in. When that window closed, its
//! threads were told to stop and the UI thread waited for them, up to two seconds. A mail app
//! whose main window closed while a compose window stayed open lost its running Send / Receive
//! that way, and a stopped thread that still held a lock kept the compose window's Send waiting
//! behind it. So every shell's close path hands the closing window's app threads to a window
//! that stays open, which polls them from then on. The threads of the closing window's own
//! nodes (a camera, a video decoder) still stop with it.

use azul_core::window::WindowType;

use super::event::PlatformWindow;

/// Whether a window of `window_type` may take over another window's threads. A menu or a
/// tooltip goes away again at once, so it never does.
#[must_use]
pub const fn can_inherit_threads(window_type: WindowType) -> bool {
    !matches!(window_type, WindowType::Menu | WindowType::Tooltip)
}

/// Moves the app threads of `closing` (a window that is going away) to `heir` and starts the
/// heir's thread poll. Returns false when `heir` may not take them ([`can_inherit_threads`]),
/// so the caller tries the next window; the threads then stay with `closing` and stop with it.
pub fn hand_over_threads<C: PlatformWindow, H: PlatformWindow>(
    closing: &mut C,
    heir: &mut H,
) -> bool {
    if !can_inherit_threads(heir.get_current_window_state().flags.window_type) {
        return false;
    }
    let Some(from) = closing.get_layout_window_mut() else {
        return true;
    };
    let threads = from.take_app_threads();
    if threads.is_empty() {
        return true;
    }
    let Some(to) = heir.get_layout_window_mut() else {
        // No layout yet: nothing could run the write-backs there.
        from.threads.extend(threads);
        return false;
    };
    if to.adopt_threads(threads) {
        heir.start_thread_poll_timer();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_or_a_tooltip_never_inherits_threads_but_a_normal_window_or_a_dialog_does() {
        assert!(can_inherit_threads(WindowType::Normal));
        assert!(can_inherit_threads(WindowType::Dialog));
        assert!(!can_inherit_threads(WindowType::Menu));
        assert!(!can_inherit_threads(WindowType::Tooltip));
    }
}
