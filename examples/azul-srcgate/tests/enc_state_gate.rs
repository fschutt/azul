//! Gate: whether a drive is encrypted is asked in one shape, and every call of an Azlin drive
//! passes through its gate (ARCH28 RF40 / AR5b-3, X-AR5b-3).
//!
//! The old answer was `Option<bool>`: `Some(true)`, `Some(false)` and a `None` that each reader
//! read its own way - one as "plain", one as "ask later", one as "not mine". `EncState` has four
//! states (`Undecided`, `Encrypted`, `SetupPending`, `Unencrypted`), `Undecided` is never plain,
//! and what each thing does in each state is an exhaustive function in `trust.rs`
//! (`admits`, `sync_kind`, `restore_kind`, `may_cache_listing`, `has_drive_index`, `index_home`).
//! Six rules keep it that way; the first five have no allowance at all:
//!
//! - `.is_encrypted()` is not a method any more, under no other name either (`is_encrypted_now`,
//!   `is_unencrypted`): a reader that wants the old yes/no asks the state's policy.
//!   (`device::is_encrypted(bucket)` is a different question - does this bucket hold key files -
//!   and a free function, not a method.)
//! - every `Drive` method of `AutoEncrypted` takes its drive from `self.gate(Op::..)` with the Op
//!   its name stands for (a read, a write, a delete), so that a new method, or one that changes
//!   from a read to a write, is a line in the table below and in `trust.rs`'s `admits` and not an
//!   unguarded path to the bucket.
//! - a `match` over the state has no arm that takes every state that is left: not `_ =>`, not a
//!   name (`other =>`, `_s =>`, `other if ..`), not `_` or a name in the state's place of a tuple
//!   (`(_, Op::Read) =>`): a fifth state is a compile error in every reader and not a quiet
//!   "plain" in the one that had a catch-all.
//! - the state is not tested outside a `match` (`matches!(s, EncState::..)`, `if let EncState::..`,
//!   `s == EncState::..`) nor renamed (`use EncState as E`): those are the non-exhaustive forms,
//!   and the exhaustive functions of `trust.rs` are what a reader asks.
//! - no data call (`put`, `get`, `delete`, `list`, ...) is made on the plain bucket
//!   (`auto.bucket().put(..)`) or on a handle named `azlin` below the gate: `AutoEncrypted::bucket()`
//!   is for the key flows and has some twenty-five callers, and the notice that went to it as a
//!   plain object (X-DELTA-AZDRIVE-KEYS-4) is how a second writer got past the gate.
//! - (counted) the inherent methods of `AutoEncrypted` read `self.bucket` in four places only:
//!   the accessor and the three reads of `resolved()`. A new method that reads it is a new way
//!   past the gate: it is a line in the allowance, which asks someone to look.
//!
//! The rules read the shipped code of the crates that open drives; test items and comments are
//! not read.

use std::path::Path;

