//! Every `#[ignore]`d test of the engine crates is on the ledger below - with what it
//! is, who owns it and the register id that closes it - and the ledger counts them, so
//! a new one cannot arrive unseen (`X-AR6-b074b0243-1`, gate G10).
//!
//! An ignored test is a decision to stop looking. The first one that parked a real
//! disagreement (`an_incremental_cpu_frame_matches_a_full_redraw`: 261 pixels at 1x,
//! found by a seed sweep, `LANE-AZ-1`) was correct handling, and nothing tracked it:
//! no list, no owner, no count that fails on growth. This test is the list, the owner
//! and the count.
//!
//! It reads the sources of the five library crates (`core`, `css`, `layout`, `dll`,
//! `doc`: `src/` and `tests/`) and compares the `#[ignore` attributes it finds - by
//! file and by the name of the test that follows - with [`LEDGER`], both ways:
//!
//! * an ignored test the ledger does not know FAILS (add a row, or - better - make the
//!   test run);
//! * a row whose test is no longer ignored FAILS (delete the row: the number only goes
//!   down on purpose);
//! * the number of rows per [`Kind`] is pinned in [`COUNTS`], a number to raise
//!   deliberately; a [`Kind::Red`] row carries the register id that will close it.
//!
//! It reads files, not the crate: it keeps working whatever the layout API does, and a
//! file or a directory it cannot read fails it by name - an unread file is not a file
//! with nothing ignored in it.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// What an ignored test is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    /// A KNOWN FAILURE: the engine disagrees with the test. Carries a register id.
    Red,
    /// A known gap of the engine or of a dependency, stated in the reason.
    Gap,
    /// A test that encodes an older behaviour and was triaged as still red; it is
    /// owed a rewrite, not a fix.
    Stale,
    /// A dump, a probe or a measurement: it asserts nothing, or measures.
    Diagnostic,
    /// Needs hardware or an OS the CI runner does not have (a display, a GPU, a Linux
    /// desktop); run by hand.
    Hardware,
}

/// One ignored test.
struct Row {
    /// The file, from the workspace root.
    file: &'static str,
    /// The test's name (the `fn` after the attribute).
    test: &'static str,
    kind: Kind,
    /// The register row that closes it (required for [`Kind::Red`]).
    id: Option<&'static str>,
    /// Who answers for it: the area, or the lane while one has the row.
    owner: &'static str,
    /// Why it is ignored, in a line.
    reason: &'static str,
}

const TEXT3_STALE: &str =
    "TRIAGED 2026-08-20 and still RED (`--test text3_suite -- --ignored` fails 12/12): it \
     encodes a hard-coded coordinate of the OLD text3 generation";

const NEEDS_DISPLAY: &str = "needs the main thread and a real display";

