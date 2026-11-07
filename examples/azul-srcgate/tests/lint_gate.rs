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
//!
//! The presence of the deny is not enough: a later `#![allow(clippy::unwrap_used)]` in the same
//! file, or an `#[allow(..)]` on one function, switches it off again and the first test above
//! still passes. So a second rule reads every attribute of the three crates' shipped code and
//! finds the ones that lower a lint of the family (`allow`, `expect`, `warn`, also behind
//! `cfg_attr`); the sites that must stay are counted, with their reason
//! (X-DELTA-AZDRIVE-KEYS-16). And the deny is clippy's, not rustc's: the CI runs clippy over these
//! crates (the "clippy (the panic family denied ..)" step of rust.yml, pinned by `ci_storage_gate`).

use std::collections::BTreeSet;

use azul_srcgate::{code, locate, workspace, Allow, Budget, Gate, Located, Orphans, Root, Rule};

/// The lints of the panic family a module denies outside tests.
const PANIC_FAMILY: &[&str] = &[
    "unwrap_used",
    "expect_used",
    "panic",
    "unreachable",
    "todo",
    "unimplemented",
];

/// The modules whose shipped code denies the panic family (a `mod.rs` covers its folder, a crate's
/// `lib.rs` the crate): the formats and the transports of azul-storage that read what a bucket, a
/// key file or an index holds, and the two crates below it that decode what a member of the drive
/// wrote (azlin-compress, azlin-preflate). A module that reads bytes of that kind is added here
/// with its attribute.
const MODULES: &[&str] = &[
    "examples/azlin-compress/src/lib.rs",
    "examples/azlin-preflate/src/lib.rs",
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

/// The attributes of the file's shipped code that lower a lint of the panic family: an
/// `#![allow(clippy::unwrap_used)]` after the module's deny, an `#[allow(clippy::panic)]` on a
/// function or a statement, `expect(..)` or `warn(..)` of one (also behind a `cfg_attr`), or of the
/// whole group (`clippy::restriction`). The lines.
fn lowers_the_panic_family(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    let mut i = 0;
    while i + 1 < t.len() {
        if t[i].tok.is_punct('#') {
            let open = if t[i + 1].tok.is_punct('!') {
                i + 2
            } else {
                i + 1
            };
            if t.get(open).is_some_and(|x| x.tok.is_punct('[')) {
                let (idents, next) = attribute(t, open);
                let lowers = ["allow", "expect", "warn"]
                    .iter()
                    .any(|name| idents.contains(*name));
                let names = idents.contains("clippy")
                    && (idents.contains("restriction")
                        || PANIC_FAMILY.iter().any(|name| idents.contains(*name)));
                if lowers && names {
                    found.push(t[i].line);
                }
                i = next;
                continue;
            }
        }
        i += 1;
    }
    found
}

/// The rule of the second test: its name, and its own cases (a lowering it must find, code it
/// must not).
const LOWERED: &str = "panic-lint-lowered-below-the-deny";

fn lowering_rules() -> Vec<Rule> {
    vec![Rule::new(
        LOWERED,
        lowers_the_panic_family,
        &[
            "#![allow(clippy::unwrap_used)]",
            "#[allow(clippy::expect_used)] let n = x.expect(\"len\");",
            "#[expect(clippy::panic)] panic!();",
            "#![warn(clippy::todo, clippy::unimplemented)]",
            "#[cfg_attr(not(test), allow(clippy::unreachable))] let n = 1;",
            "#![allow(clippy::restriction)]",
            "#[allow(\n    clippy::needless_return,\n    clippy::unwrap_used\n)]\nlet n = 1;",
        ],
        &[
            "#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]",
            "#[deny(clippy::indexing_slicing)] let n = 1;",
            "#[allow(clippy::needless_return)] let n = 1;",
            "#[allow(clippy::panic_in_result_fn)] let n = 1;",
            "#![allow(dead_code)]",
            "let allow = 1; let unwrap_used = allow;",
        ],
    )]
}

/// The crates whose shipped code denies the panic family, read whole.
const CRATES: &[Root] = &[
    Root {
        dir: "examples/azul-storage/src",
        min_files: 60,
    },
    Root {
        dir: "examples/azlin-compress/src",
        min_files: 12,
    },
    Root {
        dir: "examples/azlin-preflate/src",
        min_files: 25,
    },
];

/// Where a lint of the family is lowered on purpose, counted. TO SHRINK (or to stay, with its
/// reason).
const LOWERED_ALLOWED: &[Allow] = &[
    Allow::exactly(
        "examples/azlin-preflate/src/deflate/deflate_constants.rs",
        LOWERED,
        1,
        "`into_array` is a `const fn`: its `unwrap` runs at compile time, where a table out of \
         range is a build error and not a panic of the program",
    ),
    Allow::exactly(
        "examples/azul-storage/src/meta/seal.rs",
        LOWERED,
        2,
        "`hmac_parts` and the test double's `tag_mac`: `Hmac::new_from_slice` takes a key of any \
         length, so its `expect` cannot fire (the attribute says so as its `reason`)",
    ),
];

fn lowering_gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "no panic lint is lowered below the deny",
        roots,
        rules: lowering_rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
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
fn nothing_in_the_shipped_code_of_these_crates_lowers_a_panic_lint_below_the_deny() {
    lowering_gate(CRATES, LOWERED_ALLOWED).run();
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

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let base =
        std::env::temp_dir().join(format!("azul-srcgate-lint-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    for (path, text) in files {
        let full = base.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, text).unwrap();
    }
    base
}

const SMALL: &[Root] = &[Root {
    dir: "examples/azul-storage/src",
    min_files: 1,
}];

#[test]
fn a_later_allow_a_function_s_allow_and_an_expect_are_found_with_their_lines() {
    let planted = format!(
        "{DENYING}\
         #![allow(clippy::unwrap_used)]\n\
         pub fn parse(b: &[u8]) -> u8 {{\n\
         \x20   #[allow(clippy::expect_used)]\n\
         \x20   let n = b.first().expect(\"one\");\n\
         \x20   *n\n\
         }}\n\
         #[expect(clippy::panic)]\n\
         pub fn boom() {{ panic!(); }}\n\
         #[cfg(test)]\n\
         mod tests {{\n\
         \x20   #![allow(clippy::unwrap_used)]\n\
         }}\n"
    );
    let base = base_with("planted", &[("examples/azul-storage/src/lib.rs", &planted)]);
    let wrong = lowering_gate(SMALL, &[])
        .check(&base)
        .expect_err("three lowerings are findings");
    let _ = std::fs::remove_dir_all(&base);
    // the deny itself (DENYING is 11 lines) is none; the test module's allow is none
    assert_eq!(wrong.len(), 3, "{wrong:?}");
    for line in [12, 14, 18] {
        assert!(
            wrong
                .iter()
                .any(|w| w.contains(&format!("lib.rs:{line}:")) && w.contains(LOWERED)),
            "line {line}: {wrong:?}"
        );
    }
}

#[test]
fn a_counted_site_is_counted_and_one_more_is_a_new_site() {
    static ONE: &[Allow] = &[Allow::exactly(
        "examples/azul-storage/src/lib.rs",
        LOWERED,
        1,
        "the planted site",
    )];
    let one = "#[allow(clippy::unwrap_used)]\nconst fn f() {}\n";
    let two =
        "#[allow(clippy::unwrap_used)]\nconst fn f() {}\n#[allow(clippy::panic)]\nfn g() {}\n";
    let base = base_with("counted", &[("examples/azul-storage/src/lib.rs", one)]);
    assert!(lowering_gate(SMALL, ONE).check(&base).is_ok());
    std::fs::write(base.join("examples/azul-storage/src/lib.rs"), two).unwrap();
    let wrong = lowering_gate(SMALL, ONE)
        .check(&base)
        .expect_err("a second site");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("holds 2 x") && w.contains("says 1")),
        "{wrong:?}"
    );
    let _ = std::fs::remove_dir_all(&base);
}
