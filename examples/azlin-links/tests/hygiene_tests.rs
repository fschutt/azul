//! The crate's hygiene, the part a test can hold (the launch rules of the W1 lanes): the manifest lists
//! exactly the dependencies of the final manifest (and none of the forbidden ones), the crate root
//! forbids unsafe code and denies the panicking constructs, no function of the library reads a clock,
//! the environment, a file, a thread, a random source or an ADDRESS (`no_function_reads_a_clock_or_an_address`:
//! a device is counted by a random token it keeps, never by an address), the library code names no
//! panicking call and no lenient default, every public item of lane S16c1 names the plan section it
//! implements and its caller or the later unit it is a seam of, every source file is declared, every
//! plan section a comment cites exists in the plan (COMMON-W1R R7), and the mutation guards of
//! lane S16c1's files are the ids of `MUTANT_GUARDS`, each with a killer test that exists.
//!
//! `cargo clippy --all-targets -- -D warnings`, `rustfmt --check` and the build for wasm32 are the
//! lane's holds: cargo cannot run inside `cargo test`.
//!
//! The scans of the files of the second lane (S16c2: `resolve`, `counters`, `credential`, `misses`,
//! `open_log`, `notify`) are the banned-token and the clock-and-address scans only: their guards and
//! their documentation are theirs.
//!
//! Implements planning/architecture.md section 1.4 (engineering rules: no panics on input, I/O is a
//! parameter, every public item has a caller), section 5.12 (links count devices by tokens) and
//! section 5.15 (M7: counters only).

mod common;

use std::collections::BTreeSet;
use std::fs;

use azlin_links::MUTANT_GUARDS;
use common::{plan_text, root};

/// The files of lane S16c1.
const C1_FILES: [&str; 10] = [
    "lib", "audience", "bounds", "id", "kind", "options", "record", "strength", "update",
    "validate",
];

/// The files of lane S16c2 (stubs until that lane fills them).
const C2_FILES: [&str; 6] = [
    "resolve",
    "counters",
    "credential",
    "misses",
    "open_log",
    "notify",
];

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

/// The files of lane S16c1 only.
fn c1_source_files() -> Vec<(String, String)> {
    source_files()
        .into_iter()
        .filter(|(name, _)| C1_FILES.contains(&name.trim_end_matches(".rs")))
        .collect()
}

#[test]
fn crate_hygiene() {
    let manifest = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");

    // The dependencies of the final manifest, exactly: nothing more, nothing less.
    let dependencies: BTreeSet<String> = section_keys(&manifest, "[dependencies]")
        .into_iter()
        .collect();
    let expected: BTreeSet<String> = [
        "azlin-seams",
        "azlin-policy",
        "blake3",
        "subtle",
        "thiserror",
    ]
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
        "opaque-ke",
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
        Some("\"azlin-links\"")
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
    // The modules of the brief (section 3): lane S16c1's nine and lane S16c2's six.
    let expected: BTreeSet<String> = C1_FILES
        .iter()
        .chain(C2_FILES.iter())
        .filter(|name| **name != "lib")
        .map(|s| s.to_string())
        .collect();
    assert_eq!(declared, expected, "the modules of the brief (section 3)");
    // The second lane's files exist (stubs until that lane replaces them: its own lane owns them).
    for name in C2_FILES {
        let text = fs::read_to_string(root().join(format!("src/{name}.rs"))).expect("a file");
        assert!(!text.trim().is_empty(), "src/{name}.rs is empty");
    }
}

/// The tokens that do not belong in library code (QUALITY rules 4 and 5, launch rule 8): panics,
/// lenient defaults, unsafe code, and every way of reading the outside world.
const BANNED: &[&str] = &[
    "unsafe {",
    "unsafe fn",
    "unsafe impl",
    "unsafe trait",
    "std::fs",
    "std::io",
    "std::thread",
    "std::process",
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

/// The tokens that read a clock, the environment or an address: "`max_devices`: distinct devices
/// or browsers, counted by a random token each keeps (never an address)"; every function takes the
/// time as a `Stamp` parameter.
const CLOCK_ENVIRONMENT_AND_ADDRESS: &[&str] = &[
    "std::time",
    "SystemTime",
    "Instant",
    "now()",
    "elapsed(",
    "std::env",
    "env::var",
    "env!(",
    "option_env!",
    "var_os",
    "current_dir",
    "args()",
    "std::net",
    "IpAddr",
    "SocketAddr",
    "Ipv4Addr",
    "Ipv6Addr",
    "ToSocketAddrs",
    "TcpStream",
    "UdpSocket",
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
        scanned > 1_000,
        "the scan saw only {scanned} lines: has the layout changed?"
    );
}

