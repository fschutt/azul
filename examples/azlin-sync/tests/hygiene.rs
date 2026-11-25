//! The crate's hygiene, the part a test can hold (the launch rules of the W1 lanes, checks 9 and 10 of the
//! brief): the manifest lists exactly the dependencies of the final manifest (and none of the
//! forbidden ones), the crate root forbids unsafe code and denies the panicking constructs, the library
//! code names no panicking call, lenient default, clock, file, network, thread, environment, database
//! or random source (`no_unwrap_in_non_test_code`, `public_api_exposes_no_clock_read_no_io`), no
//! container whose iteration order is not deterministic (every voter must apply the same entry to the
//! same state), every public item names the plan section it implements and its caller or the later unit
//! it is a seam of (`public_items_cite_a_plan_section`), every source file is declared, every plan
//! section a comment cites exists in `planning/architecture.md` (COMMON-W1R R7), and every mutation
//! guard has a killer.
//!
//! `cargo clippy --all-targets -- -D warnings`, `rustfmt --check` and the builds with the features are
//! the lane's holds: cargo cannot run inside `cargo test`.
//!
//! Implements planning/architecture.md section 1.4 (engineering rules: no panics on input, I/O is a
//! parameter, every public item has a caller).

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The keys of a manifest section (`[dependencies]`, `[features]`): the text before ` =`.
fn section_keys(manifest: &str, section: &str) -> Vec<String> {
    let mut inside = false;
    let mut keys = Vec::new();
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == section;
            continue;
        }
        if !inside || t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = t.split_once('=') {
            keys.push(key.trim().to_string());
        }
    }
    keys
}

/// The value text of one key of a manifest section (up to the end of the line).
fn section_value(manifest: &str, section: &str, key: &str) -> Option<String> {
    let mut inside = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == section;
            continue;
        }
        if inside {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim() == key {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

/// The source files of `src/`, by file name (all flat: this crate has no module folder).
fn source_files() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root().join("src")).expect("src folder") {
        let entry = entry.expect("a directory entry");
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        assert!(
            name.ends_with(".rs"),
            "{name}: only .rs files live under src"
        );
        let text = fs::read_to_string(entry.path()).expect("source file");
        out.push((name, text));
    }
    out.sort();
    out
}

