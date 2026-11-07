//! Gate: whether a drive is encrypted is asked in one shape, and every call of an Azlin drive
//! passes through its gate (ARCH28 RF40 / AR5b-3, X-AR5b-3).
//!
//! The old answer was `Option<bool>`: `Some(true)`, `Some(false)` and a `None` that each reader
//! read its own way - one as "plain", one as "ask later", one as "not mine". `EncState` has four
//! states (`Undecided`, `Encrypted`, `SetupPending`, `Unencrypted`), `Undecided` is never plain,
//! and what each thing does in each state is an exhaustive function in `trust.rs`
//! (`admits`, `sync_kind`, `restore_kind`, `may_cache_listing`, `has_drive_index`, `index_home`).
//! Three rules keep it that way, each with no allowance at all:
//!
//! - `.is_encrypted()` is not a method any more: a reader that wants the old yes/no asks the
//!   state's policy. (`device::is_encrypted(bucket)` is a different question - does this bucket
//!   hold key files - and a free function, not a method.)
//! - every `Drive` method of `AutoEncrypted` takes its drive from `self.gate(Op::..)` with the Op
//!   its name stands for (a read, a write, a delete), so that a new method, or one that changes
//!   from a read to a write, is a line in the table below and in `trust.rs`'s `admits` and not an
//!   unguarded path to the bucket.
//! - a `match` over the state has no `_ =>` arm: a fifth state is a compile error in every reader
//!   and not a quiet "plain" in the one that had a wildcard.
//!
//! The rules read the shipped code of the crates that open drives; test items and comments are
//! not read.

use std::path::Path;

use azul_srcgate::{
    files, method_calls, self_test, workspace, Allow, Budget, Gate, Located, Orphans, Root, Rule,
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
        dir: "examples/azcloud-api/src",
        min_files: 1,
    },
    Root {
        dir: "examples/azul-bridge/src",
        min_files: 15,
    },
    Root {
        dir: "examples/azul-mail-core/src",
        min_files: 8,
    },
];

const TWO_VALUED: &str = "two-valued-encryption-question";
const UNGATED: &str = "drive-method-outside-its-gate";
const WILDCARD: &str = "wildcard-arm-over-the-encryption-state";

/// What each `Drive` method of `AutoEncrypted` asks its gate for: `Op::Read`, `Op::Write` or
/// `Op::Delete`. A new method of the trait that the type implements is a row here.
const OPS: &[(&str, &str)] = &[
    ("list", "Read"),
    ("get", "Read"),
    ("get_range", "Read"),
    ("get_limited", "Read"),
    ("head", "Read"),
    ("metadata", "Read"),
    ("put", "Write"),
    ("put_from", "Write"),
    ("put_file", "Write"),
    ("put_file_checked", "Write"),
    ("put_if", "Write"),
    ("put_from_if", "Write"),
    ("put_file_if", "Write"),
    ("conditional_writes", "Write"),
    ("copy", "Write"),
    ("create_folder", "Write"),
    ("rename", "Write"),
    ("put_many", "Write"),
    ("move_many", "Write"),
    ("put_from_many", "Write"),
    ("delete", "Delete"),
    ("delete_folder", "Delete"),
    ("remove_empty_folder", "Delete"),
    ("delete_many", "Delete"),
];

/// The index of the closer that matches the opener at `open` (`(`, `[` or `{`), or the end.
fn close_of(t: &[Located], open: usize) -> usize {
    let mut depth = 0usize;
    for (i, x) in t.iter().enumerate().skip(open) {
        if let azul_srcgate::Tok::Punct(c) = &x.tok {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return i;
                    }
                }
                _ => {}
            }
        }
    }
    t.len()
}

/// One `fn` of an `impl Drive for AutoEncrypted`.
struct Method {
    line: usize,
    name: String,
    /// How many times its body writes `self.gate(`.
    gates: usize,
    /// The `Op::X` words its body names, `X` each.
    ops: Vec<String>,
}

