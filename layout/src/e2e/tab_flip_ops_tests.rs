//! A vertical swipe on a tab panel (`TabContent::with_flip_of`), through the REAL dispatcher on a
//! headless window: the touch ops put a finger on the panel, the panel's own callbacks decide, and
//! a flip is told to the app exactly as a tap on a pill is. The platform's tab-change sound is asked
//! for once per tab crossed (`assert_sound`), a fling's momentum - a chain of timers - included.
//!
//! What this host cannot show is the picture of a pull (the shift of the content, the hint chip: the
//! pictures of the scenario do) and a real finger's jitter (the rules are `tab_flip_test.rs`).
//!
//! The rules the scenarios stand on: a pull of 56 px flips (the finger that carries on flips again
//! per 56 px), a fast lift carries over up to four more tabs in a slowing rhythm, a gesture that is
//! more sideways than vertical is not the panel's, nor one that went down on a node of the class
//! `__azul-claims-gesture`, the first tab has no previous and the last no next, and a lift after a
//! pause (a finger that stopped) has no speed to fling with.

use azul_core::{callbacks::Update, dom::Dom, refany::RefAny, styled_dom::StyledDom};
use azul_css::{AzString, StringVec};

use super::{runner::run_e2e_test_with_dom, E2eTest, E2eTestResult};
use crate::{
    callbacks::CallbackInfo,
    widgets::{
        tab_flip::fling_plan,
        tabs::{TabContent, TabHeader, TabHeaderState, TabOnClickCallbackType, TabStyle},
    },
};

/// How many tabs the bar has.
const TABS: usize = 6;

/// Where the finger is on the panel across: the middle of the window.
const X: f32 = 200.0;

/// What the app was told: the tab of every flip, in order.
#[derive(Default)]
struct Told {
    tabs: Vec<usize>,
}

/// The bar's `on_click`, as an app writes it: it notes the tab and answers `DoNothing` (the page
/// is not rebuilt in this host, the panel's own state is what the next flip goes on from).
extern "C" fn on_flip(mut data: RefAny, _info: CallbackInfo, state: TabHeaderState) -> Update {
    if let Some(mut told) = data.downcast_mut::<Told>() {
        told.tabs.push(state.active_tab);
    }
    Update::DoNothing
}

/// A window of the panel of a bar of [`TABS`] tabs with tab `active` shown: 300 px tall, with a
/// 100 px box at its top that claims the gestures that begin on it.
fn page(told: &RefAny, active: usize) -> StyledDom {
    let labels = StringVec::from_vec(
        (0..TABS)
            .map(|i| AzString::from(format!("tab {i}")))
            .collect(),
    );
    let bar = TabHeader::create(labels)
        .with_active_tab(active)
        .with_style(TabStyle::TouchPills)
        .with_on_click(told.clone(), on_flip as TabOnClickCallbackType);
    let claimant = Dom::create_div()
        .with_css("height: 100px;")
        .with_ids_and_classes(
            vec![azul_core::dom::IdOrClass::Class(
                crate::widgets::tabs::CLAIMS_GESTURE_CLASS.into(),
            )]
            .into(),
        );
    let panel = TabContent::new(
        Dom::create_div()
            .with_css("height: 300px;")
            .with_child(claimant),
    )
    .with_flip_of(&bar)
    .dom();
    let mut dom = Dom::create_body().with_child(panel);
    let (css, _) = azul_css::parser2::new_from_str(
        "* { margin: 0; padding: 0; } body { width: 400px; height: 600px; }",
    );
    StyledDom::create(&mut dom, css)
}

fn down(y: f32) -> serde_json::Value {
    serde_json::json!({ "op": "touch_start", "id": 0, "x": X, "y": y })
}

fn to(x: f32, y: f32) -> serde_json::Value {
    serde_json::json!({ "op": "touch_move", "id": 0, "x": x, "y": y })
}

fn up() -> serde_json::Value {
    serde_json::json!({ "op": "touch_end", "id": 0 })
}

