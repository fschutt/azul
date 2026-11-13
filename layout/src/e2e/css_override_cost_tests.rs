//! What a `set_node_css_override` costs in layout passes, driven through the
//! real op dispatch (`run_e2e_test`) on a headless window. The counter is
//! `FrameReport::layout_passes`: how many times layout ACTUALLY ran.
//!
//! R3 - a geometry override cost TWO passes in this runner and one in the
//! shells. The content chokepoint laid the window out itself and returned the
//! `Relayout` tier, and the runner laid it out again for the tier; a shell
//! writes the property (`apply_content_change_for_host`) and lays out once, for
//! the tier. The second pass also diffed against the display list the first had
//! cached, so a change the first pass had already taken in looked like no change
//! to the second: the stale frame of batch 2, line 27 (`justify-content`).
//!
//! R2 - re-setting the value a node already resolves to relaid the window out
//! all the same (`apply_node_css_change` had no old-versus-new guard, unlike
//! `apply_node_style_change`'s `*old == new_style`). The write is still
//! recorded - it pins the value against a later `:hover` or cascade change - but
//! there is nothing to lay out or paint.

use super::{run_e2e_test, E2eTest, E2eTestResult};

/// Run `steps` as one headless scenario on a 400x300 window. A red step does
/// not end the run, so every assertion is read.
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

/// The page the cost tests mount: a flex row with a box and a sibling. Each
/// property a no-op test re-sets is declared by the stylesheet, so "the value it
/// already has" is the cascade's.
fn page() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "op": "mount",
            "html": ["<body><div id=\"root\"><div id=\"box\"></div><div id=\"b\"></div></div></body>"],
            "css": [
                "html, body { margin: 0; padding: 0; }",
                "#root { display: flex; flex-direction: row; justify-content: flex-start; \
                 align-items: stretch; width: 400px; height: 300px; }",
                "#box { display: block; width: 100px; height: 80px; padding: 0px; margin: 0px; \
                 border: 2px solid #ff0000; flex-grow: 0; overflow: visible; position: static; \
                 visibility: visible; background: #0000ff; }",
                "#b { width: 50px; height: 50px; background: #00ff00; }",
            ],
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
    ]
}

/// `page()`, the counters zeroed, `overrides` applied (each one a
/// `set_node_css_override`), a frame to land them, and the cost read.
fn cost_of(
    name: &str,
    overrides: &[(&str, &str, &str)],
    bound: serde_json::Value,
) -> E2eTestResult {
    let mut steps = page();
    steps.push(serde_json::json!({ "op": "reset_frame_counters" }));
    for (selector, property, value) in overrides {
        steps.push(serde_json::json!({
            "op": "set_node_css_override", "selector": selector,
            "property": property, "value": value,
        }));
    }
    steps.push(serde_json::json!({ "op": "wait_frame" }));
    steps.push(with(
        serde_json::json!({ "op": "assert_work_bounded" }),
        &bound,
    ));
    run(name, steps)
}

/// `base` with the fields of `extra` added.
fn with(mut base: serde_json::Value, extra: &serde_json::Value) -> serde_json::Value {
    let fields = base.as_object_mut().expect("a step is an object");
    for (k, v) in extra.as_object().expect("the bound is an object") {
        fields.insert(k.clone(), v.clone());
    }
    base
}

/// R3. A geometry override lays the window out ONCE, as in a shell. The runner
/// reported 2: `exact_layout_passes: 1` failed with "got 2".
#[test]
fn r3_a_geometry_override_lays_the_window_out_once() {
    assert_passes(&cost_of(
        "r3_geometry_once",
        &[("#box", "width", "300px")],
        serde_json::json!({ "exact_layout_passes": 1 }),
    ));
}

/// R3, the other rows of the table: a paint-only override rebuilds the display
/// list and lays nothing out.
#[test]
fn r3_a_paint_only_override_lays_nothing_out() {
    assert_passes(&cost_of(
        "r3_paint_only",
        &[("#box", "background-color", "#ff0000")],
        serde_json::json!({ "exact_layout_passes": 0 }),
    ));
}

