//! `autotests = false` must never turn into "this test silently never runs".
//!
//! `layout/Cargo.toml` sets `autotests = false` so that the ~118 integration
//! tests link ONE binary (`tests/all.rs`) instead of 118 of them at 66 MB each.
//! That trade buys back several GB of linker output per build — and buys a
//! footgun with it: with auto-discovery off, a `tests/*.rs` file that nobody
//! registers in `all.rs` is not "a failing test", it is **not a test at all**.
//! Cargo will not build it, rustc will not see it, and nothing anywhere emits
//! a warning. Coverage disappears with a green suite, which is strictly worse
//! than the build cost the consolidation removed.
//!
//! This file is the interlock. It compares three lists that must agree:
//!
//!   1. the `.rs` files actually sitting in `layout/tests/`,
//!   2. the `#[path = "…"] mod …;` lines in `tests/all.rs`,
//!   3. the `[[test]] path = "tests/…"` entries in `layout/Cargo.toml`.
//!
//! Every file must appear in exactly one of (2) or (3). It deliberately links
//! nothing from the crate — `include_str!` plus a directory listing plus
//! string matching — so it keeps working no matter what the layout API does.
//!
//! TO TURN IT RED: `touch layout/tests/orphan.rs` and run the suite.
//!
//! Deleting this test to make a build pass re-opens the hole; register the
//! file, or declare it as its own `[[test]]` target and it counts as covered.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

/// `tests/all.rs`, verbatim, at compile time.
const ALL_RS: &str = include_str!("all.rs");

/// `layout/Cargo.toml`, verbatim, at compile time.
const CARGO_TOML: &str = include_str!("../Cargo.toml");

/// The crate root's `tests/` directory, resolved at compile time.
fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

/// `tests/all.rs` is the harness root, not a registered test module.
const HARNESS_ROOT: &str = "all.rs";

/// Subdirectory sources that are knowingly unreachable, with the reason.
///
/// An entry here is a standing DEFECT report, not an excuse: the file's
/// assertions do not run and have never run. It is listed rather than deleted
/// so the finding survives, and `the_orphan_exemptions_are_not_stale` fails the
/// moment one is fixed or removed, so the list cannot rot.
const KNOWN_ORPHANS: &[(&str, &str)] = &[(
    "solver3/test_inline_intrinsic_width.rs",
    "PRE-EXISTING (since 27db54be8): written as a `#[cfg(test)] mod` for the \
     layout/src/ tree, not as an integration test — it imports \
     `crate::{solver3, text3}` and `super::super::create_test_font_manager`, \
     neither of which resolves from tests/. Cargo never auto-discovers \
     subdirectory files, so nothing has ever compiled it and its 2 tests have \
     never run. Fixing it means moving it under layout/src/solver3/ (or \
     rewriting the paths to `azul_layout::…` and giving it a font-manager \
     helper), which is a source change, not a test-layout change.",
)];

/// Every `#[path = "…"]` string in `tests/all.rs`.
///
/// Returned verbatim (so `solver3/foo.rs` keeps its directory), which lets the
/// two tests below split them into top-level and subdirectory registrations.
fn registered_paths() -> BTreeSet<String> {
    ALL_RS
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("#[path = \"")?;
            rest.strip_suffix("\"]").map(str::to_string)
        })
        .collect()
}

/// Every `path = "tests/…"` string declared by a `[[test]]` in `Cargo.toml`,
/// with the leading `tests/` stripped so it is comparable to the above.
///
/// `[[example]]` and `[[bench]]` entries point at `examples/` and `benches/`,
/// so requiring the `tests/` prefix is enough to keep them out.
fn declared_paths() -> BTreeSet<String> {
    CARGO_TOML
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("path = \"tests/")?;
            rest.strip_suffix('"').map(str::to_string)
        })
        .collect()
}

/// The `.rs` files directly inside `layout/tests/`, excluding the harness root.
fn top_level_sources() -> BTreeSet<String> {
    let dir = tests_dir();
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot read {} — {e}", dir.display()));
    entries
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            (name.ends_with(".rs") && name != HARNESS_ROOT).then_some(name)
        })
        .collect()
}

