//! The crate's hygiene, the part a test can hold (the launch rules of the W1 lanes, checks 10 of
//! the brief): the manifest lists exactly the dependencies and features of the final manifest (and
//! none of the forbidden ones), the crate root forbids unsafe code and denies the panicking
//! constructs, the library code names no panicking call, lenient default, clock, file, network,
//! thread or random source (`no_unwrap_in_non_test_code`), every public item names the plan
//! section it implements and its caller or the later unit it is a seam of
//! (`public_items_cite_a_plan_section`), every source file is declared, and every plan section a
//! comment cites exists in `planning/architecture.md` (COMMON-W1R R7).
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
        "azlin-chain",
    ] {
        assert!(
            !dependencies.contains(forbidden),
            "{forbidden} is not a dependency of this crate"
        );
    }
    for section in ["[dev-dependencies]", "[build-dependencies]"] {
        assert!(
            section_keys(&manifest, section).is_empty(),
            "{section} is empty"
        );
    }
    assert!(!root().join("build.rs").exists());
    // azlin-keys is used with its default features off: a verifier crate pulls no getrandom.
    let keys = section_value(&manifest, "[dependencies]", "azlin-keys").expect("azlin-keys line");
    assert!(keys.contains("default-features = false"), "{keys}");

    // The features: `os-rng` forwards to azlin-keys, `mutants` is declared by this crate alone
    // (COMMON-W1R R19) and switches nothing else on.
    let features: BTreeSet<String> = section_keys(&manifest, "[features]").into_iter().collect();
    let expected_features: BTreeSet<String> = ["os-rng", "mutants"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(features, expected_features);
    assert_eq!(
        section_value(&manifest, "[features]", "os-rng").as_deref(),
        Some("[\"azlin-keys/os-rng\"]")
    );
    assert_eq!(
        section_value(&manifest, "[features]", "mutants").as_deref(),
        Some("[]")
    );

    // Package facts.
    assert_eq!(
        section_value(&manifest, "[package]", "name").as_deref(),
        Some("\"azlin-seams\"")
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
    let expected: BTreeSet<String> = ["bytes", "hash", "mutants", "sim", "verify", "vocab"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        declared, expected,
        "the modules of the brief (section 3 and 0R)"
    );
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
            scanned += 1;
            for banned in BANNED {
                assert!(
                    !t.contains(banned),
                    "src/{name}: `{banned}` in library code: {t} (line {})",
                    n + 1
                );
            }
            // The environment is read in exactly one place: the mutation guard.
            if name != "mutants.rs" {
                assert!(
                    !t.contains("std::env"),
                    "src/{name}: `std::env` outside mutants.rs: {t} (line {})",
                    n + 1
                );
            }
        }
    }
    assert!(
        scanned > 300,
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

/// A public item: a line at the start of a line that declares one (`pub fn`, `pub struct`,
/// `pub const`, ...), or the `macro_rules!` of an exported macro. Methods inside `impl` blocks are
/// covered by their type.
fn is_public_item(line: &str) -> bool {
    (line.starts_with("pub ") && !line.starts_with("pub use ") && !line.starts_with("pub mod "))
        || line.starts_with("macro_rules! mutant")
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
        checked >= 17,
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

/// The plan, by the rule R7: `AZLIN_PLAN_MD`; else (azul crate) `CARGO_MANIFEST_DIR/../../../azul-apps/planning/architecture.md`.
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
