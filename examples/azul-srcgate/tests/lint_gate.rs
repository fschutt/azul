//! Gate: the modules that read bytes other people wrote deny the panic family outside tests, and
//! the functions that parse a header, a key file or a hex text deny indexing (ARCH28 RF50 / AR5b-5,
//! Z-17).
//!
//! `panic = "abort"` is the shipped profile: a panic reachable from a bucket's answer, a key file
//! or a stored object ends AzDrive. The compiler is what keeps one out - `#![cfg_attr(not(test),
//! deny(clippy::unwrap_used, ..))]` on a module is a build error for the next `unwrap()` in its
//! shipped code, and `#[deny(clippy::indexing_slicing)]` on a parser is one for the next `bytes[n]`
//! with an `n` that was read. This gate keeps the attributes from being deleted or forgotten on a
//! new parser: the lints are the compiler's, the presence is the gate's.
//! (`scripts/panic_surface.py` counts what is written; the fuzz targets of
//! `examples/azul-storage/fuzz` look for what is left.)
//!
//! It reads attributes as tokens ([`azul_srcgate::lex`]), as the other gates read calls: an
//! attribute is found by its identifiers, not parsed. The cases at the end show what it relies on.

use std::collections::BTreeSet;

use azul_srcgate::{code, locate, workspace, Located};

/// The lints of the panic family a module denies outside tests.
const PANIC_FAMILY: &[&str] = &[
    "unwrap_used",
    "expect_used",
    "panic",
    "unreachable",
    "todo",
    "unimplemented",
];

/// The modules whose shipped code denies the panic family (a `mod.rs` covers its folder): the
/// formats and the transports of azul-storage that read what a bucket, a key file or an index
/// holds. A module that reads bytes of that kind is added here with its attribute.
const MODULES: &[&str] = &[
    "examples/azul-storage/src/crypto/mod.rs",
    "examples/azul-storage/src/meta/mod.rs",
    "examples/azul-storage/src/http1.rs",
    "examples/azul-storage/src/s3.rs",
    "examples/azul-storage/src/sharing.rs",
    "examples/azul-storage/src/packing.rs",
    "examples/azul-storage/src/encrypted.rs",
    "examples/azul-storage/src/encrypted_upload.rs",
    "examples/azul-storage/src/rotation.rs",
    "examples/azul-storage/src/migrate.rs",
    "examples/azul-storage/src/recompress.rs",
    "examples/azul-storage/src/xml.rs",
];

/// Files whose every function of a given name denies `clippy::indexing_slicing`: the readers of an
/// object's header and trailer, of the key files and of hex text. Every function of that name in
/// the file counts (a second `parse` added later has to deny it too), and a name with no function
/// fails (a renamed parser cannot leave the gate with nothing to read). `Where::Free` asks about
/// the functions outside any block only (`from_hex` is also a method of three id types, which
/// call it).
const NO_INDEXING: &[(&str, Where, &[&str])] = &[
    (
        "examples/azul-storage/src/crypto/azl1.rs",
        Where::Anywhere,
        &["parse", "segment_offsets"],
    ),
    (
        "examples/azul-storage/src/crypto/azl1.rs",
        Where::Free,
        &["array_at", "field", "byte"],
    ),
    (
        "examples/azul-storage/src/crypto/keys.rs",
        Where::Anywhere,
        &["parse"],
    ),
    (
        "examples/azul-storage/src/crypto/keys.rs",
        Where::Free,
        &["crockford_value"],
    ),
    (
        "examples/azul-storage/src/crypto/mod.rs",
        Where::Free,
        &["from_hex", "hex_array"],
    ),
];

/// Which functions of a name a list asks about.
#[derive(Clone, Copy)]
enum Where {
    /// Every one, in a block or not.
    Anywhere,
    /// The ones outside any block (not a method).
    Free,
}

/// Files that deny `clippy::indexing_slicing` as a whole.
const NO_INDEXING_FILES: &[&str] = &["examples/azul-storage/src/http1.rs"];

fn read(rel: &str) -> String {
    let path = workspace().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The identifiers of the attribute whose `[` is at `open`, and the index after its `]`.
fn attribute(t: &[Located], open: usize) -> (BTreeSet<String>, usize) {
    let mut depth = 0usize;
    let mut idents = BTreeSet::new();
    for (i, x) in t.iter().enumerate().skip(open) {
        if x.tok.is_punct('[') {
            depth += 1;
        } else if x.tok.is_punct(']') {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return (idents, i + 1);
            }
        } else if let Some(name) = x.tok.ident() {
            idents.insert(name.to_string());
        }
    }
    (idents, t.len())
}

