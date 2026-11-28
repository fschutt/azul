//! The system's accessibility settings over the debug server and in scenario files:
//! `get_system_accessibility` / `set_system_accessibility`, driven end to end through the REAL op
//! dispatch (`process_debug_event` + the scenario runner) on a headless window.
//!
//! The same file of a scenario runs on a desktop, an emulator and a phone, so a text size, a bold
//! text or a reduced motion is a step like any other: nothing needs a real phone to be switched.
//! The headless window mounts its own DOM, so the sizes below are the engine's own: a line
//! height in px, a box in em and a box in px, and what the text size does to each.

use super::{run_e2e_test, E2eTest, E2eTestResult};

/// Run `steps` as one headless scenario on a 600x400 window.
fn run(name: &str, continue_on_failure: bool, steps: serde_json::Value) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": continue_on_failure },
        "setup": { "window_width": 600, "window_height": 400, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test(&test)
}

/// Every failing step, one line each.
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

/// Three boxes: one whose height is a line height in px (text), one sized in em, one in px.
fn mounted() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "op": "mount",
            "html": [
                "<div id=\"line\">text</div>",
                "<div id=\"em\">x</div>",
                "<div id=\"px\">x</div>",
            ],
            "css": [
                "body { margin: 0; }",
                "#line { margin: 0; font-size: 10px; line-height: 24px; }",
                "#em { margin: 0; font-size: 10px; width: 10em; height: 5em; }",
                "#px { margin: 0; width: 100px; height: 20px; }",
            ],
        }),
        serde_json::json!({ "op": "wait_frame" }),
    ]
}

fn contains(fragment: &str) -> serde_json::Value {
    serde_json::json!({ "op": "assert_response", "contains": fragment })
}

fn layout(selector: &str, property: &str, expected: f64, tolerance: f64) -> serde_json::Value {
    serde_json::json!({
        "op": "assert_layout", "selector": selector, "property": property,
        "expected": expected, "tolerance": tolerance
    })
}

#[test]
fn a_window_nobody_set_up_reports_nothing_known_and_lays_out_at_the_default_size() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "get_system_accessibility" }),
        contains("\"effective_text_scale\":1.0"),
        contains("\"bold_text\":\"unknown\""),
        contains("\"contrast\":\"unknown\""),
        contains("\"text_scale\":null"),
        layout("#line", "height", 24.0, 0.01),
        layout("#em", "width", 100.0, 0.01),
        layout("#em", "height", 50.0, 0.01),
        layout("#px", "width", 100.0, 0.01),
        layout("#px", "height", 20.0, 0.01),
    ]);
    assert_passes(&run("a11y_nothing_set", false, serde_json::Value::Array(steps)));
}

/// The text size scales what is text (a line height in px, an em) and not what is sized in px;
/// a reset puts the system's answer back.
#[test]
fn set_system_accessibility_scales_text_and_not_boxes_in_px_and_reset_undoes_it() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "set_system_accessibility", "text_scale": 2.0 }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_system_accessibility" }),
        contains("\"effective_text_scale\":2.0"),
        contains("\"text_scale\":2.0"),
        // a line height in px is a length of text: it grows with it
        layout("#line", "height", 48.0, 0.01),
        // an em follows the font size
        layout("#em", "width", 200.0, 0.01),
        layout("#em", "height", 100.0, 0.01),
        // a box in px does not
        layout("#px", "width", 100.0, 0.01),
        layout("#px", "height", 20.0, 0.01),
        // the largest iOS size, by its short name
        serde_json::json!({ "op": "set_system_accessibility", "text_scale": "ax5" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_system_accessibility" }),
        contains("\"text_size_category\":\"accessibility-extra-extra-extra-large\""),
        layout("#line", "height", 24.0 * 53.0 / 17.0, 0.05),
        layout("#em", "width", 100.0 * 53.0 / 17.0, 0.05),
        layout("#px", "height", 20.0, 0.01),
        // smaller than the default is a scale too
        serde_json::json!({ "op": "set_system_accessibility", "text_scale": "extra-small" }),
        serde_json::json!({ "op": "wait_frame" }),
        layout("#line", "height", 24.0 * 14.0 / 17.0, 0.05),
        // the other options do not move the text
        serde_json::json!({ "op": "set_system_accessibility", "text_scale": 1.3, "bold_text": true,
                            "contrast": "more", "reduced_motion": "on" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_system_accessibility" }),
        contains("\"bold_text\":\"on\""),
        contains("\"contrast\":\"more\""),
        contains("\"reduced_motion\":\"on\""),
        layout("#line", "height", 24.0 * 1.3, 0.05),
        // an option the second step did not name keeps what the first said
        serde_json::json!({ "op": "set_system_accessibility", "bold_text": false }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_system_accessibility" }),
        contains("\"bold_text\":\"off\""),
        contains("\"contrast\":\"more\""),
        layout("#line", "height", 24.0 * 1.3, 0.05),
        // reset: what the platform said (nothing, on a headless window)
        serde_json::json!({ "op": "set_system_accessibility", "reset": true }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_system_accessibility" }),
        contains("\"effective_text_scale\":1.0"),
        contains("\"contrast\":\"unknown\""),
        layout("#line", "height", 24.0, 0.01),
        layout("#em", "width", 100.0, 0.01),
    ]);
    assert_passes(&run("a11y_text_scale_round_trip", false, serde_json::Value::Array(steps)));
}

#[test]
fn a_scale_past_the_range_is_capped_not_refused() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "set_system_accessibility", "text_scale": 99.0 }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_system_accessibility" }),
        contains("\"effective_text_scale\":4.0"),
        layout("#line", "height", 96.0, 0.01),
    ]);
    assert_passes(&run("a11y_text_scale_capped", false, serde_json::Value::Array(steps)));
}

