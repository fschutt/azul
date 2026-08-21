//! The shipped demos must be named consistently, and the release page must
//! link the names that are actually produced.
//!
//! This is a guard for a class that has now bitten twice in one release:
//!
//! 1. `examples/azul-writer` declares `[[bin]] name = "azwriter"` — the only
//!    demo whose binary name differed from its package name. CI staged
//!    `target/release/${package}`, a path that never existed, and a `[reuse]`
//!    branch swallowed the miss. The 0.2.0 release shipped ZERO azul-writer
//!    assets on all three desktop OSes while the job stayed green, and the
//!    release page advertised three downloads that 404.
//! 2. The resolver written to fix that did not resolve it either: cargo omits
//!    the package name after `#` when the directory is already named after the
//!    package (`path+file:///…/examples/azul-writer#0.1.0`), a spelling the
//!    matcher did not know. That turned a silent miss into a hard build failure.
//!
//! Both bugs are the same shape: a name is derived in one place and consumed in
//! another, and nothing checks that the two agree. So this asserts the
//! agreement directly, from the manifests, without needing a build.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("dll/ has a parent")
        .to_path_buf()
}

/// The demos CI builds and the release page links, in the order the workflow
/// iterates them.
const DEMOS: &[&str] = &["AzWidgets", "AzMaps", "AzPaint", "AzMeet", "AzWriter"];

/// Directory each demo lives in. The directories keep their historical names;
/// only the PACKAGE was renamed, so cargo emits
/// `path+file:///…/examples/azul-writer#AzWriter@0.1.0`.
const DEMO_DIRS: &[(&str, &str)] = &[
    ("AzWidgets", "azul-widgets"),
    ("AzMaps", "azul-maps"),
    ("AzPaint", "azul-paint"),
    ("AzMeet", "azul-meet"),
    ("AzWriter", "azul-writer"),
];

fn dir_of(package: &str) -> &'static str {
    DEMO_DIRS
        .iter()
        .find(|(p, _)| *p == package)
        .map(|(_, d)| *d)
        .unwrap_or_else(|| panic!("no directory recorded for {package}"))
}

/// The `[[bin]] name = "..."` a demo declares, if it declares one.
fn declared_bin_name(root: &Path, package: &str) -> Option<String> {
    let manifest = std::fs::read_to_string(root.join("examples").join(dir_of(package)).join("Cargo.toml"))
        .unwrap_or_else(|e| panic!("{package} has no Cargo.toml: {e}"));
    let mut in_bin = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_bin = t == "[[bin]]";
            continue;
        }
        if in_bin && t.starts_with("name") {
            if let Some(v) = t.split('=').nth(1) {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Every demo in the CI list must exist as a package.
#[test]
fn every_demo_the_workflow_builds_exists() {
    let root = repo_root();
    for d in DEMOS {
        assert!(
            root.join("examples").join(dir_of(d)).join("Cargo.toml").is_file(),
            "the workflow builds `{d}` but examples/{}/Cargo.toml does not exist", dir_of(d)
        );
    }
}

/// The workflow must not have quietly gained or lost a demo relative to this
/// list — otherwise the guard below checks a set nobody ships.
#[test]
fn the_workflow_and_this_test_agree_on_the_demo_list() {
    let wf = std::fs::read_to_string(repo_root().join(".github/workflows/rust.yml"))
        .expect("workflow is readable");
    let expected = DEMOS.join(" ");
    assert!(
        wf.contains(&expected),
        "the workflow's demo loop no longer reads `{expected}`. Update DEMOS in \
         this test to match, so the naming guard keeps checking what actually \
         ships."
    );
}

/// The asset name CI produces is `<package>-<os>`; the binary it copies is
/// whatever cargo built. Those must be the same name, or the copy misses.
///
/// This is the azul-writer bug, asserted from the manifests: a demo may declare
/// a `[[bin]] name`, but it must equal the package name, because that is what
/// the staging step and the release page both assume.
#[test]
fn a_demos_binary_is_named_after_its_package() {
    let root = repo_root();
    let mut wrong = Vec::new();
    for d in DEMOS {
        if let Some(bin) = declared_bin_name(&root, d) {
            if bin != *d {
                wrong.push(format!("{d} declares [[bin]] name = \"{bin}\""));
            }
        }
    }
    for d in DEMOS {
        assert!(
            d.starts_with("Az") && d.len() > 2 && d.as_bytes()[2].is_ascii_uppercase(),
            "demo package `{d}` does not follow the AzXxx convention — package, \
             binary and release asset are all this one string, so it is the name \
             users see in a download URL"
        );
    }
    assert!(
        wrong.is_empty(),
        "these demos build a binary whose name is not their package name: {}\n\n\
         CI stages `target/release/<package>` and the release page links \
         `<package>-<os>.tar.gz`, so a differing bin name means the asset is \
         never produced and the download 404s — silently, because the staging \
         step treats the miss as a deliberate reuse. Either rename the bin to \
         match the package, or change BOTH the staging step and the release \
         page to derive the name from cargo's resolved executable \
         (scripts/cargo_bin_path.py already does the resolving).",
        wrong.join("; ")
    );
}