/// The identifiers of each inner attribute (`#![..]`) of the file.
fn inner_attributes(t: &[Located]) -> Vec<BTreeSet<String>> {
    let mut found = Vec::new();
    let mut i = 0;
    while i + 2 < t.len() {
        if t[i].tok.is_punct('#') && t[i + 1].tok.is_punct('!') && t[i + 2].tok.is_punct('[') {
            let (idents, next) = attribute(t, i + 2);
            found.push(idents);
            i = next;
        } else {
            i += 1;
        }
    }
    found
}

/// A function of the file, with the identifiers of each outer attribute written above it.
struct Function {
    name: String,
    line: usize,
    /// How many blocks it stands in (0: a free function; an `impl`'s method: 1).
    depth: usize,
    attributes: Vec<BTreeSet<String>>,
}

/// Every `fn name` of the file. The attributes above one belong to it until a `;`, a `{` or a `}`
/// comes between (an `impl` block's brace, the end of the item before).
fn functions(t: &[Located]) -> Vec<Function> {
    let mut found = Vec::new();
    let mut pending: Vec<BTreeSet<String>> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < t.len() {
        let tok = &t[i].tok;
        let next = t.get(i + 1).map(|n| &n.tok);
        if tok.is_punct('#') && next.is_some_and(|n| n.is_punct('!')) {
            i = attribute(t, i + 2).1;
            continue;
        }
        if tok.is_punct('#') && next.is_some_and(|n| n.is_punct('[')) {
            let (idents, after) = attribute(t, i + 1);
            pending.push(idents);
            i = after;
            continue;
        }
        if tok.is_ident("fn") {
            if let Some(name) = next.and_then(|n| n.ident()) {
                found.push(Function {
                    name: name.to_string(),
                    line: t[i].line,
                    depth,
                    attributes: std::mem::take(&mut pending),
                });
            }
        } else if tok.is_punct(';') || tok.is_punct('{') || tok.is_punct('}') {
            pending.clear();
            if tok.is_punct('{') {
                depth += 1;
            } else if tok.is_punct('}') {
                depth = depth.saturating_sub(1);
            }
        }
        i += 1;
    }
    found
}

/// `#![cfg_attr(not(test), deny(clippy::unwrap_used, ..))]` with every lint of the family.
fn denies_the_panic_family(attribute: &BTreeSet<String>) -> bool {
    ["cfg_attr", "not", "test", "deny", "clippy"]
        .iter()
        .chain(PANIC_FAMILY)
        .all(|name| attribute.contains(*name))
}

/// `deny(clippy::indexing_slicing)`.
fn denies_indexing(attribute: &BTreeSet<String>) -> bool {
    ["deny", "clippy", "indexing_slicing"]
        .iter()
        .all(|name| attribute.contains(*name))
}

/// The modules of `MODULES` (as `(path, source)`) that do not deny the panic family.
fn without_the_panic_family(sources: &[(&str, String)]) -> Vec<String> {
    sources
        .iter()
        .filter(|(_, source)| {
            !inner_attributes(&locate(&code(source)))
                .iter()
                .any(denies_the_panic_family)
        })
        .map(|(rel, _)| (*rel).to_string())
        .collect()
}

/// What is wrong with `source` against the names that must deny indexing: a name with no function,
/// a function without the attribute (with its line).
fn indexing_open(rel: &str, source: &str, which: Where, names: &[&str]) -> Vec<String> {
    let all = functions(&locate(&code(source)));
    let mut wrong = Vec::new();
    for name in names {
        let of_name: Vec<&Function> = all
            .iter()
            .filter(|f| f.name == *name && (matches!(which, Where::Anywhere) || f.depth == 0))
            .collect();
        if of_name.is_empty() {
            wrong.push(format!(
                "{rel}: no function {name} (renamed? change NO_INDEXING with it)"
            ));
        }
        for f in of_name {
            if !f.attributes.iter().any(denies_indexing) {
                wrong.push(format!(
                    "{rel}:{}: fn {name} does not deny clippy::indexing_slicing",
                    f.line
                ));
            }
        }
    }
    wrong
}