/// THE LEDGER. Keep the rows sorted by file, then by test.
const LEDGER: &[Row] = &[
    Row {
        file: "dll/src/desktop/display.rs",
        test: "test_get_display_at_point",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/desktop",
        reason: NEEDS_DISPLAY,
    },
    Row {
        file: "dll/src/desktop/display.rs",
        test: "test_get_displays",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/desktop",
        reason: NEEDS_DISPLAY,
    },
    Row {
        file: "dll/src/desktop/display.rs",
        test: "test_get_primary_display",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/desktop",
        reason: NEEDS_DISPLAY,
    },
    Row {
        file: "dll/src/desktop/extra/screencap/dmabuf.rs",
        test: "egl_init_and_query",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/screencap",
        reason: "requires a GPU / libEGL",
    },
    Row {
        file: "dll/src/desktop/extra/video_codec/provision.rs",
        test: "reboot_safety_passes_running_kernel_fails_a_bare_one",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/video_codec",
        reason: "needs a real Linux desktop (/lib/modules + /boot)",
    },
    Row {
        file: "dll/src/desktop/extra/video_codec/provision.rs",
        test: "repair_plan_targets_modules_extra",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/video_codec",
        reason: "needs a real Linux desktop (apt metadata + kernel module trees)",
    },
    Row {
        file: "dll/src/desktop/menu.rs",
        test: "test_menu_position_auto_cursor_default",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/desktop",
        reason: NEEDS_DISPLAY,
    },
    Row {
        file: "dll/src/desktop/menu.rs",
        test: "test_menu_position_auto_hit_rect_default",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/desktop",
        reason: NEEDS_DISPLAY,
    },
    Row {
        file: "dll/src/desktop/menu.rs",
        test: "test_menu_position_overflow_bottom",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/desktop",
        reason: NEEDS_DISPLAY,
    },
    Row {
        file: "dll/src/desktop/menu.rs",
        test: "test_menu_position_overflow_right",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/desktop",
        reason: NEEDS_DISPLAY,
    },
    Row {
        file: "dll/src/desktop/menu.rs",
        test: "test_submenu_positioning_right",
        kind: Kind::Hardware,
        id: None,
        owner: "dll/desktop",
        reason: NEEDS_DISPLAY,
    },
    Row {
        file: "dll/src/desktop/shell2/headless/mod.rs",
        test: "ribbon_tab_click_does_not_damage_the_document_below",
        kind: Kind::Gap,
        id: None,
        owner: "cpurender / headless presenter",
        reason: "known gap: a structural change falls back to FULL damage",
    },
    Row {
        file: "layout/tests/a_rebuild_of_an_unchanged_page_costs_little.rs",
        test: "a_rebuild_of_an_unchanged_page_costs_little",
        kind: Kind::Diagnostic,
        id: None,
        owner: "layout/perf",
        reason: "a measurement: cargo test --release -- --ignored --nocapture",
    },
    Row {
        file: "layout/tests/flex_intrinsic_text.rs",
        test: "frame_around_overflow_hidden_strip_shrinks_too",
        kind: Kind::Gap,
        id: None,
        owner: "layout/flex",
        reason: "taffy 0.10 gap: nested scroll-container min-content contribution is not \
                 collapsed",
    },
    Row {
        file: "layout/tests/image_child_paint.rs",
        test: "dbg_image_child_tree",
        kind: Kind::Diagnostic,
        id: None,
        owner: "layout/images",
        reason: "diagnostic dump, run with --ignored when investigating",
    },
    Row {
        file: "layout/tests/inline_atomic_after_block.rs",
        test: "dbg_dump_two_passes",
        kind: Kind::Diagnostic,
        id: None,
        owner: "layout/inline",
        reason: "diagnostic dump, run with --ignored when investigating",
    },
    Row {
        file: "layout/tests/ribbon_tab_whitespace.rs",
        test: "probe_tab_wrap_and_caption_centering_across_widths",
        kind: Kind::Diagnostic,
        id: None,
        owner: "layout/widgets",
        reason: "a PROBE, not a gate: it prints the widths at which a tab label wraps; the law \
                 is enforced by `tab_labels_never_wrap_and_captions_stay_centered`",
    },
    Row {
        file: "layout/tests/text3/five.rs",
        test: "test_move_cursor_left_right_simple",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/five.rs",
        test: "test_move_cursor_line_start_end",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/five.rs",
        test: "test_move_cursor_up_down",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/four.rs",
        test: "test_get_glyph_positions",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/four.rs",
        test: "test_multi_column_layout",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/one.rs",
        test: "test_simple_line_break",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/three.rs",
        test: "test_bidi_with_start_alignment",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/three.rs",
        test: "test_get_glyph_positions",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/three.rs",
        test: "test_glyph_positions_rtl",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/three.rs",
        test: "test_inline_object_baseline_alignment",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/three.rs",
        test: "test_multi_column_layout",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
    Row {
        file: "layout/tests/text3/two.rs",
        test: "test_simple_line_break",
        kind: Kind::Stale,
        id: None,
        owner: "text3 suite",
        reason: TEXT3_STALE,
    },
];

/// How many ignored tests of each kind there are. A number to RAISE on purpose, with the
/// row that explains it; it goes down when a test is made to run.
const COUNTS: &[(Kind, usize)] = &[
    // LANE-AZ-1 (the one Red row: seed 25, frame 4) runs again: 1 -> 0, the total 30 -> 29.
    (Kind::Red, 0),
    (Kind::Gap, 2),
    (Kind::Stale, 12),
    (Kind::Diagnostic, 4),
    (Kind::Hardware, 11),
];

/// The crates whose sources are scanned (`src/` and `tests/` of each).
const CRATES: &[&str] = &["core", "css", "layout", "dll", "doc"];

/// The directories that hold no sources of ours.
const SKIPPED_DIRS: &[&str] = &["target", ".git", "node_modules"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate sits in the workspace")
        .to_path_buf()
}

/// Every `.rs` file below `dir`, from the workspace root, `/`-separated. A directory
/// or a file that cannot be read fails the walk by name.
fn rust_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()));
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry
            .file_type()
            .unwrap_or_else(|e| panic!("cannot stat {}: {e}", path.display()));
        if kind.is_dir() {
            if !SKIPPED_DIRS.contains(&name.as_str()) {
                rust_files(root, &path, out);
            }
        } else if name.ends_with(".rs") {
            let relative = path
                .strip_prefix(root)
                .expect("below the root")
                .to_string_lossy()
                .replace('\\', "/");
            out.push(relative);
        }
    }
}

