//! The checks every app's localization test runs (doc/guide/en/architecture/localization.md):
//! every key the app's source names is in its English and its German resource, both resources
//! parse, and both name the same keys.
//!
//! A key is a string literal of the app's source that starts with the app's prefix and a hyphen
//! and holds nothing but lowercase letters, digits and hyphens (`"azdrive-ribbon-copy"`); a
//! format string (`"azdrive-{x}"`), an id (`"__azdrive_view"`) and the test modules (a
//! `#[cfg(test)] mod`, a `*_tests.rs` file, a `tests` folder) are not looked at. Plain Rust:
//! an app's test calls [`check`] with its `src` folder and its two resources.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use fluent_syntax::ast::Entry;

/// The message ids of the Fluent resource `source`; what is wrong when it does not parse.
///
/// # Errors
///
/// When `source` is no valid Fluent resource: the parser's errors.
pub fn ftl_ids(source: &str) -> Result<BTreeSet<String>, String> {
    match fluent_syntax::parser::parse(source) {
        Ok(resource) => Ok(resource
            .body
            .iter()
            .filter_map(|entry| match entry {
                Entry::Message(message) => Some(message.id.name.to_string()),
                _ => None,
            })
            .collect()),
        Err((_, errors)) => Err(format!("{errors:?}")),
    }
}

/// The keys of the source text `text` with one of the `prefixes` (`azdrive`), its test module
/// left out.
#[must_use]
pub fn keys_in_source(text: &str, prefixes: &[&str]) -> BTreeSet<String> {
    let text = without_test_module(text);
    let mut keys = BTreeSet::new();
    for prefix in prefixes {
        let start = format!("\"{prefix}-");
        let mut rest = text;
        while let Some(at) = rest.find(&start) {
            let after = &rest[at + 1..];
            let len = after
                .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
                .unwrap_or(after.len());
            if after[len..].starts_with('"') && len > prefix.len() + 1 {
                keys.insert(after[..len].to_string());
            }
            rest = &after[len..];
        }
    }
    keys
}

/// `text` up to its test module (`#[cfg(test)]` before a `mod`).
fn without_test_module(text: &str) -> &str {
    let mut from = 0;
    while let Some(at) = text[from..].find("#[cfg(test)]") {
        let at = from + at;
        let after = text[at + "#[cfg(test)]".len()..].trim_start();
        if after.starts_with("mod ")
            || after.starts_with("pub mod ")
            || after.starts_with("pub(crate) mod ")
        {
            return &text[..at];
        }
        from = at + 1;
    }
    text
}

/// The problems of the sources `texts`: a key one resource lacks, a key only one resource has,
/// a resource that does not parse. Empty: all is well.
#[must_use]
pub fn check_texts(texts: &[&str], prefixes: &[&str], en: &str, de: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let mut ids_of = |lang: &str, source: &str| match ftl_ids(source) {
        Ok(ids) => Some(ids),
        Err(e) => {
            problems.push(format!("{lang}: does not parse: {e}"));
            None
        }
    };
    let en_ids = ids_of("en", en);
    let de_ids = ids_of("de", de);
    let keys: BTreeSet<String> = texts
        .iter()
        .flat_map(|text| keys_in_source(text, prefixes))
        .collect();
    for (lang, ids) in [("en", &en_ids), ("de", &de_ids)] {
        let Some(ids) = ids else { continue };
        for key in &keys {
            if !ids.contains(key) {
                problems.push(format!("{key}: not in {lang}"));
            }
        }
    }
    if let (Some(en_ids), Some(de_ids)) = (&en_ids, &de_ids) {
        for id in en_ids.difference(de_ids) {
            problems.push(format!("{id}: in en, not in de"));
        }
        for id in de_ids.difference(en_ids) {
            problems.push(format!("{id}: in de, not in en"));
        }
    }
    problems
}

/// [`check_texts`] of every `.rs` file under `dir` (but the test files and `tests` folders).
#[must_use]
pub fn check(dir: &Path, prefixes: &[&str], en: &str, de: &str) -> Vec<String> {
    let mut files = Vec::new();
    rust_files(dir, &mut files);
    let texts: Vec<String> = files
        .iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .collect();
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let mut problems = check_texts(&texts, prefixes, en, de);
    if files.is_empty() {
        problems.push(format!("{}: no source files", dir.display()));
    }
    problems
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "tests" {
                rust_files(&path, out);
            }
        } else if name.ends_with(".rs") && !name.ends_with("_tests.rs") {
            out.push(path);
        }
    }
}
