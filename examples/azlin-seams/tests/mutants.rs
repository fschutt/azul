//! The mutation guard `mutant!`: false without the feature `mutants`, and with it true only for the
//! id that `AZLIN_MUTANT` names.
//!
//! `on` reads the environment ONCE per process (the contract of azlin-proto's `mutants.rs`), so
//! the test that follows the environment runs this very test binary again as a child process with
//! the variable set. The parent is the normal run; the child is the same test function in
//! `AZLIN_MUTANT_PROBE_CHILD` mode.
//!
//! Implements planning/architecture.md section 1.4 rule 1 and the QUALITY rule 12 (a guard that a
//! fix added is proved by turning it off).

use std::process::Command;

use azlin_seams::mutant;
use azlin_seams::mutants::{on, ENV};

const TEST_NAME: &str = "mutant_macro_is_false_without_the_feature_and_follows_the_env_with_it";
const CHILD: &str = "AZLIN_MUTANT_PROBE_CHILD";

/// What the child process checks: its environment names `probe_a`.
fn child_checks() {
    #[cfg(feature = "mutants")]
    {
        // With the feature: exactly the named id is on.
        assert!(mutant!("probe_a"), "the named id is on");
        assert!(on("probe_a"));
        assert!(!mutant!("probe_b"), "another id is off");
        assert!(!mutant!("probe"), "a prefix of the id is off");
        assert!(!mutant!("probe_a_"), "an extension of the id is off");
        assert!(!mutant!("PROBE_A"), "no case folding");
        assert!(!mutant!(""), "the empty id is off");
        assert!(!on(" probe_a"), "no trimming");
        assert!(!on("probe_a "), "no trimming");
        assert!(!on("probe_a\n"), "no trimming");
    }
    #[cfg(not(feature = "mutants"))]
    {
        // Without it: the variable alone does nothing, whatever it says.
        assert_eq!(
            std::env::var(ENV).as_deref(),
            Ok("probe_a"),
            "the parent set the variable"
        );
        assert!(!mutant!("probe_a"), "the feature is off: no mutant runs");
        assert!(!on("probe_a"));
        assert!(!mutant!("probe_b"));
        assert!(!mutant!(""));
    }
}

#[test]
fn mutant_macro_is_false_without_the_feature_and_follows_the_env_with_it() {
    assert_eq!(ENV, "AZLIN_MUTANT", "the one variable of the standard");
    if std::env::var_os(CHILD).is_some() {
        child_checks();
        return;
    }

    // The parent: a process that names no mutant runs none, in both builds. (If a mutant hold set
    // the variable for this whole run, ids other than the probe names are not affected.)
    assert!(!mutant!("probe_a_for_the_parent_only"));
    assert!(!on("probe_never_named"));
    assert!(!mutant!(""));

    // The child: this test binary again, with the variable set, running only this test.
    let exe = std::env::current_exe().expect("the path of this test binary");
    let output = Command::new(exe)
        .args(["--exact", TEST_NAME, "--nocapture", "--test-threads=1"])
        .env(ENV, "probe_a")
        .env(CHILD, "1")
        .output()
        .expect("the child test process starts");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the child failed:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("1 passed"),
        "the child ran no test (a filter that matched nothing passes silently):\n{stdout}"
    );
}

#[test]
fn mutant_macro_expands_to_the_function_and_is_usable_in_conditions() {
    // `mutant!("id")` is `azlin_seams::mutants::on("id")`: a bool, usable as a guard.
    let guarded = if mutant!("no_such_mutant_is_named_by_any_run") {
        "mutant"
    } else {
        "product"
    };
    assert_eq!(guarded, "product");
    assert_eq!(
        mutant!("no_such_mutant_is_named_by_any_run"),
        on("no_such_mutant_is_named_by_any_run")
    );
}
