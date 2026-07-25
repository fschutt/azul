// Headless, JSON-driven end-to-end test harness for `azul-layout`.
//
// THIN LOADER: this used to be a ~1000-line hand-written interpreter that
// re-implemented every op against `LayoutWindow`. It now deserializes each
// fixture into the debug server's OWN `E2eTest` schema and runs it through the
// REAL op-dispatch (`azul_layout::e2e::process_debug_event` + the scenario
// runner `resume_e2e_continuation`) via the headless driver
// `azul_layout::e2e::run_e2e_test`. No parallel interpreter, no HTTP.
//
// Fixtures live under `layout/tests/e2e_fixtures/*.json`, are auto-discovered by
// the single `#[test]`, and are stripped from the published crate by
// `exclude = ["tests", ...]` in `layout/Cargo.toml`. They target the server op
// vocabulary (`mount`, `scroll_node_by`, `dpi_changed`, assertions, …); ops
// whose effect is applied asynchronously by the shell (`mount`, `focus`, `blur`,
// `move`, `key_down/up`, `scroll_into_view`) are followed by a `wait` step so the
// headless driver flushes the change + relayouts before the next assertion — the
// same yield the real event loop provides between frames.
//
// Requires the `e2e-server` feature (see the `[[test]]` entry in Cargo.toml).

use std::path::PathBuf;

use azul_layout::e2e::{run_e2e_test, E2eTest, E2eTestResult};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/e2e_fixtures")
}

/// Render a failed result into a readable multi-line report.
fn describe_failure(res: &E2eTestResult) -> String {
    let mut out = format!(
        "test '{}' -> {} ({} passed / {} failed of {} steps)",
        res.name, res.status, res.steps_passed, res.steps_failed, res.step_count
    );
    for step in &res.steps {
        if step.status != "pass" {
            out.push_str(&format!(
                "\n    step {} [{}]: {}",
                step.step_index,
                step.op,
                step.error.as_deref().unwrap_or("<no error message>")
            ));
        }
    }
    // The runner reports its own setup failures via `final_screenshot`.
    if res.steps.is_empty() {
        if let Some(msg) = &res.final_screenshot {
            out.push_str(&format!("\n    {msg}"));
        }
    }
    out
}

#[test]
fn run_all_e2e_fixtures() {
    let dir = fixtures_dir();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    entries.sort();

    assert!(
        !entries.is_empty(),
        "no *.json fixtures found under {}",
        dir.display()
    );

    let mut ran = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for path in &entries {
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        // Deserialize with the SERVER's own schema — no bespoke test types.
        let test: E2eTest = serde_json::from_str(&src)
            .unwrap_or_else(|e| panic!("cannot parse {} as E2eTest: {e}", path.display()));

        eprintln!("=== running fixture: {} ({}) ===", test.name, path.display());
        let result = run_e2e_test(&test);
        eprintln!(
            "    -> {} ({}/{} steps passed)",
            result.status, result.steps_passed, result.step_count
        );

        if result.status == "pass" {
            ran += 1;
        } else {
            failures.push(describe_failure(&result));
        }
    }

    eprintln!("e2e_json: {ran}/{} fixtures passed", entries.len());
    assert!(
        failures.is_empty(),
        "e2e_json fixtures failed:\n{}",
        failures.join("\n")
    );
}
