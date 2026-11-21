//! A string param that takes one of a CLOSED set of values refuses any other
//! value, by name, instead of falling back to its default.
//!
//! `which` (`"paint"` | `"present"`) and `frame` (`"accumulated"` | `"last"`) of
//! the damage ops used to be read with a `match` whose `_` arm was the default:
//! `{"which": "last"}` - `"last"` is a value of `frame` - was silently the paint
//! damage, and the assertion judged something the scenario did not say. The
//! first `gen-e2e` batch wrote exactly that, and the step went on to pass or fail
//! for the wrong reason. The same held for `scroll_into_view`'s `block` /
//! `inline` / `behavior`.

use super::{
    full::{
        closed_param_values, closed_value, DAMAGE_FRAME_VALUES, DAMAGE_WHICH_VALUES,
        SCROLL_ALIGN_VALUES, SCROLL_BEHAVIOR_VALUES,
    },
    run_e2e_test, E2eTest, E2eTestResult,
};

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

/// A document, a frame, a baseline snapshot, then `ops`.
fn after_a_frame(ops: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut steps = vec![
        serde_json::json!({ "op": "mount", "html": ["<body><div id=\"a\"></div></body>"],
                            "css": ["#a { width: 60px; height: 40px; background: #0000ff; }"] }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "reset_frame_counters" }),
        serde_json::json!({ "op": "snapshot_frame", "as": "before" }),
    ];
    steps.extend(ops);
    steps
}

/// The error of step `index`, which must have failed.
fn refusal(result: &E2eTestResult, index: usize) -> String {
    let step = result
        .steps
        .iter()
        .find(|s| s.step_index == index)
        .unwrap_or_else(|| panic!("step {index} never ran"));
    assert_eq!(
        step.status, "fail",
        "step {index} `{}` must be REFUSED, but it passed: {:?}",
        step.op, step.logs
    );
    step.error.clone().unwrap_or_default()
}

fn passes(result: &E2eTestResult, index: usize) {
    let step = result
        .steps
        .iter()
        .find(|s| s.step_index == index)
        .unwrap_or_else(|| panic!("step {index} never ran"));
    assert_eq!(
        step.status, "pass",
        "step {index} `{}` must pass, got {:?}",
        step.op, step.error
    );
}

#[test]
fn a_damage_op_refuses_a_which_or_frame_outside_its_closed_set() {
    // Every op that reads the damage through `damage_of`.
    let ops = [
        serde_json::json!({ "op": "assert_damage", "min_rects": 0 }),
        serde_json::json!({ "op": "assert_changed", "vs": "before", "min_damage_rects": 0 }),
        serde_json::json!({ "op": "assert_damage_covers_changes", "vs": "before" }),
        serde_json::json!({ "op": "assert_damage_incremental" }),
    ];
    let mut steps = Vec::new();
    for op in &ops {
        // "last" is a value of `frame`, not of `which`.
        let mut wrong_which = op.clone();
        wrong_which["which"] = serde_json::json!("last");
        steps.push(wrong_which);
        let mut wrong_frame = op.clone();
        wrong_frame["frame"] = serde_json::json!("bogus");
        steps.push(wrong_frame);
        let mut not_a_string = op.clone();
        not_a_string["which"] = serde_json::json!(true);
        steps.push(not_a_string);
    }
    let first = after_a_frame(Vec::new()).len();
    let result = run("closed_values_damage", after_a_frame(steps));
    for (n, op) in ops.iter().enumerate() {
        let name = op["op"].as_str().unwrap_or_default();
        let which = refusal(&result, first + n * 3);
        assert!(
            which.contains(name)
                && which.contains("which")
                && which.contains("\"last\"")
                && which.contains("paint, present"),
            "{name}: {which}"
        );
        let frame = refusal(&result, first + n * 3 + 1);
        assert!(
            frame.contains(name)
                && frame.contains("frame")
                && frame.contains("\"bogus\"")
                && frame.contains("accumulated, last"),
            "{name}: {frame}"
        );
        let flag = refusal(&result, first + n * 3 + 2);
        assert!(
            flag.contains(name) && flag.contains("which") && flag.contains("paint, present"),
            "{name}: {flag}"
        );
    }
}

