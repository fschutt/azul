//! Pointer events on the nodes of a `VirtualView`'s child document, driven
//! through the REAL dispatcher on a headless window (`mouse_move`,
//! `mouse_down`, `mouse_up`), as a map editor uses them: the view is the
//! map, its labels are nodes with their own button and hover callbacks, and
//! the host around the view pans the map on the pointer's moves.

use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use azul_core::{
    callbacks::{Update, VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, OptionDom},
    events::{EventFilter, HoverEventFilter},
    geom::{LogicalPosition, LogicalRect},
    refany::RefAny,
    styled_dom::StyledDom,
};

use super::run_e2e_test_keeping_runner;
use crate::callbacks::CallbackInfo;

/// What the callbacks saw, by name, in order. One test at a time takes it.
static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
static LOCK: Mutex<()> = Mutex::new(());
/// Re-renders of the view so far.
static RENDERS: AtomicUsize = AtomicUsize::new(0);

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
recorder!(label_mouse_down, "label MouseDown");
recorder!(label_left_down, "label LeftMouseDown");
recorder!(label_middle_down, "label MiddleMouseDown");
recorder!(label_left_up, "label LeftMouseUp");
recorder!(label_enter, "label MouseEnter");
recorder!(host_over, "host MouseOver");
recorder!(host_left_down, "host LeftMouseDown");

/// The host's move handler: every move re-renders the view (a pan).
extern "C" fn host_move(_data: RefAny, mut info: CallbackInfo) -> Update {
    seen("host MouseMove");
    info.trigger_virtual_view_rerender(azul_core::dom::DomId::ROOT_ID, VIEW);
    Update::DoNothing
}

/// The view's page: a label (a box with text) at 20,20 .. 120,50.
extern "C" fn map_page(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    RENDERS.fetch_add(1, Ordering::SeqCst);
    let page = LogicalRect::new(LogicalPosition::zero(), info.bounds.get_logical_size());
    let label = Dom::create_div()
        .with_css("position: absolute; left: 20px; top: 20px; width: 100px; height: 30px;")
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            RefAny::new(()),
            label_mouse_down as usize,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            RefAny::new(()),
            label_left_down as usize,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MiddleMouseDown),
            RefAny::new(()),
            label_middle_down as usize,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::LeftMouseUp),
            RefAny::new(()),
            label_left_up as usize,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseEnter),
            RefAny::new(()),
            label_enter as usize,
        )
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "Jessore",
        ));
    // Under the label, as in a map view: a ground reaching past the view
    // on every side, and the paper.
    let ground = Dom::create_div().with_css(
        "position: absolute; left: -256px; top: -256px; width: 812px; height: 712px; background: \
         #2f3136;",
    );
    let paper = Dom::create_div().with_css(
        "position: absolute; left: 10px; top: 10px; width: 280px; height: 180px; background: \
         white;",
    );
    VirtualViewReturn {
        dom: OptionDom::Some(
            Dom::create_body()
                .with_css("margin: 0; padding: 0; font-size: 16px; position: relative;")
                .with_child(ground)
                .with_child(paper)
                .with_child(label),
        ),
        materialized: page,
        virtual_rect: page,
    }
}

/// The view's node in the window's DOM: body (0) > div#host (1) > view (2).
const VIEW: azul_core::dom::NodeId = azul_core::dom::NodeId::new(2);

/// body > div#host (callbacks) > the `VirtualView`, 300 x 200 at the origin.
fn map_dom() -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_id("host".into())
            .with_css("width: 300px; height: 200px;")
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseOver),
                RefAny::new(()),
                host_over as usize,
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseMove),
                RefAny::new(()),
                host_move as usize,
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::LeftMouseDown),
                RefAny::new(()),
                host_left_down as usize,
            )
            .with_child(
                Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(map_page))
                    .with_css("width: 300px; height: 200px;"),
            ),
    );
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; height: 300px; }",
    );
    StyledDom::create(&mut dom, css)
}

