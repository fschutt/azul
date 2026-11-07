//! Gate: the CI runs the checks the key lanes' safety rests on (X-DELTA-AZDRIVE-KEYS-16, -42).
//!
//! Three things were written and run by nobody:
//!
//! - the panic family is DENIED outside tests in the crates that read hostile bytes
//!   (`#![cfg_attr(not(test), deny(clippy::unwrap_used, ..))]`), and a deny is clippy's - `cargo
//!   build` and `cargo test` never see it. A step of the clippy job runs clippy over those crates;
//! - `scripts/panic_surface.py` holds the places a panic is WRITTEN in them to a list that only
//!   shrinks, and no step ran it;
//! - the fuzz targets are built, so a target that stopped compiling is a red job, not a corpus
//!   nobody runs.
//!
//! This test reads the workflow, as `ci_gate` does for the source gates themselves: a step that is
//! deleted, renamed away from its command or loses its crates fails here, in the job that runs
//! this package.

use azul_srcgate::workspace;

const WORKFLOW: &str = ".github/workflows/rust.yml";

/// The crates whose clippy deny the CI makes bite.
const CLIPPY_CRATES: &[&str] = &[
    "azul-storage",
    "azlin-compress",
    "azlin-preflate",
    "azcloud-kit",
];

fn workflow() -> String {
    let path = workspace().join(WORKFLOW);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The text of each step of the workflow: from its `- name:` line to the next one.
fn steps(workflow: &str) -> Vec<String> {
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
}

/// A step that runs `command` (a line of its script, not a comment), or why there is none.
fn step_running(workflow: &str, command: &str) -> Result<String, String> {
    steps(workflow)
        .into_iter()
        .find(|step| {
            step.lines()
                .any(|l| !l.trim_start().starts_with('#') && l.contains(command))
        })
        .ok_or_else(|| format!("no step of {WORKFLOW} runs `{command}`"))
}

#[test]
fn a_step_of_the_preflight_checks_the_panic_surface_against_its_list() {
    let step = step_running(&workflow(), "python3 scripts/panic_surface.py")
        .unwrap_or_else(|why| panic!("{why}"));
    assert!(
        !step.contains("--update") && !step.contains("--print"),
        "the step checks the list, it does not rewrite it:\n{step}"
    );
}

#[test]
fn a_step_runs_clippy_over_the_crates_that_deny_the_panic_family() {
    let step = step_running(&workflow(), "cargo clippy -p azul-storage")
        .unwrap_or_else(|why| panic!("{why}"));
    for name in CLIPPY_CRATES {
        assert!(
            step.contains(&format!("-p {name}")),
            "the clippy step does not name {name}:\n{step}"
        );
    }
    assert!(
        step.contains("-D warnings"),
        "rustc's warnings are errors in this step:\n{step}"
    );
    assert!(
        !step.contains("--no-deps") && !step.contains("--lib") && !step.contains("|| true"),
        "the whole step bites, on every target:\n{step}"
    );
}

/// The folders (`examples/<crate>`) that hold a cargo-fuzz project.
fn fuzz_crates() -> Vec<String> {
    let mut found = Vec::new();
    let examples = workspace().join("examples");
    for entry in std::fs::read_dir(&examples)
        .unwrap_or_else(|e| panic!("{}: {e}", examples.display()))
        .flatten()
    {
        if entry.path().join("fuzz").join("Cargo.toml").is_file() {
            found.push(format!("examples/{}", entry.file_name().to_string_lossy()));
        }
    }
    found.sort();
    found
}

#[test]
fn every_cargo_fuzz_project_is_built_by_a_ci_step() {
    let crates = fuzz_crates();
    assert!(
        crates.len() >= 2,
        "the fuzz projects of azul-storage and azcloud-kit are found: {crates:?}"
    );
    let workflow = workflow();
    let step = step_running(&workflow, "fuzz build").unwrap_or_else(|why| panic!("{why}"));
    for krate in &crates {
        assert!(
            step.contains(krate.as_str()),
            "the step that builds the fuzz targets does not name {krate}:\n{step}"
        );
    }
    assert!(
        step.contains("cargo +nightly"),
        "cargo-fuzz needs a nightly (pinned, like the sanitizer jobs'):\n{step}"
    );
}

/// The finder itself: a workflow without the command is an error, a comment is no command.
#[test]
fn the_step_finder_reads_the_scripts_of_steps_and_not_their_comments() {
    let commented = "jobs:\n  t:\n    steps:\n      - name: A\n        run: |\n          # python3 scripts/panic_surface.py\n          echo x\n";
    assert!(step_running(commented, "python3 scripts/panic_surface.py").is_err());
    let real = format!(
        "{commented}      - name: B\n        run: python3 scripts/panic_surface.py\n      - name: C\n        run: echo y\n"
    );
    let step = step_running(&real, "python3 scripts/panic_surface.py").expect("the step");
    assert!(
        step.contains("name: B") && !step.contains("name: C"),
        "{step}"
    );
}
