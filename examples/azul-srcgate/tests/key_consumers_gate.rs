//! Gate: every function that seals or opens with the drive key is one of the things a drive key touches
//! (ARCH29 C7 step 2; the registry is `CONSUMERS` in `azul-storage/src/crypto/keystate.rs`).
//!
//! A rotation had to reseal, and a removal to erase, a list of things the drive key touches - the
//! index, the members' wraps, the recovery wrap, the kits, the shares, the period vault, the drop keys,
//! the key chain - and each time one of them was forgotten by the list that did not know it
//! (X-DELTA-AZDRIVE-KEYS-5 the vault, -27 the kits, -28 the kits' wraps, -30 the drop keys, -32 the
//! device key). The registry is the one list. This gate is its other direction: a function that
//! puts the drive key to work - it wraps or unwraps a file key with it (`wrap_file_key`,
//! `unwrap_file_key`), derives a key from it (`.derive(`), seals a wrap or a drop key file under it
//! (`RecoveryWrap::seal`, `MemberWrap::seal`, `DropKeyFile::seal`), writes the key chain
//! (`store_key_chain`) or seals an object with it (`azl1::encrypt`, `azl1::decrypt`) - must be named
//! by a consumer in `CONSUMERS` (each consumer's `functions`, `<file>#<function>`), or be one of the
//! few the gate lists itself with the reason, or the E2E harness, which forges wraps to prove the
//! client refuses them. A function that stays listed after it stopped using the key fails too: the
//! registry says what is true.
//!
//! The baseline of the registry is what this gate's scanner finds, counted by running it
//! ([`BASELINE`], checked by `the_baseline_is_what_the_scanner_finds`): the lists the registry and this
//! file hold name every function it finds and no other. A finding is a function the lists missed or one
//! that was renamed, to be added to its consumer or to the data path with its reason - the lists only
//! grow on purpose.
//!
//! It reads source text, as every gate of this crate does: a function is its `fn` keyword, its name
//! and the first `{ .. }` after them; comments and test items are not read; a function nested inside
//! another is part of it. A call is its path as written (`calls`, `method_calls`), and the spellings
//! that hide one are read too: a name a file gives a needle (`use ...::RecoveryWrap as Wrap;`,
//! `type Wrap = RecoveryWrap;`), the path of a method written as a function
//! (`DriveKey::derive(&key, ..)`), and a `macro_rules!` body - a macro that seals with the key is an
//! item the gate finds like a function (`<file>#<macro name>` in a consumer's list).

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use azul_srcgate::{
    calls, files, hits, lex, method_calls, self_test, workspace, Hit, Located, Orphans, Root, Rule,
    Tok,
};

const ROOTS: &[Root] = &[
    Root {
        dir: "examples/azul-storage/src",
        min_files: 55,
    },
    Root {
        dir: "examples/azcloud-kit/src",
        min_files: 55,
    },
    Root {
        dir: "examples/azul-drive/src",
        min_files: 65,
    },
    Root {
        dir: "examples/azul-appkit/src",
        min_files: 20,
    },
    Root {
        dir: "examples/azul-bridge/src",
        min_files: 15,
    },
    Root {
        dir: "examples/azcloud-api/src",
        min_files: 1,
    },
    Root {
        dir: "examples/azul-mail-core/src",
        min_files: 8,
    },
    Root {
        dir: "examples/azul-mail/src",
        min_files: 10,
    },
];

/// The registry's file: where the consumers' `functions` are read from.
const REGISTRY: &str = "examples/azul-storage/src/crypto/keystate.rs";

const RULE: &str = "drive-key-use";

