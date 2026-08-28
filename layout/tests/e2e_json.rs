// Headless, JSON-driven end-to-end test harness for `azul-layout`.
//
// THIN LOADER: this used to be a ~1000-line hand-written interpreter that
// re-implemented every op against `LayoutWindow`. It now deserializes each
// scenario into the debug server's OWN `E2eTest` schema and runs it through the
// REAL op-dispatch (`azul_layout::e2e::process_debug_event` + the scenario
// runner `resume_e2e_continuation`) via the headless driver
// `azul_layout::e2e::run_e2e_test`. No parallel interpreter, no HTTP.
//
// WHICH SCENARIOS. The canonical corpus is the repo-root `e2e/` directory —
// the exact same files `azul-doc e2e e2e` and CI's `e2e_headless` gate run.
// This test points AT that directory rather than at a private copy.
//
// It used to run an "adapted" copy of nine of those scenarios under
// `tests/e2e_fixtures/`. Those adaptations were made to work around a runner
// bug (efee23b4f: a `key_down`+`key_up` pair with no `wait` between them landed
// in ONE continuation slice, so only the key-RELEASED state survived), and they
// were strictly weaker than the originals AND blind to the very bug that was
// fixed: `tab-focus.json` inserted a `wait` between `key_down` and `key_up`, so
// reverting the runner fix left it GREEN while `e2e/op-tab-focus-next.json`
// correctly went red. The other eight had dropped `resize`,
// `reset_frame_counters`, `snapshot_frame`, `get_frame_report`,
// `capture_damage_png`, the `wait_frame`+`wait` pairs that separate an op from
// the assertion about its effect, and two trailing `assert_work_bounded`
// checks. They are gone; `e2e/` is the single source of truth.
//
// `tests/e2e_fixtures/` is KEPT for the four scenarios that exercise assertion
// ops the `e2e/` corpus has no scenario for — `assert_state_machines_idle`,
// `assert_manager_invariants`, `assert_composition` and `assert_damage_sound`.
// Add new scenarios to `e2e/`; only put one here if it covers an op `e2e/`
// genuinely does not.
//
// VERDICTS come from `azul_layout::e2e::render_report`, the same function the
// CLI and the CI gate use, so `expect: "fail"` is honoured identically
// everywhere: FAIL and XPASS are red, XFAIL is green. Comparing
// `result.status == "pass"` (what this file did before) turns a scenario
// legitimately marked as a known failure into a red test, and would report an
// XPASS as green — the two cases the marker exists to distinguish.
//
// Requires the `e2e-server` feature (see the `[[test]]` entry in Cargo.toml).
// Fixtures are stripped from the published crate by `exclude = ["tests", …]` in
// `layout/Cargo.toml`; `e2e/` sits outside the crate directory entirely and is
// therefore not packaged either.

use std::path::{Path, PathBuf};

use azul_layout::e2e::{load_e2e_tests, render_report, run_e2e_test, E2eTest, E2eTestResult};

/// The workspace root — `layout/`'s parent.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("layout/ always has a parent")
        .to_path_buf()
}

fn load_dir(dir: &Path) -> Vec<E2eTest> {
    let tests = load_e2e_tests(dir)
        .unwrap_or_else(|e| panic!("cannot load e2e scenarios from {}: {e}", dir.display()));
    assert!(
        !tests.is_empty(),
        "no *.json scenarios found under {} — an empty selection is a broken path, not a green \
         run",
        dir.display()
    );
    tests
}

#[test]
fn run_all_e2e_scenarios() {
    let root = repo_root();

    // Scenario files use REPO-ROOT-relative paths — `capture_damage_png` writes
    // `target/e2e/<name>.png` — exactly as the DLL host does when CI runs them
    // from the root. `cargo test` starts the binary in the PACKAGE directory
    // (`layout/`), which would silently write into `layout/target/` or fail with
    // ENOENT, so chdir first. Safe here: this is the only test in the binary.
    std::env::set_current_dir(&root)
        .unwrap_or_else(|e| panic!("cannot enter repo root {}: {e}", root.display()));

    let mut tests = load_dir(&root.join("e2e"));
    tests.extend(load_dir(&root.join("layout/tests/e2e_fixtures")));

    // `AZ_E2E_ONLY=<substring>` narrows the run to the scenarios whose name
    // contains it — for iterating on ONE scenario without paying for the
    // other ~60 every time. Unset (CI, `scripts/check.sh`) runs everything;
    // a filter that matches nothing is a typo, not a green run.
    if let Ok(only) = std::env::var("AZ_E2E_ONLY") {
        tests.retain(|t| t.name.contains(&only));
        assert!(
            !tests.is_empty(),
            "AZ_E2E_ONLY={only:?} matched no scenario — check the name"
        );
    }

    // `render_report` pairs a result to its `expect` marker BY NAME, so a
    // duplicate name would silently apply the wrong marker to one of them.
    let mut names: Vec<&str> = tests.iter().map(|t| t.name.as_str()).collect();
    names.sort_unstable();
    let dupes: Vec<&str> = names
        .windows(2)
        .filter(|w| w[0] == w[1])
        .map(|w| w[0])
        .collect();
    assert!(
        dupes.is_empty(),
        "duplicate e2e scenario name(s): {dupes:?}"
    );

    // Sort up front so BOTH the report order and the pairing in `render_report`
    // are independent of directory-iteration order.
    tests.sort_by(|a, b| a.name.cmp(&b.name));

    // Serial on purpose — not for isolation (a scenario owns its `LayoutWindow`,
    // its `E2eSession` and, since the test clock became thread-scoped, its own
    // clock; the CLI runner runs the same scenarios fully in parallel), but
    // because this gate is small and a serial run keeps the report order and any
    // failure trace trivially reproducible.
    let results: Vec<E2eTestResult> = tests.iter().map(run_e2e_test).collect();

    let (report, verdict) = render_report(&tests, &results);
    eprint!("{report}");

    assert!(
        !verdict.gate_failed(),
        "e2e gate is red: {} failed, {} unexpectedly passed ({} passed, {} xfailed)",
        verdict.failed,
        verdict.xpass,
        verdict.passed,
        verdict.xfail,
    );
}