/// Check 11 (the part of it that is lane S16c1's): no function of the library reads a clock, the
/// environment or an address (the scan that the hold script's `rg` repeats over the whole source).
#[test]
fn no_function_reads_a_clock_or_an_address() {
    let mut scanned = 0usize;
    for (name, text) in source_files() {
        for (n, line) in code_lines(&text) {
            scanned += 1;
            for banned in CLOCK_ENVIRONMENT_AND_ADDRESS {
                assert!(
                    !line.contains(banned),
                    "src/{name}: `{banned}` in library code: {line} (line {n})"
                );
            }
        }
    }
    assert!(
        scanned > 1_000,
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
/// `impl` blocks are covered by their type; the two ids of `held_id16!` are checked by name below.
fn is_public_item(line: &str) -> bool {
    line.starts_with("pub ") && !line.starts_with("pub use ") && !line.starts_with("pub mod ")
}

#[test]
fn public_items_cite_a_plan_section() {
    let mut checked = 0usize;
    for (name, text) in c1_source_files() {
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
        checked >= 60,
        "the scan found only {checked} public items: has the layout changed?"
    );

    // The two ids that a macro defines carry the same doc lines (the macro's invocation holds them).
    let record = fs::read_to_string(root().join("src/record.rs")).expect("src/record.rs");
    for id in ["RequestId", "LogId"] {
        let at = record
            .find(&format!("\n    {id}\n"))
            .unwrap_or_else(|| panic!("record.rs does not define {id}"));
        let before: Vec<&str> = record[..at].trim_end().lines().collect();
        let doc = doc_above(&before, before.len());
        assert!(
            doc.contains("planning/architecture.md section 5.12") && doc.contains("Caller:"),
            "{id}: {doc}"
        );
    }
}

/// The numbers that follow the word `section`/`sections` in the comments of lane S16c1's files: the
/// plan sections the code cites.
fn cited_sections() -> BTreeSet<String> {
    let mut cited = BTreeSet::new();
    for (_, text) in c1_source_files() {
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
        cited.len() >= 5,
        "the scan found only {cited:?}: has the comment style changed?"
    );
    for expected in ["1.4", "3.9", "5.10", "5.12", "5.15", "12"] {
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

/// The ids of the guards in the files of lane S16c1: the string of every `azlin_seams::mutant!("...")`,
/// wherever rustfmt put the line breaks.
fn guards_in_source() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for (_, text) in c1_source_files() {
        let code: String = code_lines(&text)
            .map(|(_, line)| line)
            .collect::<Vec<_>>()
            .join("\n");
        let mut rest = code.as_str();
        while let Some(at) = rest.find("azlin_seams::mutant!(") {
            let after = rest[at + "azlin_seams::mutant!(".len()..].trim_start();
            let Some(after) = after.strip_prefix('"') else {
                rest = &rest[at + 1..];
                continue;
            };
            match after.find('"') {
                Some(end) => {
                    ids.insert(after[..end].to_string());
                    rest = &after[end..];
                }
                None => break,
            }
        }
    }
    ids
}

/// The text of every test file (`tests/*.rs`).
fn test_files() -> Vec<String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(root().join("tests")).expect("tests folder") {
        let entry = entry.expect("a directory entry");
        if entry.path().extension().is_some_and(|e| e == "rs") {
            out.push(fs::read_to_string(entry.path()).expect("test file"));
        }
    }
    out
}

#[test]
fn every_mutation_guard_of_the_source_is_listed_and_every_listed_id_is_a_guard() {
    let in_source = guards_in_source();
    let listed: BTreeSet<String> = MUTANT_GUARDS.iter().map(|(id, _)| id.to_string()).collect();
    assert_eq!(
        in_source, listed,
        "the guards of the source are the ids of MUTANT_GUARDS"
    );
    assert_eq!(
        listed.len(),
        MUTANT_GUARDS.len(),
        "the ids are unique (one row each)"
    );
    for id in &listed {
        assert!(id.starts_with("links_"), "{id}");
        assert!(
            id.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{id}"
        );
    }
    // The guards of the second lane (the resolver's four) are not ours.
    for foreign in [
        "links_resolve_counts_open",
        "links_one_time_used_at_resolve",
        "links_max_devices_unbounded",
        "links_credential_ttl_unbounded",
    ] {
        assert!(!listed.contains(foreign), "{foreign} is lane S16c2's");
    }
    // Every guard has a killer test that exists, and the doc table of MUTANT_GUARDS names both.
    let tests = test_files();
    let lib = fs::read_to_string(root().join("src/lib.rs")).expect("src/lib.rs");
    for (id, killer) in MUTANT_GUARDS {
        assert!(
            tests.iter().any(|t| t.contains(&format!("fn {killer}()"))),
            "the killer of {id}: no test {killer}"
        );
        assert!(
            lib.contains(&format!("`{id}`")) && lib.contains(&format!("`{killer}`")),
            "{id} {killer}"
        );
    }
}
