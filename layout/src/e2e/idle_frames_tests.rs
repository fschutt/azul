//! `assert_idle_frames`: the window stays idle for N frames, ONE step, driven through
//! the real op dispatch (`run_e2e_test`) on a headless window (E2E-M2).
//!
//! A scenario that holds a window idle for 40 frames had to spell out a snapshot, a
//! `tick_ms`, a `wait_frame` and an `assert_idle_stable` per frame - 93 to 97 steps
//! (`gen-e2e` batch 1, "missing ops") - and the generator prompt taught that
//! unrolling. The op moves the engine clock a frame at a time, waits for each frame
//! and holds it to the picture the step found: no damage, the same pixels, and a
//! frame rendered for every turn.

use super::{run_e2e_test, E2eTest, E2eTestResult};

/// Run `steps` as one headless scenario on a 400x300 window. A red step does not
/// end the run, so every assertion is read.
fn run(name: &str, steps: Vec<serde_json::Value>) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test(&test)
}

/// Every red step, one line each - what a failing run prints.
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

/// The step named `op` of a run (the first one).
fn step_of<'a>(result: &'a E2eTestResult, op: &str) -> &'a super::E2eStepResult {
    result
        .steps
        .iter()
        .find(|s| s.op == op)
        .unwrap_or_else(|| panic!("the `{op}` step ran: {}", failures(result)))
}

/// A static red box, landed and at rest.
fn a_window_at_rest() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "op": "mount",
            "html": ["<div id=\"box\"></div>"],
            "css": ["#box { width: 50px; height: 50px; background-color: red; }"],
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
    ]
}

/// E2E-M2: a window at rest holds for forty frames in one step - and the step's
/// counters are those frames' (`exact_layout_passes: 0` follows it).
#[test]
fn e2e_m2_a_window_at_rest_holds_for_forty_frames_in_one_step() {
    let mut steps = a_window_at_rest();
    steps.push(serde_json::json!({ "op": "assert_idle_frames", "frames": 40, "step_ms": 16 }));
    steps.push(serde_json::json!({ "op": "assert_work_bounded", "exact_layout_passes": 0 }));
    let result = run("e2e_m2_holds", steps);
    assert_passes(&result);
    let step = step_of(&result, "assert_idle_frames");
    assert!(
        step.logs
            .iter()
            .any(|l| l.contains("40 frame(s), 16 ms apart (640 ms of engine time)")),
        "the pass line names what was held: {:?}",
        step.logs
    );
}

/// E2E-M2: the step is ONE step, however many frames it holds the window for.
#[test]
fn e2e_m2_forty_frames_are_one_step_not_ninety_seven() {
    let mut steps = a_window_at_rest();
    steps.push(serde_json::json!({ "op": "assert_idle_frames", "frames": 40 }));
    let result = run("e2e_m2_one_step", steps);
    assert_passes(&result);
    assert_eq!(
        result
            .steps
            .iter()
            .filter(|s| s.op == "assert_idle_frames")
            .count(),
        1,
        "one step result for the whole hold: {:?}",
        result.steps.iter().map(|s| &s.op).collect::<Vec<_>>()
    );
}

