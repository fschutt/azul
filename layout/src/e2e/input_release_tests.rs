//! What a test of a scenario file leaves PRESSED is let go before the next
//! test of the file starts (`e2e_input_released`).
//!
//! A test ends at its first failed step. One that ends between its
//! `mouse_down` and its `mouse_up` left the button down in the window's
//! state, and the next test of the same file - the same window - began with
//! it: its `mouse_down` was no edge, no `MouseDown` was determined and no
//! handler ran. The map editor's scenarios reported a click that made no
//! street index block and a drag that moved nothing, each in the test AFTER
//! the one that had really failed.

use std::sync::Mutex;

use azul_core::{
    callbacks::Update,
    dom::Dom,
    events::{EventFilter, HoverEventFilter, WindowEventFilter},
    refany::RefAny,
    styled_dom::StyledDom,
    window::{VirtualKeyCode, VirtualKeyCodeVec},
};

use super::run_e2e_tests_in_one_window;
use crate::callbacks::CallbackInfo;

/// What the callbacks saw, by name, in order. One test at a time takes it.
static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
static LOCK: Mutex<()> = Mutex::new(());

fn seen(name: &'static str) {
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).push(name);
}

macro_rules! recorder {
    ($fn_name:ident, $label:literal) => {
        extern "C" fn $fn_name(_data: RefAny, _info: CallbackInfo) -> Update {
            seen($label);
            Update::DoNothing
        }
    };
}
recorder!(box_left_down, "box LeftMouseDown");
recorder!(box_left_up, "box LeftMouseUp");
recorder!(any_key_down, "window VirtualKeyDown");

/// body > div#box, 200 x 100 at the origin, with the handlers of a press, a
/// release and a key.
fn page() -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_id("box".into())
            .with_css("width: 200px; height: 100px;")
            .with_callback(
                EventFilter::Hover(HoverEventFilter::LeftMouseDown),
                RefAny::new(()),
                box_left_down as usize,
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::LeftMouseUp),
                RefAny::new(()),
                box_left_up as usize,
            )
            .with_callback(
                EventFilter::Window(WindowEventFilter::VirtualKeyDown),
                RefAny::new(()),
                any_key_down as usize,
            ),
    );
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { width: 400px; height: 300px; }",
    );
    StyledDom::create(&mut dom, css)
}

fn count(seen: &[&str], name: &str) -> usize {
    seen.iter().filter(|s| **s == name).count()
}

