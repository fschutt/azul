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

fn assert_passes(result: &E2eTestResult, events: &[BottomSheetEvent]) {
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed:\n{}\nthe app was told: {events:?}",
        result.name,
        failures(result)
    );
}


fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.0
}

fn tick(ms: u64) -> serde_json::Value {
    serde_json::json!({ "op": "tick_ms", "ms": ms })
}

/// The finger from `from` to `to` px in steps of `step` px, a frame after each: the finger of a phone moves a few px per event and
/// the sheet's top edge follows it (the grip lies 16 px under the finger), so the finger stays on the sheet - the events of a
/// touch go to the node under it, and a finger that outruns the sheet (a jump of 76 px) is no longer over it.
fn finger(from: f32, to_y: f32, step: f32) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut y = from;
    while (y - to_y).abs() > 0.5 {
        y = if to_y < y { (y - step).max(to_y) } else { (y + step).min(to_y) };
        out.push(to(y));
        out.push(frame());
    }
    out
}

fn mouse(op: &str, y: f32) -> serde_json::Value {
    serde_json::json!({ "op": op, "x": X, "y": y })
}

#[test]
fn a_finger_pulling_the_grip_up_moves_the_sheet_and_it_settles_on_the_nearest_detent() {
    // Down on the grip (y 686); 10 px up decides the drag (the sheet is 140 px tall: its top edge follows the finger), 12 px at a time
    // on to 500 (317 px), where the finger stops (150 ms) and lifts: nearer to half (400) than to the peek (130).
    let mut steps = vec![frame(), height_is(130.0), down(686.0), frame(), to(676.0), frame(), height_is(140.0)];
    steps.extend(finger(676.0, 500.0, 12.0));
    steps.extend([height_is(316.0), tick(150), up(), frame(), frame(), height_is(400.0)]);
    let (result, events) = run("sheet_pull", SheetDetent::Peek, &steps);
    assert_passes(&result, &events);
    let dragged: Vec<f32> = events.iter().filter(|e| e.kind == BottomSheetEventKind::Dragged).map(|e| e.height).collect();
    assert!(dragged.len() >= 10, "one report per step: {events:?}");
    assert!(near(dragged[0], 140.0), "the sheet follows the finger: {dragged:?}");
    let last = events.last().expect("the app was told");
    assert_eq!((last.kind, last.detent), (BottomSheetEventKind::Settled, SheetDetent::Half), "{events:?}");
    assert!(near(last.height, 400.0));
    assert_eq!(kinds(&events).iter().filter(|k| **k == BottomSheetEventKind::Settled).count(), 1);
}

#[test]
fn a_fling_carries_the_sheet_past_the_nearest_detent_and_a_finger_that_stopped_does_not() {
    // 12 px steps up to 650 px (166 px tall): lifted at once it would land at 166 + 12 x 12 = 310: half. The same lift 150 ms after the
    // finger stopped is the nearest detent to 166: the peek. A finger moving a few px at a time has no fling either.
    let mut fast = vec![frame(), down(686.0), frame()];
    fast.extend(finger(686.0, 650.0, 12.0));
    fast.extend([up(), frame(), frame(), height_is(400.0)]);
    let (result, events) = run("sheet_fling", SheetDetent::Peek, &fast);
    assert_passes(&result, &events);
    assert_eq!(events.last().map(|e| e.detent), Some(SheetDetent::Half), "{events:?}");

    let mut stopped = vec![frame(), down(686.0), frame()];
    stopped.extend(finger(686.0, 650.0, 12.0));
    stopped.extend([tick(150), up(), frame(), frame(), height_is(130.0)]);
    let (result, events) = run("sheet_stopped", SheetDetent::Peek, &stopped);
    assert_passes(&result, &events);
    assert_eq!(events.last().map(|e| e.detent), Some(SheetDetent::Peek), "it stopped before it lifted: {events:?}");

    let mut slow = vec![frame(), down(686.0), frame()];
    slow.extend(finger(686.0, 650.0, 3.0));
    slow.extend([up(), frame(), frame(), height_is(130.0)]);
    let (result, events) = run("sheet_slow", SheetDetent::Peek, &slow);
    assert_passes(&result, &events);
    assert_eq!(events.last().map(|e| e.detent), Some(SheetDetent::Peek), "{events:?}");
}

#[test]
fn a_tap_on_the_grip_steps_a_detent_up_and_tells_the_app_once() {
    let (result, events) = run(
        "sheet_tap",
        SheetDetent::Peek,
        &[frame(), down(686.0), frame(), up(), frame(), frame(), height_is(400.0)],
    );
    assert_passes(&result, &events);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!((events[0].kind, events[0].detent), (BottomSheetEventKind::Settled, SheetDetent::Half));
}

#[test]
fn a_finger_in_the_content_is_not_the_sheets_and_moves_nothing() {
    // 100 px below the sheet's top edge is the content (the grip is the first 32 px).
    let (result, events) = run(
        "sheet_content",
        SheetDetent::Peek,
        &[frame(), down(770.0), frame(), to(760.0), frame(), to(750.0), frame(), up(), frame(), height_is(130.0)],
    );
    assert_passes(&result, &events);
    assert!(events.is_empty(), "the content's finger is the content's: {events:?}");
}

#[test]
fn a_finger_pulling_the_grip_down_from_the_full_height_ends_at_the_nearest_lower_detent() {
    // Full: 680 px, the sheet's top at 120, the grip's middle at 136. Down 465 px: 215 px, nearest the peek.
    let mut steps = vec![frame(), height_is(680.0), down(136.0), frame()];
    steps.extend(finger(136.0, 601.0, 12.0));
    steps.extend([tick(150), up(), frame(), frame(), height_is(130.0)]);
    let (result, events) = run("sheet_down", SheetDetent::Full, &steps);
    assert_passes(&result, &events);
    let last = events.last().expect("the app was told");
    assert_eq!((last.kind, last.detent), (BottomSheetEventKind::Settled, SheetDetent::Peek), "{events:?}");
}

#[test]
fn a_mouse_that_leaves_the_sheet_with_the_button_down_keeps_dragging_it() {
    // The pointer is captured at the start of the drag: it may run far ahead of the sheet (a desktop's mouse is quick), the sheet
    // follows it. Down on the grip, 220 px up in one move, a pixel more, up: 130 + 221 = 351, nearest half.
    let (result, events) = run(
        "sheet_mouse",
        SheetDetent::Peek,
        &[
            frame(),
            mouse("mouse_move", 686.0),
            frame(),
            mouse("mouse_down", 686.0),
            frame(),
            mouse("mouse_move", 676.0),
            frame(),
            mouse("mouse_move", 466.0),
            frame(),
            mouse("mouse_move", 465.0),
            frame(),
            height_is(351.0),
            tick(150),
            mouse("mouse_up", 465.0),
            frame(),
            frame(),
            height_is(400.0),
        ],
    );
    assert_passes(&result, &events);
    let last = events.last().expect("the app was told");
    assert_eq!((last.kind, last.detent), (BottomSheetEventKind::Settled, SheetDetent::Half), "{events:?}");
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
    assert_passes(&result, &events);
    assert_eq!(
        events.iter().map(|e| e.detent).collect::<Vec<_>>(),
        vec![SheetDetent::Half, SheetDetent::Collapsed],
        "{events:?}"
    );
}
