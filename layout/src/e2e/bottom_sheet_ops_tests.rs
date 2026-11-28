//! The BottomSheet under a finger and under the keys, through the REAL dispatcher on a headless window: the touch ops put a
//! finger on the sheet (what a phone sends, and what `window_y` reads: the window's touch points, the mouse's cursor does not
//! move), the sheet's own callbacks decide, the app is told what it would be told on a phone, and the layout shows the height the
//! sheet rests at.
//!
//! The rules the scenarios stand on (`widgets/bottom_sheet.rs`, the pure part `widgets/detents.rs`): a press on the grip (the
//! 32 px strip at the sheet's top) is the sheet's, one in the content is not; the finger goes past an 8 px slop and the sheet
//! follows it, one step reported per move (`Dragged`); lifted, the sheet settles on the detent nearest to where a short fling
//! would land it (`Settled`); a tap on the grip steps to the next detent up. The height a drag writes is a runtime override that is
//! handed back at rest: the node's own style carries the settled height.
//!
//! The window is 400 x 800 px; the sheet floats in a box that spans it; its detents are 30 / 130 px, half = 400 px and full =
//! 85 % = 680 px, so at its peek the sheet's top edge is at y = 670 and the grip's middle at y = 686.

use azul_core::{callbacks::Update, dom::Dom, refany::RefAny, styled_dom::StyledDom};
use azul_css::AzString;

use super::{runner::run_e2e_test_with_dom, E2eTest, E2eTestResult};
use crate::{
    callbacks::CallbackInfo,
    widgets::bottom_sheet::{
        BottomSheet, BottomSheetEvent, BottomSheetEventKind, BottomSheetOnEventCallbackType, SheetDetent,
    },
};

/// Where the finger is across the window: the middle.
const X: f32 = 200.0;
/// The window's height, px (the container of the sheet).
const HEIGHT: f32 = 800.0;

/// What the app was told, in order.
#[derive(Default)]
struct Told {
    events: Vec<BottomSheetEvent>,
}

/// The app's callback, as an app writes it: it notes the event and does not rebuild.
extern "C" fn record(mut data: RefAny, _info: CallbackInfo, event: BottomSheetEvent) -> Update {
    if let Some(mut told) = data.downcast_mut::<Told>() {
        told.events.push(event);
    }
    Update::DoNothing
}

/// The window: the sheet at `detent`, in a positioned box that spans it, telling `told`.
fn page(told: &RefAny, detent: SheetDetent) -> StyledDom {
    let sheet = BottomSheet::create(
        Dom::create_div()
            .with_css("height: 900px;")
            .with_child(Dom::create_div().with_css("height: 40px;")),
    )
    .with_label(AzString::from("Place"))
    .with_detent(detent)
    .with_container_height(HEIGHT)
    .with_on_event(told.clone(), record as BottomSheetOnEventCallbackType);
    let root = Dom::create_div()
        .with_css("position: relative; width: 400px; height: 800px;")
        .with_child(sheet.dom());
    let mut dom = Dom::create_body().with_child(root);
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; } body { width: 400px; height: 800px; }");
    StyledDom::create(&mut dom, css)
}

fn down(y: f32) -> serde_json::Value {
    serde_json::json!({ "op": "touch_start", "id": 0, "x": X, "y": y })
}

fn to(y: f32) -> serde_json::Value {
    serde_json::json!({ "op": "touch_move", "id": 0, "x": X, "y": y })
}

fn up() -> serde_json::Value {
    serde_json::json!({ "op": "touch_end", "id": 0 })
}

fn frame() -> serde_json::Value {
    serde_json::json!({ "op": "wait_frame" })
}

fn height_is(px: f64) -> serde_json::Value {
    serde_json::json!({
        "op": "assert_layout",
        "selector": ".__azul-native-bottom-sheet",
        "property": "height",
        "expected": px,
        "tolerance": 1.0,
    })
}

/// Run `steps` on [`page`] with the sheet at `detent` and say what the app was told.
fn run(name: &str, detent: SheetDetent, steps: &[serde_json::Value]) -> (E2eTestResult, Vec<BottomSheetEvent>) {
    let mut told = RefAny::new(Told::default());
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 800, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    let result = run_e2e_test_with_dom(&test, Some(page(&told, detent)));
    let seen = told.downcast_ref::<Told>().map(|t| t.events.clone()).unwrap_or_default();
    (result, seen)
}

/// Every failing step, one line each - the assertion message a red run prints.
fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| format!("  step {} `{}`: {}", s.step_index, s.op, s.error.clone().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_passes(result: &E2eTestResult) {
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed:\n{}",
        result.name,
        failures(result)
    );
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0
}

fn kinds(events: &[BottomSheetEvent]) -> Vec<BottomSheetEventKind> {
    events.iter().map(|e| e.kind).collect()
}