#[test]
fn set_system_accessibility_refuses_what_is_no_option_and_changes_nothing() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "set_system_accessibility", "sparkles": true }),
        serde_json::json!({ "op": "set_system_accessibility", "bold_text": "perhaps" }),
        serde_json::json!({ "op": "set_system_accessibility", "text_scale": "huge" }),
        serde_json::json!({ "op": "set_system_accessibility", "contrast": [1] }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_system_accessibility" }),
        contains("\"effective_text_scale\":1.0"),
        contains("\"bold_text\":\"unknown\""),
        layout("#line", "height", 24.0, 0.01),
    ]);
    let result = run("a11y_refusals", true, serde_json::Value::Array(steps));
    let refused: Vec<_> = result
        .steps
        .iter()
        .filter(|s| s.op == "set_system_accessibility")
        .collect();
    assert_eq!(refused.len(), 4);
    for step in &refused {
        assert_eq!(step.status, "fail", "refused: {step:#?}");
    }
    let first = refused[0].error.clone().unwrap_or_default();
    assert!(
        first.contains("sparkles") && first.contains("text_scale") && first.contains("bold_text"),
        "the refusal names the key and the options there are, got {first:?}"
    );
    let rest: Vec<_> = result
        .steps
        .iter()
        .filter(|s| s.op != "set_system_accessibility" && s.status != "pass")
        .collect();
    assert!(rest.is_empty(), "nothing changed:\n{}", failures(&result));
}

/// "Reduce motion" switches off every declared animation: the transition the old tree declares for
/// a change of width does not run, the node is at its end state at once. The first half is the
/// control (the same two mounts DO start a transition without the setting).
#[test]
fn reduced_motion_makes_a_declared_transition_instant() {
    let mount = |width: u32, animated: bool| {
        serde_json::json!({
            "op": "mount",
            "html": ["<div id=\"root\"><div id=\"side\">side</div></div>"],
            "css": [
                "html, body { margin: 0; padding: 0; width: 100%; height: 100%; }",
                "#root { width: 100%; height: 100%; display: flex; flex-direction: row; }",
                format!(
                    "#side {{ width: {width}px; height: 100%; background: #2255aa; {} }}",
                    if animated { "animation: all 1s linear;" } else { "" }
                ),
            ],
        })
    };
    let width_is = |width: &str| {
        serde_json::json!({ "op": "assert_response", "type": "node_layout",
                            "contains": format!("\"width\":{width}") })
    };
    let steps = serde_json::json!([
        // the control: mount 1 declares the animation, mount 2 changes the width
        mount(200, true),
        { "op": "wait_frame" },
        mount(40, false),
        { "op": "wait_frame" },
        { "op": "get_animations" },
        { "op": "assert_response", "type": "animations", "contains": "\"transitions\":1" },
        { "op": "get_node_layout", "selector": "#side" },
        width_is("200.0"),
        { "op": "tick_animations", "dt_micros": 16666, "steps": 90 },
        { "op": "wait_frame" },
        { "op": "get_node_layout", "selector": "#side" },
        width_is("40.0"),
        // the same two mounts under the user's "Reduce motion"
        { "op": "set_system_accessibility", "reduced_motion": "on" },
        { "op": "wait_frame" },
        mount(200, true),
        { "op": "wait_frame" },
        mount(40, false),
        { "op": "wait_frame" },
        { "op": "get_animations" },
        { "op": "assert_response", "type": "animations", "contains": "\"transitions\":0" },
        { "op": "get_node_layout", "selector": "#side" },
        width_is("40.0"),
    ]);
    assert_passes(&run("a11y_reduced_motion_is_instant", false, steps));
}
