//! The release of a button that went down on a node reaches that node ONCE, through the real dispatcher on a headless
//! window: a node that holds the pointer (`CallbackInfo::capture_pointer`, W3C `setPointerCapture`) is told of the moves
//! and of the release wherever the mouse goes, and a node that does not is still given the release of the button
//! pressed on it.
//!
//! The two mechanisms used to stack: the press-target capture added a release for the pressed node, then the pointer
//! capture retargeted the real release AND the added one to the node that held the pointer - a widget that captured (a
//! slider's thumb, a colour plane, a sheet's grip) heard the release of every drag that ended off its box twice, in the
//! shells and in this runner alike. One body now (`LayoutWindow::apply_input_captures`), the explicit capture first.

use azul_core::{
    callbacks::Update,
    dom::{Dom, EventFilter, HoverEventFilter},
    refany::RefAny,
    styled_dom::StyledDom,
};

use super::{runner::run_e2e_test_with_dom, E2eTest, E2eTestResult};
use crate::callbacks::CallbackInfo;

/// What the thumb heard: the y of each move it was told of, and how many releases.
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

extern "C" fn on_move(mut data: RefAny, info: CallbackInfo) -> Update {
    if let (Some(mut heard), Some(at)) = (
        data.downcast_mut::<Heard>(),
        info.get_cursor_relative_to_viewport().into_option(),
    ) {
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

/// A 100 x 100 px `thumb` at the top left of a 400 x 400 px window; with `captures` it holds the pointer from its press.
fn page(heard: &RefAny, captures: bool) -> StyledDom {
    let mut thumb = Dom::create_div().with_css("width: 100px; height: 100px;").with_id("thumb".into());
    if captures {
        thumb = thumb.with_callback(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            RefAny::new(()),
            on_down as usize,
        );
    }
    let thumb = thumb
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseMove), heard.clone(), on_move as usize)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), heard.clone(), on_up as usize);
    let mut dom = Dom::create_body().with_child(thumb);
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; } body { width: 400px; height: 400px; }");
    StyledDom::create(&mut dom, css)
}

/// A press on the thumb, a drag far below it, the release there.
fn drag(captures: bool) -> (E2eTestResult, Vec<f32>, usize) {
    let mut heard = RefAny::new(Heard::default());
    let mv = |y: f32| serde_json::json!({ "op": "mouse_move", "x": 50.0, "y": y });
    let frame = || serde_json::json!({ "op": "wait_frame" });
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": "drag_off_the_thumb",
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 400, "dpi": 96 },
        "steps": [
            frame(),
            mv(50.0),
            frame(),
            { "op": "mouse_down", "x": 50.0, "y": 50.0 },
            frame(),
            mv(250.0),
            frame(),
            mv(350.0),
            frame(),
            { "op": "mouse_up", "x": 50.0, "y": 350.0 },
            frame(),
        ],
    }))
    .expect("the scenario literal is a valid E2eTest");
    let result = run_e2e_test_with_dom(&test, Some(page(&heard, captures)));
    let seen = heard.downcast_ref::<Heard>().map(|h| (h.moves.clone(), h.releases)).unwrap_or_default();
    (result, seen.0, seen.1)
}

/// The thumb holds the pointer: the moves below its box are its, and the release below it is told to it ONCE.
#[test]
fn a_node_that_holds_the_pointer_is_told_of_the_release_below_its_box_once() {
    let (result, moves, releases) = drag(true);
    assert!(result.status == "pass" && result.steps_failed == 0, "{result:?}");
    assert!(moves.iter().any(|y| (*y - 250.0).abs() < 1.0), "the move to 250 px reached it: {moves:?}");
    assert!(moves.iter().any(|y| (*y - 350.0).abs() < 1.0), "and the move to 350 px: {moves:?}");
    assert_eq!(releases, 1, "the release below its box, once");
}

/// No capture: only the moves over the thumb are its, and the button pressed on it is still given its release - once.
#[test]
fn without_a_capture_the_node_pressed_is_given_its_release_once() {
    let (result, moves, releases) = drag(false);
    assert!(result.status == "pass" && result.steps_failed == 0, "{result:?}");
    assert!(moves.iter().all(|y| *y < 101.0), "only moves over the box: {moves:?}");
    assert_eq!(releases, 1, "the press-target capture hands it the release the pointer made elsewhere");
}