/// The methods of every `impl Drive for AutoEncrypted` in a file's tokens.
fn drive_methods(t: &[Located]) -> Vec<Method> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < t.len() {
        if t[i].tok.is_ident("impl") {
            let Some(open) = (i..t.len()).find(|&k| t[k].tok.is_punct('{')) else {
                break;
            };
            let header: Vec<&str> = t[i + 1..open]
                .iter()
                .filter_map(|x| x.tok.ident())
                .collect();
            if header.ends_with(&["Drive", "for", "AutoEncrypted"]) {
                let end = close_of(t, open);
                let mut k = open + 1;
                while k < end {
                    if t[k].tok.is_ident("fn") {
                        let name = t
                            .get(k + 1)
                            .and_then(|x| x.tok.ident())
                            .unwrap_or("")
                            .to_string();
                        let Some(body_open) = (k..end).find(|&j| t[j].tok.is_punct('{')) else {
                            break;
                        };
                        let body_close = close_of(t, body_open).min(t.len());
                        let body = &t[body_open..body_close];
                        let gates = body
                            .windows(4)
                            .filter(|w| {
                                w[0].tok.is_ident("self")
                                    && w[1].tok.is_punct('.')
                                    && w[2].tok.is_ident("gate")
                                    && w[3].tok.is_punct('(')
                            })
                            .count();
                        let ops = body
                            .windows(4)
                            .filter(|w| {
                                w[0].tok.is_ident("Op")
                                    && w[1].tok.is_punct(':')
                                    && w[2].tok.is_punct(':')
                            })
                            .filter_map(|w| w[3].tok.ident().map(str::to_string))
                            .collect();
                        out.push(Method {
                            line: t[k].line,
                            name,
                            gates,
                            ops,
                        });
                        k = body_close;
                    }
                    k += 1;
                }
                i = end;
            }
        }
        i += 1;
    }
    out
}

/// The methods that do not take their drive from the gate with the Op their name stands for.
fn ungated(t: &[Located]) -> Vec<usize> {
    drive_methods(t)
        .into_iter()
        .filter(|m| match OPS.iter().find(|(name, _)| *name == m.name) {
            None => true,
            Some((_, op)) => m.gates == 0 || m.ops.is_empty() || m.ops.iter().any(|o| o != op),
        })
        .map(|m| m.line)
        .collect()
}

/// A `match` whose arms name `EncState` and that has a top-level `_ =>` arm.
fn wildcard_over_the_state(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if !x.tok.is_ident("match") {
            continue;
        }
        // the `{` that opens the arms: the first one outside parentheses and brackets
        let mut paren = 0usize;
        let mut open = None;
        for (k, y) in t.iter().enumerate().skip(i + 1) {
            if let azul_srcgate::Tok::Punct(c) = &y.tok {
                match c {
                    '(' | '[' => paren += 1,
                    ')' | ']' => paren = paren.saturating_sub(1),
                    '{' if paren == 0 => {
                        open = Some(k);
                        break;
                    }
                    _ => {}
                }
            }
        }
        let Some(open) = open else { continue };
        let end = close_of(t, open).min(t.len());
        let arms = &t[open..end];
        if !arms.iter().any(|y| y.tok.is_ident("EncState")) {
            continue;
        }
        // a `_ =>` at the arms' own level (one brace, no parenthesis open)
        let (mut braces, mut parens) = (0usize, 0usize);
        for (k, y) in arms.iter().enumerate() {
            if let azul_srcgate::Tok::Punct(c) = &y.tok {
                match c {
                    '{' => braces += 1,
                    '}' => braces = braces.saturating_sub(1),
                    '(' | '[' => parens += 1,
                    ')' | ']' => parens = parens.saturating_sub(1),
                    _ => {}
                }
            }
            if braces == 1
                && parens == 0
                && y.tok.is_ident("_")
                && arms.get(k + 1).is_some_and(|n| n.tok.is_punct('='))
                && arms.get(k + 2).is_some_and(|n| n.tok.is_punct('>'))
            {
                found.push(y.line);
            }
        }
    }
    found
}

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            TWO_VALUED,
            |t| method_calls(t, "is_encrypted"),
            &[
                "let plain = drive.is_encrypted() == Some(false);",
                "if auto.is_encrypted().unwrap_or(false) { go(); }",
            ],
            &[
                "let listed = device::is_encrypted(bucket.as_ref())?;",
                "let state = drive.state();",
                "let is_encrypted = state.encrypted();",
            ],
        ),
        Rule::new(
            UNGATED,
            ungated,
            &[
                "impl Drive for AutoEncrypted { fn get(&self, key: &str) -> R { self.inner.get(key) } }",
                "impl Drive for AutoEncrypted { fn put(&self, key: &str, b: &[u8]) -> R { self.gate(Op::Read)?.put(key, b) } }",
                "impl Drive for AutoEncrypted { fn frobnicate(&self) -> R { self.gate(Op::Read)?.frobnicate() } }",
                "impl Drive for AutoEncrypted { fn delete(&self, key: &str) -> R { self.drive().delete(key) } fn get(&self, key: &str) -> R { self.gate(Op::Read)?.get(key) } }",
            ],
            &[
                "impl Drive for AutoEncrypted { fn get(&self, key: &str) -> R { self.gate(Op::Read)?.get(key) } }",
                "impl Drive for AutoEncrypted { fn put_many(&self, i: &[B]) -> V { match self.gate(Op::Write) { Ok(d) => d.put_many(i), Err(e) => vec![] } } }",
                "impl Drive for MemBucket { fn get(&self, key: &str) -> R { self.map.get(key) } }",
                "impl Drive for AutoEncrypted { fn delete(&self, k: &str) -> R { self.gate(Op::Delete)?.delete(k) } fn list(&self, r: &L) -> R { self.gate(Op::Read)?.list(r) } }",
            ],
        ),
        Rule::new(
            WILDCARD,
            wildcard_over_the_state,
            &[
                "let n = match drive.state() { EncState::Encrypted { .. } => 1, _ => 0 };",
                "match state { EncState::Undecided => {} _ => { go(); } }",
            ],
            &[
                "let n = match drive.state() { EncState::Undecided => 0, EncState::SetupPending => 1, EncState::Encrypted { .. } => 2, EncState::Unencrypted(_) => 3 };",
                "let n = match (state, op) { (EncState::Undecided, _) => 0, (_, Op::Read) => 1 };",
                "let n = match x { Some(1) => 1, _ => 0 };",
                "let n = match x { Some(EncState::Undecided) => match y { 1 => 1, _ => 2 }, other => 3 };",
            ],
        ),
    ]
}

