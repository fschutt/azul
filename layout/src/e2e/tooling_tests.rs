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

/// `AZ_E2E=<directory>` runs each file in a child process and tallied only
/// the child's exit code, so a file whose tests were all SKIPPED printed
/// "ok": a pass it did not earn. The parent reads the child's summary line
/// back into the verdict `render_report` wrote.
#[test]
fn a_summary_line_reads_back_into_the_verdict_that_wrote_it() {
    use crate::e2e::E2eVerdict;

    let tests = vec![
        gated("skipped_here", serde_json::json!(["windows"])),
        gated("failed_here", serde_json::json!(["linux"])),
    ];
    let results = vec![
        result(serde_json::json!({
            "name": "skipped_here",
            "status": "skip",
            "skip_reason": "only on windows; this host is linux",
            "duration_ms": 0,
            "step_count": 3,
            "steps_passed": 0,
            "steps_failed": 0,
            "steps": []
        })),
        result(serde_json::json!({
            "name": "failed_here",
            "status": "fail",
            "duration_ms": 4,
            "step_count": 3,
            "steps_passed": 2,
            "steps_failed": 1,
            "steps": []
        })),
    ];
    let (report, verdict) = render_report(&tests, &results);
    let line = report
        .lines()
        .rev()
        .find(|l| l.contains("test result:"))
        .expect("the report ends in a summary line");

    assert_eq!(E2eVerdict::parse_summary(line), Some(verdict), "{line}");
    assert_eq!(E2eVerdict::parse_summary("running 2 tests"), None);
}

// ==== the notification op and `assert_notification`'s payload ====

/// The notification tests share the process-wide mailbox and recorder.
static NOTIFICATION_GLOBALS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn notification_globals() -> std::sync::MutexGuard<'static, ()> {
    NOTIFICATION_GLOBALS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A scenario that runs every step, so each step's own verdict is visible.
fn every_step(name: &str, steps: serde_json::Value) -> E2eTest {
    serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": true },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest")
}

fn step_errors(result: &E2eTestResult) -> Vec<String> {
    result.steps.iter().filter_map(|s| s.error.clone()).collect()
}

/// A scenario could post a notification and assert it was shown, but never
/// click it: no op put an event in front of the notification's callback.
/// `notification_event` queues one into the mailbox the platform backends
/// post to, so the dll's pump routes it to the callback (with the payload
/// the post carried, when the event brings none) - the path a real click
/// takes after the OS.
#[test]
fn the_notification_event_op_queues_a_click_an_action_and_a_dismissal_with_their_payload() {
    use azul_core::notification::NotificationEventType;
    use crate::managers::notification::drain_notification_events;

    let _globals = notification_globals();
    drop(drain_notification_events());

    let result = run_e2e_test(&every_step(
        "notification_events",
        serde_json::json!([
            { "op": "notification_event", "id": "e1-click", "kind": "click", "payload": "doc-1" },
            { "op": "notification_event", "id": "e1-action", "kind": "action",
              "action": "open", "payload": "doc-2" },
            { "op": "notification_event", "id": "e1-dismiss", "kind": "dismiss",
              "reason": "closed by the user" },
            { "op": "notification_event", "id": "e1-fail", "kind": "failed",
              "reason": "no notification server" },
            { "op": "notification_event", "id": "e1-launch", "kind": "click",
              "launched_app": true }
        ]),
    ));
    let events = drain_notification_events();

    assert_eq!(result.status, "pass", "{:#?}", step_errors(&result));
    let event = |id: &str| {
        events
            .iter()
            .find(|e| e.notification_id.as_str() == id)
            .unwrap_or_else(|| panic!("{id} was queued: {events:#?}"))
            .clone()
    };
    let click = event("e1-click");
    assert_eq!(click.kind, NotificationEventType::Activated);
    assert_eq!(click.payload.as_str(), "doc-1");
    assert!(!click.launched_app);
    let action = event("e1-action");
    assert_eq!(action.kind, NotificationEventType::ActionInvoked);
    assert_eq!(action.action_id.as_str(), "open");
    assert_eq!(action.payload.as_str(), "doc-2");
    let dismiss = event("e1-dismiss");
    assert_eq!(dismiss.kind, NotificationEventType::Dismissed);
    assert_eq!(dismiss.reason.as_str(), "closed by the user");
    assert_eq!(dismiss.payload.as_str(), "", "no payload: routing fills the post's in");
    let failed = event("e1-fail");
    assert_eq!(failed.kind, NotificationEventType::Failed);
    assert_eq!(failed.reason.as_str(), "no notification server");
    assert!(event("e1-launch").launched_app);
}

/// A typo'd kind, an action without its button id and a missing id are
/// refused by name, and nothing is queued for them.
#[test]
fn a_notification_event_needs_an_id_a_known_kind_and_an_action_for_a_button() {
    use crate::managers::notification::drain_notification_events;

    let _globals = notification_globals();
    drop(drain_notification_events());

    let result = run_e2e_test(&every_step(
        "notification_event_refusals",
        serde_json::json!([
            { "op": "notification_event", "id": "e1-typo", "kind": "clicked" },
            { "op": "notification_event", "id": "e1-no-button", "kind": "action" },
            { "op": "notification_event", "id": "", "kind": "click" }
        ]),
    ));
    let events = drain_notification_events();

    let errors = step_errors(&result);
    assert_eq!(errors.len(), 3, "every step is refused: {errors:#?}");
    assert!(
        errors[0].contains("clicked") && errors[0].contains("dismiss"),
        "the unknown kind is named, with the known ones: {errors:#?}"
    );
    assert!(errors[1].contains("action"), "{errors:#?}");
    assert!(events.is_empty(), "nothing was queued: {events:#?}");
}

/// `assert_notification` could check a post's id, title, body, buttons and
/// withdrawn state, but not the `payload` the app attached - the one field
/// an app-level handler in a relaunched process gets back.
#[test]
fn assert_notification_checks_the_payload_a_notification_was_posted_with() {
    use azul_core::notification::Notification;
    use azul_css::AzString;
    use crate::managers::notification::record_posted_notification;

    let _globals = notification_globals();
    record_posted_notification(
        &Notification::create(AzString::from("e1-payload"), AzString::from("E1"))
            .with_payload(AzString::from("doc-42")),
    );

    let same = run_e2e_test(&every_step(
        "payload_matches",
        serde_json::json!([
            { "op": "assert_notification", "id": "e1-payload", "payload": "doc-42" }
        ]),
    ));
    let other = run_e2e_test(&every_step(
        "payload_differs",
        serde_json::json!([
            { "op": "assert_notification", "id": "e1-payload", "payload": "doc-43" }
        ]),
    ));

    assert_eq!(same.status, "pass", "{:#?}", step_errors(&same));
    assert_eq!(other.status, "fail", "a different payload fails");
    assert!(
        step_errors(&other).iter().any(|e| e.contains("payload")),
        "the failure says what differs: {:#?}",
        step_errors(&other)
    );
}
