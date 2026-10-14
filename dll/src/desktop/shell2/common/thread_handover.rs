//! A window that closes while others stay open hands the app's background work to one of them.
//!
//! An app's thread (a Send / Receive, a download, a save) lives in the window whose callback
//! started it, because a write-back needs a window to run in. When that window closed, its
//! threads were told to stop and the UI thread waited for them, up to two seconds. A mail app
//! whose main window closed while a compose window stayed open lost its running Send / Receive
//! that way, and a stopped thread that still held a lock kept the compose window's Send waiting
//! behind it. So every shell's close path hands the closing window's app threads to a window
//! that stays open, which polls them from then on. The threads of the closing window's own
//! nodes (a camera, a video decoder) still stop with it.
//!
//! A keyring op the closing window sent out goes along too. Its answer comes back untagged, and
//! a window takes only as many answers as it has ops in flight, so without the hand-over the
//! answer would wait in the channel for a window that no longer exists.

use azul_core::window::WindowType;

use super::event::PlatformWindow;

/// Whether a window of `window_type` may take over another window's work. A menu or a tooltip
/// goes away again at once, so it never does.
#[must_use]
pub const fn can_inherit_work(window_type: WindowType) -> bool {
    !matches!(window_type, WindowType::Menu | WindowType::Tooltip)
}

/// Moves the app threads and the keyring ops in flight of `closing` (a window that is going
/// away) to `heir`, and starts the heir's polls for them. Returns false when `heir` may not take
/// them, so the caller tries the next window; the work then stays with `closing` and stops with
/// it.
pub fn hand_over_background_work<C: PlatformWindow, H: PlatformWindow>(
    closing: &mut C,
    heir: &mut H,
) -> bool {
    if !can_inherit_work(heir.get_current_window_state().flags.window_type) {
        return false;
    }
    let Some(from) = closing.get_layout_window_mut() else {
        return true;
    };
    let Some(to) = heir.get_layout_window_mut() else {
        // No layout yet: nothing could run the write-backs there.
        return false;
    };
    let threads = from.take_app_threads();
    let keyring_ops = core::mem::take(&mut from.keyring_manager.in_flight);
    to.keyring_manager.mark_requests_dispatched(keyring_ops);
    let has_threads = to.adopt_threads(threads);
    if has_threads {
        heir.start_thread_poll_timer();
    }
    if keyring_ops > 0 {
        heir.sync_capability_pump_timer();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_or_a_tooltip_never_inherits_work_but_a_normal_window_or_a_dialog_does() {
        assert!(can_inherit_work(WindowType::Normal));
        assert!(can_inherit_work(WindowType::Dialog));
        assert!(!can_inherit_work(WindowType::Menu));
        assert!(!can_inherit_work(WindowType::Tooltip));
    }
}
