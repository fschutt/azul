//! A window that closes while others stay open hands the app's background work to one of them.
//!
//! An app's thread (a Send / Receive, a download, a save) lives in the window whose callback
//! started it, because a write-back needs a window to run in. A window that stopped its
//! threads when it closed made the UI thread wait for them, up to two seconds, and a mail app
//! whose main window closed while a compose window stayed open lost its running Send / Receive.
//! So every shell's close path hands ALL the closing window's threads to a window that stays
//! open, which polls them from then on: the app's go on, the ones the app stopped still deliver
//! their last write-backs, and the workers of the closing window's own nodes (a camera, a video
//! decoder) are told to stop and retire there. Closing a window never waits for a worker.
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

/// Moves the threads and the keyring ops in flight of `closing` (a window that is going away)
/// to `heir`, and starts the heir's polls for them. Returns false when `heir` may not take them,
/// so the caller tries the next window; the work then stays with `closing` and stops with it.
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
    let threads = from.take_threads_for_heir();
    let keyring_ops = core::mem::take(&mut from.keyring_manager.in_flight);
    to.keyring_manager.mark_requests_dispatched(keyring_ops);
    if to.adopt_handed_over_threads(threads) {
        heir.sync_thread_poll_timer();
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