use azul_srcgate::{
    files, self_test, workspace, Allow, Budget, Gate, Located, Orphans, Root, Rule,
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
const NON_EXHAUSTIVE: &str = "encryption-state-tested-outside-a-match";
const BELOW_THE_GATE: &str = "data-call-on-a-handle-below-the-gate";
const RAW_BUCKET: &str = "inherent-method-reads-the-raw-bucket";

/// The `Drive` methods that go to a bucket's objects, as opposed to the handle's own.
const DATA_METHODS: &[&str] = &[
    "list",
    "get",
    "get_range",
    "get_limited",
    "head",
    "put",
    "put_from",
    "put_file",
    "put_file_checked",
    "put_if",
    "put_from_if",
    "put_file_if",
    "copy",
    "create_folder",
    "rename",
    "put_many",
    "move_many",
    "put_from_many",
    "delete",
    "delete_folder",
    "remove_empty_folder",
    "delete_many",
];

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

fn punct_at(t: &[Located], i: usize, c: char) -> bool {
    t.get(i).is_some_and(|x| x.tok.is_punct(c))
}

/// The arms (from the `{` that opens them to the closer) of the `match` whose keyword is at `i`,
/// when they name `EncState`.
fn state_arms(t: &[Located], i: usize) -> Option<&[Located]> {
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
    let open = open?;
    let end = close_of(t, open).min(t.len());
    let arms = &t[open..end];
    arms.iter()
        .any(|y| y.tok.is_ident("EncState"))
        .then_some(arms)
}

/// An identifier that takes whatever it is given as a pattern: `_`, `_s`, `other`.
fn takes_everything(name: &str) -> bool {
    name.starts_with('_') || name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
}

/// A `match` whose arms name `EncState` and that has an arm which takes every state that is left:
/// a top-level `_ =>`, a name (`other =>`, `_s =>`, `other if ..`), or `_` or a name in the place
/// of a tuple pattern where another arm names the state (`(_, Op::Read) =>`).
fn wildcard_over_the_state(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if !x.tok.is_ident("match") {
            continue;
        }
        let Some(arms) = state_arms(t, i) else {
            continue;
        };
        // The arm-level tokens only (one brace, no parenthesis open): a catch-all pattern, and the
        // elements of each tuple pattern.
        let (mut braces, mut parens) = (0usize, 0usize);
        let mut tuples: Vec<Vec<&[Located]>> = Vec::new();
        for (k, y) in arms.iter().enumerate() {
            let at_arm_level = braces == 1 && parens == 0;
            let starts_an_arm = k > 0
                && (punct_at(arms, k - 1, '{')
                    || punct_at(arms, k - 1, ',')
                    || punct_at(arms, k - 1, '}'));
            if at_arm_level && starts_an_arm {
                if let Some(name) = y.tok.ident() {
                    let then_arrow = punct_at(arms, k + 1, '=') && punct_at(arms, k + 2, '>');
                    let then_guard = arms.get(k + 1).is_some_and(|n| n.tok.is_ident("if"));
                    if takes_everything(name) && (then_arrow || then_guard) {
                        found.push(y.line);
                    }
                } else if y.tok.is_punct('(') {
                    let close = close_of(arms, k).min(arms.len());
                    let mut elements: Vec<&[Located]> = Vec::new();
                    let (mut depth, mut from) = (0usize, k + 1);
                    for (j, z) in arms.iter().enumerate().take(close).skip(k + 1) {
                        if let azul_srcgate::Tok::Punct(c) = &z.tok {
                            match c {
                                '(' | '[' | '{' => depth += 1,
                                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                                ',' if depth == 0 => {
                                    elements.push(&arms[from..j]);
                                    from = j + 1;
                                }
                                _ => {}
                            }
                        }
                    }
                    if from < close {
                        elements.push(&arms[from..close]);
                    }
                    tuples.push(elements);
                }
            }
            if let azul_srcgate::Tok::Punct(c) = &y.tok {
                match c {
                    '{' => braces += 1,
                    '}' => braces = braces.saturating_sub(1),
                    '(' | '[' => parens += 1,
                    ')' | ']' => parens = parens.saturating_sub(1),
                    _ => {}
                }
            }
        }
        // Where some arm of a tuple pattern names the state, no other arm may take anything there.
        let state_places: Vec<usize> = (0..8)
            .filter(|&place| {
                tuples.iter().any(|elements| {
                    elements
                        .get(place)
                        .is_some_and(|e| e.iter().any(|y| y.tok.is_ident("EncState")))
                })
            })
            .collect();
        for elements in &tuples {
            for &place in &state_places {
                if let Some([only]) = elements.get(place) {
                    if only.tok.ident().is_some_and(takes_everything) {
                        found.push(only.line);
                    }
                }
            }
        }
    }
    found
}

