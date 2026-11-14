//! The crate's hygiene, the part a test can hold (the launch rules of the W1 lanes, check 10 of the
//! brief): the manifest lists exactly the dependencies of the final manifest (and none of the
//! forbidden ones), the crate root forbids unsafe code and denies the panicking constructs, the
//! library code names no panicking call, lenient default, clock, file, network, thread, environment
//! or random source (`no_unwrap_in_non_test_code`), every public item names the plan section it
//! implements and its caller or the later unit it is a seam of (`public_items_cite_a_plan_section`),
//! every source file is declared, and every plan section a comment cites exists in
//! `planning/architecture.md` (COMMON-W1R R7).
//!
//! `cargo clippy --all-targets -- -D warnings`, `rustfmt --check` and the builds with the features
//! are the lane's holds: cargo cannot run inside `cargo test`.
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
fn c10_crate_hygiene() {
    let manifest = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");

    // The dependencies of the final manifest, exactly: nothing more, nothing less.
    let dependencies: BTreeSet<String> = section_keys(&manifest, "[dependencies]")
        .into_iter()
        .collect();
    let expected: BTreeSet<String> = [
        "azlin-keys",
        "azlin-seams",
        "blake3",
        "ed25519-dalek",
        "zeroize",
        "thiserror",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(dependencies, expected, "the dependencies of the crate");
    for forbidden in [
        "tokio",
        "reqwest",
        "rand",
        "proptest",
        "serde",
        "serde_json",
        "azlin-cbor",
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
    // azlin-keys is used with its default features off.
    let keys = section_value(&manifest, "[dependencies]", "azlin-keys").expect("azlin-keys line");
    assert!(keys.contains("default-features = false"), "{keys}");

    // Package facts.
    assert_eq!(
        section_value(&manifest, "[package]", "name").as_deref(),
        Some("\"azlin-chain\"")
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
    // DRIFT X-DELTA-W1CLOSE1-1: the six modules of the brief (section 3) and the ONE lib-test module of
    // the replayable patches of X-DELTA-W1S17A-3 (the replay tool runs lib tests only, so its killers
    // cannot live in tests/); lib.rs declares it behind `#[cfg(test)]`.
    let expected: BTreeSet<String> = [
        "checkpoint",
        "entry",
        "link",
        "peers",
        "pin",
        "restore",
        "restore_epoch_tests",
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
    "std::time",
    "std::thread",
    "std::process",
    "std::env",
    "SystemTime",
    "Instant",
    "rand::",
    "serde",
    ".unwrap()",
    ".expect(",
    "panic!",
    "unreachable!",
    "todo!",
    "unimplemented!",
    "unwrap_or_default",
    "unwrap_unchecked",
    "get_unchecked",
    "mem::transmute",
    "println!",
    "dbg!",
    ".ok()",
    "let _ =",
    "map_or(0",
    "unwrap_or(false)",
];

#[test]
fn no_unwrap_in_non_test_code() {
    let mut scanned = 0usize;
    for (name, text) in source_files() {
        // DRIFT X-DELTA-W1CLOSE1-1: the one lib-test module of the replayable patches is test code: its
        // file is not library code, and `lib.rs` declares it behind `#[cfg(test)]` and nothing else.
        if name == "restore_epoch_tests.rs" {
            continue;
        }
        let text = text.replace("#[cfg(test)]\nmod restore_epoch_tests;\n", "");
        // Every other file keeps no unit tests in `src/`: the whole file is library code.
        assert!(
            !text.contains("#[cfg(test)]"),
            "{name}: tests live in tests/"
        );
        for (n, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            scanned += 1;
            for banned in BANNED {
                assert!(
                    !t.contains(banned),
                    "src/{name}: `{banned}` in library code: {t} (line {})",
                    n + 1
                );
            }
        }
    }
    assert!(
        scanned > 500,
        "the scan saw only {scanned} lines: has the layout changed?"
    );
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
        checked >= 35,
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
        cited.len() >= 6,
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
/// `AZLIN_MUTANT=<id> cargo test -p azlin-chain --features azlin-seams/mutants` and expects a red test.
const MUTANTS: &[(&str, &str)] = &[
    ("chain_checks_chain_id", "entry_for_another_chain_refused"),
    (
        "chain_checks_intact",
        "corrupt_entry_class_is_refetch_not_fork",
    ),
    (
        "chain_genesis_prev_zero",
        "genesis_must_be_seq_zero_with_zero_prev",
    ),
    (
        "chain_checks_seq",
        "gap_and_replay_refused_with_expected_seq",
    ),
    ("chain_checks_prev", "wrong_prev_names_expected_and_got"),
    ("chain_checks_stamp", "stamp_going_backwards_refused"),
    (
        "chain_checks_epoch",
        "an_entry_written_before_the_restore_is_refused",
    ),
    (
        "chain_fork_check_off",
        "same_position_different_entry_is_a_fork_and_alarms",
    ),
    ("chain_second_genesis_check_off", "second_genesis_refused"),
    (
        "chain_pin_skips_signature_check",
        "head_pin_rejects_unsigned_or_wrongly_signed_head",
    ),
    (
        "chain_pin_accepts_older",
        "head_pin_accepts_newer_and_same_refuses_older",
    ),
    (
        "chain_pin_fork_unchecked",
        "head_pin_fork_at_same_seq_alarms_and_keeps_the_old_head",
    ),
    (
        "chain_pin_epoch_floor_off",
        "a_head_signed_before_the_restore_cannot_advance_the_pin",
    ),
    (
        "chain_peer_skips_signature_check",
        "peer_head_with_bad_signature_is_ignored_not_an_alarm",
    ),
    (
        "chain_peer_ahead_is_quiet",
        "peer_ahead_of_us_raises_withheld_alarm_naming_both_seqs",
    ),
    ("chain_peer_fork_is_quiet", "peer_fork_alarms"),
    (
        "chain_peer_epoch_ignored",
        "peer_head_of_another_restore_epoch_is_judged_by_the_epoch_before_the_position",
    ),
    (
        "chain_checkpoint_entries_off",
        "checkpoint_due_at_1000_entries_and_at_30_days",
    ),
    (
        "chain_checkpoint_age_off",
        "checkpoint_due_at_1000_entries_and_at_30_days",
    ),
    (
        "chain_checkpoint_head_unverified",
        "entries_before_the_checkpoint_are_not_needed",
    ),
    (
        "chain_restore_group_alone_accepted",
        "restore_record_signed_by_group_alone_is_refused_under_strict_trust",
    ),
    (
        "chain_restore_replay_check_off",
        "replayed_restore_record_refused",
    ),
    (
        "chain_restore_late_record_check_off",
        "a_late_record_cannot_reanchor_behind_a_newer_head",
    ),
    (
        "chain_restore_trust_skipped",
        "the_two_trusts_by_every_signature_set",
    ),
    (
        "chain_restore_resend_off",
        "reanchor_returns_the_positions_the_device_must_send_again",
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
            id.starts_with("chain_") && id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{id}: lower-case snake case under the prefix of the crate"
        );
        assert!(
            tests.contains(&format!("fn {killer}()")),
            "{id}: its killer `{killer}` is no test of this crate"
        );
    }
}
