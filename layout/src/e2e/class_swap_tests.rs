//! `set_node_classes` changes the DOM class AND the cascade: the new class's
//! rules apply, driven through the real op dispatch (`run_e2e_test`) on a
//! headless window.
//!
//! Batch 3's probes `s1`-`s4` found a colour-only class swap (`.r` -> `.b`) to be
//! a DEAD CONTROL while `min_layout_passes: 1` passes: layout ran, the cascade
//! did not. Both `CallbackChange::SetNodeIdsAndClasses` arms (`runner.rs` and
//! the shell's `common/event.rs`) only called `NodeData::set_ids_and_classes`;
//! nothing re-matched the retained author sheet, so the node kept the styles of
//! its old class. Both now call `LayoutWindow::set_node_ids_and_classes`, which
//! re-runs the cascade from the retained sheet (`StyledDom::restyle_retained`,
//! the call the media-query path and an inserted node make) - `:hover` by the
//! state the node is in, a user override still on top - and lays the window out
//! cold, because a class on one node can reach the boxes around it
//! (`.dark .row`). The classes a node already has are no change at all.
//!
//! The window-level cases (a scoped `with_css` sheet, the cost in cascade
//! generations and cold layouts) are in
//! `layout/tests/a_class_swap_applies_the_rules_of_the_new_class.rs`.

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

fn props(selector: &str) -> serde_json::Value {
    serde_json::json!({ "op": "get_node_css_properties", "selector": selector })
}

fn response_contains(fragment: &str) -> serde_json::Value {
    serde_json::json!({ "op": "assert_response", "contains": fragment })
}

fn response_lacks(fragment: &str) -> serde_json::Value {
    serde_json::json!({ "op": "assert_response", "not_contains": fragment })
}

/// One box under two colour classes (and a hover rule on the second), mounted
/// and landed.
fn coloured_box() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "op": "mount",
            "html": ["<div id=\"box\" class=\"r\"></div>"],
            "css": ["html, body { margin: 0; }",
                    "#box { width: 200px; height: 100px; }",
                    ".r { background-color: #ff0000; }",
                    ".b { background-color: #0000ff; }",
                    ".b:hover { background-color: #00ff00; }"],
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
    ]
}

/// R-classes: a class swap whose only difference is a colour must change a pixel.
#[test]
fn r_classes_a_colour_only_class_swap_changes_a_pixel() {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": "r_classes_colour_swap",
        "config": { "continue_on_failure": true },
        "setup": { "window_width": 400, "window_height": 300, "dpi": 96 },
        "steps": [
            { "op": "mount",
              "html": ["<div id=\"box\" class=\"r\"></div>"],
              "css": ["html, body { margin: 0; }",
                      "#box { width: 200px; height: 100px; }",
                      ".r { background-color: red; }",
                      ".b { background-color: blue; }"] },
            { "op": "wait_frame" },
            { "op": "wait", "ms": 100 },
            { "op": "reset_frame_counters" },
            { "op": "snapshot_frame", "as": "before" },
            { "op": "set_node_classes", "selector": "#box", "classes": ["b"] },
            { "op": "wait_frame" },
            { "op": "wait", "ms": 100 },
            { "op": "assert_changed", "vs": "before", "min_damage_rects": 1 },
        ],
    }))
    .expect("the scenario literal is a valid E2eTest");
    let result = run_e2e_test(&test);
    assert!(
        result.status == "pass" && result.steps_failed == 0,
        "the class swap was a dead control: {:?}",
        result
            .steps
            .iter()
            .filter(|s| s.status != "pass")
            .map(|s| s.error.clone().unwrap_or_default())
            .collect::<Vec<_>>()
    );
}

/// R-classes: what the node RESOLVES to follows its class, not only the pixels:
/// the rule of the class it has now, and none of the class it lost.
#[test]
fn r_classes_the_resolved_style_follows_the_class() {
    let mut steps = coloured_box();
    steps.extend([
        props("#box"),
        response_contains("background-color: #ff0000ff"),
        serde_json::json!({ "op": "set_node_classes", "selector": "#box", "classes": ["b"] }),
        serde_json::json!({ "op": "wait_frame" }),
        props("#box"),
        response_contains("background-color: #0000ffff"),
        response_lacks("background-color: #ff0000ff"),
        // A class no rule names: neither colour is left.
        serde_json::json!({ "op": "set_node_classes", "selector": "#box", "classes": ["x"] }),
        serde_json::json!({ "op": "wait_frame" }),
        props("#box"),
        response_lacks("background-color: #0000ffff"),
        response_lacks("background-color: #ff0000ff"),
    ]);
    assert_passes(&run("r_classes_resolved_style", steps));
}

/// R-classes: the `:hover` rule of the class the node has now is the one the
/// pointer finds - the hover restyle reads the rules the swap matched.
#[test]
fn r_classes_the_pointer_finds_the_hover_rule_of_the_new_class() {
    let mut steps = coloured_box();
    steps.extend([
        serde_json::json!({ "op": "set_node_classes", "selector": "#box", "classes": ["b"] }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "reset_frame_counters" }),
        serde_json::json!({ "op": "snapshot_frame", "as": "swapped" }),
        serde_json::json!({ "op": "mouse_move", "selector": "#box" }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        props("#box"),
        response_contains("background-color: #00ff00ff"),
        // ... and it is on the screen, not only in the cascade.
        serde_json::json!({ "op": "assert_changed", "vs": "swapped", "min_damage_rects": 1 }),
    ]);
    assert_passes(&run("r_classes_hover_rule", steps));
}

/// R-classes: a class on a PARENT reaches the boxes below it through a descendant
/// rule. The row's own data did not change, so a warm reconcile would hand it its
/// old box.
#[test]
fn r_classes_a_class_on_the_parent_resizes_the_row_below_it() {
    let steps = vec![
        serde_json::json!({
            "op": "mount",
            "html": ["<div id=\"list\" class=\"light\"><div id=\"row\"></div></div>"],
            "css": ["html, body { margin: 0; }",
                    ".light > div { height: 20px; }",
                    ".dark > div { height: 50px; }"],
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "assert_layout", "selector": "#row",
                            "property": "height", "expected": 20 }),
        serde_json::json!({ "op": "set_node_classes", "selector": "#list", "classes": ["dark"] }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "assert_layout", "selector": "#row",
                            "property": "height", "expected": 50 }),
    ];
    assert_passes(&run("r_classes_descendant_rule", steps));
}

/// R-classes: what a class swap costs. A swap to another class lays out once
/// (the generator prompt's "one pass per override, like `set_node_classes`");
/// writing the classes the node already has lays nothing out and repaints
/// nothing - the swap used to cost its pass whether or not anything changed.
#[test]
fn r_classes_a_swap_costs_one_layout_pass_and_the_same_classes_none() {
    let mut steps = coloured_box();
    steps.extend([
        serde_json::json!({ "op": "reset_frame_counters" }),
        serde_json::json!({ "op": "set_node_classes", "selector": "#box", "classes": ["b"] }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_work_bounded", "exact_layout_passes": 1 }),
        // The classes it has: nothing to do.
        serde_json::json!({ "op": "reset_frame_counters" }),
        serde_json::json!({ "op": "snapshot_frame", "as": "settled" }),
        serde_json::json!({ "op": "set_node_classes", "selector": "#box", "classes": ["b"] }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "assert_work_bounded", "exact_layout_passes": 0 }),
        serde_json::json!({ "op": "assert_damage", "kind": "none" }),
        serde_json::json!({ "op": "assert_idle_stable", "vs": "settled" }),
    ]);
    assert_passes(&run("r_classes_cost", steps));
}
