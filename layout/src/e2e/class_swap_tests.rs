//! `set_node_classes` changes the DOM class; the new class's rules must apply.
//!
//! Batch 3's probes `s1`-`s4` found a colour-only class swap (`.r` -> `.b`) to be
//! a DEAD CONTROL while `min_layout_passes: 1` passes: layout runs, the cascade
//! does not. Finding (read, not run): both `CallbackChange::SetNodeIdsAndClasses`
//! arms (`runner.rs` and the shell's `common/event.rs`) only call
//! `NodeData::set_ids_and_classes`; nothing re-matches the retained author sheet
//! (`StyledDom::restyle_retained`, which only `insert_runtime_child` and the
//! media-query path call), so the node keeps the styles of its old class. The
//! shells have the same gap, so this is an engine bug, not a runner port gap.
//! It is not fixed here because the fix needs a decision no blind lane can
//! verify: whether `restyle_retained` is cheap and correct for every retained
//! sheet (scoped sheets, `:hover` state, user overrides) - see the report.

use super::{run_e2e_test, E2eTest};

/// R-classes: a class swap whose only difference is a colour must change a pixel.
#[test]
#[ignore = "R-classes: set_node_classes does not re-run the cascade (batch-3 probes s1-s4); \
            fix = StyledDom::restyle_retained in both SetNodeIdsAndClasses arms, unverified"]
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