#[test]
fn crate_hygiene() {
    let manifest = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");

    // The dependencies of the final manifest, exactly: nothing more, nothing less.
    let dependencies: BTreeSet<String> = section_keys(&manifest, "[dependencies]")
        .into_iter()
        .collect();
    let expected: BTreeSet<String> = ["azlin-seams", "azlin-chain", "blake3", "thiserror"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(dependencies, expected, "the dependencies of the crate");
    for forbidden in [
        "tokio",
        "rusqlite",
        "reqwest",
        "rand",
        "proptest",
        "serde",
        "serde_json",
        "azlin-cbor",
        "azlin-keys",
    ] {
        assert!(
            !dependencies.contains(forbidden),
            "{forbidden} is not a dependency of this crate"
        );
    }
    for section in ["[dev-dependencies]", "[build-dependencies]", "[features]"] {
        assert!(
            section_keys(&manifest, section).is_empty(),
            "{section} is empty (the feature `mutants` is azlin-seams's alone, R19)"
        );
    }
    assert!(!root().join("build.rs").exists());

    // Package facts.
    assert_eq!(
        section_value(&manifest, "[package]", "name").as_deref(),
        Some("\"azlin-sync\"")
    );
    assert_eq!(
        section_value(&manifest, "[package]", "edition").as_deref(),
        Some("\"2021\"")
    );
    assert_eq!(
        section_value(&manifest, "[package]", "rust-version").as_deref(),
        Some("\"1.91\"")
    );
    assert_eq!(
        section_value(&manifest, "[package]", "publish").as_deref(),
        Some("false")
    );

    // The crate root.
    let lib = fs::read_to_string(root().join("src/lib.rs")).expect("src/lib.rs");
    assert!(lib.contains("#![forbid(unsafe_code)]"));
    for lint in [
        "clippy::unwrap_used",
        "clippy::expect_used",
        "clippy::panic",
        "clippy::unreachable",
        "clippy::todo",
        "clippy::unimplemented",
        "clippy::indexing_slicing",
    ] {
        assert!(lib.contains(lint), "the crate root does not deny {lint}");
    }
    assert!(lib.contains("#![warn(missing_docs)]"));
}

/// Every source file is declared by a `mod` line of `lib.rs`, and every `mod` line has its file.
#[test]
fn every_source_file_is_declared_and_every_module_has_its_file() {
    let lib = fs::read_to_string(root().join("src/lib.rs")).expect("src/lib.rs");
    let mut declared = BTreeSet::new();
    for line in lib.lines() {
        let t = line.trim();
        if t.starts_with("//") {
            continue;
        }
        let t = t.strip_prefix("pub ").unwrap_or(t);
        if let Some(rest) = t.strip_prefix("mod ") {
            if let Some(name) = rest.strip_suffix(';') {
                declared.insert(name.trim().to_string());
            }
        }
    }
    let on_disk: BTreeSet<String> = source_files()
        .into_iter()
        .map(|(name, _)| name.trim_end_matches(".rs").to_string())
        .filter(|name| name != "lib")
        .collect();
    assert_eq!(
        declared, on_disk,
        "lib.rs declares exactly the files of src/"
    );
    let expected: BTreeSet<String> = [
        "access",
        "cas",
        "compact",
        "guard",
        "head",
        "history",
        "log",
        "lost_reply",
        "quota",
        "restore",
        "version",
        "watch",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(declared, expected, "the modules of the brief (section 3)");
}

/// The tokens that do not belong in library code (QUALITY rules 4 and 5, launch rule 8).
const BANNED: &[&str] = &[
    "unsafe {",
    "unsafe fn",
    "unsafe impl",
    "unsafe trait",
    "std::fs",
    "std::net",
    "std::io",
    "std::time",
    "std::thread",
    "std::process",
    "std::env",
    "SystemTime",
    "Instant",
    "rand::",
    "tokio",
    "rusqlite",
    "serde",
    "HashMap",
    "HashSet",
    ".unwrap()",
    ".expect(",
    "panic!",
    "unreachable!",
    "todo!",
    "unimplemented!",
    "assert!(",
    "assert_eq!(",
    "debug_assert",
    "unwrap_or_default",
    "unwrap_unchecked",
    "get_unchecked",
    "mem::transmute",
    "println!",
    "eprintln!",
    "dbg!",
    ".ok()",
    "let _ =",
    "map_or(0",
    "unwrap_or(false)",
];

/// The library lines of `src/`: everything but comments.
fn library_lines() -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    for (name, text) in source_files() {
        // This crate keeps no unit tests in `src/`: the whole file is library code.
        assert!(
            !text.contains("#[cfg(test)]"),
            "{name}: tests live in tests/"
        );
        for (n, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            out.push((name.clone(), n + 1, t.to_string()));
        }
    }
    out
}

#[test]
fn no_unwrap_in_non_test_code() {
    let lines = library_lines();
    for (name, n, t) in &lines {
        // A `const _: () = assert!(..)` is a compile-time check, not a runtime panic.
        if t.starts_with("const _: () = assert!(") {
            continue;
        }
        for banned in BANNED {
            assert!(
                !t.contains(banned),
                "src/{name}: `{banned}` in library code: {t} (line {n})"
            );
        }
    }
    assert!(
        lines.len() > 1_500,
        "the scan saw only {} lines: has the layout changed?",
        lines.len()
    );
}

/// Check 9 of the brief (the seam guard, an `rg` self-check): no public function of the crate reads a
/// clock or does I/O, and none takes or returns a raw integer for a time or a size. Every function that
/// needs a time takes the leader's `Stamp`; every size is a `ByteCount`; every span a `Span`.
#[test]
fn public_api_exposes_no_clock_read_no_io() {
    let io_and_clock = [
        "SystemTime",
        "Instant",
        "std::time",
        "std::fs",
        "std::net",
        "std::io",
        "std::thread",
        "std::process",
        "std::env",
        "tokio",
        "rusqlite",
        "rand::",
        "getrandom",
    ];
    let raw_units = [
        "i64",
        "secs: u64",
        "seconds",
        "millis",
        "now: u64",
        "size: u64",
        "bytes: u64",
        "len: u64",
    ];
    let lines = library_lines();
    for (name, n, t) in &lines {
        for banned in io_and_clock {
            assert!(!t.contains(banned), "src/{name} line {n}: `{banned}`: {t}");
        }
        if t.starts_with("pub ") || t.starts_with("pub(crate) ") || t.contains("fn ") {
            for raw in raw_units {
                assert!(
                    !t.contains(raw),
                    "src/{name} line {n}: a raw unit `{raw}` in a signature: {t}"
                );
            }
        }
    }
    // The time of every operation that needs one is the caller's `Stamp`.
    let all: String = source_files().into_iter().map(|(_, text)| text).collect();
    assert!(
        all.contains("at: Stamp"),
        "the operations take the leader's stamp"
    );
    assert!(all.contains("now: Stamp"), "prune takes the leader's stamp");
}

/// The doc block (the `///` lines) above line `i`, attributes skipped, joined by spaces.
fn doc_above(lines: &[&str], i: usize) -> String {
    let mut doc = Vec::new();
    let mut j = i;
    while j > 0 {
        j -= 1;
        let t = lines[j].trim();
        if let Some(rest) = t.strip_prefix("///") {
            doc.push(rest.trim().to_string());
        } else if t.starts_with("#[") || t.starts_with("#![") {
            continue;
        } else {
            break;
        }
    }
    doc.reverse();
    doc.join(" ")
}

/// A public item at the start of a line (`pub fn`, `pub struct`, `pub const`, ...). Methods inside
/// `impl` blocks are covered by their type.
fn is_public_item(line: &str) -> bool {
    line.starts_with("pub ") && !line.starts_with("pub use ") && !line.starts_with("pub mod ")
}

#[test]
fn public_items_cite_a_plan_section() {
    let mut checked = 0usize;
    for (name, text) in source_files() {
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !is_public_item(line) {
                continue;
            }
            let doc = doc_above(&lines, i);
            assert!(
                doc.contains("planning/architecture.md section ")
                    || doc.contains("planning/architecture.md §"),
                "src/{name} line {}: `{}` does not cite a plan section (`planning/architecture.md section X.Y`)",
                i + 1,
                line.trim()
            );
            assert!(
                doc.contains("Caller:") || doc.contains("Seam:"),
                "src/{name} line {}: `{}` has no `Caller:` or `Seam:` doc line",
                i + 1,
                line.trim()
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 50,
        "the scan found only {checked} public items: has the layout changed?"
    );
}

/// The numbers that follow the word `section`/`sections` in the comments of the source files: the
/// plan sections the code cites.
fn cited_sections() -> BTreeSet<String> {
    let mut cited = BTreeSet::new();
    for (_, text) in source_files() {
        let comments: String = text
            .lines()
            .filter_map(|l| {
                let t = l.trim_start();
                t.strip_prefix("//!")
                    .or_else(|| t.strip_prefix("///"))
                    .or_else(|| t.strip_prefix("//"))
            })
            .collect::<Vec<_>>()
            .join(" ");
        let tokens: Vec<&str> = comments.split_whitespace().collect();
        for (i, token) in tokens.iter().enumerate() {
            let word = token.trim_matches(|c: char| !c.is_ascii_alphabetic());
            if word != "section" && word != "sections" {
                continue;
            }
            // `section 5.9`, `sections 5.3, 5.10 and 7.2`.
            let mut k = i + 1;
            while let Some(next) = tokens.get(k) {
                // The number is the run of digits and dots that starts at the first digit.
                let from_digit = next.trim_start_matches(|c: char| !c.is_ascii_digit());
                let run: String = from_digit
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                let number = run.trim_end_matches('.');
                if number.is_empty() {
                    break;
                }
                cited.insert(number.to_string());
                // Continue only through a list ("5.3, 5.10 and 7.2").
                let more = next.ends_with(',') || tokens.get(k + 1) == Some(&"and");
                if !more {
                    break;
                }
                k += if tokens.get(k + 1) == Some(&"and") {
                    2
                } else {
                    1
                };
            }
        }
    }
    cited
}

/// The plan, by the rule R7: `AZLIN_PLAN_MD`; else (azul crate)
/// `CARGO_MANIFEST_DIR/../../../azul-apps/planning/architecture.md`.
fn plan_text() -> Option<String> {
    let from_env = std::env::var_os("AZLIN_PLAN_MD");
    let path = match &from_env {
        Some(path) => PathBuf::from(path),
        None => root().join("../../../azul-apps/planning/architecture.md"),
    };
    match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(error) => {
            // Absent: skipped when the variable is unset, a failure when it is set.
            assert!(
                from_env.is_none(),
                "AZLIN_PLAN_MD is set but {} cannot be read: {error}",
                path.display()
            );
            None
        }
    }
}

#[test]
fn every_cited_plan_section_exists_in_the_plan() {
    let cited = cited_sections();
    assert!(
        cited.len() >= 8,
        "the scan found only {cited:?}: has the comment style changed?"
    );
    let Some(plan) = plan_text() else {
        eprintln!("SKIPPED: plan not present");
        return;
    };
    let headings: Vec<&str> = plan
        .lines()
        .filter(|l| l.starts_with('#'))
        .map(|l| l.trim_start_matches('#').trim_start())
        .collect();
    for number in &cited {
        let found = headings.iter().any(|h| {
            h.strip_prefix(number.as_str())
                .is_some_and(|rest| rest.starts_with(' ') || rest.starts_with(". "))
        });
        assert!(
            found,
            "the code cites section {number}, which is no heading of the plan"
        );
    }
}

/// Each guard of the crate (`mutant!("id")`) with the test that must fail when it is turned off: the
/// mutation standard (QUALITY rule 12) kept as one table. The mutant hold runs every id with
/// `AZLIN_MUTANT=<id> cargo test -p azlin-sync --features azlin-seams/mutants` and expects the red test.
const MUTANTS: &[(&str, &str)] = &[
    // the register
    (
        "sync_cas_size_unchecked",
        "head_value_over_head_max_is_too_large",
    ),
    (
        "sync_cas_create_overwrites",
        "cas_expecting_none_on_an_existing_register_conflicts",
    ),
    (
        "sync_cas_expected_unchecked",
        "cas_with_a_stale_version_conflicts_and_returns_the_current_head",
    ),
    (
        "sync_cas_version_skips",
        "cas_with_the_current_version_advances_by_one",
    ),
    (
        "sync_cas_limits_unchecked",
        "a_refused_cas_or_restore_at_the_limits_changes_nothing",
    ),
    // history and retention
    (
        "sync_history_page_unbounded",
        "history_is_oldest_first_bounded_and_resumable",
    ),
    (
        "sync_history_bytes_unbounded",
        "history_page_is_bounded_by_bytes_and_a_big_record_still_makes_progress",
    ),
    ("sync_prune_ignores_hold", "a_legal_hold_stops_every_prune"),
    (
        "sync_prune_drops_head",
        "prune_drops_versions_older_than_retention_but_never_the_current_head",
    ),
    (
        "sync_prune_boundary_inclusive",
        "prune_boundary_is_exclusive_and_future_stamps_are_kept",
    ),
    (
        "sync_prune_ignores_lock",
        "a_version_under_a_lock_survives_prune_until_retain_until",
    ),
    // restore
    (
        "sync_restore_auth_failure_ignored",
        "restore_as_of_needs_the_authorization_witness",
    ),
    (
        "sync_restore_wrong_space_allowed",
        "restore_as_of_needs_the_authorization_witness",
    ),
    (
        "sync_restore_moment_unchecked",
        "restore_as_of_needs_the_authorization_witness",
    ),
    (
        "sync_restore_before_first_allowed",
        "restore_as_of_before_the_first_version_is_refused",
    ),
    (
        "sync_restore_epoch_not_above_allowed",
        "restore_needs_an_epoch_above_the_heads_and_keeps_the_lock",
    ),
    (
        "sync_restore_version_regresses",
        "restore_as_of_writes_a_new_higher_version_with_the_old_value_and_epoch_plus_one",
    ),
    (
        "sync_restore_epoch_not_raised",
        "restore_as_of_writes_a_new_higher_version_with_the_old_value_and_epoch_plus_one",
    ),
    // the lost reply
    (
        "sync_lost_reply_ignores_writer",
        "lost_reply_not_landed_when_history_shows_no_such_value",
    ),
    (
        "sync_lost_reply_counts_restores",
        "a_restore_record_is_never_my_landed_write",
    ),
    (
        "sync_lost_reply_ignores_value",
        "lost_reply_not_landed_when_history_shows_no_such_value",
    ),
    (
        "sync_lost_reply_unknown_is_not_landed",
        "lost_reply_with_too_short_history_is_unknown_never_not_landed",
    ),
    // the device guard
    ("sync_seen_regresses", "the_memory_after_a_standing"),
    (
        "sync_judge_notice_coverage_ignored",
        "an_announcement_for_another_space_excuses_nothing",
    ),
    (
        "sync_judge_notice_epoch_ignored",
        "an_announcement_whose_epoch_is_not_above_the_seen_one_excuses_nothing",
    ),
    (
        "sync_judge_lost_timeline_excused",
        "a_head_below_the_announced_epoch_is_a_rollback_even_when_an_announcement_applies",
    ),
    (
        "sync_judge_unannounced_epoch_accepted",
        "a_served_epoch_above_seen_without_a_notice_is_unannounced_epoch",
    ),
    (
        "sync_judge_lost_timeline_accepted",
        "a_lost_timeline_head_with_a_higher_version_is_refused_without_any_notice",
    ),
    (
        "sync_judge_rollback_as_behind",
        "judge_lower_version_is_rollback",
    ),
    (
        "sync_judge_equivocation_unchecked",
        "same_version_other_value_is_equivocation",
    ),
    // the logs
    ("sync_log_size_unchecked", "record_over_64_kib_is_too_large"),
    (
        "sync_log_access_unchecked",
        "author_without_write_access_refused",
    ),
    (
        "sync_log_signature_unchecked",
        "author_signature_is_verified",
    ),
    (
        "sync_log_seq_skips",
        "append_assigns_consecutive_seq_from_1",
    ),
    (
        "sync_log_quota_per_log",
        "cap_is_per_account_across_logs_not_per_log",
    ),
    (
        "sync_log_quota_unchecked",
        "append_at_the_account_cap_is_quota_exceeded_with_a_compact_notice",
    ),
    (
        "sync_read_compacted_as_empty",
        "compaction_frees_quota_and_read_reports_compacted_through",
    ),
    (
        "sync_read_count_unbounded",
        "read_is_bounded_by_count_and_bytes_and_resumable",
    ),
    (
        "sync_read_bytes_unbounded",
        "read_is_bounded_by_count_and_bytes_and_resumable",
    ),
    (
        "sync_compact_wrong_log_allowed",
        "compaction_needs_a_pack_proof_and_never_drops_beyond_it",
    ),
    (
        "sync_compact_beyond_proof_allowed",
        "compaction_needs_a_pack_proof_and_never_drops_beyond_it",
    ),
    (
        "sync_compact_quota_not_released",
        "compaction_frees_quota_and_read_reports_compacted_through",
    ),
    // watch
    (
        "sync_watch_limit_unchecked",
        "subscription_over_20000_spaces_is_too_many_bad_request",
    ),
    (
        "sync_watch_keeps_lowest_level",
        "duplicate_spaces_collapse_to_the_highest_level",
    ),
    (
        "sync_watch_keeps_inaccessible",
        "no_access_and_unknown_spaces_are_dropped_identically",
    ),
];

/// The text of every test source of the crate: `tests/*.rs` and `tests/common/mod.rs`.
fn test_sources() -> String {
    let mut out = String::new();
    let mut folders = vec![root().join("tests")];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).expect("tests folder") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push_str(&fs::read_to_string(&path).expect("test source"));
                out.push('\n');
            }
        }
    }
    out
}