/// The functions of the DATA PATH: they wrap or unwrap one file's key with the drive key when an
/// object is written, read, packed or shared, or they are the primitives themselves. A rotation does
/// not reseal them - it re-wraps the index's entries (INDEX) and leaves the objects under the key they
/// were written with, which the key chain opens (CHAIN) - so they are no consumer of a key change; they
/// are listed here so that a new one is a decision, not an accident.
const DATA_PATH: &[&str] = &[
    "examples/azul-storage/src/crypto/mod.rs#wrap_file_key",
    "examples/azul-storage/src/crypto/mod.rs#unwrap_file_key",
    "examples/azul-storage/src/crypto/azl1.rs#start",
    "examples/azul-storage/src/crypto/azl1.rs#open_with_drive_key",
    "examples/azul-storage/src/encrypted.rs#get",
    "examples/azul-storage/src/encrypted.rs#open_reader",
    "examples/azul-storage/src/encrypted.rs#opened",
    "examples/azul-storage/src/encrypted.rs#unwrap_file_key",
    "examples/azul-storage/src/packing.rs#read_packed",
    "examples/azul-storage/src/packing.rs#repack_selected",
    "examples/azul-storage/src/packing.rs#seal_pack",
];

/// Files and folders whose functions are no consumer, with the reason. TO SHRINK.
const EXCEPTIONS: &[(&str, &str)] = &[(
    "examples/azul-drive/src/e2e/",
    "the E2E harness forges wraps and reads keys to prove that the client refuses what a hostile \
     bucket hands it (b_tamper.rs): an attack, not a flow of the drive",
)];

/// What the drive key is put to work with, by the name the code gives it: a type or a module and the
/// functions of it that seal or open, or a function by itself (`""`).
const NEEDLES: &[(&str, &[&str])] = &[
    ("RecoveryWrap", &["seal"]),
    ("MemberWrap", &["seal"]),
    ("DropKeyFile", &["seal"]),
    ("azl1", &["encrypt", "decrypt"]),
    ("DriveKey", &["derive", "wrap_file_key", "unwrap_file_key"]),
    ("store_key_chain", &[""]),
];