/// The name of the test an attribute at line `at` belongs to: the first `fn` after it.
fn test_name_after(lines: &[&str], at: usize) -> Option<String> {
    lines.iter().skip(at + 1).take(12).find_map(|line| {
        let rest = line.trim_start();
        let rest = rest.strip_prefix("pub ").unwrap_or(rest);
        let rest = rest.strip_prefix("async ").unwrap_or(rest);
        let rest = rest.strip_prefix("fn ")?;
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        (!name.is_empty()).then_some(name)
    })
}

/// Every ignored test of the scanned crates: `(file, test name, line)`.
fn ignored_tests_found() -> Vec<(String, String, usize)> {
    let root = workspace_root();
    let mut files = Vec::new();
    for krate in CRATES {
        for part in ["src", "tests"] {
            let dir = root.join(krate).join(part);
            if dir.is_dir() {
                rust_files(&root, &dir, &mut files);
            }
        }
    }
    files.sort();
    let mut found = Vec::new();
    for file in &files {
        let source = fs::read_to_string(root.join(file))
            .unwrap_or_else(|e| panic!("cannot read {file}: {e}"));
        let lines: Vec<&str> = source.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            if !line.trim_start().starts_with("#[ignore") {
                continue;
            }
            let test = test_name_after(&lines, at)
                .unwrap_or_else(|| panic!("{file}:{}: an #[ignore] with no `fn` after it", at + 1));
            found.push((file.clone(), test, at + 1));
        }
    }
    found
}

#[test]
fn every_ignored_test_is_on_the_ledger_and_every_row_is_ignored() {
    let found = ignored_tests_found();
    let found_keys: BTreeSet<(&str, &str)> = found
        .iter()
        .map(|(file, test, _)| (file.as_str(), test.as_str()))
        .collect();
    let ledger_keys: BTreeSet<(&str, &str)> = LEDGER.iter().map(|r| (r.file, r.test)).collect();

    let unlisted: Vec<String> = found
        .iter()
        .filter(|(file, test, _)| !ledger_keys.contains(&(file.as_str(), test.as_str())))
        .map(|(file, test, line)| format!("  {file}:{line}  {test}"))
        .collect();
    assert!(
        unlisted.is_empty(),
        "ignored tests the ledger (layout/tests/ignored_tests_are_on_the_ledger.rs) does not \
         know - add a row with its kind, owner and register id, or make the test run:\n{}",
        unlisted.join("\n")
    );

    let stale: Vec<String> = LEDGER
        .iter()
        .filter(|r| !found_keys.contains(&(r.file, r.test)))
        .map(|r| format!("  {}  {}", r.file, r.test))
        .collect();
    assert!(
        stale.is_empty(),
        "ledger rows whose test is no longer ignored (or no longer exists) - delete the row, \
         the number goes down on purpose:\n{}",
        stale.join("\n")
    );

    assert_eq!(
        found.len(),
        LEDGER.len(),
        "the same test twice in one file? the ledger is keyed by file and name"
    );
}

#[test]
fn the_ledger_counts_its_rows_by_kind_and_a_red_one_names_its_register_row() {
    let mut keys = BTreeSet::new();
    for row in LEDGER {
        assert!(
            keys.insert((row.file, row.test)),
            "{} {} is on the ledger twice",
            row.file,
            row.test
        );
        assert!(
            !row.owner.is_empty() && !row.reason.is_empty(),
            "{} {}: a row names its owner and says why",
            row.file,
            row.test
        );
        if row.kind == Kind::Red {
            assert!(
                row.id.is_some_and(|id| !id.is_empty()),
                "{} {}: a RED test is a known failure - it carries the register id that closes it",
                row.file,
                row.test
            );
        }
    }
    for (kind, expected) in COUNTS {
        let counted = LEDGER.iter().filter(|r| r.kind == *kind).count();
        assert_eq!(
            counted, *expected,
            "{kind:?}: the ledger holds {counted} rows and COUNTS says {expected} - a count goes \
             up on purpose (with the row that explains it) and down when a test runs again"
        );
    }
    let total: usize = COUNTS.iter().map(|(_, n)| n).sum();
    assert_eq!(
        LEDGER.len(),
        total,
        "every row has a kind that COUNTS lists"
    );
}

/// The rows stay sorted, so the diff of a new one lands where it belongs and a duplicate
/// shows up next to its twin.
#[test]
fn the_ledger_is_sorted_by_file_and_test() {
    let keys: Vec<(&str, &str)> = LEDGER.iter().map(|r| (r.file, r.test)).collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "keep the rows sorted by file, then by test");
}