/// Two tests of one file. The first fails (a frame report can never say
/// `frames_since_reset == -1`) after its `mouse_down` and its `key_down`:
/// its `mouse_up` and `key_up` never run. The second presses the same button
/// and the same key: both are edges, both handlers run - and the release the
/// runner pushed before it was an up edge of the window's state machine (the
/// box heard `LeftMouseUp` for it), not a rewritten field.
///
/// Mutation: without the release at the start of a test the second test's
/// `mouse_down` and `key_down` change nothing in the window's state: one
/// `LeftMouseDown`, one `VirtualKeyDown`, and one `LeftMouseUp` instead of two.
#[test]
fn the_second_test_of_a_file_starts_with_nothing_held() {
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let tests: Vec<super::E2eTest> = serde_json::from_value(serde_json::json!([
        {
            "name": "fails_with_the_button_and_a_key_down",
            "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "mouse_move", "x": 40.0, "y": 30.0 },
                { "op": "wait_frame" },
                { "op": "mouse_down", "x": 40.0, "y": 30.0, "button": "left" },
                { "op": "wait_frame" },
                { "op": "key_down", "key": "a" },
                { "op": "wait_frame" },
                { "op": "get_frame_report" },
                { "op": "assert_response", "path": "frames_since_reset", "equals": -1 },
                { "op": "key_up", "key": "a" },
                { "op": "wait_frame" },
                { "op": "mouse_up", "x": 40.0, "y": 30.0, "button": "left" },
                { "op": "wait_frame" }
            ]
        },
        {
            "name": "presses_the_same_button_and_key",
            "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "mouse_down", "x": 40.0, "y": 30.0, "button": "left" },
                { "op": "wait_frame" },
                { "op": "mouse_up", "x": 40.0, "y": 30.0, "button": "left" },
                { "op": "wait_frame" },
                { "op": "key_down", "key": "a" },
                { "op": "wait_frame" },
                { "op": "key_up", "key": "a" },
                { "op": "wait_frame" }
            ]
        }
    ]))
    .expect("scenario json");
    let (results, runner) = run_e2e_tests_in_one_window(
        &tests,
        Some(page()),
        None,
        None,
        &crate::settings::LayoutSettings::default(),
    );
    let results = results.expect("the run answers");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].status, "fail", "premise: the first test fails: {:#?}", results[0].steps);
    assert_eq!(
        results[0].steps.len(),
        9,
        "premise: it ends at its failed step, before its key_up and mouse_up"
    );
    assert_eq!(results[1].status, "pass", "{:#?}", results[1].steps);
    let seen = SEEN.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(count(&seen, "box LeftMouseDown"), 2, "one press per test: {seen:?}");
    assert_eq!(count(&seen, "window VirtualKeyDown"), 2, "one key per test: {seen:?}");
    assert_eq!(
        count(&seen, "box LeftMouseUp"),
        2,
        "the runner's release between the tests, and the second test's own: {seen:?}"
    );
    // Nothing is held when the file is through.
    let state = &runner.window_state;
    assert!(!state.mouse_state.left_down);
    assert!(state.keyboard_state.pressed_virtual_keycodes.as_ref().is_empty());
}

/// The state the runner pushes: every button of every seat up, every key of
/// every seat up with the modifiers in step, and nothing to push when nothing
/// is held (no state change, no yield, at the start of the first test and of
/// every test that follows a tidy one).
#[test]
fn the_released_state_holds_nothing_and_is_none_when_nothing_is_held() {
    use crate::window_state::FullWindowState;
    let rest = FullWindowState::default();
    assert!(super::super::full::e2e_input_released(&rest).is_none());

    let mut held = FullWindowState::default();
    held.mouse_state.right_down = true;
    held.mouse_state.other_down = 0b10;
    held.pointer_seat_mut(7).middle_down = true;
    held.keyboard_state.pressed_virtual_keycodes =
        VirtualKeyCodeVec::from_vec(vec![VirtualKeyCode::LShift, VirtualKeyCode::A]);
    held.keyboard_state.current_virtual_keycode = Some(VirtualKeyCode::A).into();
    held.keyboard_state.sync_modifiers();
    held.keyboard_seat_mut(7).pressed_virtual_keycodes =
        VirtualKeyCodeVec::from_vec(vec![VirtualKeyCode::B]);
    assert!(held.keyboard_state.modifiers.shift, "premise: shift is held");

    let released = super::super::full::e2e_input_released(&held).expect("something is held");
    assert!(!released.mouse_state.right_down && released.mouse_state.other_down == 0);
    assert!(
        released.pointer_seat(7).is_some_and(|seat| !seat.middle_down),
        "a second seat's button too"
    );
    assert!(released.keyboard_state.pressed_virtual_keycodes.as_ref().is_empty());
    assert!(released.keyboard_state.current_virtual_keycode.is_none());
    assert!(!released.keyboard_state.modifiers.shift, "the modifiers follow the keys");
    assert!(released
        .keyboard_seat(7)
        .is_some_and(|seat| seat.pressed_virtual_keycodes.as_ref().is_empty()));
    // Where the pointer is, the size, the focus: as they were.
    assert_eq!(released.mouse_state.cursor_position, held.mouse_state.cursor_position);
    assert_eq!(released.size, held.size);
    assert!(super::super::full::e2e_input_released(&released).is_none(), "and once is enough");
}
