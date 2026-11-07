//! Gate: the CI runs the source gates (X-DELTA-AZDRIVE-FILES-12, K29 "a gate nobody asserts").
//!
//! The gates of this crate are tests that read other crates' code. No workflow step ran
//! `cargo test -p azul-srcgate`, so two of them were red at HEAD for days (a count that moved when
//! a lane deleted a call, a file a gate named that a lane had deleted) and nobody saw. This test
//! reads the workflow: a step runs this package's tests, through the guard that fails a run in
//! which no test ran.

use azul_srcgate::workspace;

const WORKFLOW: &str = ".github/workflows/rust.yml";

/// The text of the workflow step that runs the source gates, or why there is none.
fn gate_step(workflow: &str) -> Result<String, String> {
    let mut steps: Vec<String> = Vec::new();
    for line in workflow.lines() {
        if line.trim_start().starts_with("- name:") {
            steps.push(String::new());
        }
        if let Some(step) = steps.last_mut() {
            step.push_str(line);
            step.push('\n');
        }
    }
    steps
        .into_iter()
        .find(|step| step.contains("cargo test -p azul-srcgate"))
        .ok_or_else(|| format!("no step of {WORKFLOW} runs `cargo test -p azul-srcgate`"))
}

#[test]
fn a_ci_step_runs_the_source_gates_and_fails_a_run_in_which_no_test_ran() {
    let text = std::fs::read_to_string(workspace().join(WORKFLOW))
        .unwrap_or_else(|e| panic!("{WORKFLOW}: {e}"));
    let step = gate_step(&text).unwrap_or_else(|why| panic!("{why}"));
    assert!(
        step.contains("--no-fail-fast"),
        "the gates run to the end, so one red gate does not hide the next:\n{step}"
    );
    assert!(
        step.contains("zero_test_guard.sh --strict"),
        "a run in which nothing ran is no pass:\n{step}"
    );
    assert!(
        !step.contains("--lib") && !step.contains("--test "),
        "the whole package runs, every gate file:\n{step}"
    );
}

/// The finder itself: a workflow without the step is an error, one with it a step.
#[test]
fn the_step_finder_reads_steps_by_their_name_line() {
    let without = "jobs:\n  t:\n    steps:\n      - name: Run azul-storage tests\n        run: |\n          cargo test -p azul-storage\n";
    assert!(gate_step(without).is_err());
    let with = format!(
        "{without}      - name: Run gates\n        run: |\n          cargo test -p azul-srcgate --no-fail-fast\n      - name: Later\n        run: echo x\n"
    );
    let step = gate_step(&with).expect("the step");
    assert!(
        step.contains("Run gates") && !step.contains("Later"),
        "{step}"
    );
}
