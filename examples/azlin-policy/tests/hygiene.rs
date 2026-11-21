//! The crate's hygiene, the part a test can hold (the launch rules of the W1 lanes; check 8 of the
//! brief): the manifest lists exactly the dependencies of the final manifest (and none of the
//! forbidden ones), the crate root forbids unsafe code and denies the panicking constructs, no
//! function of the library reads a clock, the environment, a file, a network, a thread or a random
//! source (`no_function_of_this_crate_reads_a_clock_or_the_environment`), the library code names no
//! panicking call and no lenient default, every public item names the plan section it implements
//! and its caller or the later unit it is a seam of, every source file is declared, every plan
//! section a comment cites exists in the plan (COMMON-W1R R7), and the mutation guards of the source
//! are the ids of `MUTANT_IDS`.
//!
//! `cargo clippy --all-targets -- -D warnings`, `rustfmt --check` and the build for wasm32 are the
//! lane's holds: cargo cannot run inside `cargo test`.
//!
//! Implements planning/architecture.md section 1.4 (engineering rules: no panics on input, I/O is a
//! parameter, every public item has a caller) and section 5.10 (the policy never reads time).

mod common;

use std::collections::BTreeSet;
use std::fs;

use azlin_policy::MUTANT_IDS;
use common::{plan_text, root};

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

/// The source files of `src/`, by file name (flat: this crate has no module folder).
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
    let expected: BTreeSet<String> = ["azlin-seams", "thiserror"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(dependencies, expected, "the dependencies of the crate");
    for forbidden in [
        "serde",
        "serde_json",
        "ciborium",
        "azlin-cbor",
        "tokio",
        "reqwest",
        "rand",
        "proptest",
        "azlin-keys",
        "azlin-chain",
        "chrono",
        "time",
    ] {
        assert!(
            !dependencies.contains(forbidden),
            "{forbidden} is not a dependency of this crate"
        );
    }
    for section in ["[dev-dependencies]", "[build-dependencies]", "[features]"] {
        assert!(
            section_keys(&manifest, section).is_empty(),
            "{section} is empty"
        );
    }
    assert!(!root().join("build.rs").exists());

    // Package facts.
    assert_eq!(
        section_value(&manifest, "[package]", "name").as_deref(),
        Some("\"azlin-policy\"")
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
        "kind",
        "fields",
        "defaults",
        "patch",
        "tighten",
        "effective",
        "quorum",
        "link_policy",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(declared, expected, "the modules of the brief (section 3)");
}

/// The tokens that do not belong in library code (QUALITY rules 4 and 5, launch rule 8): panics,
/// lenient defaults, unsafe code, and every way of reading the outside world.
const BANNED: &[&str] = &[
    "unsafe {",
    "unsafe fn",
    "unsafe impl",
    "unsafe trait",
    "std::fs",
    "std::net",
    "std::thread",
    "std::process",
    "std::io",
    "rand::",
    "getrandom",
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
    "eprintln!",
    "dbg!",
    ".ok()",
    "let _ =",
    "map_or(0",
    "unwrap_or(false)",
];

/// The tokens that read a clock or the environment: the policy never reads time, and every input
/// is a parameter. Not even a `Stamp` or an `UnixTime` appears: the policy has durations (`Span`)
/// only.
const CLOCK_AND_ENVIRONMENT: &[&str] = &[
    "std::time",
    "SystemTime",
    "Instant",
    "UnixTime",
    "Stamp",
    "Clock",
    "now()",
    "elapsed(",
    "std::env",
    "env::var",
    "env!(",
    "option_env!",
    "var_os",
    "current_dir",
    "args()",
];

/// The code lines of a source file: the lines that are not comments.
fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(n, line)| (n + 1, line.trim_start()))
        .filter(|(_, line)| !line.starts_with("//"))
}

#[test]
fn no_unwrap_in_non_test_code() {
    let mut scanned = 0usize;
    for (name, text) in source_files() {
        // This crate keeps no unit tests in `src/`: the whole file is library code.
        assert!(
            !text.contains("#[cfg(test)]"),
            "{name}: tests live in tests/"
        );
        for (n, line) in code_lines(&text) {
            scanned += 1;
            for banned in BANNED {
                assert!(
                    !line.contains(banned),
                    "src/{name}: `{banned}` in library code: {line} (line {n})"
                );
            }
        }
    }
    assert!(
        scanned > 400,
        "the scan saw only {scanned} lines: has the layout changed?"
    );
}

