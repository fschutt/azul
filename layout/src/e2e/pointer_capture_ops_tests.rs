//! Pointer capture in the headless runner, through the real dispatcher: a node that calls `capture_pointer` in its press handler gets
//! the mouse's moves (`MouseMove`) and its release wherever the mouse goes, and the release ends the capture - what the shells do right after
//! the determination of a pass (`apply_pointer_capture`). The runner stored the capture and never applied it: a drag of a widget
//! that captures (a slider's thumb, a colour plane, a sheet's grip) stopped at the edge of its box in a scenario.

use azul_core::{
    callbacks::Update,
    dom::{Dom, EventFilter, HoverEventFilter},
    refany::RefAny,
    styled_dom::StyledDom,
};

use super::{runner::run_e2e_test_with_dom, E2eTest, E2eTestResult};
use crate::callbacks::CallbackInfo;

/// What the thumb heard: the y of each move it was told of, and the releases.
#[derive(Default)]
struct Heard {
    moves: Vec<f32>,
    releases: usize,
}

extern "C" fn on_down(_data: RefAny, mut info: CallbackInfo) -> Update {
    let thumb = info.get_hit_node();
    info.capture_pointer(thumb);
    Update::DoNothing
}

extern "C" fn on_over(mut data: RefAny, info: CallbackInfo) -> Update {
    if let (Some(mut heard), Some(at)) = (data.downcast_mut::<Heard>(), info.get_cursor_relative_to_viewport().into_option()) {
        heard.moves.push(at.y);
    }
    Update::DoNothing
}

extern "C" fn on_up(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut heard) = data.downcast_mut::<Heard>() {
        heard.releases += 1;
    }
    Update::DoNothing
}

/// A 100 x 100 px `thumb` at the top left of a 400 x 400 px window; it captures on a press, and listens to the moves and the release.
fn page(heard: &RefAny, captures: bool) -> StyledDom {
    let mut thumb = Dom::create_div().with_css("width: 100px; height: 100px;").with_id("thumb".into());
    if captures {
        thumb = thumb.with_callback(EventFilter::Hover(HoverEventFilter::MouseDown), RefAny::new(()), on_down as usize);
    }
    let thumb = thumb
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseMove), heard.clone(), on_over as usize)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), heard.clone(), on_up as usize);
    let mut dom = Dom::create_body().with_child(thumb);
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; } body { width: 400px; height: 400px; }");
    StyledDom::create(&mut dom, css)
}

fn run(captures: bool) -> (E2eTestResult, Vec<f32>, usize) {
    let mut heard = RefAny::new(Heard::default());
    let mv = |y: f32| serde_json::json!({ "op": "mouse_move", "x": 50.0, "y": y });
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": "capture",
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 400, "dpi": 96 },
        "steps": [
            { "op": "wait_frame" },
            mv(50.0),
            { "op": "wait_frame" },
            { "op": "mouse_down", "x": 50.0, "y": 50.0 },
            { "op": "wait_frame" },
            mv(250.0),
            { "op": "wait_frame" },
            mv(350.0),
            { "op": "wait_frame" },
            { "op": "mouse_up", "x": 50.0, "y": 350.0 },
            { "op": "wait_frame" },
        ],
    }))
    .expect("the scenario literal is a valid E2eTest");
    let result = run_e2e_test_with_dom(&test, Some(page(&heard, captures)));
    let seen = heard.downcast_ref::<Heard>().map(|h| (h.moves.clone(), h.releases)).unwrap_or_default();
    (result, seen.0, seen.1)
}

#[test]
fn a_node_that_captures_the_pointer_hears_the_moves_and_the_release_outside_its_box() {
    let (result, moves, releases) = run(true);
    assert!(result.status == "pass" && result.steps_failed == 0, "{result:?}");
    assert!(moves.iter().any(|y| (*y - 250.0).abs() < 1.0), "the move to 250 px (outside the 100 px box) reached it: {moves:?}");
    assert!(moves.iter().any(|y| (*y - 350.0).abs() < 1.0), "and the move to 350 px: {moves:?}");
    assert_eq!(releases, 1, "the release outside its box reached it, once");
}

#[test]
fn without_the_capture_the_moves_outside_the_box_are_not_the_nodes() {
    let (result, moves, releases) = run(false);
    assert!(result.status == "pass" && result.steps_failed == 0, "{result:?}");
    assert!(moves.iter().all(|y| *y < 101.0), "only moves over the box: {moves:?}");
    // (the release still reaches the node a button was pressed on: the press-target capture, a mechanism of its own)
    assert_eq!(releases, 1);
}