/// The `.rs` files one level down (`tests/solver3/…`, `tests/common/…`, …),
/// as `dir/file.rs`.
fn subdir_sources() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let dir = tests_dir();
    let entries =
        std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("cannot read {} — {e}", dir.display()));
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Ok(sub) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(inner) = std::fs::read_dir(entry.path()) else {
            continue;
        };
        for f in inner.flatten() {
            if let Ok(name) = f.file_name().into_string() {
                if name.ends_with(".rs") {
                    out.insert(format!("{sub}/{name}"));
                }
            }
        }
    }
    out
}

#[test]
fn every_test_file_is_either_registered_in_all_rs_or_declared_in_cargo_toml() {
    // The premise. If `autotests` came back, this whole file is checking a
    // rule that is no longer load-bearing, and the failure message below would
    // point at nothing.
    assert!(
        CARGO_TOML.contains("autotests = false"),
        "layout/Cargo.toml no longer sets `autotests = false`, so Cargo is \
         auto-discovering tests/*.rs again — every file links its own ~66 MB \
         binary. Either restore it (and keep this guard) or delete tests/all.rs \
         and this file together; a half-applied consolidation is the worst of \
         both."
    );

    let registered = registered_paths();
    let declared = declared_paths();
    let actual = top_level_sources();

    // A zero is not a measurement: three parsers that all match nothing would
    // make every assertion below pass while proving nothing at all.
    assert!(
        registered.len() >= 100,
        "parsed only {} `#[path = \"…\"]` registrations out of tests/all.rs — \
         the parser broke. An empty list makes this test vacuous.",
        registered.len()
    );
    assert!(
        declared.len() >= 5,
        "parsed only {} `[[test]] path = \"tests/…\"` entries out of \
         layout/Cargo.toml — the parser broke.",
        declared.len()
    );
    assert!(
        actual.len() >= 100,
        "found only {} .rs files in layout/tests/ — the directory listing \
         broke (or the tests were deleted).",
        actual.len()
    );

    let top_level_registered: BTreeSet<&str> = registered
        .iter()
        .map(String::as_str)
        .filter(|p| !p.contains('/'))
        .collect();
    let top_level_declared: BTreeSet<&str> = declared
        .iter()
        .map(String::as_str)
        .filter(|p| !p.contains('/'))
        .collect();

    let unregistered: Vec<&str> = actual
        .iter()
        .map(String::as_str)
        .filter(|f| !top_level_registered.contains(f) && !top_level_declared.contains(f))
        .collect();

    assert!(
        unregistered.is_empty(),
        "layout/tests/ contains .rs file(s) that NOTHING compiles: \
         {unregistered:?}\n\n\
         `autotests = false` is set, so these are not failing tests — they are \
         not tests. No target references them, so rustc never opens them and \
         no warning is emitted anywhere. Fix by adding, in alphabetical order, \
         to layout/tests/all.rs:\n\n    \
         #[path = \"<file>.rs\"]\n    mod <file>;\n\n\
         or, if the file needs its own `required-features` / must run in its \
         own process, declare it as a `[[test]]` in layout/Cargo.toml and say \
         why in its module docs."
    );

    let both: Vec<&&str> = top_level_registered
        .iter()
        .filter(|f| top_level_declared.contains(**f))
        .collect();
    assert!(
        both.is_empty(),
        "test file(s) BOTH registered in tests/all.rs and declared as their own \
         `[[test]]` in layout/Cargo.toml: {both:?}. They compile and run twice, \
         which doubles the link cost this consolidation exists to remove — and \
         if they touch shared state they now race themselves. Pick one."
    );

    let stale: Vec<&str> = top_level_registered
        .iter()
        .copied()
        .filter(|f| !actual.contains(*f))
        .collect();
    assert!(
        stale.is_empty(),
        "tests/all.rs registers file(s) that no longer exist: {stale:?}. \
         (This normally fails at compile time; if you are seeing it as a test \
         failure the module was cfg'd out.) Delete the `#[path]`/`mod` lines."
    );
}

