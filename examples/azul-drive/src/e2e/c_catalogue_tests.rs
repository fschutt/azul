//! The scenario files themselves: every step of `e2e/sync/` (the catalogue too) is one the
//! runner and AzDrive's commands can carry.

use std::path::{Path, PathBuf};

/// Every `*.json` under `e2e/sync/`, its subfolders included.
fn scenario_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut folders = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("e2e/sync")];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).expect("the scenario folder reads") {
            let path = entry.expect("a folder entry").path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The tests of a scenario file (one object, or an array of them).
fn tests_of(path: &Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(path).expect("the scenario reads");
    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(serde_json::Value::Array(tests)) => tests,
        Ok(test) => vec![test],
        Err(e) => panic!("{} is not JSON: {e}", path.display()),
    }
}

/// The steps of a test, each as the command it calls (`custom_op` spelled out) and its fields.
fn steps_of(test: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    test["steps"]
        .as_array()
        .map(|steps| {
            steps
                .iter()
                .map(|step| match step["op"].as_str() {
                    Some("custom_op") => (
                        step["name"].as_str().unwrap_or_default().to_string(),
                        step["args"].clone(),
                    ),
                    other => (other.unwrap_or_default().to_string(), step.clone()),
                })
                .collect()
        })
        .collect::<Vec<_>>()
        .concat()
}

/// A check's fields ride on a `drive_check` step. The catalogue's generator wrote max_ops's
/// kind as `"op": "trash"`, which put the check's field in the step's own `op`: the runner
/// then called a command named `trash`. The field is `change` now, and no step may carry a
/// `check` under any other command.
#[test]
fn every_step_with_a_check_calls_drive_check() {
    let mut wrong = Vec::new();
    for file in scenario_files() {
        for test in tests_of(&file) {
            for (op, fields) in steps_of(&test) {
                if fields.get("check").is_some() && op != "drive_check" {
                    wrong.push(format!("{}: op {op:?} carries a check", file.display()));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// A test that is not pending (and not marked as not applicable) calls only commands AzDrive
/// registers: a missing command belongs in the test's `pending` marker, where the runner
/// reports it as PENDING instead of failing on the step.
#[test]
fn a_test_that_is_not_pending_calls_only_registered_commands() {
    let known: Vec<&str> = super::OPS.iter().map(|op| op.name).collect();
    let mut missing = Vec::new();
    for file in scenario_files() {
        for test in tests_of(&file) {
            if test.get("pending").is_some() || test.get("not_applicable").is_some() {
                continue;
            }
            for (op, _) in steps_of(&test) {
                if op.starts_with("drive_") && !known.contains(&op.as_str()) {
                    missing.push(format!(
                        "{} ({}): {op}",
                        file.display(),
                        test["name"].as_str().unwrap_or_default()
                    ));
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

/// `max_ops` reads its kind from `change`; nothing passes it as `op` any more.
#[test]
fn no_max_ops_check_names_its_kind_op() {
    for file in scenario_files() {
        for test in tests_of(&file) {
            for (_, fields) in steps_of(&test) {
                if fields["check"] == "max_ops" {
                    assert!(
                        fields.get("op").is_none_or(|op| op == "drive_check"),
                        "{}: {fields}",
                        file.display()
                    );
                }
            }
        }
    }
}