/// Check 8: no function of the library reads a clock or the environment (the scan that the hold
/// script's `rg` repeats over the whole source).
#[test]
fn no_function_of_this_crate_reads_a_clock_or_the_environment() {
    let mut scanned = 0usize;
    for (name, text) in source_files() {
        for (n, line) in code_lines(&text) {
            scanned += 1;
            for banned in CLOCK_AND_ENVIRONMENT {
                assert!(
                    !line.contains(banned),
                    "src/{name}: `{banned}` in library code: {line} (line {n})"
                );
            }
        }
    }
    assert!(
        scanned > 400,
        "the scan saw only {scanned} lines: has the layout changed?"
    );
    // The one switch of the mutation guards is read inside azlin-seams, not here: the library
    // names the macro and nothing of the environment.
    let all: String = source_files().into_iter().map(|(_, text)| text).collect();
    assert!(all.contains("azlin_seams::mutant!"));
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
/// `impl` blocks are covered by their type, and `Field` (written by a macro) is checked by name.
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
                doc.contains("planning/architecture.md section ") || doc.contains("planning/architecture.md §"),
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

    // The enum written by the macro of fields.rs carries the same doc lines.
    let fields = fs::read_to_string(root().join("src/fields.rs")).expect("src/fields.rs");
    let at = fields
        .find("pub enum Field {")
        .expect("the macro defines Field");
    let before: Vec<&str> = fields[..at].trim_end().lines().collect();
    let doc = doc_above(&before, before.len());
    assert!(
        doc.contains("planning/architecture.md section 5.10") && doc.contains("Caller:"),
        "{doc}"
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

#[test]
fn every_cited_plan_section_exists_in_the_plan() {
    let cited = cited_sections();
    assert!(
        cited.len() >= 8,
        "the scan found only {cited:?}: has the comment style changed?"
    );
    for expected in ["5.9", "5.10", "5.11", "5.12", "5.15", "7.2", "12"] {
        assert!(
            cited.contains(expected),
            "the code does not cite section {expected}: {cited:?}"
        );
    }
    let Some(plan) = plan_text() else {
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

/// The ids of the guards in the source: the string of every `mutant!("...")`.
fn guards_in_source() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for (_, text) in source_files() {
        for (_, line) in code_lines(&text) {
            let mut rest = line;
            while let Some(at) = rest.find("azlin_seams::mutant!(\"") {
                let after = &rest[at + "azlin_seams::mutant!(\"".len()..];
                match after.find('"') {
                    Some(end) => {
                        ids.insert(after[..end].to_string());
                        rest = &after[end..];
                    }
                    None => break,
                }
            }
        }
    }
    ids
}

#[test]
fn every_mutation_guard_of_the_source_is_listed_and_every_listed_id_is_a_guard() {
    let in_source = guards_in_source();
    let listed: BTreeSet<String> = MUTANT_IDS.iter().map(|id| id.to_string()).collect();
    assert_eq!(
        in_source, listed,
        "the guards of the source are the ids of MUTANT_IDS"
    );
    assert_eq!(listed.len(), MUTANT_IDS.len(), "the ids are unique");
    for id in MUTANT_IDS {
        assert!(id.starts_with("policy_"), "{id}");
        assert!(
            id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{id}"
        );
    }
    // Every guard has a killer test that exists (the table in the doc of `MUTANT_IDS`).
    let killers = [
        (
            "policy_effective_skips_clamp",
            "effective",
            "effective_policy_clamps_a_child_looser_than_a_later_tightened_parent",
        ),
        (
            "policy_epoch_always_bumps",
            "effective",
            "policy_epoch_bumps_only_when_the_effective_policy_changes",
        ),
        (
            "policy_quorum_needs_one",
            "quorum",
            "quorum_two_for_business_admin_change",
        ),
        (
            "policy_quorum_counts_devices",
            "quorum",
            "quorum_counts_distinct_people_never_devices",
        ),
        (
            "policy_quorum_counts_apps",
            "quorum",
            "quorum_app_principal_never_counts",
        ),
        (
            "policy_quorum_off_by_one",
            "quorum",
            "quorum_two_for_business_admin_change",
        ),
    ];
    let named: BTreeSet<String> = killers.iter().map(|(id, _, _)| id.to_string()).collect();
    assert_eq!(named, listed, "every guard has a killer in this table");
    for (id, file, test) in killers {
        let text =
            fs::read_to_string(root().join(format!("tests/{file}.rs"))).expect("a test file");
        assert!(
            text.contains(&format!("fn {test}()")),
            "the killer of {id}: tests/{file}.rs has no test {test}"
        );
        // And the doc table of MUTANT_IDS names it too.
        let lib = fs::read_to_string(root().join("src/lib.rs")).expect("src/lib.rs");
        assert!(
            lib.contains(&format!("`{id}`")) && lib.contains(&format!("`{test}`")),
            "{id} {test}"
        );
    }
}
