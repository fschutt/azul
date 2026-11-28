use azul_core::window::VirtualKeyCode;

use super::*;

fn state() -> FullWindowState {
    FullWindowState::default()
}

#[test]
fn a_click_that_ends_is_a_gesture_and_a_click_that_begins_is_not() {
    let up = state();
    let mut down = state();
    down.mouse_state.left_down = true;
    assert!(!user_gesture_between(&up, &down), "the press alone is not yet a click");
    assert!(user_gesture_between(&down, &up), "the release ends the click");
    let mut right = state();
    right.mouse_state.right_down = true;
    assert!(user_gesture_between(&right, &up));
    let mut middle = state();
    middle.mouse_state.middle_down = true;
    assert!(user_gesture_between(&middle, &up));
}

#[test]
fn nothing_that_is_not_the_user_acting_is_a_gesture() {
    let before = state();
    let mut after = state();
    after.window_focused = !before.window_focused;
    after.title = "another".into();
    assert!(!user_gesture_between(&before, &after), "focus and title are no gesture");
    assert!(!user_gesture_between(&before, &before.clone()), "an idle pass is none");
}

#[test]
fn a_key_that_goes_down_is_a_gesture_and_one_that_goes_up_is_not() {
    let none = state();
    let mut one = state();
    one.keyboard_state.pressed_virtual_keycodes = vec![VirtualKeyCode::A].into();
    assert!(user_gesture_between(&none, &one));
    assert!(!user_gesture_between(&one, &none));
    assert!(!user_gesture_between(&one, &one.clone()), "a held key is no new gesture");
}

#[test]
fn a_finger_that_lifts_is_a_gesture_and_one_that_lands_is_not() {
    let none = state();
    let mut touching = state();
    touching.touch_state.num_touches = 1;
    assert!(!user_gesture_between(&none, &touching));
    assert!(user_gesture_between(&touching, &none));
}
