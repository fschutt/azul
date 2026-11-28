//! The display a window sits on, over the debug server: `set_display_preset` makes any window
//! behave as if it sat on a Pixel 8 or an iPhone (`WindowFlags::display_preset`),
//! `get_display_features` reads the cutouts, the system bars, the corners and the safe-area
//! insets the window has, driven end to end through the REAL op dispatch on a headless window.
//!
//! The same scenario file then runs on a desktop (where the preset is the only source of a
//! cutout) and on a phone (where the preset replaces what the shell reports).
//!
//! Not compiled by the author (house rule).

use super::{run_e2e_test, E2eTest, E2eTestResult};

/// Run `steps` as one headless scenario on a window of `width` x `height` px.
fn run(
    name: &str,
    width: u32,
    height: u32,
    continue_on_failure: bool,
    steps: serde_json::Value,
) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": continue_on_failure },
        "setup": { "window_width": width, "window_height": height, "dpi": 96 },
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

fn mounted() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({ "op": "mount", "html": ["<p id=\"t\">display</p>"] }),
        serde_json::json!({ "op": "wait_frame" }),
    ]
}

fn contains(fragment: &str) -> serde_json::Value {
    serde_json::json!({ "op": "assert_response", "contains": fragment })
}

#[test]
fn a_window_has_no_cutout_until_a_preset_gives_it_one() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "get_display_features" }),
        contains("\"preset\":\"off\""),
        contains("\"cutouts\":[]"),
        contains("\"status_bar\":null"),
        serde_json::json!({ "op": "set_display_preset", "preset": "pixel8" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_display_features" }),
        contains("\"preset\":\"pixel8\""),
        contains("\"shape\":\"Circle\""),
        contains("\"approximate\":true"),
        // The Pixel 8's status bar is 48 dp and its gesture bar 24 dp: the insets say so.
        contains("\"top\":48.0"),
        contains("\"bottom\":24.0"),
        serde_json::json!({ "op": "set_display_preset", "preset": "off" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_display_features" }),
        contains("\"preset\":\"off\""),
        contains("\"cutouts\":[]"),
    ]);
    assert_passes(&run(
        "display_preset_round_trip",
        360,
        720,
        false,
        serde_json::Value::Array(steps),
    ));
}

#[test]
fn the_dynamic_island_is_a_pill_at_the_top_and_turns_to_the_left_edge_in_landscape() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "set_display_preset", "preset": "iphone_island" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_display_features" }),
        contains("\"shape\":\"Pill\""),
        contains("\"top\":59.0"),
        contains("\"bottom\":34.0"),
    ]);
    assert_passes(&run(
        "iphone_island_portrait",
        390,
        844,
        false,
        serde_json::Value::Array(steps),
    ));

    let mut landscape = mounted();
    landscape.extend([
        serde_json::json!({ "op": "set_display_preset", "preset": "iphone_island" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_display_features" }),
        contains("\"shape\":\"Pill\""),
        // On its side the island is at the left edge and the insets move to the sides.
        contains("\"left\":59.0"),
        contains("\"right\":59.0"),
        contains("\"status_bar\":null"),
    ]);
    assert_passes(&run(
        "iphone_island_landscape",
        844,
        390,
        false,
        serde_json::Value::Array(landscape),
    ));
}

#[test]
fn set_display_preset_refuses_a_name_that_is_no_preset_and_keeps_the_window_as_it_was() {
    let mut steps = mounted();
    steps.extend([
        serde_json::json!({ "op": "set_display_preset", "preset": "pixel42" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "get_display_features" }),
        contains("\"preset\":\"off\""),
    ]);
    let result = run(
        "display_preset_refuses",
        360,
        720,
        true,
        serde_json::Value::Array(steps),
    );
    let refused = result
        .steps
        .iter()
        .find(|s| s.op == "set_display_preset")
        .expect("the set_display_preset step ran");
    assert_eq!(
        refused.status, "fail",
        "\"pixel42\" is no preset: {refused:#?}"
    );
    let error = refused.error.clone().unwrap_or_default();
    assert!(
        error.contains("pixel8") && error.contains("iphone_island") && error.contains("off"),
        "the refusal names the presets there are, got {error:?}"
    );
    let rest: Vec<_> = result
        .steps
        .iter()
        .filter(|s| s.op != "set_display_preset" && s.status != "pass")
        .collect();
    assert!(
        rest.is_empty(),
        "the window stayed as it was:\n{}",
        failures(&result)
    );
}