/// A method call `.name(` whose name `is` accepts.
fn method_calls_where(t: &[Located], is: impl Fn(&str) -> bool) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let Some(name) = x.tok.ident() else { continue };
        if i > 0 && punct_at(t, i - 1, '.') && is(name) && punct_at(t, i + 1, '(') {
            found.push(x.line);
        }
    }
    found
}

/// The old yes/no question under any of its names: `is_encrypted`, `is_encrypted_now`,
/// `is_unencrypted`.
fn two_valued(t: &[Located]) -> Vec<usize> {
    method_calls_where(t, |name| {
        name == "is_encrypted"
            || name.starts_with("is_encrypted_")
            || name.starts_with("is_unencrypted")
    })
}

/// The state asked about outside a `match`: `matches!(.., EncState::..)`, a refutable `let` (an
/// `if let`, a `while let`, a `let .. else`) whose pattern names it, `== EncState::..`,
/// `!= EncState::..`, and `EncState as Other`.
fn state_tested_outside_a_match(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        if x.tok.is_ident("matches") && punct_at(t, i + 1, '!') && punct_at(t, i + 2, '(') {
            let close = close_of(t, i + 2).min(t.len());
            if t[i + 3..close].iter().any(|y| y.tok.is_ident("EncState")) {
                found.push(x.line);
            }
        } else if x.tok.is_ident("let") {
            // the pattern: up to the first top-level `=`; a lone `:` first is a type annotation
            // (`let s: EncState = ..` names the type of a binding, which tests nothing)
            let mut depth = 0usize;
            let mut pattern_end = None;
            for (k, y) in t.iter().enumerate().skip(i + 1) {
                match &y.tok {
                    azul_srcgate::Tok::Punct('(' | '[' | '{') => depth += 1,
                    azul_srcgate::Tok::Punct(')' | ']' | '}') => {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                    }
                    azul_srcgate::Tok::Punct(';') if depth == 0 => break,
                    azul_srcgate::Tok::Punct(':')
                        if depth == 0 && !punct_at(t, k + 1, ':') && !punct_at(t, k - 1, ':') =>
                    {
                        break;
                    }
                    azul_srcgate::Tok::Punct('=')
                        if depth == 0 && !punct_at(t, k + 1, '=') && !punct_at(t, k + 1, '>') =>
                    {
                        pattern_end = Some(k);
                        break;
                    }
                    _ => {}
                }
            }
            if let Some(end) = pattern_end {
                if t[i + 1..end].iter().any(|y| y.tok.is_ident("EncState")) {
                    found.push(x.line);
                }
            }
        } else if x.tok.is_ident("EncState") {
            let compared = i >= 2
                && punct_at(t, i - 1, '=')
                && (punct_at(t, i - 2, '=') || punct_at(t, i - 2, '!'));
            let renamed = t.get(i + 1).is_some_and(|n| n.tok.is_ident("as"));
            if compared || renamed {
                found.push(x.line);
            }
        }
    }
    found
}

/// The chain of names a method call is made on, nearest first, each with whether it is itself
/// called: `auto.bucket().as_ref().put(..)` gives `[("as_ref", true), ("bucket", true), ("auto",
/// false)]` for the `put`. The chain ends at the first plain identifier or at something it cannot
/// read.
fn receiver_chain(t: &[Located], dot: usize) -> Vec<(&str, bool)> {
    let mut chain = Vec::new();
    let mut at = dot;
    while at >= 1 {
        let mut j = at - 1;
        let called = punct_at(t, j, ')');
        // a call's `( .. )` before the dot: step back over it
        if called {
            let mut depth = 0usize;
            let mut open = None;
            for k in (0..=j).rev() {
                if punct_at(t, k, ')') {
                    depth += 1;
                } else if punct_at(t, k, '(') {
                    depth -= 1;
                    if depth == 0 {
                        open = Some(k);
                        break;
                    }
                }
            }
            let Some(open) = open else { break };
            if open == 0 {
                break;
            }
            j = open - 1;
        }
        let Some(name) = t[j].tok.ident() else { break };
        chain.push((name, called));
        if j >= 1 && punct_at(t, j - 1, '.') {
            at = j - 1;
        } else {
            break;
        }
    }
    chain
}