/// Runs `steps` on the map and returns what the callbacks saw.
fn run(steps: serde_json::Value) -> Vec<&'static str> {
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).clear();
    RENDERS.store(0, Ordering::SeqCst);
    let test: super::E2eTest = serde_json::from_value(serde_json::json!({
        "name": "virtual_view_events",
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps
    }))
    .expect("scenario json");
    let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(map_dom()));
    assert_eq!(result.status, "pass", "{:#?}", result.steps);
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn count(seen: &[&str], name: &str) -> usize {
    seen.iter().filter(|s| **s == name).count()
}

/// A press on a node inside the view reaches that node's button callbacks,
/// as hover (`MouseEnter`) does.
#[test]
fn button_events_reach_the_nodes_inside_a_virtual_view() {
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let seen = run(serde_json::json!([
        { "op": "wait_frame" },
        { "op": "mouse_move", "x": 40.0, "y": 30.0 },
        { "op": "wait_frame" },
        { "op": "mouse_down", "x": 40.0, "y": 30.0, "button": "left" },
        { "op": "wait_frame" },
        { "op": "mouse_up", "x": 40.0, "y": 30.0, "button": "left" },
        { "op": "wait_frame" },
        { "op": "mouse_down", "x": 40.0, "y": 30.0, "button": "middle" },
        { "op": "wait_frame" },
        { "op": "mouse_up", "x": 40.0, "y": 30.0, "button": "middle" },
        { "op": "wait_frame" }
    ]));
    assert!(
        count(&seen, "label MouseEnter") >= 1,
        "premise: hover reaches the label: {seen:?}"
    );
    // The move onto the label enters seven nodes at once (the window's
    // body, the host, the view, the page's body, ground, paper, label): the
    // host hears of it once, by the MouseOver of the topmost one bubbling.
    assert_eq!(count(&seen, "host MouseOver"), 1, "{seen:?}");
    assert_eq!(count(&seen, "label LeftMouseDown"), 1, "{seen:?}");
    assert_eq!(
        count(&seen, "label MouseDown"),
        2,
        "left and middle: {seen:?}"
    );
    assert_eq!(count(&seen, "label MiddleMouseDown"), 1, "{seen:?}");
    assert_eq!(count(&seen, "label LeftMouseUp"), 1, "{seen:?}");
    // The press bubbles on to the host, after the label.
    let label_at = seen.iter().position(|s| *s == "label LeftMouseDown");
    let host_at = seen.iter().position(|s| *s == "host LeftMouseDown");
    assert!(
        label_at.is_some() && host_at.is_some() && label_at < host_at,
        "{seen:?}"
    );
}

/// A drag across the view, which re-renders on every move (a pan): each
/// move is told to the host once (`MouseMove`), and the re-render does not
/// make the pointer "enter" the nodes it has been over all along - a
/// `MouseOver` the host saw for every node of the view's hover chain after
/// every render was what a pan that read movement from `MouseOver` (with
/// the same incremental drag delta each time) jumped by. (The deltas
/// themselves come from the shells' gesture samples, which this runner does
/// not record.)
#[test]
fn a_move_that_re_renders_the_view_is_dispatched_once() {
    let _one = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut steps = vec![
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "mouse_move", "x": 40.0, "y": 30.0 }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "mouse_down", "x": 40.0, "y": 30.0, "button": "left" }),
        serde_json::json!({ "op": "wait_frame" }),
    ];
    for i in 1..=5 {
        steps.push(
            serde_json::json!({ "op": "mouse_move", "x": 40.0 + 10.0 * i as f32, "y": 30.0 }),
        );
        steps.push(serde_json::json!({ "op": "wait_frame" }));
        steps.push(serde_json::json!({ "op": "wait_frame" }));
    }
    steps.push(serde_json::json!({ "op": "mouse_up", "x": 90.0, "y": 30.0, "button": "left" }));
    steps.push(serde_json::json!({ "op": "wait_frame" }));
    let seen = run(serde_json::Value::Array(steps));
    assert!(
        RENDERS.load(Ordering::SeqCst) >= 5,
        "premise: the moves re-render the view"
    );
    assert_eq!(count(&seen, "host MouseMove"), 6, "one per move: {seen:?}");
    // Entering the label once, at the first move onto it; the re-renders
    // are no entries.
    assert_eq!(count(&seen, "label MouseEnter"), 1, "{seen:?}");
    assert_eq!(count(&seen, "host MouseOver"), 1, "{seen:?}");
}
