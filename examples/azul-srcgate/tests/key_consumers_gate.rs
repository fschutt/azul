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
//! The first run of this gate is the baseline of the registry: the functions it finds are the ones
//! the lists below name, written from a read of the sources (BLIND MODE: no test was run to write
//! them). A finding on that run is a function the lists missed or one that was renamed, to be
//! added to its consumer or to the data path with its reason - the lists only grow on purpose.
//!
//! It reads source text, as every gate of this crate does: a function is its `fn` keyword, its name
//! and the first `{ .. }` after them; comments and test items are not read; a function nested inside
//! another is part of it. A call is its path as written (`calls`, `method_calls`).

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

/// Whether the tokens of a function's body put the drive key to work.
fn uses_the_drive_key(body: &[Located]) -> bool {
    !method_calls(body, "wrap_file_key").is_empty()
        || !method_calls(body, "unwrap_file_key").is_empty()
        || !method_calls(body, "derive").is_empty()
        || !calls(body, "RecoveryWrap::seal").is_empty()
        || !calls(body, "MemberWrap::seal").is_empty()
        || !calls(body, "DropKeyFile::seal").is_empty()
        || !calls(body, "store_key_chain").is_empty()
        || !calls(body, "azl1::encrypt").is_empty()
        || !calls(body, "azl1::decrypt").is_empty()
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

/// The lines of the `fn` of every function that uses the drive key, once per function.
fn functions_using_the_drive_key(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < t.len() {
        let is_fn =
            t[i].tok.is_ident("fn") && t.get(i + 1).is_some_and(|name| name.tok.ident().is_some());
        if !is_fn {
            i += 1;
            continue;
        }
        match body_of(t, i) {
            Some((open, close)) => {
                if uses_the_drive_key(&t[open..close.min(t.len())]) {
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
        ],
        &[
            "let id = key.id();",
            "let wrapped = wrap_with(&sealing, id, &file_key);",
            "let chain = load_key_chain(bucket)?;",
            "let wrap = RecoveryWrap::parse(&bytes)?;",
            "let text = \"RecoveryWrap::seal(\";",
            "let open = MemberWrap::open(&wrap, drive, &secret)?;",
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

/// The name a function line declares: what follows its `fn`.
fn name_of(line: &str) -> Option<String> {
    let after = line.split_once("fn ")?.1;
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
