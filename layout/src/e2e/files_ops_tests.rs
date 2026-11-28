//! The share sheet and the open requests of a scenario (`share`, `assert_shared`, `simulate_open`), run through the real dispatcher on a
//! headless window: a request is CHECKED against the limits and recorded, `assert_shared` reads it back, a request the platform would
//! refuse fails its step with the reason, and `simulate_open` queues what it is given and refuses what is no request.

use azul_core::{refany::RefAny, resources::AppConfig};

use super::{runner::run_e2e_test_with_app, E2eTest, E2eTestResult};

/// The app data of the scenarios below: they need none.
struct Nothing;

fn run(name: &str, steps: serde_json::Value) -> E2eTestResult {
    let test: E2eTest = serde_json::from_value(serde_json::json!({
        "name": name,
        "config": { "continue_on_failure": false },
        "setup": { "window_width": 200, "window_height": 100, "dpi": 96 },
        "steps": steps,
    }))
    .expect("the scenario literal is a valid E2eTest");
    run_e2e_test_with_app(&test, RefAny::new(Nothing), &AppConfig::create())
}

fn failures(result: &E2eTestResult) -> String {
    result
        .steps
        .iter()
        .filter(|s| s.status != "pass")
        .map(|s| format!("  step {} `{}`: {}", s.step_index, s.op, s.error.clone().unwrap_or_default()))
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

fn step_error(result: &E2eTestResult, index: usize) -> String {
    result
        .steps
        .iter()
        .find(|s| s.step_index == index)
        .and_then(|s| s.error.clone())
        .unwrap_or_default()
}

/// A file of `bytes` called `name`, in a folder of its own (`folder`) under the temporary directory; its path.
fn file(folder: &str, name: &str, bytes: &[u8]) -> String {
    let dir = std::env::temp_dir().join(format!("azul-files-ops-{}-{folder}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder under the temporary directory");
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("a file under the temporary directory");
    path.to_string_lossy().into_owned()
}

#[test]
fn a_shared_text_is_recorded_and_assert_shared_reads_it_back_by_title_text_and_count() {
    let result = run(
        "share a text",
        serde_json::json!([
            { "op": "share", "text": "a line to send", "title": "Send it" },
            { "op": "assert_shared", "title": "Send it", "text": "line", "files": 0, "count": 1 },
            { "op": "assert_shared", "title": "another title", "count": 0 },
            { "op": "assert_shared", "open_with": false, "mime": "text/plain", "count": 1 },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn two_files_of_one_name_are_shown_to_the_receiver_under_two_names_and_typed_by_their_extension() {
    let first = file("one", "a.md", b"# one\n");
    let second = file("two", "a.md", b"# two\n");
    let result = run(
        "share two files of one name",
        serde_json::json!([
            { "op": "share", "files": [first, second], "title": "Two notes" },
            { "op": "assert_shared", "files": 2, "file": "a.md", "count": 1 },
            { "op": "assert_shared", "files": 2, "file": "a (2).md", "mime": "text/markdown", "count": 1 },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn open_with_is_recorded_for_one_file_and_the_given_type_wins_over_the_extension() {
    let note = file("openwith", "note.md", b"# a note\n");
    let result = run(
        "open with",
        serde_json::json!([
            { "op": "share", "files": [note], "mime": "text/x-custom", "open_with": true },
            { "op": "assert_shared", "open_with": true, "mime": "text/x-custom", "files": 1, "count": 1 },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn assert_shared_fails_when_nothing_that_matches_was_shared() {
    let result = run(
        "nothing matches",
        serde_json::json!([
            { "op": "share", "text": "one" },
            { "op": "assert_shared", "title": "a title nobody used" },
        ]),
    );
    assert_eq!(result.status, "fail", "an assertion about a share that never happened must fail the scenario");
    assert!(
        result.steps.iter().any(|s| s.step_index == 1 && s.status != "pass"),
        "the failed step is the assertion:\n{}",
        failures(&result)
    );
}

#[test]
fn a_request_the_platform_would_refuse_fails_its_step_and_says_why() {
    let missing = "/azul-files-ops/no/such/file.txt";
    for (steps, why) in [
        (serde_json::json!([{ "op": "share", "url": "not a link" }]), "is not a link"),
        (serde_json::json!([{ "op": "share", "text": "   " }]), "text to share is empty"),
        (serde_json::json!([{ "op": "share", "text": "a", "open_with": true }]), "'open with' needs exactly one file"),
        (serde_json::json!([{ "op": "share", "files": [missing] }]), "share: "),
    ] {
        let result = run("refused", steps);
        assert_eq!(result.status, "fail", "{why}: the scenario must fail");
        let error = step_error(&result, 0);
        assert!(error.contains("share: "), "{why}: the step names the call: {error}");
        assert!(error.contains(why), "{why}: the step says why: {error}");
    }
}

#[test]
fn simulate_open_queues_what_it_is_given() {
    let note = file("open", "note.md", b"# a note\n");
    let result = run(
        "open a file, a link and a text",
        serde_json::json!([
            { "op": "simulate_open", "action": "edit", "files": [note], "launched_app": true },
            { "op": "simulate_open", "urls": ["mailto:a@b.c"], "texts": ["a line"] },
            { "op": "simulate_open", "action": "send", "texts": ["shared to the app"] },
        ]),
    );
    assert_passes(&result);
}

#[test]
fn simulate_open_refuses_what_is_no_request() {
    let missing = "/azul-files-ops/no/such/file.md";
    for (steps, why) in [
        (serde_json::json!([{ "op": "simulate_open" }]), "nothing to open"),
        (serde_json::json!([{ "op": "simulate_open", "action": "launch", "texts": ["x"] }]), "unknown action"),
        (serde_json::json!([{ "op": "simulate_open", "files": [missing] }]), "no/such/file.md"),
    ] {
        let result = run("refused open", steps);
        assert_eq!(result.status, "fail", "{why}: the scenario must fail");
        let error = step_error(&result, 0);
        assert!(error.contains("simulate_open: "), "{why}: the step names the op: {error}");
        assert!(error.contains(why), "{why}: the step says why: {error}");
    }
}
