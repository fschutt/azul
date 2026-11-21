//! `assert_damage_sound` takes `which`, like the other damage assertions: the
//! change may be asked to lie inside the PAINT damage (the rects re-rasterised,
//! the default) or inside the PRESENT damage (what changed on screen), driven
//! through the real op dispatch (`run_e2e_test`) on a headless window.
//!
//! A box a neighbour's change pushes aside is moved by a blit
//! (`cpurender::execute_translate_blit`): its pixels change on screen and are in
//! the present damage, but nothing re-rasterised them, so a coverage check that
//! reads the paint damage calls the frame stale when it is right. Batch 1 of
//! `gen-e2e` found the three scenarios that put an `<img>` or a flex item
//! beside a resized neighbour red for it (R11); `assert_damage_covers_changes`
//! had `which` and `assert_damage_sound` did not, so the one assertion that
//! checks soundness AND tightness could not be used for them.

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

/// Two blue items on a red flex container that fills the window, mounted,
/// landed and snapshotted as `before` (the page of `moved_frame_tests`).
fn two_items_on_red() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "op": "mount",
            "html": ["<div id=\"frame\"><div id=\"flex\"><div class=\"item\" id=\"a\"></div>\
                      <div class=\"item\" id=\"b\"></div></div></div>"],
            "css": ["html, body { margin: 0; width: 100%; height: 100%; }",
                    "#frame { width: 400px; height: 300px; }",
                    "#flex { display: flex; flex-direction: row; justify-content: flex-start; \
                     align-items: stretch; width: 400px; height: 300px; background: red; }",
                    ".item { width: 60px; background: blue; }"],
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "reset_frame_counters" }),
        serde_json::json!({ "op": "snapshot_frame", "as": "before" }),
    ]
}

/// R11: `justify-content: space-between` moves the second item from x=60 to
/// x=340 by a blit. The change - the vacated strip and the item at its new
/// place - lies inside the PRESENT damage; `which: "present"` is how
/// `assert_damage_sound` is asked to judge it. (Before, the op refused the
/// parameter by name, so its soundness AND tightness check was unavailable for
/// any scenario with a displaced box.)
#[test]
fn r11_assert_damage_sound_judges_a_blit_displaced_box_by_the_present_damage() {
    let mut steps = two_items_on_red();
    steps.extend([
        serde_json::json!({
            "op": "set_node_css_override", "selector": "#flex",
            "property": "justify-content", "value": "space-between",
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "assert_damage_sound", "vs": "before", "which": "present" }),
    ]);
    let result = run("r11_damage_sound_present", steps);
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "scenario '{}' failed:\n{}",
        result.name,
        failures(&result)
    );
}

/// R11: `which` is a closed set. A value of `frame` ("last") is not one of
/// `which`, and the op says so by name instead of reading it as `paint`.
#[test]
fn r11_assert_damage_sound_refuses_a_which_outside_the_set_by_name() {
    let mut steps = two_items_on_red();
    steps.extend([
        serde_json::json!({
            "op": "set_node_css_override", "selector": "#flex",
            "property": "justify-content", "value": "space-between",
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_damage_sound", "vs": "before", "which": "last" }),
    ]);
    let result = run("r11_damage_sound_which_last", steps);
    let refusal = result
        .steps
        .iter()
        .find(|s| s.op == "assert_damage_sound")
        .expect("the assertion step ran");
    assert_eq!(refusal.status, "fail", "a value outside the set is refused");
    let error = refusal.error.clone().unwrap_or_default();
    assert!(
        error.contains("assert_damage_sound: which is \"last\"")
            && error.contains("paint, present"),
        "the refusal names the op, the value and the set: {error}"
    );
}
