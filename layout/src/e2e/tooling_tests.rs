//! The E2E tooling follow-ups (task E1), pinned through the REAL dispatcher
//! on a headless window: the per-platform gate (`only_on`) and the SKIP
//! verdict it produces.
//!
//! A child of `runner`, so the tests that need the finished window (the
//! managers a scenario left behind) can use `run_e2e_test_keeping_runner`.

use crate::e2e::{load_e2e_tests, render_report, run_e2e_test, E2eTest, E2eTestResult};

// ==== the platform gate (`only_on`) ====

/// Every host name the gate knows, as a scenario spells it.
const KNOWN_PLATFORMS: &[&str] = &["linux", "windows", "macos", "ios", "android", "web"];

/// The name this test binary's host goes by in a scenario.
fn this_host() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web"
    } else {
        std::env::consts::OS
    }
}

/// A scenario whose one step FAILS, so a verdict of "skip" proves no step
/// ran and a verdict of "fail" proves the steps did.
fn gated(name: &str, only_on: serde_json::Value) -> E2eTest {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "only_on": only_on,
        "steps": [
            { "op": "mount", "html": ["<div id=\"here\"></div>"] },
            { "op": "wait_frame" },
            { "op": "assert_exists", "selector": "#not-here" }
        ]
    }))
    .expect("the scenario literal is a valid E2eTest")
}

/// A result as the dispatcher reports it, from JSON (the shape every host
/// serialises).
fn result(value: serde_json::Value) -> E2eTestResult {
    serde_json::from_value(value).expect("the result literal is a valid E2eTestResult")
}

/// `global_hotkey.json` spelled the Linux / Windows accelerator and failed on
/// every Mac host. A scenario that holds only on some platforms says so,
/// and on the others it is SKIPPED - with the reason - rather than run
/// (a red it does not deserve) or passed (a green it did not earn).
#[test]
fn a_test_gated_to_other_platforms_is_skipped_with_the_reason_and_runs_no_step() {
    let others: Vec<&str> = KNOWN_PLATFORMS
        .iter()
        .copied()
        .filter(|p| *p != this_host())
        .collect();
    let test = gated("gated_elsewhere", serde_json::json!(others));

    let result = run_e2e_test(&test);

    assert_eq!(
        result.status, "skip",
        "gated to {others:?} on {}: {:#?}",
        this_host(),
        result.steps
    );
    assert!(result.steps.is_empty(), "no step ran: {:#?}", result.steps);
    let json = serde_json::to_value(&result).expect("a result serialises");
    let reason = json["skip_reason"].as_str().unwrap_or_default();
    assert!(
        reason.contains(this_host()) && reason.contains(others[0]),
        "the reason names this host and where the test runs, got {reason:?}"
    );
}

/// The gate lets the test through on a listed host: its steps run, so the
/// failing one fails it.
#[test]
fn a_test_gated_to_this_platform_runs_its_steps() {
    if !KNOWN_PLATFORMS.contains(&this_host()) {
        return; // a host the gate has no name for (a BSD) is never listed
    }
    let test = gated("gated_here", serde_json::json!([this_host()]));

    let result = run_e2e_test(&test);

    assert_eq!(result.status, "fail", "the steps ran: {:#?}", result.steps);
}

/// A typo in the gate ("macOS") must not skip the test on every host
/// forever: an unknown platform name FAILS it, naming the known ones.
#[test]
fn an_unknown_platform_name_fails_the_test_instead_of_skipping_it_everywhere() {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": "gated_by_a_typo",
        "only_on": ["macOS"],
        "steps": [ { "op": "wait_frame" } ]
    }))
    .expect("scenario json");

    let result = run_e2e_test(&test);

    assert_eq!(result.status, "fail", "{:#?}", result.steps);
    let error = result
        .steps
        .iter()
        .find_map(|s| s.error.clone())
        .unwrap_or_default();
    assert!(
        error.contains("macOS") && error.contains("macos"),
        "the failure names the typo and the known spelling, got {error:?}"
    );
}