#[test]
fn a_damage_op_takes_every_value_of_its_closed_sets() {
    let mut steps = Vec::new();
    for which in ["paint", "present"] {
        for frame in ["accumulated", "last"] {
            steps.push(serde_json::json!({ "op": "assert_damage", "min_rects": 0,
                                           "which": which, "frame": frame }));
        }
    }
    let first = after_a_frame(Vec::new()).len();
    let n = steps.len();
    let result = run("closed_values_accepted", after_a_frame(steps));
    for i in 0..n {
        passes(&result, first + i);
    }
}

#[test]
fn capture_damage_png_refuses_a_which_outside_its_closed_set() {
    let first = after_a_frame(Vec::new()).len();
    let result = run(
        "closed_values_capture",
        after_a_frame(vec![serde_json::json!({
            "op": "capture_damage_png", "path": "target/e2e/_closed_values_never_written.png",
            "which": "last"
        })]),
    );
    let error = refusal(&result, first);
    assert!(
        error.contains("capture_damage_png")
            && error.contains("\"last\"")
            && error.contains("paint, present"),
        "{error}"
    );
    assert!(
        !std::path::Path::new("target/e2e/_closed_values_never_written.png").exists(),
        "a refused capture writes no file"
    );
}

#[test]
fn scroll_into_view_refuses_an_alignment_or_behavior_outside_its_closed_set() {
    let steps = vec![
        serde_json::json!({ "op": "scroll_into_view", "selector": "#a", "block": "middle" }),
        serde_json::json!({ "op": "scroll_into_view", "selector": "#a", "inline": "left" }),
        serde_json::json!({ "op": "scroll_into_view", "selector": "#a", "behavior": "fast" }),
        // every value the op documents is still taken
        serde_json::json!({ "op": "scroll_into_view", "selector": "#a", "block": "center",
                            "inline": "end", "behavior": "instant" }),
        serde_json::json!({ "op": "scroll_into_view", "selector": "#a", "block": "nearest",
                            "inline": "start", "behavior": "smooth" }),
        serde_json::json!({ "op": "scroll_into_view", "selector": "#a" }),
    ];
    let first = after_a_frame(Vec::new()).len();
    let result = run("closed_values_scroll", after_a_frame(steps));
    let block = refusal(&result, first);
    assert!(
        block.contains("scroll_into_view")
            && block.contains("block")
            && block.contains("\"middle\"")
            && block.contains("nearest, start, center, end"),
        "{block}"
    );
    let inline = refusal(&result, first + 1);
    assert!(
        inline.contains("inline") && inline.contains("\"left\""),
        "{inline}"
    );
    let behavior = refusal(&result, first + 2);
    assert!(
        behavior.contains("behavior")
            && behavior.contains("\"fast\"")
            && behavior.contains("auto, instant, smooth"),
        "{behavior}"
    );
    for i in 3..6 {
        passes(&result, first + i);
    }
}

/// ONE definition: the sets the ops accept are the sets `gen-e2e`'s gate reads.
#[test]
fn the_closed_sets_are_one_definition_for_the_engine_and_the_gate() {
    assert_eq!(closed_param_values("which"), Some(DAMAGE_WHICH_VALUES));
    assert_eq!(closed_param_values("frame"), Some(DAMAGE_FRAME_VALUES));
    assert_eq!(closed_param_values("block"), Some(SCROLL_ALIGN_VALUES));
    assert_eq!(closed_param_values("inline"), Some(SCROLL_ALIGN_VALUES));
    assert_eq!(
        closed_param_values("behavior"),
        Some(SCROLL_BEHAVIOR_VALUES)
    );
    // A param that is not a closed set says nothing.
    for open in ["selector", "text", "path", "kind", "x"] {
        assert_eq!(closed_param_values(open), None, "{open}");
    }
    assert_eq!(DAMAGE_WHICH_VALUES, ["paint", "present"]);
    assert_eq!(DAMAGE_FRAME_VALUES, ["accumulated", "last"]);

    assert_eq!(
        closed_value("assert_damage", "which", &serde_json::json!("present")),
        Ok("present")
    );
    let refused = closed_value("assert_damage", "which", &serde_json::json!("last"))
        .expect_err("\"last\" is a value of `frame`");
    assert!(
        refused.contains("assert_damage")
            && refused.contains("which")
            && refused.contains("\"last\"")
            && refused.contains("paint, present"),
        "{refused}"
    );
    // Not a string at all.
    assert!(closed_value("assert_damage", "frame", &serde_json::json!(1)).is_err());
}
