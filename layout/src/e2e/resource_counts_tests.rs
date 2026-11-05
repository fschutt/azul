//! `assert_resource_counts` judges EVERY counter it is given and reports each
//! one that fails, driven through the real op dispatch (`run_e2e_test`) on a
//! headless window.
//!
//! It walked its keys in alphabetical order and returned at the first failure.
//! The renderer-side counters (`fonts`, `images`, ...) are frozen at 0 on a
//! headless window, so a scenario that asserted them next to the live ones
//! (`parsed_fonts`, `font_chain_cache`, `font_hash_to_families`) always ended at
//! the dead one, and `parsed_fonts` - which sorts after both - was never read:
//! a dead counter hid the live ones (batch 2 of `gen-e2e`, note B of its
//! review).

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

/// A page with text in a mock font (so the font counters hold something), a
/// frame to land it, and the resource counters as a baseline.
fn baseline() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "op": "mount",
            "html": ["<body><p id=\"t\">hello</p></body>"],
            "css": ["p { font-family: \"Azul Mock Mono\"; }"],
        }),
        serde_json::json!({ "op": "wait_frame" }),
        serde_json::json!({ "op": "wait", "ms": 100 }),
        serde_json::json!({ "op": "snapshot_resources", "as": "baseline" }),
    ]
}

/// The error of the LAST step (`None`: it passed) of `baseline()` followed by
/// `assertion`.
fn error_of(name: &str, assertion: serde_json::Value) -> Option<String> {
    let mut steps = baseline();
    steps.push(assertion);
    let last = steps.len() - 1;
    let result = run(name, steps);
    let step = result
        .steps
        .iter()
        .find(|s| s.step_index == last)
        .unwrap_or_else(|| panic!("the assertion never ran: {result:#?}"));
    step.error.clone()
}

/// Note B. Nothing changed after the baseline, so `gt` is red for all three:
/// `fonts` and `images` are dead (0 == 0) and `parsed_fonts` is live but flat.
/// The old loop reported `fonts` and stopped.
#[test]
fn note_b_a_dead_counter_does_not_hide_a_live_one() {
    let error = error_of(
        "note_b_all_failures",
        serde_json::json!({
            "op": "assert_resource_counts", "vs": "baseline",
            "fonts": "gt", "images": "gt", "parsed_fonts": "gt",
        }),
    )
    .expect("three counters that did not grow cannot pass `gt`");
    for counter in ["'fonts'", "'images'", "'parsed_fonts'"] {
        assert!(
            error.contains(counter),
            "{counter} is red and must be reported, got: {error}"
        );
    }
}

/// A counter that holds is not on the list, and a single failure reads as it
/// always did.
#[test]
fn note_b_only_the_counters_that_fail_are_reported() {
    let error = error_of(
        "note_b_one_failure",
        serde_json::json!({
            "op": "assert_resource_counts", "vs": "baseline",
            "fonts": "eq", "images": "gt", "parsed_fonts": "eq",
        }),
    )
    .expect("`images` did not grow");
    assert_eq!(
        error,
        "assert_resource_counts: 'images' leaked / drifted from the baseline: expected gt 0, \
         got 0"
    );
}

/// A counter the engine does not have is red too - and it does not hide a
/// counter that failed.
#[test]
fn note_b_an_unknown_counter_is_reported_next_to_a_failing_one() {
    let error = error_of(
        "note_b_unknown",
        serde_json::json!({
            "op": "assert_resource_counts", "vs": "baseline",
            "bogus": "eq", "images": "gt",
        }),
    )
    .expect("`bogus` is no counter and `images` did not grow");
    assert!(error.contains("unknown counter 'bogus'"), "{error}");
    assert!(error.contains("'images'"), "{error}");
}

/// Nothing is red when every counter holds.
#[test]
fn note_b_counters_that_hold_pass() {
    let error = error_of(
        "note_b_all_hold",
        serde_json::json!({
            "op": "assert_resource_counts", "vs": "baseline",
            "fonts": "eq", "images": "eq", "parsed_fonts": "eq", "font_chain_cache": "eq",
        }),
    );
    assert_eq!(error, None);
}
