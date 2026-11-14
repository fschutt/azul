//! The crate's hygiene (the common rules of the W1 lanes, rule 7; S9E-15 and S9E-19 of the brief): the
//! library code names no panicking call, lenient default, hash map, clock, file, network, thread,
//! database, environment variable or random source of its own; no integer named like a time or a size is
//! a field, parameter or constant; the manifest lists exactly the final dependencies, no forbidden crate,
//! no feature (the brief's `interim-codec` is dropped: the codec is azlin-cbor); the crate root forbids
//! unsafe code and denies the panicking constructs; every source file is declared and every declaration
//! has its file; every mutation guard has a unique id.
//!
//! The gate kit of azlin-proto is not available to an azul crate, so this is its local mirror: a small
//! lexer blanks comments, string contents and `#[cfg(test)]` items, then the needles are searched line by
//! line. EVERY needle carries a planted case it must find and a clean case it must not (the self test
//! fails on a needle without both), as the kit does.
//!
//! Implements planning/architecture.md section 1.4 (rules 1, 3, 4 and 11: I/O is a parameter, protocol
//! code is a pure codec, every limit is stated, invariants are asserted).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ---- the lexer ------------------------------------------------------------------------------

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// `src` with comments and the contents of string and character literals replaced by spaces (line
/// breaks kept, so every line number is the file's).
fn blank(src: &str) -> String {
    let b = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    let keep_break = |c: u8| if c == b'\n' { b'\n' } else { b' ' };
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        if c == b'/' && next == Some(b'/') {
            while i < b.len() && b[i] != b'\n' {
                out.push(b' ');
                i += 1;
            }
        } else if c == b'/' && next == Some(b'*') {
            let mut depth = 0;
            while i < b.len() {
                if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    out.extend(b"  ");
                    i += 2;
                } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    out.extend(b"  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(keep_break(b[i]));
                    i += 1;
                }
            }
        } else if c == b'r'
            && (next == Some(b'"') || next == Some(b'#'))
            && (i == 0 || !is_ident(b[i - 1]))
        {
            // a raw string r"..", r#".."#
            let mut hashes = 0;
            let mut j = i + 1;
            while b.get(j) == Some(&b'#') {
                hashes += 1;
                j += 1;
            }
            if b.get(j) != Some(&b'"') {
                out.push(c);
                i += 1;
                continue;
            }
            out.extend(std::iter::repeat_n(b' ', j - i));
            i = j + 1;
            out.push(b'"');
            while i < b.len() {
                let closing = b[i] == b'"' && (1..=hashes).all(|k| b.get(i + k) == Some(&b'#'));
                if closing {
                    out.push(b'"');
                    out.extend(std::iter::repeat_n(b' ', hashes));
                    i += 1 + hashes;
                    break;
                }
                out.push(keep_break(b[i]));
                i += 1;
            }
        } else if c == b'"' {
            out.push(b'"');
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' && i + 1 < b.len() {
                    out.push(b' ');
                    out.push(keep_break(b[i + 1]));
                    i += 2;
                } else {
                    out.push(keep_break(b[i]));
                    i += 1;
                }
            }
            out.push(b'"');
            i += 1;
        } else if c == b'\'' {
            // a character literal, or a lifetime
            let rest = &src[i + 1..];
            let literal_end = if rest.starts_with('\\') {
                rest.get(2..).and_then(|r| r.find('\'')).map(|k| i + 3 + k)
            } else {
                rest.chars()
                    .next()
                    .filter(|ch| rest[ch.len_utf8()..].starts_with('\''))
                    .map(|ch| i + 1 + ch.len_utf8())
            };
            match literal_end {
                Some(end) => {
                    out.push(b'\'');
                    out.extend(std::iter::repeat_n(b' ', end - i - 1));
                    out.push(b'\'');
                    i = end + 1;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    String::from_utf8(out).expect("blanking keeps UTF-8")
}

/// `text` (already blanked) with every `#[cfg(test)]` item removed: the attribute and the item it is
/// on, to its `;` or its matching `}`.
fn without_test_items(text: &str) -> String {
    let mut bytes = text.as_bytes().to_vec();
    let mut from = 0;
    while let Some(found) = text[from..].find("#[cfg(test)]") {
        let start = from + found;
        let mut i = start + "#[cfg(test)]".len();
        let mut depth = 0;
        let mut end = bytes.len();
        while i < bytes.len() {
            match bytes[i] {
                b';' if depth == 0 => {
                    end = i + 1;
                    break;
                }
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        for byte in &mut bytes[start..end] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
        from = end;
    }
    String::from_utf8(bytes).expect("blanking keeps UTF-8")
}

/// The code of a source: comments, literals' contents and test items gone.
fn code(src: &str) -> String {
    without_test_items(&blank(src))
}

// ---- the needles ----------------------------------------------------------------------------

struct Needle {
    pattern: &'static str,
    planted: &'static str,
    clean: &'static str,
}

fn needles() -> Vec<Needle> {
    let n = |pattern, planted, clean| Needle {
        pattern,
        planted,
        clean,
    };
    vec![
        n(
            ".unwrap()",
            "fn f() { x.unwrap(); }",
            "// x.unwrap()\n#[cfg(test)]\nmod t { fn g() { y.unwrap(); } }",
        ),
        n(
            ".expect(",
            "fn f() { x.expect(\"why\"); }",
            "fn f() { let s = \"x.expect(\"; }",
        ),
        n(
            "panic!",
            "fn f() { panic!(\"no\"); }",
            "/* panic!(\"no\") */ fn f() {}",
        ),
        n(
            "unreachable!",
            "fn f() { unreachable!(); }",
            "// unreachable!\nfn f() {}",
        ),
        n(
            "todo!",
            "fn f() { todo!(); }",
            "#[cfg(test)]\nfn g() { todo!(); }",
        ),
        n(
            "unimplemented!",
            "fn f() { unimplemented!(); }",
            "/// unimplemented!\nfn f() {}",
        ),
        n(
            "unwrap_or_default",
            "fn f() { x.unwrap_or_default(); }",
            "// unwrap_or_default\nfn f() {}",
        ),
        n(
            "let _ =",
            "fn f() { let _ = write(); }",
            "fn f() { let _x = 1; let s = \"let _ =\"; }",
        ),
        n(".ok()", "fn f() { r.ok(); }", "fn f() { r.is_ok(); }"),
        n(
            "map_or(0",
            "fn f() { o.map_or(0, g); }",
            "fn f() { o.map_or(1, g); }",
        ),
        n(
            "HashMap",
            "use std::collections::HashMap;",
            "use std::collections::BTreeMap;",
        ),
        n(
            "HashSet",
            "use std::collections::HashSet;",
            "use std::collections::BTreeSet;",
        ),
        n(
            "SystemTime",
            "fn f() { SystemTime::now(); }",
            "// SystemTime\nfn f() {}",
        ),
        n(
            "Instant::now",
            "fn f() { Instant::now(); }",
            "fn f() { let instant = 1; }",
        ),
        n("std::fs", "use std::fs;", "use std::fmt;"),
        n(
            "std::net",
            "use std::net::TcpStream;",
            "use std::num::NonZeroU8;",
        ),
        n("tokio", "use tokio::sync::mpsc;", "// tokio is not used"),
        n(
            "rusqlite",
            "use rusqlite::Connection;",
            "// rusqlite is not used",
        ),
        n(
            "getrandom",
            "fn f() { getrandom::getrandom(&mut b); }",
            "// getrandom is the caller's",
        ),
        n(
            "std::thread",
            "fn f() { std::thread::spawn(g); }",
            "fn f() { let thread = 1; }",
        ),
        n(
            "std::process",
            "fn f() { std::process::exit(1); }",
            "fn f() { let process = 1; }",
        ),
        n(
            "std::env",
            "fn f() { std::env::var(\"X\"); }",
            "// std::env is the caller's\nfn f() {}",
        ),
        n(
            "env::var",
            "use std::env; fn f() { env::var(\"X\"); }",
            "fn f() { let s = \"env::var\"; }",
        ),
    ]
}

/// The lines of `text` (blanked code) that contain `pattern`.
fn lines_with(text: &str, pattern: &str) -> Vec<usize> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains(pattern))
        .map(|(n, _)| n + 1)
        .collect()
}

/// The integer type names a raw size, span or stamp would have.
const INTS: [&str; 12] = [
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];

/// The names that say a number is a time or a size.
const UNIT_SUFFIXES: [&str; 7] = ["secs", "ms", "millis", "days", "at", "bytes", "size"];

/// `(line, name)` of every `name: <integer>` (a field, a parameter, a constant) whose name is a time or
/// a size: `secs`, `*_secs`, `*_ms`, `*_millis`, `*_days`, `*_at`, `*_bytes`, `*_size`.
fn raw_unit_names(text: &str) -> Vec<(usize, String)> {
    let b = text.as_bytes();
    let mut found = Vec::new();
    let mut line = 1;
    for (i, &c) in b.iter().enumerate() {
        if c == b'\n' {
            line += 1;
        }
        if c != b':' || b.get(i + 1) == Some(&b':') || (i > 0 && b[i - 1] == b':') {
            continue;
        }
        // the identifier before the colon
        let mut end = i;
        while end > 0 && b[end - 1] == b' ' {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && is_ident(b[start - 1]) {
            start -= 1;
        }
        let name = &text[start..end];
        // the type after the colon
        let mut t = i + 1;
        while t < b.len() && b[t] == b' ' {
            t += 1;
        }
        let mut t_end = t;
        while t_end < b.len() && is_ident(b[t_end]) {
            t_end += 1;
        }
        let ty = &text[t..t_end];
        let lower = name.to_ascii_lowercase();
        let named_like_a_unit = UNIT_SUFFIXES
            .iter()
            .any(|suffix| lower == *suffix || lower.ends_with(&format!("_{suffix}")));
        if !name.is_empty() && named_like_a_unit && INTS.contains(&ty) {
            found.push((line, name.to_string()));
        }
    }
    found
}

#[test]
fn needles_find_what_they_say_and_only_that() {
    for needle in needles() {
        let planted = code(needle.planted);
        assert!(
            !lines_with(&planted, needle.pattern).is_empty(),
            "needle {:?} does not find its planted case {:?}",
            needle.pattern,
            needle.planted
        );
        let clean = code(needle.clean);
        assert!(
            lines_with(&clean, needle.pattern).is_empty(),
            "needle {:?} finds its clean case {:?}",
            needle.pattern,
            needle.clean
        );
    }
    // the unit-name scan has its own planted and clean cases
    assert_eq!(
        raw_unit_names(&code("pub struct A { pub ttl_secs: u64 }")).len(),
        1
    );
    assert_eq!(raw_unit_names(&code("fn f(created_at: i64) {}")).len(), 1);
    assert_eq!(
        raw_unit_names(&code("pub const MAX_BYTES: usize = 8;")).len(),
        1
    );
    assert_eq!(raw_unit_names(&code("fn f(secs: u64) {}")).len(), 1);
    assert!(raw_unit_names(&code(
        "pub struct A { pub ttl: Span, pub len: usize, pub at: DayStart }"
    ))
    .is_empty());
    assert!(raw_unit_names(&code(
        "// ttl_secs: u64\n#[cfg(test)]\nmod t { const X_SECS: u64 = 1; }"
    ))
    .is_empty());
    assert!(raw_unit_names(&code("fn f() -> u64 { let secs: Span = x; 0 }")).is_empty());
}

#[test]
fn the_lexer_handles_what_the_sources_contain() {
    let src = "let a = 'x'; let b = '\\''; let c: &'static str = \"a // not a comment\"; // real comment\nlet d = r#\"raw \" panic!\"#;\n";
    let blanked = blank(src);
    assert_eq!(blanked.lines().count(), 2, "line breaks are kept");
    assert!(!blanked.contains("not a comment") && !blanked.contains("real comment"));
    assert!(
        !blanked.contains("panic!"),
        "raw strings are blanked: {blanked}"
    );
    assert!(
        blanked.contains("&'static str"),
        "a lifetime is not a literal: {blanked}"
    );
    // test items are removed whole, including their nested braces and a declaration `mod t;`
    let text = code("fn a() {}\n#[cfg(test)]\nmod t { fn b() { if x { y } } }\nfn c() {}\n#[cfg(test)]\nmod tests;\nfn d() {}\n");
    assert!(text.contains("fn a") && text.contains("fn c") && text.contains("fn d"));
    assert!(!text.contains("fn b") && !text.contains("mod tests"));
}

// ---- the sources ----------------------------------------------------------------------------

/// Every `.rs` file under `dir`, with its path relative to `dir`, sorted.
fn rust_files(dir: &Path) -> Vec<(String, String)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .map(|e| e.expect("a directory entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                walk(&path, base, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text =
                    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                let rel = path
                    .strip_prefix(base)
                    .expect("under base")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, text));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

fn sources() -> Vec<(String, String)> {
    rust_files(&root().join("src"))
}

/// S9E-19: the library code has none of the forbidden constructs.
#[test]
fn s9e_19_the_library_code_is_pure_logic() {
    let files = sources();
    assert!(
        files.len() >= 8,
        "the scan found only {} files",
        files.len()
    );
    let mut hits = Vec::new();
    for (name, source) in &files {
        let text = code(source);
        for needle in needles() {
            for line in lines_with(&text, needle.pattern) {
                hits.push(format!("src/{name}:{line}: {}", needle.pattern));
            }
        }
        for (line, unit) in raw_unit_names(&text) {
            hits.push(format!("src/{name}:{line}: raw integer named {unit}"));
        }
    }
    assert!(
        hits.is_empty(),
        "forbidden constructs in non-test code:\n{}",
        hits.join("\n")
    );
}

/// At least eight files, every file declared, every declaration has its file, `forbid(unsafe_code)` and
/// the deny block are at the top of the crate root.
#[test]
fn s9e_19_the_crate_declares_every_file_and_forbids_what_it_forbids() {
    let files = sources();
    assert!(files.len() >= 8, "{} files", files.len());
    let lib = &files
        .iter()
        .find(|(name, _)| name == "lib.rs")
        .expect("src/lib.rs")
        .1;
    let mut declared = BTreeSet::new();
    for line in code(lib).lines() {
        let t = line.trim();
        let t = t.strip_prefix("pub ").unwrap_or(t);
        if let Some(rest) = t.strip_prefix("mod ") {
            if let Some(name) = rest.strip_suffix(';') {
                declared.insert(name.trim().to_string());
            }
        }
    }
    let on_disk: BTreeSet<String> = files
        .iter()
        .map(|(name, _)| name.trim_end_matches(".rs").to_string())
        .filter(|name| name != "lib")
        .collect();
    assert_eq!(
        declared, on_disk,
        "lib.rs declares exactly the files of src/ (no orphan, no missing)"
    );
    assert!(
        files.iter().all(|(name, _)| !name.contains('/')),
        "the crate is flat"
    );

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

/// Every mutation guard of the crate has an id of its own, prefixed `creds_`: the mutant holds run one
/// id at a time (`AZLIN_MUTANT=<id> cargo test --features azlin-seams/mutants`).
#[test]
fn s9e_19_every_mutation_guard_has_its_own_id() {
    let mut ids: Vec<String> = Vec::new();
    for (name, source) in sources() {
        for line in source.lines() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            let mut rest = line;
            while let Some(at) = rest.find("mutant!(\"") {
                let after = &rest[at + "mutant!(\"".len()..];
                let id = after.split('"').next().expect("an id");
                ids.push(format!("{id} ({name})"));
                rest = after;
            }
        }
    }
    let plain: Vec<&str> = ids
        .iter()
        .map(|s| s.split(' ').next().expect("an id"))
        .collect();
    let unique: BTreeSet<&str> = plain.iter().copied().collect();
    assert_eq!(
        unique.len(),
        plain.len(),
        "a mutation id is used twice: {ids:?}"
    );
    for id in &plain {
        assert!(id.starts_with("creds_"), "{id}");
    }
}

/// The lines of blanked, test-free code that implement `AccountGroups` (`impl AccountGroups for X`, and
/// any generic or blanket form of it).
fn placement_impls(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains("AccountGroups for"))
        .map(|(n, line)| (n + 1, line.trim().to_string()))
        .collect()
}

/// D-W1S9E3 (drift row X-DELTA-W1S9E-3): the library has NO implementation of the placement seam that
/// answers for every account. Its one implementation, `InstalledAccount`, knows exactly one; a test double
/// lives in `#[cfg(test)]` or in test support, never here.
#[test]
fn d_w1s9e3_the_only_library_implementation_of_the_placement_seam_knows_one_account() {
    // the scan has its planted and its clean case, as every needle of this file has
    assert_eq!(
        placement_impls(&code("impl AccountGroups for Everything {}")).len(),
        1
    );
    assert_eq!(
        placement_impls(&code("impl<T: Any> AccountGroups for T {}")).len(),
        1
    );
    assert!(placement_impls(&code(
        "// impl AccountGroups for X\n#[cfg(test)]\nmod t { impl AccountGroups for Fixed {} }"
    ))
    .is_empty());
    let mut found = Vec::new();
    for (name, source) in sources() {
        for (line, text) in placement_impls(&code(&source)) {
            found.push(format!("src/{name}:{line}: {text}"));
        }
    }
    assert_eq!(
        found.len(),
        1,
        "exactly one library implementation of AccountGroups: {found:?}"
    );
    assert!(
        found[0].starts_with("src/groups.rs:") && found[0].contains("for InstalledAccount"),
        "the one implementation is InstalledAccount's: {found:?}"
    );
}

// ---- the manifest ---------------------------------------------------------------------------

/// The keys of a manifest section: the text before ` =`.
fn section_keys(manifest: &str, section: &str) -> BTreeSet<String> {
    let mut inside = false;
    let mut keys = BTreeSet::new();
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == section;
            continue;
        }
        if inside && !t.is_empty() && !t.starts_with('#') {
            if let Some((key, _)) = t.split_once('=') {
                keys.insert(key.trim().to_string());
            }
        }
    }
    keys
}