fn gate(roots: &'static [Root]) -> Gate<'static> {
    Gate {
        name: "the encryption state is asked in one shape",
        roots,
        rules: rules(),
        budget: Budget(&[] as &[Allow]),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn x_ar5b_3_the_encryption_state_is_asked_in_one_shape() {
    gate(ROOTS).run();
}

/// The gate reads the impl it guards: all the methods of the table, each once, and no method the
/// table does not know. (A rename of the type would otherwise leave the rule with nothing to
/// read.)
#[test]
fn x_ar5b_3_the_gate_reads_every_drive_method_of_the_auto_encrypted_drive() {
    self_test(&rules());
    let read = files(
        &workspace(),
        &[Root {
            dir: "examples/azul-storage/src",
            min_files: 55,
        }],
        Orphans::Ignore,
    )
    .unwrap_or_else(|wrong| panic!("{wrong:#?}"));
    let file = read
        .iter()
        .find(|f| f.rel == "examples/azul-storage/src/encrypted.rs")
        .expect("encrypted.rs is read");
    let methods = drive_methods(&file.tokens());
    let mut named: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
    named.sort_unstable();
    let mut table: Vec<&str> = OPS.iter().map(|(n, _)| *n).collect();
    table.sort_unstable();
    assert_eq!(
        named, table,
        "the methods of `impl Drive for AutoEncrypted` and the table of this gate differ: a new \
         method is a row (with the Op it asks), a removed one leaves it"
    );
    assert!(
        methods.iter().all(|m| m.gates >= 1),
        "a method without self.gate(..): {:?}",
        methods
            .iter()
            .filter(|m| m.gates == 0)
            .map(|m| (&m.name, m.line))
            .collect::<Vec<_>>()
    );
}

/// A tree on disk below a fresh base: (path below the base, text).
fn base_with(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!(
        "azul-srcgate-encstate-{name}-{}",
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

fn check(base: &Path) -> Result<usize, Vec<String>> {
    gate(SMALL).check(base).map(|hits| hits.len())
}

#[test]
fn x_ar5b_3_a_plant_of_each_of_the_three_forms_is_caught_with_its_line() {
    let planted = "pub struct AutoEncrypted;\n\
impl Drive for AutoEncrypted {\n\
    fn get(&self, key: &str) -> R {\n\
        self.bucket.get(key)\n\
    }\n\
    fn put(&self, key: &str, bytes: &[u8]) -> R {\n\
        self.gate(Op::Write)?.put(key, bytes)\n\
    }\n\
}\n\
pub fn plain(d: &AutoEncrypted) -> bool {\n\
    d.is_encrypted() == Some(false)\n\
}\n\
pub fn kind(s: EncState) -> u8 {\n\
    match s {\n\
        EncState::Undecided => 0,\n\
        _ => 1,\n\
    }\n\
}\n";
    let base = base_with("planted", &[("examples/azul-storage/src/lib.rs", planted)]);
    let wrong = check(&base).expect_err("the three plants are findings");
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(wrong.len(), 3, "{wrong:?}");
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("lib.rs:3") && w.contains(UNGATED)),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("lib.rs:11") && w.contains(TWO_VALUED)),
        "{wrong:?}"
    );
    assert!(
        wrong
            .iter()
            .any(|w| w.contains("lib.rs:16") && w.contains(WILDCARD)),
        "{wrong:?}"
    );
}