/// Every test source in the tree, concatenated — the text a subdirectory file
/// must be named in to count as reachable (a `mod one;` inside
/// `tests/text3/mod.rs`, a `#[path = "common/fakefont.rs"]` in a root file, …).
/// Excluded from the corpus: THIS file.
///
/// `KNOWN_ORPHANS` below names its entries as string literals, and the
/// reachability check is textual — so including this file would make every
/// exempted orphan look reached BY ITS OWN EXEMPTION, and
/// `the_orphan_exemptions_are_not_stale` would report the whole list as stale.
/// (Observed the first time this guard ran.) A test source is never compiled
/// by being mentioned in a manifest test, so dropping it loses nothing.
const THIS_FILE: &str = "integration_test_registry_is_exhaustive.rs";

fn reachability_corpus() -> String {
    let mut corpus = String::from(ALL_RS);
    let dir = tests_dir();
    let mut push = |p: &Path| {
        if let Ok(s) = std::fs::read_to_string(p) {
            corpus.push_str(&s);
            corpus.push('\n');
        }
    };
    for f in top_level_sources() {
        if f == THIS_FILE {
            continue;
        }
        push(&dir.join(&f));
    }
    for f in subdir_sources() {
        push(&dir.join(&f));
    }
    // A zero is not a measurement: an empty corpus would make the reachability
    // check report either everything or nothing, depending on its polarity.
    assert!(
        corpus.len() > 100_000,
        "the reachability corpus is only {} bytes — the file reads failed.",
        corpus.len()
    );
    corpus
}

/// Is `rel` (as `dir/file.rs`) compiled by anything?
fn is_reachable(
    rel: &str,
    corpus: &str,
    declared: &BTreeSet<String>,
    registered: &BTreeSet<String>,
) -> bool {
    if declared.contains(rel) || registered.contains(rel) {
        return true;
    }
    let stem = rel
        .rsplit('/')
        .next()
        .and_then(|f| f.strip_suffix(".rs"))
        .unwrap_or("");
    // Reached by a plain `mod stem;` from a sibling, or by any
    // `#[path = "…/stem.rs"]` anywhere in the tree.
    corpus.contains(&format!("mod {stem};")) || corpus.contains(&format!("{stem}.rs\""))
}

#[test]
fn no_test_source_in_a_subdirectory_is_orphaned() {
    let subdir = subdir_sources();
    assert!(
        subdir.len() >= 8,
        "found only {} .rs files under layout/tests/*/ — the listing broke.",
        subdir.len()
    );

    let declared = declared_paths();
    let registered = registered_paths();
    let corpus = reachability_corpus();

    let orphans: Vec<&String> = subdir
        .iter()
        .filter(|rel| {
            !KNOWN_ORPHANS.iter().any(|&(k, _)| k == rel.as_str())
                && !is_reachable(rel.as_str(), &corpus, &declared, &registered)
        })
        .collect();

    assert!(
        orphans.is_empty(),
        "test source(s) under layout/tests/*/ that nothing reaches: \
         {orphans:?}\n\n\
         Subdirectory files are never auto-discovered by Cargo — not even with \
         `autotests = true` — so an unreferenced one has never been compiled \
         and its assertions have never run, however green the suite looks. \
         Either register it (a `#[path = \"<dir>/<file>.rs\"] mod …;` in \
         tests/all.rs, or a `mod <file>;` from the suite's mod.rs), declare it \
         as a `[[test]]` in layout/Cargo.toml, or delete it."
    );
}

#[test]
fn the_orphan_exemptions_are_not_stale() {
    let subdir = subdir_sources();
    let declared = declared_paths();
    let registered = registered_paths();
    let corpus = reachability_corpus();

    for &(rel, reason) in KNOWN_ORPHANS {
        assert!(
            subdir.contains(rel),
            "`{rel}` is listed as a known orphan but no longer exists — delete \
             the entry from KNOWN_ORPHANS in this file (reason on file: \
             {reason})"
        );
        assert!(
            !is_reachable(rel, &corpus, &declared, &registered),
            "`{rel}` is now reachable, so its KNOWN_ORPHANS entry is masking \
             nothing and would hide the NEXT orphan that lands beside it — \
             delete the entry from this file (reason on file: {reason})"
        );
    }
}
