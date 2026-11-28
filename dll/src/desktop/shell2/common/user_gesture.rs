//! What counts as a user gesture, for the one place that cares: a permission that may ask the
//! user (the location prompt a probe node's Subscribe starts) goes out only after one
//! (`PermissionManager::admit`), so nothing asks at launch.
//!
//! Read off the window state of two passes, so every backend gets it without its own wiring: a
//! mouse button that went up (a click is over), a finger that lifted, a key that went down.
//! Moving the mouse, scrolling, hovering, resizing, focusing or a timer are no gesture.

use azul_layout::window_state::FullWindowState;

/// Whether the user did something between `previous` and `current` that may start a request:
/// a click, a tap, a key press.
#[must_use]
pub fn user_gesture_between(previous: &FullWindowState, current: &FullWindowState) -> bool {
    let mouse = |state: &FullWindowState| {
        (
            state.mouse_state.left_down,
            state.mouse_state.right_down,
            state.mouse_state.middle_down,
        )
    };
    let (was_left, was_right, was_middle) = mouse(previous);
    let (is_left, is_right, is_middle) = mouse(current);
    let button_released =
        (was_left && !is_left) || (was_right && !is_right) || (was_middle && !is_middle);
    let finger_lifted = current.touch_state.num_touches < previous.touch_state.num_touches;
    let key_pressed = current.keyboard_state.pressed_virtual_keycodes.as_ref().len()
        > previous.keyboard_state.pressed_virtual_keycodes.as_ref().len();
    button_released || finger_lifted || key_pressed
}

#[cfg(test)]
#[path = "user_gesture_test.rs"]
mod user_gesture_test;