/// S9E-15: the manifest lists the final dependencies and no forbidden crate; it has no feature (the
/// interim codec is dropped: the codec is azlin-cbor) and no dev-dependency.
#[test]
fn s9e_15_the_manifest_lists_the_final_dependencies_no_feature_and_no_forbidden_crate() {
    let manifest = fs::read_to_string(root().join("Cargo.toml")).expect("Cargo.toml");
    let dependencies = section_keys(&manifest, "[dependencies]");
    let expected: BTreeSet<String> = [
        "azlin-regbase",
        "azlin-keys",
        "azlin-seams",
        "azlin-cbor",
        "ed25519-dalek",
        "blake3",
        "subtle",
        "thiserror",
        "zeroize",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(
        dependencies, expected,
        "the dependencies of the final manifest, exactly"
    );
    for forbidden in [
        "tokio",
        "hyper",
        "reqwest",
        "rusqlite",
        "openraft",
        "serde",
        "serde_json",
        "getrandom",
        "rand",
        "proptest",
        "azlin-proto",
        "azlin-contract",
    ] {
        assert!(
            !dependencies.contains(forbidden),
            "{forbidden} is not a dependency of azlin-creds"
        );
    }
    for section in ["[dev-dependencies]", "[build-dependencies]", "[features]"] {
        assert!(
            section_keys(&manifest, section).is_empty(),
            "{section} is empty"
        );
    }
    assert!(
        !manifest.lines().any(|l| {
            let t = l.trim();
            !t.starts_with('#') && t.contains("interim-codec")
        }),
        "no interim codec: the codec is azlin-cbor"
    );
    assert!(!root().join("build.rs").exists());
    // azlin-keys without its default features: a client build pulls no getrandom
    let keys_line = manifest
        .lines()
        .find(|l| l.trim().starts_with("azlin-keys"))
        .expect("azlin-keys line");
    assert!(
        keys_line.contains("default-features = false"),
        "{keys_line}"
    );
}
