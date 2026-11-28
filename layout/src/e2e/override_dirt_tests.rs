//! A callback that writes a LAYOUT property on the override channel (`CallbackInfo::override_css_property`: `height`
//! here, what a sheet that follows a finger writes on every move) is laid out by the pass the write asks for - on every
//! write, whatever input moved the node, through the real dispatcher on a headless window.
//!
//! The layout pass reads node data and interaction states, never overrides. The override channel leaves the inline style
//! alone, so the reconcile of the retained tree found every node as it was and kept its box, unless something ELSE
//! staged the node as css dirt in the same pass: a `:hover` / `:active` / `:focus` restyle of it, or a tween. The e2e
//! runner's touch ops (no pointer, no hover) showed it: the box did not follow the finger, while the write of the same
//! property through `set_css_property` did (the inline style is data the reconcile reads). The content chokepoint now
//! stages the diff of an override write that is laid out, as every restyle does.

use azul_core::{
    callbacks::Update,
    dom::{Dom, EventFilter, HoverEventFilter},
    refany::RefAny,
    styled_dom::StyledDom,
};
use azul_css::props::{layout::dimensions::LayoutHeight, property::CssProperty};

use super::{runner::run_e2e_test_with_dom, E2eTest, E2eTestResult};
use crate::callbacks::CallbackInfo;

/// How many moves the sheet was told of.
#[derive(Default)]
struct Moves {
    count: usize,
}

/// Where the finger is (the first touch point of the window state), or the pointer.
fn y_of_the_pointer(info: &CallbackInfo) -> Option<f32> {
    let state = info.get_current_window_state();
    if let Some(point) = state.touch_state.touch_points.as_ref().first() {
        return Some(point.position.y);
    }
    info.get_cursor_relative_to_viewport().into_option().map(|at| at.y)
}

/// A move of the pointer or the finger: the sheet is twice as tall as the pointer is far down, written on the override
/// channel only.
extern "C" fn grow(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut moves) = data.downcast_mut::<Moves>() else {
        return Update::DoNothing;
    };
    moves.count += 1;
    let Some(y) = y_of_the_pointer(&info) else {
        return Update::DoNothing;
    };
    let sheet = info.get_hit_node();
    info.override_css_property(sheet, CssProperty::height(LayoutHeight::px(2.0 * y)));
    Update::DoNothing
}

/// A 200 x 100 px `sheet` at the top left of a 400 x 600 px window, with no rule of any state: nothing but the write
/// itself can say it needs a layout.
fn page(moves: &RefAny, filter: HoverEventFilter) -> StyledDom {
    let sheet = Dom::create_div()
        .with_css("width: 200px; height: 100px;")
        .with_id("sheet".into())
        .with_callback(EventFilter::Hover(filter), moves.clone(), grow as usize);
    let mut dom = Dom::create_body().with_child(sheet);
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; } body { width: 400px; height: 600px; }");
    StyledDom::create(&mut dom, css)
}

fn height_is(px: f32) -> serde_json::Value {
    serde_json::json!({ "op": "assert_layout", "selector": "#sheet", "property": "height", "expected": px, "tolerance": 0.5 })
}

fn frame() -> serde_json::Value {
    serde_json::json!({ "op": "wait_frame" })
}

fn run(name: &str, filter: HoverEventFilter, steps: Vec<serde_json::Value>) -> (E2eTestResult, usize) {
    let mut moves = RefAny::new(Moves::default());
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 600, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    let result = run_e2e_test_with_dom(&test, Some(page(&moves, filter)));
    let count = moves.downcast_ref::<Moves>().map_or(0, |m| m.count);
    (result, count)
}

fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| format!("  step {} `{}`: {}", s.step_index, s.op, s.error.clone().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A finger that moves inside the sheet: every move is a write of `height`, and every write is laid out.
#[test]
fn a_touch_that_writes_a_height_on_every_move_is_laid_out_on_every_move() {
    let touch = |y: f32| serde_json::json!({ "op": "touch_move", "id": 0, "x": 50.0, "y": y });
    let (result, moves) = run(
        "touch_override_height",
        HoverEventFilter::TouchMove,
        vec![
            frame(),
            serde_json::json!({ "op": "touch_start", "id": 0, "x": 50.0, "y": 50.0 }),
            frame(),
            touch(55.0),
            frame(),
            height_is(110.0),
            touch(60.0),
            frame(),
            height_is(120.0),
            touch(65.0),
            frame(),
            height_is(130.0),
            serde_json::json!({ "op": "touch_end", "id": 0 }),
            frame(),
        ],
    );
    assert!(moves >= 3, "the sheet was told of the moves: {moves}");
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "the box follows the finger:\n{}",
        failures(&result)
    );
}

/// The same with a mouse: the pointer enters the sheet once, then moves inside it - the moves after the one that entered
/// have no hover change to ride on.
#[test]
fn a_mouse_that_writes_a_height_on_every_move_is_laid_out_on_every_move() {
    let mv = |y: f32| serde_json::json!({ "op": "mouse_move", "x": 50.0, "y": y });
    let (result, moves) = run(
        "mouse_override_height",
        HoverEventFilter::MouseMove,
        vec![
            frame(),
            mv(50.0),
            frame(),
            mv(55.0),
            frame(),
            height_is(110.0),
            mv(60.0),
            frame(),
            height_is(120.0),
            mv(65.0),
            frame(),
            height_is(130.0),
        ],
    );
    assert!(moves >= 3, "the sheet heard the moves: {moves}");
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "the box follows the pointer:\n{}",
        failures(&result)
    );
}
