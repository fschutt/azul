//! `assert_changed` says WHOSE fault a missing repaint is, driven through the
//! real op dispatch (`run_e2e_test`) on a headless window.
//!
//! "Nothing was repainted" is one report for two different situations: the
//! engine changed what is shown and did not repaint it (a stale screen), or
//! nothing the scenario did changed a pixel in the first place (a control that
//! cannot move one). From the damage alone they look alike, and every one of
//! them read "stale screen" - the review of batch 2 of `gen-e2e` had to
//! re-derive the difference by hand for each red control. The pixels tell:
//! when there is no damage and a snapshot to compare with, a fresh render is
//! diffed against it. Pixels that differ are `STALE SCREEN (engine)`; pixels
//! that do not are `DEAD CONTROL (the test)`.

use super::{run_e2e_test, E2eTest, E2eTestResult};

/// Run `steps` as one headless scenario on a 400x300 window. A red step does
/// not end the run, so the step under test is always reached.
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

/// A blue box, landed, and a snapshot of the frame as `before`.
fn a_box_and_its_snapshot() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "op": "mount",
            "html": ["<body><div id=\"box\"></div></body>"],
            "css": [
                "html, body { margin: 0; padding: 0; }",
                "#box { width: 100px; height: 80px; background: #0000ff; }",
            ],
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "snapshot_frame", "as": "before" }),
    ]
}

/// The error of the LAST step (`None`: it passed) of the box scenario followed
/// by `steps`.
fn error_of_last(name: &str, steps: Vec<serde_json::Value>) -> Option<String> {
    let mut all = a_box_and_its_snapshot();
    all.extend(steps);
    let last = all.len() - 1;
    let result = run(name, all);
    result
        .steps
        .iter()
        .find(|s| s.step_index == last)
        .unwrap_or_else(|| panic!("the last step never ran: {result:#?}"))
        .error
        .clone()
}

/// The box was recoloured (the pixels differ from `before`), and the damage
/// that said so was zeroed by `reset_frame_counters`: the screen changed and
/// the damage does not know. That is what a missed repaint looks like.
#[test]
fn a_fresh_render_that_differs_with_no_damage_is_a_stale_screen() {
    let error = error_of_last(
        "stale_screen",
        vec![
            serde_json::json!({ "op": "set_node_css_override", "selector": "#box",
                                "property": "background-color", "value": "#ff0000" }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "reset_frame_counters" }),
            serde_json::json!({ "op": "assert_changed", "vs": "before" }),
        ],
    )
    .expect("there is no damage, so assert_changed is red");
    assert!(error.contains("STALE SCREEN (engine)"), "{error}");
    assert!(!error.contains("DEAD CONTROL"), "{error}");
}

/// Nothing was done: no damage, and a fresh render is the snapshot. A control
/// that cannot move a pixel looks exactly like this.
#[test]
fn a_fresh_render_identical_to_the_snapshot_is_a_dead_control() {
    let error = error_of_last(
        "dead_control",
        vec![
            serde_json::json!({ "op": "reset_frame_counters" }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "assert_changed", "vs": "before" }),
        ],
    )
    .expect("there is no damage, so assert_changed is red");
    assert!(error.contains("DEAD CONTROL (the test)"), "{error}");
    assert!(!error.contains("STALE SCREEN"), "{error}");
}

/// Without a snapshot there is nothing to compare, and the report does not
/// pretend to know: no attribution, the old words.
#[test]
fn without_a_snapshot_a_missing_repaint_is_not_attributed() {
    let error = error_of_last(
        "no_snapshot",
        vec![
            serde_json::json!({ "op": "reset_frame_counters" }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "assert_changed" }),
        ],
    )
    .expect("there is no damage, so assert_changed is red");
    assert!(error.contains("nothing was repainted"), "{error}");
    assert!(!error.contains("STALE SCREEN (engine)"), "{error}");
    assert!(!error.contains("DEAD CONTROL"), "{error}");
}

/// What did repaint and change pixels still passes.
#[test]
fn a_repaint_that_changed_pixels_passes() {
    let error = error_of_last(
        "repaint_passes",
        vec![
            serde_json::json!({ "op": "reset_frame_counters" }),
            serde_json::json!({ "op": "set_node_css_override", "selector": "#box",
                                "property": "background-color", "value": "#ff0000" }),
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "assert_changed", "vs": "before" }),
        ],
    );
    assert_eq!(error, None);
}