/// E2E-M2: a window that repaints on its own fails at the frame that does, and says
/// which. A scroll bar is on its way out 500 ms after the wheel and gone 200 ms later;
/// 20 frames of 100 ms catch it.
#[test]
fn e2e_m2_a_scrollbar_fading_out_is_not_an_idle_window() {
    let rows: String = (1..=40)
        .map(|i| format!("<div class=\"row\">Row {i}</div>"))
        .collect();
    let result = run(
        "e2e_m2_fade",
        vec![
            serde_json::json!({
                "op": "mount",
                "html": [format!("<div id=\"scroller\">{rows}</div>")],
                "css": [
                    "html, body { margin: 0; }",
                    "#scroller { width: 300px; height: 200px; overflow-y: scroll; \
                     background-color: white; font-family: \"Azul Mock Mono\"; }",
                    ".row { height: 30px; font-size: 14px; color: black; }",
                ],
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({
                "op": "scroll_node_by", "selector": "#scroller", "delta_x": 0, "delta_y": 60,
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "assert_idle_frames", "frames": 20, "step_ms": 100 }),
        ],
    );
    let step = step_of(&result, "assert_idle_frames");
    assert_eq!(
        step.status, "fail",
        "a bar that fades within 700 ms of the scroll is not idle for 2000 ms"
    );
    let error = step.error.clone().unwrap_or_default();
    assert!(
        error.contains("the window repainted by frame") && error.contains(" of 20 (+"),
        "the error names the frame it failed at: {error}"
    );
}

/// E2E-M2: after the fade the same window holds - the hold does not fail for being a
/// window with a scroller on it.
#[test]
fn e2e_m2_the_same_window_holds_once_the_fade_is_over() {
    let rows: String = (1..=40)
        .map(|i| format!("<div class=\"row\">Row {i}</div>"))
        .collect();
    let result = run(
        "e2e_m2_after_fade",
        vec![
            serde_json::json!({
                "op": "mount",
                "html": [format!("<div id=\"scroller\">{rows}</div>")],
                "css": [
                    "html, body { margin: 0; }",
                    "#scroller { width: 300px; height: 200px; overflow-y: scroll; \
                     background-color: white; font-family: \"Azul Mock Mono\"; }",
                    ".row { height: 30px; font-size: 14px; color: black; }",
                ],
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait", "ms": 100 }),
            serde_json::json!({
                "op": "scroll_node_by", "selector": "#scroller", "delta_x": 0, "delta_y": 60,
            }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "wait_settled" }),
            serde_json::json!({ "op": "assert_idle_frames", "frames": 10, "step_ms": 100 }),
        ],
    );
    assert_passes(&result);
}

/// E2E-M2: `vs` holds every frame to a snapshot. The window was changed after it, so
/// the very first frame is not that picture - though the window is idle (no damage).
#[test]
fn e2e_m2_vs_holds_every_frame_to_the_named_snapshot() {
    let mut steps = a_window_at_rest();
    steps.extend([
        serde_json::json!({ "op": "snapshot_frame", "as": "red" }),
        serde_json::json!({
            "op": "set_node_css_override", "selector": "#box",
            "property": "background-color", "value": "blue",
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "assert_idle_frames", "frames": 3, "vs": "red" }),
    ]);
    let result = run("e2e_m2_vs_stale", steps);
    let step = step_of(&result, "assert_idle_frames");
    assert_eq!(step.status, "fail", "the window is blue, the snapshot red");
    let error = step.error.clone().unwrap_or_default();
    assert!(
        error.contains("frame 1 of 3") && error.contains("is not the frame it is held to"),
        "the first frame is the one that differs: {error}"
    );

    // And the picture it IS: a snapshot of the window as it is now holds.
    let mut steps = a_window_at_rest();
    steps.extend([
        serde_json::json!({
            "op": "set_node_css_override", "selector": "#box",
            "property": "background-color", "value": "blue",
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "snapshot_frame", "as": "blue" }),
        serde_json::json!({ "op": "assert_idle_frames", "frames": 3, "vs": "blue" }),
    ]);
    assert_passes(&run("e2e_m2_vs_holds", steps));
}

/// E2E-M2: `vs` names a snapshot that does not exist - the step says so, it does not
/// hold the window to nothing.
#[test]
fn e2e_m2_an_unknown_snapshot_is_refused_by_name() {
    let mut steps = a_window_at_rest();
    steps.push(serde_json::json!({ "op": "assert_idle_frames", "frames": 3, "vs": "nope" }));
    let result = run("e2e_m2_no_snapshot", steps);
    let step = step_of(&result, "assert_idle_frames");
    assert_eq!(step.status, "fail");
    let error = step.error.clone().unwrap_or_default();
    assert!(error.contains("no frame snapshot named 'nope'"), "{error}");
}

/// E2E-M2: a parameter the op does not know is refused by name (a typo'd `frame` was
/// dropped by the parser and the step held the window for the default).
#[test]
fn e2e_m2_an_unknown_parameter_is_refused_by_name() {
    let mut steps = a_window_at_rest();
    steps.push(serde_json::json!({ "op": "assert_idle_frames", "frames": 3, "frame_ms": 16 }));
    let result = run("e2e_m2_typo", steps);
    let step = step_of(&result, "assert_idle_frames");
    assert_eq!(step.status, "fail");
    let error = step.error.clone().unwrap_or_default();
    assert!(
        error.contains("assert_idle_frames: unknown parameter 'frame_ms'")
            && error.contains("step_ms"),
        "the refusal names the op, the key and the known ones: {error}"
    );
}

/// E2E-M2: `frames` is required and bounded; `step_ms` too.
#[test]
fn e2e_m2_frames_and_step_ms_are_required_and_bounded() {
    for (params, want) in [
        (serde_json::json!({}), "needs `frames`"),
        (
            serde_json::json!({ "frames": 0 }),
            "frames is 0, which is not a whole number from 1 to 120",
        ),
        (
            serde_json::json!({ "frames": 121 }),
            "frames is 121, which is not a whole number from 1 to 120",
        ),
        (
            serde_json::json!({ "frames": "forty" }),
            "frames is \"forty\", which is not a whole number",
        ),
        (
            serde_json::json!({ "frames": 3, "step_ms": 0 }),
            "step_ms is 0, which is not a whole number from 1 to 60000",
        ),
        (
            serde_json::json!({ "frames": 3, "threshold": 256 }),
            "threshold is 256, which is not a whole number from 0 to 255",
        ),
        (
            serde_json::json!({ "frames": 3, "vs": 7 }),
            "vs is 7, which is not a snapshot name",
        ),
    ] {
        let mut step = serde_json::json!({ "op": "assert_idle_frames" });
        step.as_object_mut()
            .expect("an object")
            .extend(params.as_object().expect("an object").clone());
        let mut steps = a_window_at_rest();
        steps.push(step);
        let result = run("e2e_m2_bounds", steps);
        let step = step_of(&result, "assert_idle_frames");
        assert_eq!(step.status, "fail", "{params}");
        let error = step.error.clone().unwrap_or_default();
        assert!(error.contains(want), "{params}: {error}");
    }
}

/// E2E-M2: the hold runs at the step's engine time - 40 frames of 250 ms are ten
/// seconds of the engine's clock.
#[test]
fn e2e_m2_the_hold_spends_the_engine_time_of_its_step_ms() {
    let mut steps = a_window_at_rest();
    steps.push(serde_json::json!({ "op": "assert_idle_frames", "frames": 40, "step_ms": 250 }));
    let result = run("e2e_m2_engine_time", steps);
    assert_passes(&result);
    let step = step_of(&result, "assert_idle_frames");
    assert!(
        step.logs
            .iter()
            .any(|l| l.contains("40 frame(s), 250 ms apart (10000 ms of engine time)")),
        "{:?}",
        step.logs
    );
}