/// `"only_on": []` would run nowhere: that is a mistake, not a gate.
#[test]
fn an_empty_platform_gate_fails_the_test() {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": "gated_to_nowhere",
        "only_on": [],
        "steps": [ { "op": "wait_frame" } ]
    }))
    .expect("scenario json");

    let result = run_e2e_test(&test);

    assert_eq!(result.status, "fail", "{:#?}", result.steps);
}

/// The verdict tally counts a skip as a SKIP: not a pass, not a failure,
/// with its reason on the line, and the gate stays green.
#[test]
fn a_skipped_result_is_reported_as_a_skip_and_leaves_the_gate_green() {
    let tests = vec![gated("gated_elsewhere", serde_json::json!(["windows"]))];
    let results = vec![result(serde_json::json!({
        "name": "gated_elsewhere",
        "status": "skip",
        "skip_reason": "only on windows; this host is linux",
        "duration_ms": 0,
        "step_count": 3,
        "steps_passed": 0,
        "steps_failed": 0,
        "steps": []
    }))];

    let (report, verdict) = render_report(&tests, &results);

    assert!(!verdict.gate_failed(), "a skip does not fail the gate:\n{report}");
    assert_eq!(verdict.passed, 0, "a skip is not a pass:\n{report}");
    assert!(
        report.contains("SKIP") && report.contains("only on windows; this host is linux"),
        "the line says SKIP and why:\n{report}"
    );
    assert!(
        report.contains("0 passed; 0 failed; 0 xfailed; 0 xpassed; 1 skipped"),
        "the summary counts it as skipped:\n{report}"
    );
}

/// A skipped test carrying `"expect": "fail"` did not run, so it neither
/// failed as expected (XFAIL) nor passed unexpectedly (XPASS).
#[test]
fn a_skipped_test_marked_expect_fail_is_still_a_skip() {
    let mut test = gated("known_bad_elsewhere", serde_json::json!(["windows"]));
    test.expect = Some("fail".to_string());
    let results = vec![result(serde_json::json!({
        "name": "known_bad_elsewhere",
        "status": "skip",
        "skip_reason": "only on windows; this host is linux",
        "duration_ms": 0,
        "step_count": 3,
        "steps_passed": 0,
        "steps_failed": 0,
        "steps": []
    }))];

    let (report, verdict) = render_report(&[test], &results);

    assert_eq!(verdict.xfail, 0, "not an expected failure:\n{report}");
    assert!(!verdict.gate_failed(), "{report}");
    assert!(report.contains("1 skipped"), "{report}");
}

/// The demo's hotkey scenario comes in one variant per desktop host, each
/// gated to its host and spelling that host's accelerator: `Cmd+Shift+K` on
/// a Mac, `Ctrl+Alt+K` on Linux and Windows (`examples/azul-widgets/src/
/// hotkeys.rs`). It is an ARRAY file, which the shared loader reads.
#[test]
fn the_widgets_hotkey_scenario_has_exactly_one_variant_per_desktop_host() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../examples/azul-widgets/e2e/global_hotkey.json");
    let tests = load_e2e_tests(&path)
        .unwrap_or_else(|e| panic!("the scenario file loads: {e}"));

    for (host, accelerator) in [
        ("linux", "Ctrl+Alt+K"),
        ("windows", "Ctrl+Alt+K"),
        ("macos", "Cmd+Shift+K"),
    ] {
        let variants: Vec<&E2eTest> = tests
            .iter()
            .filter(|t| {
                let json = serde_json::to_value(t).expect("a test serialises");
                json["only_on"]
                    .as_array()
                    .is_some_and(|hosts| hosts.iter().any(|h| h.as_str() == Some(host)))
            })
            .collect();
        assert_eq!(variants.len(), 1, "exactly one variant runs on {host}");
        let steps = serde_json::to_string(&variants[0].steps).expect("steps serialise");
        assert!(
            steps.contains(accelerator),
            "the {host} variant spells {accelerator}"
        );
    }
}
