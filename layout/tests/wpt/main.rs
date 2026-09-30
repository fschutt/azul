//! Reference comparisons that find engine bugs: web-platform-tests run
//! against azul, with a known-failures list so CI goes red only on change.
//!
//! Two suites, one binary (`harness = false`, so it prints a compact summary
//! instead of one line per case):
//!
//! - `reftest`: the curated WPT reftests of `tests/wpt/reftests.tsv` (and
//!   azul's own in the same format, `tests/wpt/local/`). Each test page and its
//!   reference are loaded with azul's XML loader, laid out in an 800 x 600
//!   `LayoutWindow` and painted by the CPU renderer; the two pixmaps must be
//!   equal (`match`) or differ (`mismatch`) within the page's WPT `fuzzy`
//!   allowance. No browser is involved.
//! - `editing`: the WPT `editing/data` cases of `tests/wpt/editing/*.json` for
//!   the commands mail compose needs, run against azul's editing engine the
//!   way a key press reaches it (`editing.rs`).
//!
//! Every case that does not pass must be listed in
//! `tests/wpt/<suite>_expectations.txt` with the engine gap it shows; the run
//! fails (exit 1) only on a REGRESSION (a case that fails and is not listed)
//! or an UNEXPECTED PASS (a listed case that passes: take it off the list).
//!
//! ```text
//! cargo test --release -p azul-layout --features wpt_tests --test wpt
//! cargo test --release -p azul-layout --features wpt_tests --test wpt -- reftest
//! cargo test --release -p azul-layout --features wpt_tests --test wpt -- editing insertparagraph
//! AZ_WPT_BLESS=1 cargo test --release -p azul-layout --features wpt_tests --test wpt
//! ```
//!
//! Environment:
//! - `AZ_WPT_BLESS=1` rewrites the expectation files from this run;
//! - `AZ_WPT_OUT=<dir>` (default `target/wpt`): `<suite>_results.tsv`,
//!   `<suite>_expectations.suggested.txt` and, for reftests, one
//!   test | reference | difference PNG per failure under `reftest_diffs/`;
//! - `AZ_WPT_SHOW=<n>` (default 20): how many failures the summary lists.
//!
//! The data is vendored by `scripts/refci/vendor_wpt.py`; see
//! `scripts/REFCI_2026_09_30.md`.

mod editing;
mod expect;
mod reftest;

use std::path::PathBuf;

fn main() {
    // `cargo test ... -- --nocapture` hands libtest flags to every test
    // binary; this one has no use for them.
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .collect();
    let (suite, filter) = match args.first().map(String::as_str) {
        Some("reftest") | Some("reftests") => ("reftest", args.get(1).cloned()),
        Some("editing") => ("editing", args.get(1).cloned()),
        Some(other) => ("all", Some(other.to_string())),
        None => ("all", None),
    };
    let root = wpt_dir();
    let out = out_dir();
    let _ = std::fs::create_dir_all(&out);

    let mut ok = true;
    if suite == "all" || suite == "reftest" {
        ok &= reftest::run(&root, &out, filter.as_deref());
    }
    if suite == "all" || suite == "editing" {
        ok &= editing::run(&root, &out, filter.as_deref());
    }
    std::process::exit(if ok { 0 } else { 1 });
}

/// `tests/wpt/` at the repository root.
fn wpt_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("wpt")
}

/// Where results and diff images go.
fn out_dir() -> PathBuf {
    std::env::var_os("AZ_WPT_OUT").map_or_else(
        || {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("target")
                .join("wpt")
        },
        PathBuf::from,
    )
}