#[test]
fn a_finger_pulling_the_grip_up_moves_the_sheet_and_it_settles_on_the_nearest_detent() {
    // Down on the grip (y 686), 10 px up decides the drag, 86 px and 186 px up follow it; the finger stops (one pixel more) and
    // lifts at 317 px: nearer to half (400) than to the peek (130).
    let (result, events) = run(
        "sheet_pull",
        SheetDetent::Peek,
        &[
            frame(),
            height_is(130.0),
            down(686.0),
            frame(),
            to(676.0),
            frame(),
            to(600.0),
            frame(),
            height_is(216.0),
            to(500.0),
            frame(),
            to(499.0),
            frame(),
            up(),
            frame(),
            frame(),
            height_is(400.0),
        ],
    );
    assert_passes(&result);
    let dragged: Vec<f32> = events
        .iter()
        .filter(|e| e.kind == BottomSheetEventKind::Dragged)
        .map(|e| e.height)
        .collect();
    assert!(dragged.len() >= 4, "one report per step: {events:?}");
    assert!(near(dragged[0], 140.0) && near(dragged[1], 216.0), "the sheet follows the finger: {dragged:?}");
    let last = events.last().expect("the app was told");
    assert_eq!((last.kind, last.detent), (BottomSheetEventKind::Settled, SheetDetent::Half), "{events:?}");
    assert!(near(last.height, 400.0));
    assert_eq!(kinds(&events).iter().filter(|k| **k == BottomSheetEventKind::Settled).count(), 1);
}

#[test]
fn a_fling_carries_the_sheet_past_the_nearest_detent() {
    // Lifted at 166 px with the last step 16 px: it would land at 166 + 16 x 12 = 358: half. A slow lift at 166 stays at the peek, and
    // so does a finger that stopped (150 ms, longer than a fling's speed lasts) before it lifted.
    let (fast, events) = run(
        "sheet_fling",
        SheetDetent::Peek,
        &[frame(), down(686.0), frame(), to(666.0), frame(), to(650.0), frame(), up(), frame(), frame(), height_is(400.0)],
    );
    assert_passes(&fast);
    assert_eq!(events.last().map(|e| e.detent), Some(SheetDetent::Half));
    let (slow, events) = run(
        "sheet_slow",
        SheetDetent::Peek,
        &[frame(), down(686.0), frame(), to(676.0), frame(), to(651.0), frame(), to(650.0), frame(), up(), frame(), frame(), height_is(130.0)],
    );
    assert_passes(&slow);
    assert_eq!(events.last().map(|e| e.detent), Some(SheetDetent::Peek), "{events:?}");
    let (stopped, events) = run(
        "sheet_stopped",
        SheetDetent::Peek,
        &[
            frame(),
            down(686.0),
            frame(),
            to(666.0),
            frame(),
            to(650.0),
            frame(),
            serde_json::json!({ "op": "tick_ms", "ms": 150 }),
            up(),
            frame(),
            frame(),
            height_is(130.0),
        ],
    );
    assert_passes(&stopped);
    assert_eq!(events.last().map(|e| e.detent), Some(SheetDetent::Peek), "it stopped before it lifted: {events:?}");
}

#[test]
fn a_tap_on_the_grip_steps_a_detent_up_and_tells_the_app_once() {
    let (result, events) = run(
        "sheet_tap",
        SheetDetent::Peek,
        &[frame(), down(686.0), frame(), up(), frame(), frame(), height_is(400.0)],
    );
    assert_passes(&result);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!((events[0].kind, events[0].detent), (BottomSheetEventKind::Settled, SheetDetent::Half));
}

#[test]
fn a_finger_in_the_content_is_not_the_sheets_and_moves_nothing() {
    // 100 px below the sheet's top edge is the content (the grip is the first 32 px).
    let (result, events) = run(
        "sheet_content",
        SheetDetent::Peek,
        &[frame(), down(770.0), frame(), to(700.0), frame(), to(600.0), frame(), up(), frame(), height_is(130.0)],
    );
    assert_passes(&result);
    assert!(events.is_empty(), "the content's finger is the content's: {events:?}");
}

#[test]
fn a_finger_pulling_the_grip_down_from_the_full_height_ends_at_the_nearest_lower_detent() {
    // Full: 680 px, the sheet's top at 120, the grip's middle at 136. Down 465 px: 215 px, nearest the peek.
    let (result, events) = run(
        "sheet_down",
        SheetDetent::Full,
        &[frame(), height_is(680.0), down(136.0), frame(), to(150.0), frame(), to(600.0), frame(), to(601.0), frame(), up(), frame(), frame(), height_is(130.0)],
    );
    assert_passes(&result);
    let last = events.last().expect("the app was told");
    assert_eq!((last.kind, last.detent), (BottomSheetEventKind::Settled, SheetDetent::Peek), "{events:?}");
}

#[test]
fn the_keys_on_the_focused_grip_settle_the_sheet_and_the_layout_follows() {
    let (result, events) = run(
        "sheet_keys",
        SheetDetent::Peek,
        &[
            frame(),
            serde_json::json!({ "op": "focus_node", "selector": ".__azul-native-bottom-sheet-grip" }),
            frame(),
            serde_json::json!({ "op": "key_down", "key": "Up" }),
            serde_json::json!({ "op": "key_up", "key": "Up" }),
            frame(),
            frame(),
            height_is(400.0),
            serde_json::json!({ "op": "key_down", "key": "End" }),
            serde_json::json!({ "op": "key_up", "key": "End" }),
            frame(),
            frame(),
            height_is(30.0),
        ],
    );
    assert_passes(&result);
    assert_eq!(
        events.iter().map(|e| e.detent).collect::<Vec<_>>(),
        vec![SheetDetent::Half, SheetDetent::Collapsed],
        "{events:?}"
    );
}