/// A data call on the plain bucket or on a handle named `azlin`: `auto.bucket().put(..)`,
/// `azlin.get(..)`. The key flows pass the bucket to functions; they do not call it. The accessor
/// is the method `bucket()`: a field of that name is another thing (a transport's own bucket, the
/// raw bucket of the unlocked drive that the period vault seals its state for).
fn data_call_below_the_gate(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let Some(name) = x.tok.ident() else { continue };
        if i == 0
            || !punct_at(t, i - 1, '.')
            || !punct_at(t, i + 1, '(')
            || !DATA_METHODS.contains(&name)
        {
            continue;
        }
        let chain = receiver_chain(t, i - 1);
        let below = chain.first().is_some_and(|(n, _)| *n == "azlin")
            || chain
                .iter()
                .enumerate()
                .any(|(k, (n, called))| *n == "bucket" && *called && chain.get(k + 1).is_some());
        if below {
            found.push(x.line);
        }
    }
    found
}

/// The reads of the field `self.bucket` inside `impl AutoEncrypted { .. }` (not the trait impls,
/// whose methods go through `self.gate(..)`): a use of the field, not a call of the method.
fn raw_bucket_reads(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
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
            if header == ["AutoEncrypted"] {
                let end = close_of(t, open).min(t.len());
                for k in open..end.saturating_sub(2) {
                    if t[k].tok.is_ident("self")
                        && punct_at(t, k + 1, '.')
                        && t[k + 2].tok.is_ident("bucket")
                        && !punct_at(t, k + 3, '(')
                    {
                        found.push(t[k].line);
                    }
                }
                i = end;
            }
        }
        i += 1;
    }
    found
}

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            TWO_VALUED,
            two_valued,
            &[
                "let plain = drive.is_encrypted() == Some(false);",
                "if auto.is_encrypted().unwrap_or(false) { go(); }",
                "if auto.is_encrypted_now() { go(); }",
                "let plain = drive.is_unencrypted();",
            ],
            &[
                "let listed = device::is_encrypted(bucket.as_ref())?;",
                "let state = drive.state();",
                "let is_encrypted = state.encrypted();",
                "let n = state.is_decided();",
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
                // a catch-all that has a name, with or without a guard
                "let n = match state { EncState::Undecided => 0, other => 1 };",
                "let n = match state { EncState::Undecided => 0, _s => 1 };",
                "let n = match state { EncState::Undecided => 0, rest if ready => 1, _ => 2 };",
                // `_` in the state's place of a tuple pattern: any state meets it
                "let n = match (state, op) { (EncState::Undecided, _) => 0, (_, Op::Read) => 1 };",
                "let n = match (op, state) { (Op::Read, EncState::Undecided) => 0, (Op::Write, other) => 1 };",
            ],
            &[
                "let n = match drive.state() { EncState::Undecided => 0, EncState::SetupPending => 1, EncState::Encrypted { .. } => 2, EncState::Unencrypted(_) => 3 };",
                "let n = match (state, op) { (EncState::Undecided, _) => 0, (EncState::Encrypted { .. }, Op::Read) => 1, (EncState::SetupPending, Op::Write) => 2 };",
                "let n = match x { Some(1) => 1, _ => 0 };",
                // the catch-all of a match that does not name the state is not this rule's
                "let n = match x { Some(EncState::Undecided) => match y { 1 => 1, _ => 2 }, Some(EncState::SetupPending) => 3, None => 4 };",
                "let n = match state { EncState::Encrypted { key_id, via } => key_id, EncState::Undecided | EncState::SetupPending | EncState::Unencrypted(_) => None };",
            ],
        ),
        Rule::new(
            NON_EXHAUSTIVE,
            state_tested_outside_a_match,
            &[
                "if matches!(auto.state(), EncState::Encrypted { .. }) { go(); }",
                "if let EncState::Undecided = auto.state() { go(); }",
                "let EncState::Encrypted { .. } = auto.state() else { return; };",
                "while let EncState::Undecided = auto.state() { wait(); }",
                "let plain = auto.state() == EncState::Undecided;",
                "let moved = auto.state() != EncState::SetupPending;",
                "use crate::trust::EncState as Shape;",
            ],
            &[
                "let state: EncState = auto.state();",
                "let state = EncState::Undecided;",
                "let n = match auto.state() { EncState::Undecided => 0, _ => 1 }; // the wildcard rule's",
                "let ok = state.is_decided();",
                "if let Some(s) = auto.state_if_known() { go(s); }",
                "let same = a.key_id == b.key_id;",
            ],
        ),
        Rule::new(
            BELOW_THE_GATE,
            data_call_below_the_gate,
            &[
                "auto.bucket().put(&key, text.as_bytes())?;",
                "let bytes = auto.bucket().as_ref().get(&key)?;",
                "azlin.put(&key, text.as_bytes())",
                "let page = azlin.list(&request)?;",
                "slot.auto.bucket().delete(&key)?;",
            ],
            &[
                "let bucket = Arc::clone(auto.bucket());",
                "device::setup_new_drive(auto.bucket().as_ref(), &keyring, auto.drive(), kdf)?;",
                "azlin.lockdown()?;",
                "azlin.apply(grant);",
                "let usage = azlin.usage_proof();",
                "let found = azlin_drives(s).get(drive_id);",
                "let one = drives.get(&id);",
                "let page = drive.list(&request)?;",
                // a field that is called `bucket` is not the accessor
                "self.bucket.put(key, data)",
                "match self.drive.bucket.put_if(KEY, &sealed, &condition) { Ok(_) => {} Err(e) => return Err(e) }",
            ],
        ),
        Rule::new(
            RAW_BUCKET,
            raw_bucket_reads,
            &[
                "impl AutoEncrypted { fn peek(&self) -> usize { self.bucket.list(&r) } }",
                "impl AutoEncrypted { pub fn raw(&self) -> Arc<dyn Drive> { Arc::clone(&self.bucket) } }",
            ],
            &[
                "impl AutoEncrypted { pub fn bucket(&self) -> &Arc<dyn Drive> { self.bucket() } }",
                "impl AutoEncrypted { fn drive(&self) -> &str { &self.drive } }",
                "impl Drive for AutoEncrypted { fn get(&self, k: &str) -> R { self.bucket.get(k) } }",
                "impl Other { fn f(&self) { self.bucket.list(&r); } }",
            ],
        ),
    ]
}

/// Where a rule is broken on purpose, counted. TO SHRINK (or to stay, with its reason).
const ALLOWED: &[Allow] = &[Allow::exactly(
    "examples/azul-storage/src/encrypted.rs",
    RAW_BUCKET,
    4,
    "the accessor `bucket()` (the keys live below the encryption), and the three reads of \
     `resolved()`: the decision (`Facts::gather`), the open (`open_encrypted`) and the bare bucket \
     of a drive that is not encrypted, which the gate then guards",
)];

fn gate(roots: &'static [Root], allowed: &'static [Allow]) -> Gate<'static> {
    Gate {
        name: "the encryption state is asked in one shape",
        roots,
        rules: rules(),
        budget: Budget(allowed),
        orphans: Orphans::Ignore,
    }
}

#[test]
fn x_ar5b_3_the_encryption_state_is_asked_in_one_shape() {
    gate(ROOTS, ALLOWED).run();
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
    gate(SMALL, &[]).check(base).map(|hits| hits.len())
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
