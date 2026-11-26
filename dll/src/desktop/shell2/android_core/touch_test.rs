//! The mouse a touch phase makes. No device.

use super::*;

/// A cancelled gesture releases the button over nothing: no node is clicked.
///
/// Mutation: `Cancel` treated as `Up` (the button comes up over the node under the finger: a click on
/// whatever the finger was on when the system took the gesture over).
#[test]
fn a_cancelled_gesture_is_not_a_click() {
    let cancel = mouse_effect(Phase::Cancel, false);
    assert_eq!(cancel.left_down, Some(false), "the button comes up");
    assert!(cancel.pointer_leaves, "the pointer leaves");
    assert_eq!(cancel.hit_test, HitTest::Nowhere, "over no node, so no node is clicked");
    let up = mouse_effect(Phase::Up, false);
    assert_eq!(up.left_down, Some(false));
    assert!(!up.pointer_leaves);
    assert_eq!(up.hit_test, HitTest::AtCursor, "a finger that lifts clicks the node under it");
}

#[test]
fn the_first_finger_presses_and_the_last_one_releases() {
    assert_eq!(mouse_effect(Phase::Down, false).left_down, Some(true));
    assert_eq!(mouse_effect(Phase::Up, true).left_down, Some(false));
}

/// A second finger changes nothing for the mouse, and lifting a secondary finger is nothing to it;
/// the PRIMARY lifting while others stay is a release.
#[test]
fn only_the_primary_finger_presses_and_releases_the_mouse() {
    assert_eq!(mouse_effect(Phase::PointerDown, false).left_down, None);
    assert_eq!(mouse_effect(Phase::PointerUp, false).left_down, None);
    assert_eq!(mouse_effect(Phase::PointerUp, true).left_down, Some(false));
    assert_eq!(mouse_effect(Phase::Move, false).left_down, None);
    assert_eq!(mouse_effect(Phase::HoverMove, false).left_down, None);
}

/// Only a cancelled gesture makes the pointer leave.
#[test]
fn nothing_but_a_cancel_makes_the_pointer_leave() {
    for phase in [
        Phase::Down,
        Phase::PointerDown,
        Phase::Move,
        Phase::HoverMove,
        Phase::PointerUp,
        Phase::Up,
        Phase::Other,
    ] {
        assert!(!mouse_effect(phase, true).pointer_leaves, "{phase:?}");
        assert!(!mouse_effect(phase, false).pointer_leaves, "{phase:?}");
    }
}