fn frame() -> serde_json::Value {
    serde_json::json!({ "op": "wait_frame" })
}

fn tick(ms: u64) -> serde_json::Value {
    serde_json::json!({ "op": "tick_ms", "ms": ms })
}

fn ticks(count: usize) -> serde_json::Value {
    serde_json::json!({ "op": "assert_sound", "sound": "TabChange", "count": count })
}

fn silence() -> serde_json::Value {
    serde_json::json!({ "op": "assert_no_sound" })
}

/// Run `steps` on [`page`] of tab `active` and say what the app was told.
fn swipe(name: &str, active: usize, steps: &[serde_json::Value]) -> (E2eTestResult, Vec<usize>) {
    let mut told = RefAny::new(Told::default());
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 600, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    let result = run_e2e_test_with_dom(&test, Some(page(&told, active)));
    let seen = told
        .downcast_ref::<Told>()
        .map(|t| t.tabs.clone())
        .unwrap_or_default();
    (result, seen)
}

/// Every failing step, one line each - the assertion message a red run prints.
fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| {
            format!(
                "  step {} `{}`: {}",
                s.step_index,
                s.op,
                s.error.clone().unwrap_or_default()
            )
        })
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

#[test]
fn a_finger_that_carries_on_flips_once_per_threshold_and_the_sound_ticks_once_per_tab() {
    // Down at 280, a first move of 10 px decides the pull but flips nothing, 80 px up crosses the
    // threshold (flip to 1) and 70 px further crosses it again from where the finger was (flip to 2).
    // The finger paused 150 ms before it lifts: no speed, no fling.
    let (result, told) = swipe(
        "flip_per_threshold",
        0,
        &[
            frame(),
            silence(),
            down(280.0),
            frame(),
            to(X, 270.0),
            frame(),
            silence(),
            to(X, 200.0),
            frame(),
            ticks(1),
            to(X, 130.0),
            frame(),
            ticks(2),
            tick(150),
            up(),
            frame(),
            ticks(2),
        ],
    );
    assert_passes(&result);
    assert_eq!(
        told,
        vec![1, 2],
        "the app was told as of a tap on pill 1 and pill 2"
    );
}

#[test]
fn a_finger_that_pulls_down_goes_to_the_previous_tab() {
    let (result, told) = swipe(
        "flip_previous",
        3,
        &[
            frame(),
            down(150.0),
            frame(),
            to(X, 230.0),
            frame(),
            ticks(1),
            tick(150),
            up(),
            frame(),
            ticks(1),
        ],
    );
    assert_passes(&result);
    assert_eq!(told, vec![2]);
}

#[test]
fn a_short_pull_springs_back_and_a_cancelled_one_never_flips() {
    // 30 px is no flip, however long the finger waits; and a pull that is let go slowly stays.
    let (result, told) = swipe(
        "flip_short",
        1,
        &[
            frame(),
            down(280.0),
            frame(),
            to(X, 250.0),
            frame(),
            tick(150),
            up(),
            frame(),
            silence(),
        ],
    );
    assert_passes(&result);
    assert!(told.is_empty(), "nothing was told, got {told:?}");
}

#[test]
fn a_gesture_that_is_more_sideways_than_vertical_is_not_the_panels() {
    let (result, told) = swipe(
        "flip_sideways",
        1,
        &[
            frame(),
            down(280.0),
            frame(),
            to(X + 90.0, 250.0),
            frame(),
            to(X + 90.0, 150.0),
            frame(),
            tick(150),
            up(),
            frame(),
            silence(),
        ],
    );
    assert_passes(&result);
    assert!(told.is_empty(), "a sideways drag is the strip's: {told:?}");
}