/// R3, the decisive experiment of the batch-2 review (line 27): a
/// `justify-content` override moved the second item to x=350 - a fresh render
/// differed by 30,000 px - and no paint or present damage was reported, so the
/// window kept showing the old frame. The second layout pass diffed against the
/// display list the first had cached. With one layout the change reaches the
/// frame.
#[test]
fn r3_a_justify_content_override_repaints_what_it_moved() {
    let mut steps = vec![
        serde_json::json!({
            "op": "mount",
            "html": ["<body><div id=\"flex\"><div id=\"a\"></div><div id=\"b\"></div></div></body>"],
            "css": [
                "html, body { margin: 0; padding: 0; }",
                "#flex { display: flex; flex-direction: row; justify-content: flex-start; \
                 width: 400px; height: 300px; background: #ff0000; }",
                "#a, #b { width: 50px; height: 100px; background: #0000ff; }",
            ],
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "reset_frame_counters" }),
        serde_json::json!({ "op": "snapshot_frame", "as": "before" }),
    ];
    steps.push(serde_json::json!({
        "op": "set_node_css_override", "selector": "#flex", "property": "justify-content",
        "value": "space-between",
    }));
    steps.push(serde_json::json!({ "op": "wait_frame" }));
    steps.push(serde_json::json!({ "op": "wait", "ms": 100 }));
    steps.push(serde_json::json!({ "op": "assert_work_bounded", "min_layout_passes": 1 }));
    steps
        .push(serde_json::json!({ "op": "assert_changed", "vs": "before", "min_damage_rects": 1 }));
    assert_passes(&run("r3_justify_content", steps));
}

/// R2. Re-setting the value the node already has costs no layout pass, and
/// repaints nothing.
#[test]
fn r2_re_setting_the_value_a_node_already_has_lays_nothing_out() {
    let mut steps = page();
    steps.push(serde_json::json!({ "op": "reset_frame_counters" }));
    steps.push(serde_json::json!({ "op": "snapshot_frame", "as": "before" }));
    steps.push(serde_json::json!({
        "op": "set_node_css_override", "selector": "#box", "property": "width", "value": "100px",
    }));
    steps.push(serde_json::json!({ "op": "wait_frame" }));
    steps.push(serde_json::json!({ "op": "assert_work_bounded", "exact_layout_passes": 0 }));
    steps.push(serde_json::json!({ "op": "assert_damage", "kind": "none" }));
    steps.push(serde_json::json!({ "op": "assert_idle_stable", "vs": "before" }));
    assert_passes(&run("r2_same_width", steps));
}

/// R2, for every property a one-liner of the corpus re-sets: the value the
/// stylesheet declares is the value the node has, shorthands included.
#[test]
fn r2_re_setting_each_declared_value_costs_no_layout_pass() {
    let declared: [(&str, &str, &str); 14] = [
        ("#root", "display", "flex"),
        ("#root", "flex-direction", "row"),
        ("#root", "justify-content", "flex-start"),
        ("#root", "align-items", "stretch"),
        ("#box", "display", "block"),
        ("#box", "width", "100px"),
        ("#box", "height", "80px"),
        ("#box", "padding", "0px"),
        ("#box", "margin", "0px"),
        ("#box", "border-width", "2px"),
        ("#box", "flex-grow", "0"),
        ("#box", "overflow", "visible"),
        ("#box", "position", "static"),
        ("#box", "visibility", "visible"),
    ];
    let mut red = Vec::new();
    for (selector, property, value) in declared {
        let result = cost_of(
            "r2_declared_value",
            &[(selector, property, value)],
            serde_json::json!({ "exact_layout_passes": 0 }),
        );
        if result.status != "pass" {
            red.push(format!(
                "{selector} {property}: {value}\n{}",
                failures(&result)
            ));
        }
    }
    assert!(
        red.is_empty(),
        "re-setting a declared value must lay nothing out:\n{}",
        red.join("\n")
    );
}

/// R2: a real change after a no-op still costs its pass, and a no-op does not
/// leave the window thinking the value moved.
#[test]
fn r2_a_real_change_after_a_no_op_still_lays_out() {
    assert_passes(&cost_of(
        "r2_then_real",
        &[("#box", "width", "100px"), ("#box", "width", "300px")],
        serde_json::json!({ "exact_layout_passes": 1 }),
    ));
}

/// R2: the no-op write is still RECORDED. The override pins the width against a
/// later `:hover` rule (a user override outranks every declaration), so the
/// pointer entering the box does not widen it. A fix that dropped the write
/// would let `#hov:hover { width: 200px }`-style rules win again.
#[test]
fn r2_the_no_op_write_still_pins_the_value() {
    let mut steps = vec![
        serde_json::json!({
            "op": "mount",
            "html": ["<body><div id=\"hov\"></div></body>"],
            "css": [
                "html, body { margin: 0; padding: 0; }",
                "#hov { width: 100px; height: 80px; background: #0000ff; }",
                "#hov:hover { width: 200px; }",
            ],
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({
            "op": "set_node_css_override", "selector": "#hov", "property": "width",
            "value": "100px",
        }),
        serde_json::json!({ "op": "wait_frame" }),
    ];
    steps.push(serde_json::json!({ "op": "mouse_move", "selector": "#hov" }));
    steps.push(serde_json::json!({ "op": "wait_frame" }));
    steps.push(serde_json::json!({ "op": "wait", "ms": 100 }));
    steps.push(serde_json::json!({
        "op": "assert_layout", "selector": "#hov", "property": "width", "expected": 100,
    }));
    assert_passes(&run("r2_pin", steps));
}