#[test]
fn mutant_guards_in_the_code_are_the_documented_set_and_each_has_a_killer() {
    let mut in_code = BTreeSet::new();
    for (name, text) in source_files() {
        for line in text.lines() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            let mut rest = t;
            while let Some(at) = rest.find("mutant!(\"") {
                let after = &rest[at + "mutant!(\"".len()..];
                let end = after
                    .find('"')
                    .unwrap_or_else(|| panic!("src/{name}: an unterminated mutant id: {t}"));
                in_code.insert(after[..end].to_string());
                rest = &after[end..];
            }
        }
    }
    let documented: BTreeSet<String> = MUTANTS.iter().map(|(id, _)| (*id).to_string()).collect();
    assert_eq!(documented.len(), MUTANTS.len(), "an id twice in the table");
    assert_eq!(
        in_code, documented,
        "the guards in src/ and the table of this test are one set"
    );
    let tests = test_sources();
    for (id, killer) in MUTANTS {
        assert!(
            id.starts_with("sync_") && id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{id}: lower-case snake case under the prefix of the crate"
        );
        assert!(
            tests.contains(&format!("fn {killer}()")),
            "{id}: its killer `{killer}` is no test of this crate"
        );
    }
}

/// The names the brief's checks give (section 6): each is a `#[test]` of exactly that name somewhere in
/// `tests/`, or the check says otherwise (the `compile_fail` doctests of `src/` and the ignored golden).
#[test]
fn every_named_check_of_the_brief_is_a_test() {
    let named = [
        // 1 CAS
        "first_cas_with_none_creates_version_1",
        "cas_with_the_current_version_advances_by_one",
        "cas_with_a_stale_version_conflicts_and_returns_the_current_head",
        "cas_expecting_none_on_an_existing_register_conflicts",
        "head_value_over_head_max_is_too_large",
        "of_concurrent_cas_with_one_expected_version_exactly_one_wins",
        // 2 history and retention
        "history_is_oldest_first_bounded_and_resumable",
        "prune_drops_versions_older_than_retention_but_never_the_current_head",
        "a_version_under_a_lock_survives_prune_until_retain_until",
        "prune_is_deterministic_for_the_same_now",
        // 3 lost reply
        "lost_reply_finds_my_write_as_the_head",
        "lost_reply_finds_my_write_in_history_after_others_advanced",
        "lost_reply_not_landed_when_history_shows_no_such_value",
        "lost_reply_with_too_short_history_is_unknown_never_not_landed",
        // 4 restore
        "restore_as_of_writes_a_new_higher_version_with_the_old_value_and_epoch_plus_one",
        "restore_as_of_before_the_first_version_is_refused",
        "after_a_restore_a_cas_on_the_old_expected_version_conflicts",
        "restore_as_of_needs_the_authorization_witness",
        // 5 device guard
        "judge_newer_verified_epoch_is_restored_whatever_the_counter_says",
        "judge_same_version_same_value_is_in_sync",
        "judge_higher_version_is_behind",
        "judge_lower_version_is_rollback",
        "judge_own_counter_drive_decides_by_epoch_only",
        "a_replayed_old_head_is_a_rollback_not_a_reset",
        "an_announcement_for_another_space_excuses_nothing",
        "an_announcement_whose_epoch_is_not_above_the_seen_one_excuses_nothing",
        "an_unverified_announcement_excuses_nothing",
        "same_version_other_value_is_equivocation",
        "a_served_epoch_above_seen_without_a_notice_is_unannounced_epoch",
        "baseline_of_a_new_pairing_is_the_current_epoch",
        "the_seen_version_never_decreases_over_10000_seeded_sequences",
        // 6 append log
        "append_assigns_consecutive_seq_from_1",
        "record_over_64_kib_is_too_large",
        "append_at_the_account_cap_is_quota_exceeded_with_a_compact_notice",
        "cap_is_per_account_across_logs_not_per_log",
        "compaction_frees_quota_and_read_reports_compacted_through",
        "compaction_needs_a_pack_proof_and_never_drops_beyond_it",
        "read_is_bounded_by_count_and_bytes_and_resumable",
        "author_signature_is_verified",
        "author_without_write_access_refused",
        "append_is_deterministic_across_two_replicas_state_hash",
        // 7 watch
        "subscription_over_20000_spaces_is_too_many_bad_request",
        "duplicate_spaces_collapse_to_the_highest_level",
        "no_access_and_unknown_spaces_are_dropped_identically",
        "levels_live_sync_notify_are_the_only_ones",
        // 8 limits
        "memory_is_bounded_by_the_caps",
        // 9 the seam guard
        "public_api_exposes_no_clock_read_no_io",
        // 10 golden
        "head_record_and_log_record_match_azlin_contract_fixtures",
    ];
    let tests = test_sources();
    for name in named {
        assert!(
            tests.contains(&format!("fn {name}()")),
            "the brief's check `{name}` is no test of this crate"
        );
    }
}

