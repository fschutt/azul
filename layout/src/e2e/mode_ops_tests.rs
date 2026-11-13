//! The app's light / dark MODE and its THEME over the debug server:
//! `get_mode` / `set_mode` and `get_theme` / `set_theme`, driven end to end
//! through the REAL op dispatch (`process_debug_event` + the scenario runner)
//! on a headless window.
//!
//! AzBuilder's page has its own Auto / Light / Dark toggle, and the app window
//! it builds has the app's mode (`AppConfig::mode`, `CallbackInfo::set_mode`).
//! The two were separate settings ("the dark / light mode in the AzBuilder
//! doesn't get synchronized"): the page had no op to read or switch the app's
//! mode. These ops are that channel - the page reads `get_mode` on load and on
//! its poll, and its toggle calls `set_mode` - and a curl one-liner for anyone
//! else (`doc/guide/en/debugging.md`).

use super::{run_e2e_test, E2eTest, E2eTestResult};

/// Run `steps` as one headless scenario on a 400x300 window.
fn run(name: &str, continue_on_failure: bool, steps: serde_json::Value) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": continue_on_failure },
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test(&test)
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

/// The window and a frame to read it from.
fn mounted() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "op": "mount", "html": ["<p id=\"t\">mode</p>"] }),
        serde_json::json!({ "op": "wait_frame" }),
    ]
}

fn contains(fragment: &str) -> serde_json::Value {
    serde_json::json!({ "op": "assert_response", "contains": fragment })
}

#[test]
fn get_mode_reports_the_apps_choice_and_set_mode_pins_light_or_dark_and_follows_the_desktop_again()
{
    let mut steps = mounted();
    steps.extend([
        // A fresh app follows the desktop.
        serde_json::json!({ "op": "get_mode" }),
        contains("\"mode\":\"system\""),
        // Pin dark: the answer names the choice, the window shows it after its frame.
        serde_json::json!({ "op": "set_mode", "mode": "dark" }),
        contains("\"mode\":\"dark\""),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_mode" }),
        contains("\"mode\":\"dark\""),
        contains("\"resolved\":\"dark\""),
        // Pin light.
        serde_json::json!({ "op": "set_mode", "mode": "light" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_mode" }),
        contains("\"mode\":\"light\""),
        contains("\"resolved\":\"light\""),
        // Back to the desktop.
        serde_json::json!({ "op": "set_mode", "mode": "system" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_mode" }),
        contains("\"mode\":\"system\""),
    ]);
    assert_passes(&run(
        "mode_ops_round_trip",
        false,
        serde_json::Value::Array(steps),
    ));
}

#[test]
fn set_mode_refuses_a_name_that_is_no_mode_and_keeps_the_mode() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "set_mode", "mode": "purple" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_mode" }),
        contains("\"mode\":\"system\""),
    ]);
    let result = run("mode_op_refuses", true, serde_json::Value::Array(steps));

    let refused = result
        .steps
        .iter()
        .find(|s| s.op == "set_mode")
        .expect("the set_mode step ran");
    assert_eq!(
        refused.status, "fail",
        "\"purple\" is no mode: {refused:#?}"
    );
    let error = refused.error.clone().unwrap_or_default();
    assert!(
        error.contains("light") && error.contains("dark") && error.contains("system"),
        "the refusal names the modes there are, got {error:?}"
    );
    let rest: Vec<_> = result
        .steps
        .iter()
        .filter(|s| s.op != "set_mode" && s.status != "pass")
        .collect();
    assert!(
        rest.is_empty(),
        "the mode stayed \"system\":\n{}",
        failures(&result)
    );
}

#[test]
fn get_theme_reports_the_app_theme_and_set_theme_switches_it() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "get_theme" }),
        contains("\"theme\":\"flat\""),
        serde_json::json!({ "op": "set_theme", "theme": "flora" }),
        contains("\"theme\":\"flora\""),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_theme" }),
        contains("\"theme\":\"flora\""),
        serde_json::json!({ "op": "set_theme", "theme": "flat" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_theme" }),
        contains("\"theme\":\"flat\""),
    ]);
    assert_passes(&run(
        "theme_ops_round_trip",
        false,
        serde_json::Value::Array(steps),
    ));
}

#[test]
fn set_theme_refuses_an_empty_name() {
    let mut steps = mounted();
    steps.push(serde_json::json!({ "op": "set_theme", "theme": "  " }));
    let result = run("theme_op_refuses", true, serde_json::Value::Array(steps));
    let refused = result
        .steps
        .iter()
        .find(|s| s.op == "set_theme")
        .expect("the set_theme step ran");
    assert_eq!(refused.status, "fail", "an empty theme name: {refused:#?}");
}