/// The other names a file gives the names of [`NEEDLES`]: `(alias, original)` of every
/// `use .. Original as Alias` (also inside braces) and every `type Alias = Original;`.
fn aliases_of(t: &[Located]) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let original = |name: &str| NEEDLES.iter().any(|(needle, _)| *needle == name);
    let mut i = 0;
    while i < t.len() {
        if t[i].tok.is_ident("use") {
            // up to the `;` of the item
            let end = t[i..]
                .iter()
                .position(|x| x.tok.is_punct(';'))
                .map_or(t.len(), |p| i + p);
            for k in i..end.saturating_sub(2) {
                if let (Some(name), true, Some(alias)) = (
                    t[k].tok.ident(),
                    t[k + 1].tok.is_ident("as"),
                    t[k + 2].tok.ident(),
                ) {
                    if original(name) && alias != "_" {
                        found.push((alias.to_string(), name.to_string()));
                    }
                }
            }
            i = end;
        } else if t[i].tok.is_ident("type") && i + 3 < t.len() {
            // `type Alias = Original;` (a path before the name is read too: the last name counts)
            if let (Some(alias), true) = (t[i + 1].tok.ident(), t[i + 2].tok.is_punct('=')) {
                let end = t[i..]
                    .iter()
                    .position(|x| x.tok.is_punct(';'))
                    .map_or(t.len(), |p| i + p);
                if let Some(name) = t[i + 3..end]
                    .iter()
                    .rev()
                    .find_map(|x| x.tok.ident().filter(|n| original(n)))
                {
                    found.push((alias.to_string(), name.to_string()));
                }
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    found
}

/// Whether the body writes or takes away the drive's drop key file: `DROP_KEY_FILE` (the name of
/// `_drop.key`, which holds a secret sealed under the drive key) put, conditionally put or deleted through
/// a bucket. A function that does is a thing the drive key touches even where it seals nothing itself
/// (`disable_drop` deletes the file; `enable_drop` puts the key `DropKeyFile::seal` made): the mail the
/// Worker sealed to that key opens with nothing else, and a writer that forgot the key change was the
/// finding (X-DELTA-KEYS3-2).
fn writes_the_drop_key_file(body: &[Located]) -> bool {
    // `.put(DROP_KEY_FILE`, `.put_if(DROP_KEY_FILE`, `.delete(DROP_KEY_FILE`: the file is the call's first
    // argument; a function that merely names the file or puts another one is none.
    body.windows(4).any(|w| {
        w[0].tok.is_punct('.')
            && ["put", "put_if", "delete"]
                .iter()
                .any(|method| w[1].tok.is_ident(method))
            && w[2].tok.is_punct('(')
            && w[3].tok.is_ident("DROP_KEY_FILE")
    })
}

/// Whether the tokens of a function's (or a macro's) body put the drive key to work, under the
/// names of [`NEEDLES`] and the `aliases` the file gives them.
fn uses_the_drive_key(body: &[Located], aliases: &[(String, String)]) -> bool {
    !method_calls(body, "wrap_file_key").is_empty()
        || !method_calls(body, "unwrap_file_key").is_empty()
        || !method_calls(body, "derive").is_empty()
        || writes_the_drop_key_file(body)
        || NEEDLES.iter().any(|(needle, functions)| {
            let names = std::iter::once(*needle).chain(
                aliases
                    .iter()
                    .filter(|(_, original)| original.as_str() == *needle)
                    .map(|(alias, _)| alias.as_str()),
            );
            names.into_iter().any(|name| {
                functions.iter().any(|function| {
                    let path = if function.is_empty() {
                        name.to_string()
                    } else {
                        format!("{name}::{function}")
                    };
                    !calls(body, &path).is_empty()
                })
            })
        })
}

/// The token range `[open, close]` of the body of the function whose `fn` is token `fn_at`; `None`
/// for a declaration that ends in `;` first. Parentheses and brackets keep a `;` (an array's length)
/// or a `{` (a closure's) in the signature from being read as the end of it.
fn body_of(t: &[Located], fn_at: usize) -> Option<(usize, usize)> {
    let mut parens = 0i32;
    let mut j = fn_at + 2;
    while j < t.len() {
        match &t[j].tok {
            Tok::Punct('(' | '[') => parens += 1,
            Tok::Punct(')' | ']') => parens -= 1,
            Tok::Punct(';') if parens <= 0 => return None,
            Tok::Punct('{') if parens <= 0 => {
                let mut depth = 0i32;
                for (k, x) in t.iter().enumerate().skip(j) {
                    match &x.tok {
                        Tok::Punct('{') => depth += 1,
                        Tok::Punct('}') => {
                            depth -= 1;
                            if depth == 0 {
                                return Some((j, k));
                            }
                        }
                        _ => {}
                    }
                }
                return Some((j, t.len()));
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// The token range `[open, close]` of the body of the `macro_rules! name { .. }` (or `( .. );`, `[ .. ];`)
/// whose `macro_rules` is token `at`: the first delimiter after the name and its match.
fn macro_body_of(t: &[Located], at: usize) -> Option<(usize, usize)> {
    let open = at + 3;
    let (open_c, close_c) = match t.get(open).map(|x| &x.tok) {
        Some(Tok::Punct('{')) => ('{', '}'),
        Some(Tok::Punct('(')) => ('(', ')'),
        Some(Tok::Punct('[')) => ('[', ']'),
        _ => return None,
    };
    let mut depth = 0i32;
    for (k, x) in t.iter().enumerate().skip(open) {
        match &x.tok {
            Tok::Punct(c) if *c == open_c => depth += 1,
            Tok::Punct(c) if *c == close_c => {
                depth -= 1;
                if depth == 0 {
                    return Some((open, k));
                }
            }
            _ => {}
        }
    }
    Some((open, t.len()))
}

/// The lines of the `fn` of every function - and of the `macro_rules` of every macro - that uses the
/// drive key, once per item. A macro is read by its body: the functions that call it do not show what
/// it seals with.
fn functions_using_the_drive_key(t: &[Located]) -> Vec<usize> {
    let aliases = aliases_of(t);
    let mut found = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let is_fn =
            t[i].tok.is_ident("fn") && t.get(i + 1).is_some_and(|name| name.tok.ident().is_some());
        let is_macro = t[i].tok.is_ident("macro_rules")
            && t.get(i + 1).is_some_and(|bang| bang.tok.is_punct('!'))
            && t.get(i + 2).is_some_and(|name| name.tok.ident().is_some());
        if is_macro {
            match macro_body_of(t, i) {
                Some((open, close)) => {
                    if uses_the_drive_key(&t[open..close.min(t.len())], &aliases) {
                        found.push(t[i].line);
                    }
                    i = close + 1;
                }
                None => i += 1,
            }
            continue;
        }
        if !is_fn {
            i += 1;
            continue;
        }
        match body_of(t, i) {
            Some((open, close)) => {
                if uses_the_drive_key(&t[open..close.min(t.len())], &aliases) {
                    found.push(t[i].line);
                }
                i = close + 1;
            }
            None => i += 1,
        }
    }
    found
}

fn rules() -> Vec<Rule> {
    vec![Rule::new(
        RULE,
        functions_using_the_drive_key,
        &[
            "pub fn seal(drive_key: &DriveKey) { let wrapped = drive_key.wrap_file_key(&file_key, &object); }",
            "let file_key = key.unwrap_file_key(&wrapped, &object)?;",
            "let wrap = RecoveryWrap::seal(&key, drive, &code, kdf)?;",
            "let wrap = keys::MemberWrap::seal(&key, drive, &member, &public)?;",
            "let file = DropKeyFile::seal(new, drive, &secret)?;",
            "store_key_chain(bucket, &chain)?;",
            "let sealing = key.derive(CONTEXT);",
            "let (sealed, _) = azl1::encrypt(&plain, id, &key, &options)?;",
            "let plain = azl1::decrypt(&bytes, &id, &file_key)?;",
            "use crate::crypto::keys::RecoveryWrap as Wrap; let wrap = Wrap::seal(&key, drive, &code, kdf)?;",
            "use crate::crypto::{keys::{MemberWrap as Mw, KeyChain}, azl1 as a1}; let e = a1::encrypt(&plain, id, &key, &options)?;",
            "type Wrap = keys::MemberWrap; let wrap = Wrap::seal(&key, drive, &member, &public)?;",
            "let sealing = DriveKey::derive(&key, CONTEXT);",
            "let wrapped = DriveKey::wrap_file_key(&key, &file_key, &object);",
            "bucket.put(DROP_KEY_FILE, &file.to_bytes())?;",
            "bucket.delete(DROP_KEY_FILE)",
            "bucket.put_if(DROP_KEY_FILE, &bytes, &Precondition::Absent)?;",
        ],
        &[
            "let id = key.id();",
            "let bytes = bucket.get(DROP_KEY_FILE)?;",
            "let at = DROP_KEY_FILE; bucket.put(OTHER_FILE, &bytes)?;",
            "let wrapped = wrap_with(&sealing, id, &file_key);",
            "let chain = load_key_chain(bucket)?;",
            "let wrap = RecoveryWrap::parse(&bytes)?;",
            "let text = \"RecoveryWrap::seal(\";",
            "let open = MemberWrap::open(&wrap, drive, &secret)?;",
            "use std::fmt::Display as Show; let n = x as u64;",
            "type Name = String; let n = Name::new();",
            "let wrap = Wrap::seal(&key);",
        ],
    )]
}

/// The functions `CONSUMERS` names: every string `<file>#<function>` in the registry's file.
fn registered(base: &Path) -> Result<BTreeSet<String>, String> {
    let path = base.join(REGISTRY);
    let src = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(lex(&src)
        .into_iter()
        .filter_map(|tok| match tok {
            Tok::Str(text) if text.starts_with("examples/") && text.contains(".rs#") => Some(text),
            _ => None,
        })
        .collect())
}

/// The name an item line declares: what follows its `fn`, or its `macro_rules!`.
fn name_of(line: &str) -> Option<String> {
    let after = match line.split_once("macro_rules!") {
        Some((_, rest)) => rest.trim_start(),
        None => line.split_once("fn ")?.1,
    };
    let name: String = after
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// What is wrong with `found` against the registry and the lists of this gate.
fn judge(found: &[Hit], registered: &BTreeSet<String>) -> Vec<String> {
    let mut wrong = Vec::new();
    let data_path: BTreeSet<&str> = DATA_PATH.iter().copied().collect();
    for both in registered.iter().filter(|r| data_path.contains(r.as_str())) {
        wrong.push(format!(
            "{both} is a consumer and a function of the data path: it is one of them"
        ));
    }
    let mut used = BTreeSet::new();
    for hit in found {
        let Some(name) = name_of(&hit.text) else {
            wrong.push(format!(
                "{}:{}: {} - the gate cannot read the function's name",
                hit.rel, hit.line, hit.text
            ));
            continue;
        };
        let key = format!("{}#{name}", hit.rel);
        if registered.contains(&key) || data_path.contains(key.as_str()) {
            used.insert(key);
        } else if EXCEPTIONS.iter().any(|(path, _)| {
            hit.rel == *path
                || path
                    .strip_suffix('/')
                    .is_some_and(|dir| hit.rel.starts_with(&format!("{dir}/")))
        }) {
            // the harness
        } else {
            wrong.push(format!(
                "{}:{}: `{}` puts the drive key to work and is in no consumer of CONSUMERS \
                 (azul-storage crypto/keystate.rs): add it to the consumer whose data it seals or \
                 opens, or - the data path of a file - to DATA_PATH here, with the reason",
                hit.rel, hit.line, hit.text
            ));
        }
    }
    for listed in registered
        .iter()
        .map(String::as_str)
        .chain(DATA_PATH.iter().copied())
    {
        if !used.contains(listed) {
            wrong.push(format!(
                "{listed} is listed and no function of that name uses the drive key any more (or it \
                 moved): the list says what is true"
            ));
        }
    }
    wrong
}

/// What the scanner finds in the tree, counted by RUNNING it (ARCH29-KEYSTATE-3, X-DELTA-STRUCT-KEYSTATE-8):
/// the functions and macros the consumers of the registry name, the ones of the data path, and the
/// E2E harness's. The lists above and in the registry are exactly these; a number here changes when a list
/// does, on purpose.
#[derive(Debug, PartialEq, Eq)]
struct Counts {
    consumers: usize,
    data_path: usize,
    harness: usize,
}

/// The counts the scanner gives over `found`.
fn counted(found: &[Hit], registered: &BTreeSet<String>) -> Counts {
    let data_path: BTreeSet<&str> = DATA_PATH.iter().copied().collect();
    let mut counts = Counts {
        consumers: 0,
        data_path: 0,
        harness: 0,
    };
    let mut seen = BTreeSet::new();
    for hit in found {
        let Some(name) = name_of(&hit.text) else {
            continue;
        };
        let key = format!("{}#{name}", hit.rel);
        if !seen.insert(key.clone()) {
            continue;
        }
        if registered.contains(&key) {
            counts.consumers += 1;
        } else if data_path.contains(key.as_str()) {
            counts.data_path += 1;
        } else if EXCEPTIONS.iter().any(|(path, _)| {
            hit.rel == *path
                || path
                    .strip_suffix('/')
                    .is_some_and(|dir| hit.rel.starts_with(&format!("{dir}/")))
        }) {
            counts.harness += 1;
        }
    }
    counts
}

/// The gate against the tree under `base` and the roots it reads.
fn check(base: &Path, roots: &[Root]) -> Result<usize, Vec<String>> {
    let rules = rules();
    self_test(&rules);
    let registered = registered(base).map_err(|e| vec![e])?;
    let read = files(base, roots, Orphans::Ignore)?;
    let found = hits(&read, &rules);
    let wrong = judge(&found, &registered);
    if wrong.is_empty() {
        Ok(found.len())
    } else {
        Err(wrong)
    }
}

/// The baseline of the registry (X-DELTA-STRUCT-KEYSTATE-8): the numbers the scanner gave when it was RUN over the
/// tree (2026-11-12), not a count written from a read of the sources. A list that grows or shrinks on purpose
/// changes its number here in the same commit.
const BASELINE: Counts = Counts {
    consumers: 27,
    data_path: 11,
    harness: 1,
};

#[test]
fn the_baseline_is_what_the_scanner_finds() {
    let base = workspace();
    let registered = registered(&base).unwrap();
    let read = files(&base, ROOTS, Orphans::Ignore).unwrap();
    let found = hits(&read, &rules());
    let counts = counted(&found, &registered);
    assert_eq!(
        counts.consumers,
        registered.len(),
        "every function the registry names is one the scanner finds"
    );
    assert_eq!(
        counts.data_path,
        DATA_PATH.len(),
        "every function of the data path is one the scanner finds"
    );
    assert_eq!(counts, BASELINE, "the scanner's count over the tree");
}

#[test]
fn x_delta_azdrive_keys_5_every_function_that_seals_or_opens_with_the_drive_key_is_a_consumer_or_the_data_path(
) {
    match check(&workspace(), ROOTS) {
        Ok(found) => assert!(found > 0, "the gate read the places the key is used"),
        Err(wrong) => panic!(
            "what a drive key touches: {} finding(s):\n{}",
            wrong.len(),
            wrong.join("\n")
        ),
    }
}

/// The registry is read from its file: the strings the consumers name are found, one per function.
#[test]
fn the_registry_names_its_functions_in_strings_the_gate_can_read() {
    let names = registered(&workspace()).unwrap();
    assert!(
        names.len() >= 20,
        "the consumers name their functions: {names:?}"
    );
    for name in &names {
        let (file, function) = name.split_once('#').unwrap();
        assert!(
            workspace().join(file).is_file(),
            "{name}: the file is not there"
        );
        assert!(!function.is_empty());
    }
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-key-consumers-{name}-{}",
        std::process::id()
    ));
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

/// A function that seals with the drive key and is in no consumer is a finding with its file and its
/// line, so is a function that stays listed after it stopped, and a function the gate lists
/// itself or the registry names is not.
#[test]
fn x_delta_azdrive_keys_5_a_planted_function_that_uses_the_key_unlisted_and_a_stale_entry_are_caught(
) {
    let lib = "pub fn registered_one(key: &DriveKey) {\n\
    let wrapped = key.wrap_file_key(&file_key, &object);\n\
}\n\
pub fn forgotten(key: &DriveKey) {\n\
    let file_key = key.unwrap_file_key(&wrapped, &object);\n\
}\n\
pub fn innocent(key: &DriveKey) -> KeyId {\n\
    key.id()\n\
}\n\
#[cfg(test)]\n\
mod tests {\n\
    fn in_a_test(key: &DriveKey) {\n\
        let k = key.derive(CONTEXT);\n\
    }\n\
}\n";
    let registry = "pub const A: &[&str] = &[\n\
    \"examples/azul-storage/src/lib.rs#registered_one\",\n\
    \"examples/azul-storage/src/lib.rs#gone\",\n\
];\n";
    let base = base_with(
        "planted",
        &[
            ("examples/azul-storage/src/lib.rs", lib),
            (REGISTRY, registry),
        ],
    );
    let wrong = check(&base, SMALL).expect_err("an unlisted function and a stale entry");
    let _ = std::fs::remove_dir_all(&base);
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("lib.rs:4") && w.contains("forgotten")),
        "{wrong:?}"
    );
    assert!(wrong.iter().any(|w| w.contains("lib.rs#gone")), "{wrong:?}");
    assert!(
        wrong.iter().all(|w| !w.contains("registered_one")
            && !w.contains("innocent")
            && !w.contains("in_a_test")),
        "{wrong:?}"
    );
    // the data path of the gate is stale in the fixture too (none of its functions is there): the
    // list says what is true
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("crypto/mod.rs#wrap_file_key")),
        "{wrong:?}"
    );
}

/// X-DELTA-KEYS3-2: a function that writes or takes away the drop key file is a consumer of the drive key
/// even where it seals nothing itself - found, with its line, when no consumer names it; and a function that
/// only reads the file, or writes another, is not.
#[test]
fn x_delta_keys3_2_a_planted_writer_of_the_drop_key_file_is_found_and_a_reader_is_not() {
    let lib = "pub fn turn_off(bucket: &dyn Drive) {\n\
    bucket.delete(DROP_KEY_FILE);\n\
}\n\
pub fn turn_on(bucket: &dyn Drive) {\n\
    bucket.put(DROP_KEY_FILE, &bytes);\n\
}\n\
pub fn conditionally(bucket: &dyn Drive) {\n\
    bucket.put_if(DROP_KEY_FILE, &bytes, &Precondition::Absent);\n\
}\n\
pub fn looks_at_it(bucket: &dyn Drive) {\n\
    let bytes = bucket.get(DROP_KEY_FILE);\n\
}\n\
pub fn puts_another_file(bucket: &dyn Drive) {\n\
    bucket.put(OTHER_FILE, &bytes);\n\
}\n";
    let base = base_with(
        "drop-key-file",
        &[("examples/azul-storage/src/lib.rs", lib), (REGISTRY, "")],
    );
    let wrong = check(&base, SMALL).expect_err("three writers that are in no consumer");
    let _ = std::fs::remove_dir_all(&base);
    for (name, line) in [("turn_off", 1), ("turn_on", 4), ("conditionally", 7)] {
        assert!(
            wrong
                .iter()
                .any(|w| w.contains(&format!("lib.rs:{line}")) && w.contains(name)),
            "{name} is found at line {line}: {wrong:?}"
        );
    }
    assert!(
        wrong
            .iter()
            .all(|w| !w.contains("looks_at_it") && !w.contains("puts_another_file")),
        "{wrong:?}"
    );
}

/// X-DELTA-STRUCT-KEYSTATE-8: the spellings that hid a use of the key from the scanner are found, as items of
/// their own with their lines: a name the file gives a needle (`use .. as`, `type`), the path of a method
/// written as a function (`DriveKey::derive(&key, ..)`), and a `macro_rules!` that seals - the functions that
/// call it do not show what it seals with, so it is listed (`<file>#<macro>`) like a function.
#[test]
fn x_delta_struct_keystate_8_an_alias_a_path_call_and_a_macro_body_are_found() {
    let lib = "use crate::crypto::keys::RecoveryWrap as Wrap;
use crate::crypto::azl1 as a1;
type Member = keys::MemberWrap;
pub fn aliased(key: &DriveKey) {
    let w = Wrap::seal(key, drive, &code, kdf);
}
pub fn module_aliased(key: &DriveKey) {
    let e = a1::encrypt(&plain, id, key, &options);
}
pub fn type_aliased(key: &DriveKey) {
    let w = Member::seal(key, drive, &member, &public);
}
pub fn path_call(key: &DriveKey) {
    let s = DriveKey::derive(key, CONTEXT);
}
macro_rules! sealing {
    ($key:expr) => { DropKeyFile::seal($key, drive, &secret) };
}
macro_rules! harmless {
    ($a:expr) => { $a + 1 };
}
pub fn innocent(n: u64) -> u8 {
    n as u8
}
";
    let base = base_with(
        "spellings",
        &[("examples/azul-storage/src/lib.rs", lib), (REGISTRY, "")],
    );
    let wrong = check(&base, SMALL).expect_err("five spellings that are in no consumer");
    let _ = std::fs::remove_dir_all(&base);
    for (name, line) in [
        ("aliased", 4),
        ("module_aliased", 7),
        ("type_aliased", 10),
        ("path_call", 13),
        ("sealing", 16),
    ] {
        assert!(
            wrong
                .iter()
                .any(|w| w.contains(&format!("lib.rs:{line}")) && w.contains(name)),
            "{name} is found at line {line}: {wrong:?}"
        );
    }
    assert!(
        wrong
            .iter()
            .all(|w| !w.contains("innocent") && !w.contains("harmless")),
        "{wrong:?}"
    );
}

/// The name of a macro is read from its line: a registry that lists it by that name covers it.
#[test]
fn a_macro_is_named_by_its_macro_rules_line() {
    assert_eq!(
        name_of("macro_rules! sealing {").as_deref(),
        Some("sealing")
    );
    assert_eq!(
        name_of("pub fn sealed(key: &DriveKey) {").as_deref(),
        Some("sealed")
    );
}

#[test]
fn a_consumer_that_is_also_a_function_of_the_data_path_is_a_finding() {
    let registered: BTreeSet<String> = DATA_PATH.iter().map(|s| (*s).to_string()).collect();
    let wrong = judge(&[], &registered);
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("is a consumer and a function of the data path")),
        "{wrong:?}"
    );
}

#[test]
fn the_e2e_harness_is_no_consumer_and_no_finding() {
    let hit = Hit {
        rel: String::from("examples/azul-drive/src/e2e/b_tamper.rs"),
        line: 631,
        rule: String::from(RULE),
        text: String::from("fn replace_drive_key("),
    };
    let wrong = judge(&[hit], &BTreeSet::new());
    assert!(
        wrong.iter().all(|w| !w.contains("e2e")),
        "the harness is excepted: {wrong:?}"
    );
}
