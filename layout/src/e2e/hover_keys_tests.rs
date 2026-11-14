//! Keys reach the node under the pointer, as `HoverEventFilter::VirtualKeyDown`
//! promises ("pressed while the element is hovered"), driven through the REAL
//! dispatcher on a headless window (`mouse_move`, `focus_node`, `key_down`).
//!
//! A key is aimed at the focused node, the root when nothing has the focus, and
//! the hover listeners of a key were planned along that path only: a map view,
//! which takes no focus, never heard its tool keys (maps4print's editor, the
//! keys G, R, S, ... over the map did nothing). The hovered node's path hears
//! them now - except while a field that edits text has the focus: typing is the
//! field's.

use std::sync::Mutex;

use azul_core::{
    callbacks::Update,
    dom::{Dom, TabIndex},
    events::{EventFilter, FocusEventFilter, HoverEventFilter},
    refany::RefAny,
    styled_dom::StyledDom,
};

use super::run_e2e_test_keeping_runner;
use crate::callbacks::CallbackInfo;

/// What the callbacks saw, by name, in order. One test at a time takes it.
static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
static LOCK: Mutex<()> = Mutex::new(());

macro_rules! recorder {
    ($fn_name:ident, $label:literal) => {
        extern "C" fn $fn_name(_data: RefAny, _info: CallbackInfo) -> Update {
            SEEN.lock().unwrap_or_else(|e| e.into_inner()).push($label);
            Update::DoNothing
        }
    };
}
recorder!(body_key, "body key");
recorder!(map_key, "map key");
recorder!(map_key_up, "map key up");
recorder!(label_key, "label key");
recorder!(field_key, "field key");
recorder!(button_key, "button key");

fn on(dom: Dom, filter: EventFilter, callback: extern "C" fn(RefAny, CallbackInfo) -> Update) -> Dom {
    dom.with_callback(filter, RefAny::new(()), callback as usize)
}

/// body (`Hover` key) >
///   #map 300 x 200 at the origin (`Hover` key down and up, takes no focus) >
///     #label 100 x 30 at 20,20 (`Hover` key down)
///   #field, an editable text 300 x 40 at y 200 (`Focus` key down)
///   #button, focusable 100 x 30 at y 240 (`Focus` key down)
fn page() -> StyledDom {
    let down = EventFilter::Hover(HoverEventFilter::VirtualKeyDown);
    let up = EventFilter::Hover(HoverEventFilter::VirtualKeyUp);
    let focus_down = EventFilter::Focus(FocusEventFilter::VirtualKeyDown);
    let label = on(Dom::create_div().with_id("label".into()), down, label_key);
    let map = on(
        on(Dom::create_div().with_id("map".into()), down, map_key),
        up,
        map_key_up,
    )
    .with_child(label);
    let field = on(
        Dom::create_div()
            .with_id("field".into())
            .with_contenteditable(true)
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("ab")),
        focus_down,
        field_key,
    );
    let button = on(
        Dom::create_div()
            .with_id("button".into())
            .with_tab_index(TabIndex::Auto),
        focus_down,
        button_key,
    );
    let mut dom = on(Dom::create_body(), down, body_key)
        .with_child(map)
        .with_child(field)
        .with_child(button);
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; height: 300px; } \
         #map { width: 300px; height: 200px; padding: 20px; box-sizing: border-box; } \
         #label { width: 100px; height: 30px; } \
         #field { width: 300px; height: 40px; } \
         #button { width: 100px; height: 30px; }",
    );
    StyledDom::create(&mut dom, css)
}

/// Runs `steps` on the page and returns what the callbacks saw.
fn run(steps: serde_json::Value) -> Vec<&'static str> {
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let test: super::E2eTest = serde_json::from_value(serde_json::json!({
        "name": "hover_keys",
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps
    }))
    .expect("scenario json");
    let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(page()));
    assert_eq!(result.status, "pass", "{:#?}", result.steps);
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn count(seen: &[&str], name: &str) -> usize {
    seen.iter().filter(|s| **s == name).count()
}

/// The repro: nothing focused, the pointer over the map's label, a key. It
/// bubbles from the node under the pointer up to the root, as a press there
/// does, and its release reaches the map too.
#[test]
fn a_key_reaches_the_hovered_node_when_nothing_is_focused() {
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let seen = run(serde_json::json!([
        { "op": "wait_frame" },
        { "op": "mouse_move", "x": 40.0, "y": 30.0 },
        { "op": "wait_frame" },
        { "op": "key_down", "key": "s" },
        { "op": "wait_frame" },
        { "op": "key_up", "key": "s" },
        { "op": "wait_frame" }
    ]));
    assert_eq!(
        seen,
        vec!["label key", "map key", "body key", "map key up"],
        "the hovered label first, then the map, then the root - each once"
    );
}

/// The pointer beside the map: the map hears nothing, the root (on every
/// path) still does.
#[test]
fn a_key_beside_the_map_does_not_reach_it() {
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let seen = run(serde_json::json!([
        { "op": "wait_frame" },
        { "op": "mouse_move", "x": 350.0, "y": 285.0 },
        { "op": "wait_frame" },
        { "op": "key_down", "key": "s" },
        { "op": "wait_frame" },
        { "op": "key_up", "key": "s" },
        { "op": "wait_frame" }
    ]));
    assert_eq!(seen, vec!["body key"]);
}

/// Typing into a focused field while the pointer rests on the map: the field
/// gets the key and the letter, the map neither - its tool keys would act on
/// every letter typed (and a map that prevented a key's default would eat it).
#[test]
fn typing_into_a_focused_field_does_not_reach_the_hovered_map() {
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let seen = run(serde_json::json!([
        { "op": "wait_frame" },
        { "op": "focus_node", "selector": "#field" },
        { "op": "wait_frame" },
        { "op": "mouse_move", "x": 40.0, "y": 30.0 },
        { "op": "wait_frame" },
        { "op": "key_down", "key": "x", "text": "X" },
        { "op": "key_up", "key": "x" },
        { "op": "wait_frame" },
        { "op": "assert_text", "selector": "#field", "expected": "abX" }
    ]));
    assert_eq!(count(&seen, "field key"), 1, "{seen:?}");
    assert_eq!(count(&seen, "label key"), 0, "{seen:?}");
    assert_eq!(count(&seen, "map key"), 0, "{seen:?}");
    assert_eq!(count(&seen, "map key up"), 0, "{seen:?}");
    // The root is an ancestor of the field: the key bubbles to it as before.
    assert_eq!(count(&seen, "body key"), 1, "{seen:?}");
}

/// A focused node that does not edit text (a button) keeps its key, and the
/// hovered map hears it too: the focused branch first, then the hovered one,
/// then the root they share - each listener once.
#[test]
fn a_key_reaches_a_focused_button_and_the_hovered_map() {
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let seen = run(serde_json::json!([
        { "op": "wait_frame" },
        { "op": "focus_node", "selector": "#button" },
        { "op": "wait_frame" },
        { "op": "mouse_move", "x": 40.0, "y": 30.0 },
        { "op": "wait_frame" },
        { "op": "key_down", "key": "s" },
        { "op": "wait_frame" },
        { "op": "key_up", "key": "s" },
        { "op": "wait_frame" }
    ]));
    assert_eq!(count(&seen, "button key"), 1, "{seen:?}");
    let hover: Vec<_> = seen
        .iter()
        .copied()
        .filter(|s| *s != "button key")
        .collect();
    assert_eq!(
        hover,
        vec!["label key", "map key", "body key", "map key up"],
        "{seen:?}"
    );
}