#[test]
fn the_modules_that_read_untrusted_bytes_deny_the_panic_family_outside_tests() {
    let sources: Vec<(&str, String)> = MODULES.iter().map(|rel| (*rel, read(rel))).collect();
    let wrong = without_the_panic_family(&sources);
    assert!(
        wrong.is_empty(),
        "these modules lack #![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, \
         clippy::panic, clippy::unreachable, clippy::todo, clippy::unimplemented))]:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_parsers_of_a_header_a_key_file_and_a_hex_text_cannot_index_out_of_bounds() {
    let mut wrong = Vec::new();
    for (rel, which, names) in NO_INDEXING {
        wrong.extend(indexing_open(rel, &read(rel), *which, names));
    }
    for rel in NO_INDEXING_FILES {
        let attributes = inner_attributes(&locate(&code(&read(rel))));
        if !attributes.iter().any(denies_indexing) {
            wrong.push(format!("{rel}: no #![deny(clippy::indexing_slicing)]"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// ==== What the reading relies on ====

const DENYING: &str =
    "#![cfg_attr(\n    not(test),\n    deny(\n        clippy::unwrap_used,\n        \
     clippy::expect_used,\n        clippy::panic,\n        clippy::unreachable,\n        \
     clippy::todo,\n        clippy::unimplemented\n    )\n)]\n";

#[test]
fn the_panic_family_is_found_whole_in_an_inner_attribute_and_nowhere_else() {
    let whole = format!("//! docs\n\n{DENYING}\nuse std::fmt;\n");
    assert!(without_the_panic_family(&[("whole.rs", whole)]).is_empty());

    // One lint missing, the attribute not an inner one, in a comment, or in a test module.
    let missing = DENYING.replace("clippy::todo,\n        ", "");
    let outer = DENYING.replace("#![", "#[");
    let commented: String = DENYING.lines().map(|l| format!("// {l}\n")).collect();
    let in_tests = format!("#[cfg(test)]\nmod tests {{\n{DENYING}}}\n");
    let shallow = String::from("#![deny(clippy::indexing_slicing)]\n");
    let sources = [
        ("missing.rs", missing),
        ("outer.rs", outer),
        ("commented.rs", commented),
        ("in_tests.rs", in_tests),
        ("shallow.rs", shallow),
        ("nothing.rs", String::from("use std::fmt;\n")),
    ];
    let wrong = without_the_panic_family(&sources);
    for (rel, _) in &sources {
        assert!(
            wrong.iter().any(|w| w.as_str() == *rel),
            "{rel} is not found wrong: {wrong:?}"
        );
    }
}

#[test]
fn a_function_denies_indexing_by_the_attribute_above_it_and_by_no_other() {
    let source = "\
        #[must_use]\n#[deny(clippy::indexing_slicing)]\npub(crate) fn parse(b: &[u8]) {}\n\nimpl \
                  Header {\n\x20   #[deny(clippy::indexing_slicing)]\n\x20   fn take() {}\n\x20   \
                  fn open() {}\n}\n\n#[allow(clippy::indexing_slicing)]\nfn field() \
                  {}\n\n#[deny(clippy::indexing_slicing)]\nstruct NotAFunction;\nfn byte() {}\n";
    // parse and take deny it; open is not asked about; field only allows it; byte comes after a
    // struct that carried the attribute, which is not byte's.
    assert!(indexing_open("x.rs", source, Where::Anywhere, &["parse", "take"]).is_empty());
    let wrong = indexing_open(
        "x.rs",
        source,
        Where::Anywhere,
        &["field", "byte", "missing"],
    );
    assert_eq!(wrong.len(), 3, "{wrong:?}");
    assert!(wrong[0].contains("fn field"), "{wrong:?}");
    assert!(wrong[1].contains("fn byte"), "{wrong:?}");
    assert!(wrong[2].contains("no function missing"), "{wrong:?}");
}

#[test]
fn a_second_function_of_the_same_name_has_to_deny_it_too() {
    let source = "\
        impl A {\n\x20   #[deny(clippy::indexing_slicing)]\n\x20   fn parse() {}\n}\nimpl B \
                  {\n\x20   fn parse() {}\n}\n";
    let wrong = indexing_open("x.rs", source, Where::Anywhere, &["parse"]);
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert!(wrong[0].contains("x.rs:6"), "{wrong:?}");
}

#[test]
fn a_free_function_is_told_from_a_method_of_the_same_name() {
    let source = "\
        #[deny(clippy::indexing_slicing)]\nfn from_hex() {}\nimpl KeyId {\n\x20   pub fn \
                  from_hex() {}\n}\n";
    // The free one denies it; the method calls it and does not need to.
    assert!(indexing_open("x.rs", source, Where::Free, &["from_hex"]).is_empty());
    // Asked about every function of the name, the method is the one that fails.
    let wrong = indexing_open("x.rs", source, Where::Anywhere, &["from_hex"]);
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert!(wrong[0].contains("x.rs:4"), "{wrong:?}");
}
