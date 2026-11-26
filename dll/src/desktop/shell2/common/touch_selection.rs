//! Which presses may start a text selection by dragging.
//!
//! The press edge of a pointer latches the ANCHOR of a text-selection drag (`LayoutWindow::text_selection_drag_anchor`)
//! when it lands on selectable text or on a selection handle, and every move after it extends a selection from there.
//! That is the desktop mouse: press on a paragraph, drag, the text under the drag is selected.
//!
//! A FINGER on a phone does the other thing with the same drag: it scrolls (the one-finger pan, `ScrollInputDevice::
//! Touchscreen`). The shells turn a touch into the mouse pipe's press and moves (and the pan), so a swipe over text armed
//! the selection too: at the top of a page, where there is nothing left to scroll, the swipe selected 'fired so far: 0)'
//! through 'TextArea' with two handles (QA run 45776829730). The conventions of both mobile systems are the same: a
//! selection by finger starts from a double tap, a long press, or a drag of a selection HANDLE; never from a swipe.
//!
//! A contact that is not a fingertip (a mouse reported through the touch stream, a stylus) is a pointer and keeps the
//! mouse's behaviour.

use azul_core::{
    geom::LogicalPosition,
    window::{TouchPoint, TouchToolType},
};

/// Whether a FINGER is on the glass: a fingertip, or a contact the platform did not name (touches arrive as `Unknown` on
/// iOS and X11). A mouse or a stylus in the touch stream, an eraser and a palm are not fingers; no contact at all is a
/// plain mouse.
#[must_use]
pub fn finger_is_down(contacts: &[TouchPoint]) -> bool {
    contacts
        .iter()
        .any(|c| matches!(c.tool_type, TouchToolType::Finger | TouchToolType::Unknown))
}

/// The anchor a press latches for a text-selection drag, `None` for no drag.
///
/// - a press on a selection HANDLE always arms it (the handle hangs below the line, often outside the text);
/// - a press on selectable text arms it for a mouse or a stylus, not for a finger (it scrolls).
#[must_use]
pub fn press_selection_anchor(
    press_on_text: bool,
    press_on_handle: bool,
    finger_down: bool,
    position: Option<LogicalPosition>,
) -> Option<LogicalPosition> {
    if press_on_handle || (press_on_text && !finger_down) {
        position
    } else {
        None
    }
}

#[cfg(test)]
#[path = "touch_selection_test.rs"]
mod tests;
