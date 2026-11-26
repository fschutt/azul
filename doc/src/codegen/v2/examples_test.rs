//! The examples against the C API they are written for.
//!
//! Nothing builds an example but its own leg of the language board, and a leg can stay red for
//! days before anyone reads it. api.json named the constructor of `WindowCreateOptions` that takes
//! no arguments `create_default`, and `AzWindowCreateOptions_default` stood on in the Perl, the
//! Lisp and the Red example: the Perl leg died at `Undefined subroutine
//! &Azul::FFI::AzWindowCreateOptions_default` (website run 45738701245, macOS), Racket at
//! `make-window-create-options: unbound identifier`.
//!
//! Every `Az<Type>_<name>` an example names (a function, an enum constant, a variant struct, in
//! whatever quoting its language uses) must be a name the generated `azul.h` declares.

use std::{collections::BTreeSet, path::Path};

use super::bug_classes::generated;

/// Every `Az<Upper>...<_>...` identifier of `text`: not `Azul_...`, which is a binding's own.
fn names(text: &str) -> BTreeSet<String> {
    let b = text.as_bytes();
    let mut out = BTreeSet::new();
    let mut i = 0;
    while i + 2 < b.len() {
        let boundary = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if boundary && b[i] == b'A' && b[i + 1] == b'z' && b[i + 2].is_ascii_uppercase() {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if text[i..j].contains('_') {
                out.insert(text[i..j].to_string());
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// The hello-world files of every language directory of examples/, with their text.
fn examples() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("examples");
    let mut out = Vec::new();
    for dir in std::fs::read_dir(&root).expect("examples/").filter_map(Result::ok) {
        if !dir.path().is_dir() {
            continue;
        }
        for file in std::fs::read_dir(dir.path()).into_iter().flatten().filter_map(Result::ok) {
            let name = file.file_name().to_string_lossy().to_string();
            if !(name.starts_with("hello-world") || name.starts_with("hello_world")) || !file.path().is_file() {
                continue;
            }
            let Ok(text) = std::fs::read(file.path()) else { continue };
            let rel = format!("examples/{}/{}", dir.file_name().to_string_lossy(), name);
            out.push((rel, String::from_utf8_lossy(&text).into_owned()));
        }
    }
    out.sort();
    out
}

#[test]
fn the_names_are_read_from_the_examples() {
    let found = names("x AzDom_createBody() Azul_RefAny_Create 'AzApp_run' (AzDom_withCss d) AzString_ AzFoo");
    let expected: BTreeSet<String> =
        ["AzDom_createBody", "AzApp_run", "AzDom_withCss", "AzString_"].iter().map(|s| s.to_string()).collect();
    assert_eq!(found, expected);
    let files = examples();
    assert!(files.len() >= 20, "only {} hello-world files found under examples/", files.len());
}

#[test]
fn every_example_names_only_what_azul_h_declares() {
    let declared = names(&generated("azul.h"));
    assert!(declared.len() > 10_000, "only {} names read from azul.h", declared.len());
    let mut stale = Vec::new();
    for (file, text) in examples() {
        let unknown: Vec<String> = names(&text).into_iter().filter(|n| !declared.contains(n)).collect();
        if !unknown.is_empty() {
            stale.push(format!("{file}: {unknown:?}"));
        }
    }
    assert!(
        stale.is_empty(),
        "examples name C symbols azul.h does not declare (renamed in api.json, or never there):\n{}",
        stale.join("\n")
    );
}