#[test]
fn the_first_tab_has_no_previous_and_the_last_no_next() {
    // Down on the first tab: a pull down has nowhere to go.
    let (first, told_first) = swipe(
        "flip_first",
        0,
        &[
            frame(),
            down(150.0),
            frame(),
            to(X, 260.0),
            frame(),
            tick(150),
            up(),
            frame(),
            silence(),
        ],
    );
    assert_passes(&first);
    assert!(
        told_first.is_empty(),
        "no tab before the first: {told_first:?}"
    );
    // On the last tab: a pull up has nowhere to go.
    let (last, told_last) = swipe(
        "flip_last",
        TABS - 1,
        &[
            frame(),
            down(280.0),
            frame(),
            to(X, 170.0),
            frame(),
            tick(150),
            up(),
            frame(),
            silence(),
        ],
    );
    assert_passes(&last);
    assert!(told_last.is_empty(), "no tab after the last: {told_last:?}");
}

#[test]
fn a_finger_that_went_down_on_a_node_that_claims_the_gesture_flips_nothing() {
    // The claimant is the first 100 px of the content: a finger down there (a slider, a map) is its.
    let (claimed, told) = swipe(
        "flip_claimed",
        1,
        &[
            frame(),
            down(60.0),
            frame(),
            to(X, 130.0),
            frame(),
            to(X, 200.0),
            frame(),
            tick(150),
            up(),
            frame(),
            silence(),
        ],
    );
    assert_passes(&claimed);
    assert!(
        told.is_empty(),
        "the claimant's gesture is not a flip: {told:?}"
    );
    // The same pull from below the claimant is a flip.
    let (free, told) = swipe(
        "flip_not_claimed",
        1,
        &[
            frame(),
            down(280.0),
            frame(),
            to(X, 200.0),
            frame(),
            ticks(1),
        ],
    );
    assert_passes(&free);
    assert_eq!(told, vec![2]);
}

#[test]
fn a_fast_lift_carries_the_fling_over_more_tabs_one_sound_each() {
    // 48 px per 12 ms is 4000 px/s: the finger flips once on its way (tab 1), lifts without a pause,
    // and the momentum carries over as many more tabs as the plan says (four here, one per timer,
    // each later than the one before): 2, 3, 4, 5. The window has the tabs for all of them.
    let plan = fling_plan(4000.0, TABS - 2);
    assert_eq!(
        plan.len(),
        4,
        "premise: the plan for this speed has four flips"
    );
    let mut steps = vec![
        frame(),
        down(300.0),
        tick(12),
        to(X, 252.0),
        tick(12),
        to(X, 204.0),
        frame(),
        ticks(1),
        tick(4),
        up(),
        frame(),
        // the lift itself flips nothing more
        ticks(1),
    ];
    // The timers: one flip each, the next set when the one before has gone off. A tick longer than
    // the longest delay fires the timer that is due and no more.
    let longest = *plan.iter().max().expect("a plan");
    for round in 0..plan.len() {
        steps.push(tick(longest + 40));
        steps.push(frame());
        steps.push(ticks(2 + round));
    }
    let (result, told) = swipe("flip_fling", 0, &steps);
    assert_passes(&result);
    assert_eq!(
        told,
        vec![1, 2, 3, 4, 5],
        "the finger flipped to 1 and the momentum went on to 5, one tab per timer"
    );
}

#[test]
fn a_new_touch_ends_the_fling_in_flight() {
    // The fling of the test above, but a finger goes down before its first timer fires: whatever
    // was left of the momentum is the finger's now.
    let (result, told) = swipe(
        "flip_fling_interrupted",
        0,
        &[
            frame(),
            down(300.0),
            tick(12),
            to(X, 252.0),
            tick(12),
            to(X, 204.0),
            frame(),
            tick(4),
            up(),
            frame(),
            ticks(1),
            // the finger is back (and rests) before the first 110 ms are over
            tick(20),
            down(280.0),
            frame(),
            tick(1000),
            frame(),
            ticks(1),
            tick(150),
            up(),
            frame(),
            tick(1000),
            frame(),
            ticks(1),
        ],
    );
    assert_passes(&result);
    assert_eq!(told, vec![1], "only the flip the first finger made");
}
