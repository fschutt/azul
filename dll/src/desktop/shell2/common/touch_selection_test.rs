//! Which presses may start a text selection by dragging. No window.

use azul_core::{
    geom::LogicalPosition,
    window::{TouchPoint, TouchToolType, PRIMARY_POINTER_SEAT},
};

use super::*;

fn contact(tool_type: TouchToolType) -> TouchPoint {
    TouchPoint {
        seat_id: PRIMARY_POINTER_SEAT,
        id: 0,
        position: LogicalPosition::new(10.0, 20.0),
        force: 0.5,
        major: 0.0,
        minor: 0.0,
        orientation_rad: 0.0,
        tool_type,
    }
}

static AT: Option<LogicalPosition> = Some(LogicalPosition { x: 10.0, y: 20.0 });

/// A fingertip on the glass drags to SCROLL; it does not select text by dragging over it (QA run 45776829730: a
/// swipe at the top of a page, where there was nothing left to scroll, selected 'fired so far: 0)' through
/// 'TextArea' with two handles). A selection by finger starts from a double tap, a long press or a handle.
///
/// Mutation: the anchor latched for any press on selectable text, whatever pressed it (the state before).
#[test]
fn a_finger_that_presses_on_text_does_not_start_a_selection_drag() {
    assert_eq!(press_selection_anchor(true, false, true, AT), None);
    assert_eq!(press_selection_anchor(true, false, false, AT), AT, "a mouse press on text does");
}

/// A held selection HANDLE is dragged by a finger: that press does latch the anchor, with or without text under
/// it (the handle hangs below the line).
///
/// Mutation: the handle rule dropped together with the text rule.
#[test]
fn a_finger_that_presses_a_selection_handle_drags_it() {
    assert_eq!(press_selection_anchor(false, true, true, AT), AT);
    assert_eq!(press_selection_anchor(true, true, true, AT), AT);
    assert_eq!(press_selection_anchor(false, true, false, AT), AT);
}

/// A press on nothing selectable latches nothing, for a finger and a mouse alike.
#[test]
fn a_press_on_nothing_selectable_latches_nothing() {
    assert_eq!(press_selection_anchor(false, false, true, AT), None);
    assert_eq!(press_selection_anchor(false, false, false, AT), None);
}

/// Which contacts are a finger: a fingertip, and a contact the platform did not name (a phone's touches arrive as
/// `Unknown` on iOS and X11). A stylus and a mouse reported through the touch stream select by dragging like a
/// mouse, an eraser and a palm do nothing of the kind.
///
/// Mutation: a mouse contact counted as a finger (a mouse plugged into a phone could not select by dragging).
#[test]
fn a_fingertip_and_an_unnamed_contact_are_fingers_a_mouse_and_a_stylus_are_not() {
    assert!(finger_is_down(&[contact(TouchToolType::Finger)]));
    assert!(finger_is_down(&[contact(TouchToolType::Unknown)]));
    assert!(!finger_is_down(&[contact(TouchToolType::Mouse)]));
    assert!(!finger_is_down(&[contact(TouchToolType::Stylus)]));
    assert!(!finger_is_down(&[contact(TouchToolType::Eraser)]));
    assert!(!finger_is_down(&[contact(TouchToolType::Palm)]));
    assert!(!finger_is_down(&[]), "no contact at all is a mouse");
    // a finger among others still counts
    assert!(finger_is_down(&[contact(TouchToolType::Stylus), contact(TouchToolType::Finger)]));
}