/// Check 10 (rule 9 of the brief): the head record and the log record against the golden fixtures of
/// `azlin-contract` (S5's `head_record.json` and `log_record.json`). The fixtures do not exist yet, so
/// the test stays `#[ignore]`d until INT-WIRE lands them; the expected bytes it can check without them
/// are inline: the provisional preimage of a log record (the same bytes `tests/logs.rs` pins and an
/// independent Ed25519 implementation signed). When the fixtures exist, the body below fails with the
/// message of what to do next, which is the reminder.
#[test]
#[ignore = "X-DELTA-W1S16A-14: S5 fixture not landed"]
fn head_record_and_log_record_match_azlin_contract_fixtures() {
    let dir = root().join("../azlin-contract/fixtures/c4");
    let head = fs::read_to_string(dir.join("head_record.json")).expect(
        "S5's fixture head_record.json (INT-WIRE copies it into azlin-contract/fixtures/c4)",
    );
    let log = fs::read_to_string(dir.join("log_record.json")).expect(
        "S5's fixture log_record.json (INT-WIRE copies it into azlin-contract/fixtures/c4)",
    );
    // The fields of a head record the register keeps (DRIFT X-DELTA-W1S16A-1 and -4).
    for field in [
        "version",
        "value",
        "at",
        "writer",
        "restore_epoch",
        "retain_until",
    ] {
        assert!(head.contains(field), "head_record.json lacks `{field}`");
    }
    // The fields of a log record (section 5.13: `author_device` and a signature).
    for field in ["author", "body", "sig"] {
        assert!(log.contains(field), "log_record.json lacks `{field}`");
    }
}
