//! Gate: every deleter of data the history may still name asks ONE function (X-DELTA-SYNC2-2).
//!
//! The reclaimer's switch to deleting was gated for the reclaimer's own round only: the pack upkeep
//! (`collect_retired`, `recover_packs`) deleted in the mode the provider was CONFIGURED with, with none
//! of the clean rounds, days, devices or footing the switch promises. Now there is one function,
//! `ask_retired_delete` (azul-storage `meta/reclaim.rs`), and a deleter takes its answer: the two
//! functions that delete a data object on a proof (`delete_reclaimable`, `remove_reclaimable`) take a
//! `&Gate` - `Result<DeleteLicence, Hold>`, a licence only that function (and the two documented others:
//! an index with no history, a purge the user confirmed) makes. The type keeps a caller from deleting
//! without one; this gate keeps the list of deleters what it is:
//!
//! - `raw-data-delete`: a function that deletes by an object's bucket key (a `.delete(`,
//!   `.delete_many(` or `.remove(` in a function that names `bucket_key` or `bucket_object`) is one of
//!   the four of `meta/reclaim.rs`; a fifth, anywhere, is a deleter nobody asked.
//! - `unasked-delete-call`: a call of `delete_reclaimable(` or `remove_reclaimable(` whose arguments
//!   do not carry the `gate` its function was given.
//! - `deleter-without-a-gate`: a function that calls one of them and neither takes a `Gate` nor gets
//!   one from `ask_retired_delete`, `retired_delete_gate` or `live_objects` (which hands the upkeep its
//!   gate).
//! - `proof-deleter-call`: the callers of those two, counted: the reclaimer's delete run and the three
//!   functions of the pack upkeep.
//! - `licence-made`: `DeleteLicence { .. }` is written in `meta/reclaim.rs` three times and nowhere
//!   else (`#[cfg(test)]` items are not read: a test makes its own).
//! - `ask-site`: the calls of `ask_retired_delete(`: the reclaimer's round, the upkeep's gate, the
//!   upkeep's index without history.
//! - `confirmed-licence`: `DeleteLicence::confirmed(`, the purge a user confirmed.
//! - `go-call`: the table of the evidence, `go(`, is asked in ONE place - `ask_retired_delete` - and
//!   nowhere else: a second call is a copy of the rule.
//!
//! It reads source text as every gate of this crate does (comments and test items are not read; a
//! function is its `fn`, its name and the first `{ .. }` after them), so a deleter that is written
//! through a name this gate does not know is the case the rules' own planted cases show it does
//! not hide.

use azul_srcgate::{calls, method_calls, Allow, Budget, Gate, Located, Orphans, Root, Rule, Tok};

const ROOTS: &[Root] = &[
    Root {
        dir: "examples/azul-storage/src",
        min_files: 80,
    },
    Root {
        dir: "examples/azcloud-kit/src",
        min_files: 72,
    },
    Root {
        dir: "examples/azul-drive/src",
        min_files: 88,
    },
];

const RAW: &str = "raw-data-delete";
const UNASKED: &str = "unasked-delete-call";
const NO_GATE: &str = "deleter-without-a-gate";
const CALLER: &str = "proof-deleter-call";
const MADE: &str = "licence-made";
const ASKED: &str = "ask-site";
const CONFIRMED: &str = "confirmed-licence";
const GO: &str = "go-call";

/// The calls that delete a data object on a proof.
const PROOF_DELETERS: &[&str] = &["delete_reclaimable", "remove_reclaimable"];

/// Where a function gets the gate it hands a proof deleter.
const GATE_SOURCES: &[&str] = &["ask_retired_delete", "retired_delete_gate", "live_objects"];

/// The token range `[open, close]` of the body of the function whose `fn` is token `fn_at`; `None`
/// for a declaration that ends in `;` first.
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

/// Every function of the file: the token of its `fn`, and its signature and body ranges.
fn functions(t: &[Located]) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
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
                out.push((i, open, close.min(t.len())));
                i = close + 1;
            }
            None => i += 1,
        }
    }
    out
}

/// Whether the tokens name `name` as an identifier.
fn names(t: &[Located], name: &str) -> bool {
    t.iter().any(|x| x.tok.is_ident(name))
}

/// The token ranges of the argument list of every method call `.name(` in `t`.
fn method_call_args(t: &[Located], name: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let is_call = x.tok.is_ident(name)
            && i.checked_sub(1).is_some_and(|b| t[b].tok.is_punct('.'))
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct('('));
        if !is_call {
            continue;
        }
        let mut depth = 0i32;
        let mut end = t.len();
        for (k, y) in t.iter().enumerate().skip(i + 1) {
            match &y.tok {
                Tok::Punct('(') => depth += 1,
                Tok::Punct(')') => {
                    depth -= 1;
                    if depth == 0 {
                        end = k;
                        break;
                    }
                }
                _ => {}
            }
        }
        found.push((i, end));
    }
    found
}

/// The lines of the `fn` of each function that deletes a data object by its bucket key: a `.delete(` or
/// `.remove(` whose arguments name `bucket_key` or `bucket_object`, or a `.delete_many(` in a function
/// that names `bucket_key` (the keys are made before the batch is built). A `.remove(` on a map of this
/// process (`known.remove(&id)`) names no key of the bucket and is no deleter.
fn raw_data_deleters(t: &[Located]) -> Vec<usize> {
    functions(t)
        .into_iter()
        .filter(|(_, open, close)| {
            let body = &t[*open..*close];
            let by_argument = ["delete", "remove"].iter().any(|name| {
                method_call_args(body, name).into_iter().any(|(at, end)| {
                    names(&body[at..end], "bucket_key") || names(&body[at..end], "bucket_object")
                })
            });
            let by_batch =
                !method_calls(body, "delete_many").is_empty() && names(body, "bucket_key");
            by_argument || by_batch
        })
        .map(|(at, _, _)| t[at].line)
        .collect()
}

/// The token ranges of the argument lists of every call of the proof deleters in `t`.
fn proof_delete_calls(t: &[Located]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let is_call = PROOF_DELETERS.iter().any(|name| x.tok.is_ident(name))
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct('('))
            && !i.checked_sub(1).is_some_and(|b| t[b].tok.is_ident("fn"));
        if !is_call {
            continue;
        }
        let mut depth = 0i32;
        let mut end = t.len();
        for (k, y) in t.iter().enumerate().skip(i + 1) {
            match &y.tok {
                Tok::Punct('(') => depth += 1,
                Tok::Punct(')') => {
                    depth -= 1;
                    if depth == 0 {
                        end = k;
                        break;
                    }
                }
                _ => {}
            }
        }
        found.push((i, end));
    }
    found
}

/// The lines of the calls of a proof deleter whose arguments do not name the `gate`.
fn unasked_calls(t: &[Located]) -> Vec<usize> {
    proof_delete_calls(t)
        .into_iter()
        .filter(|(at, end)| !names(&t[*at..*end], "gate"))
        .map(|(at, _)| t[at].line)
        .collect()
}

/// The lines of the calls of a proof deleter, once each.
fn proof_callers(t: &[Located]) -> Vec<usize> {
    proof_delete_calls(t)
        .into_iter()
        .map(|(at, _)| t[at].line)
        .collect()
}

/// The lines of the `fn` of each function that calls a proof deleter and has no source of a gate: no
/// `Gate` in its signature, and no call of the functions that make one.
fn deleters_without_a_gate(t: &[Located]) -> Vec<usize> {
    functions(t)
        .into_iter()
        .filter(|(at, open, close)| {
            let calls_one = PROOF_DELETERS
                .iter()
                .any(|name| !calls(&t[*open..*close], name).is_empty());
            let signature = &t[*at..*open];
            let body = &t[*open..*close];
            let has_a_gate =
                names(signature, "Gate") || GATE_SOURCES.iter().any(|source| names(body, source));
            calls_one && !has_a_gate
        })
        .map(|(at, _, _)| t[at].line)
        .collect()
}

/// A struct literal `DeleteLicence { .. }`: the name and a brace, not a `struct` or `impl` header and
/// not a return type (`-> DeleteLicence {` opens the body of a function).
fn licences_made(t: &[Located]) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let returned = i >= 2 && t[i - 2].tok.is_punct('-') && t[i - 1].tok.is_punct('>');
        if x.tok.is_ident("DeleteLicence")
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct('{'))
            && !returned
            && !i.checked_sub(1).is_some_and(|b| {
                t[b].tok.is_ident("struct") || t[b].tok.is_ident("impl") || t[b].tok.is_ident("for")
            })
        {
            found.push(x.line);
        }
    }
    found
}

/// Every call of the function `name` however its path is written (`ask_retired_delete(`,
/// `crate::meta::reclaim::ask_retired_delete(`): not a method and not a definition.
fn named_calls(t: &[Located], name: &str) -> Vec<usize> {
    let mut found = Vec::new();
    for (i, x) in t.iter().enumerate() {
        let before = i.checked_sub(1).map(|b| &t[b].tok);
        if x.tok.is_ident(name)
            && t.get(i + 1).is_some_and(|n| n.tok.is_punct('('))
            && !before.is_some_and(|b| b.is_punct('.') || b.is_ident("fn"))
        {
            found.push(x.line);
        }
    }
    found
}

fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            RAW,
            raw_data_deleters,
            &[
                "self.inner.delete(&pack.id.bucket_key())?;",
                "for id in ids { drive.delete(&id.bucket_key())?; }",
                "bucket.remove(&object.bucket_object().bucket_key())?;",
                "let keys: Vec<String> = ids.iter().map(ObjectId::bucket_key).collect(); drive.delete_many(&items);",
            ],
            &[
                "let key = id.bucket_key();",
                "self.inner.delete(&name)?;",
                "map.remove(&id);",
                "let key = pack.id.bucket_key(); hot.reads.remove(&pack.id);",
                "let entry = index.bucket_object(); entries.remove(&path);",
            ],
        ),
        Rule::new(
            UNASKED,
            unasked_calls,
            &[
                "delete_reclaimable(&self.inner, ReclaimMode::Delete, proof)",
                "remove_reclaimable(bucket, mode, proof)?;",
                "match delete_reclaimable(&self.inner, mode, proof) { _ => {} }",
            ],
            &[
                "delete_reclaimable(&self.inner, &gate, proof)",
                "remove_reclaimable(repo.store().bucket(), gate, proof)?;",
                "let delete_reclaimable = 1;",
            ],
        ),
        Rule::new(
            NO_GATE,
            deleters_without_a_gate,
            &[
                "delete_reclaimable(&self.inner, &gate, proof)",
                "let proof = live.prove_unnamed(id)?; remove_reclaimable(bucket, &gate, proof)?;",
            ],
            &[
                "let (live, gate, _) = self.live_objects(now)?; delete_reclaimable(&self.inner, &gate, proof)",
                "let gate = ask_retired_delete(history, now); remove_reclaimable(bucket, &gate, proof)?;",
                "let gate = self.retired_delete_gate(now); delete_reclaimable(&self.inner, &gate, proof)",
                "let none = 1;",
            ],
        ),
        Rule::new(
            CALLER,
            proof_callers,
            &[
                "delete_reclaimable(&self.inner, &gate, proof)",
                "remove_reclaimable(bucket, &gate, proof)?;",
            ],
            &["let delete_reclaimable = 1;", "reclaimable(&gate)"],
        ),
        Rule::new(
            MADE,
            licences_made,
            &["DeleteLicence { _sealed: () }", "Ok(DeleteLicence { _sealed: () })"],
            &["let l: DeleteLicence = other;", "fn f(l: &DeleteLicence) {}"],
        ),
        Rule::new(
            ASKED,
            |t| named_calls(t, "ask_retired_delete"),
            &[
                "ask_retired_delete(History::None, now)",
                "let gate = ask_retired_delete(History::Kept { mode, standing }, now);",
                "crate::meta::reclaim::ask_retired_delete(history, now)",
            ],
            &["let ask_retired_delete = 1;", "fn ask_retired_delete() {}"],
        ),
        Rule::new(
            CONFIRMED,
            |t| calls(t, "DeleteLicence::confirmed"),
            &[
                "Ok(DeleteLicence::confirmed(confirmation))",
                "let l = DeleteLicence::confirmed(&c);",
            ],
            &["let confirmed = 1;", "fn confirmed(c: &PurgeConfirmation) {}"],
        ),
    ]
}

/// The rule of the evidence's table, read in the storage crate only: AzDrive has its own `go` (a
/// window's navigation) that is no part of this.
fn go_rules() -> Vec<Rule> {
    vec![Rule::new(
        GO,
        |t| named_calls(t, "go"),
        &[
            "match go(mode, &evidence) { _ => {} }",
            "let decided = crate::meta::reclaim::go(mode, &e);",
        ],
        &[
            "let go = 1;",
            "fn go(mode: ReclaimMode) {}",
            "let x = ago(mode);",
            "board.go();",
        ],
    )]
}

/// Where each of the rules may be, and how often.
const ALLOWED: &[Allow] = &[
    Allow::exactly(
        "examples/azul-storage/src/meta/reclaim.rs",
        RAW,
        4,
        "the four deleters of a data object: drop_new_object (the compensation of a change that was refused, on a Refusal), delete_reclaimable and remove_reclaimable (on a proof AND the answer of ask_retired_delete), delete_released (the objects an index that keeps no history released)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/meta/reclaim.rs",
        CALLER,
        1,
        "delete_under: the reclaimer's own delete run, in the batches of its fence",
    ),
    Allow::exactly(
        "examples/azul-storage/src/packing.rs",
        CALLER,
        3,
        "the pack upkeep: reencrypt_packs (an index without history only), collect_retired and recover_packs",
    ),
    Allow::exactly(
        "examples/azul-storage/src/meta/reclaim.rs",
        MADE,
        3,
        "ask_retired_delete (from the evidence), of_an_index_without_history and confirmed (a purge the user confirmed)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/meta/index.rs",
        ASKED,
        2,
        "the reclaimer's own round, which asks it with the standing it has just made: the licence it hands the run, and the reason a round that stopped before it judged anything says",
    ),
    Allow::exactly(
        "examples/azul-storage/src/encrypted.rs",
        ASKED,
        1,
        "EncryptedDrive::retired_delete_gate: the pack upkeep of a drive that keeps history asks with the standing its provider holds",
    ),
    Allow::exactly(
        "examples/azul-storage/src/packing.rs",
        ASKED,
        1,
        "live_objects: the upkeep of a drive whose index keeps no history (History::None)",
    ),
    Allow::exactly(
        "examples/azul-storage/src/meta/index.rs",
        CONFIRMED,
        1,
        "MetaIndexProvider::purge: the user confirmed the erasure of the old versions",
    ),
];

/// The table of the evidence is asked in one place of the storage crate.
const GO_ALLOWED: &[Allow] = &[Allow::exactly(
    "examples/azul-storage/src/meta/reclaim.rs",
    GO,
    1,
    "ask_retired_delete: the one place the table of the evidence is asked",
)];

const STORAGE_ONLY: &[Root] = &[Root {
    dir: "examples/azul-storage/src",
    min_files: 80,
}];

#[test]
fn the_table_of_the_evidence_is_asked_in_one_place_and_that_is_the_one_gate() {
    let gate = Gate {
        name: "retired_delete_gate (go)",
        roots: STORAGE_ONLY,
        rules: go_rules(),
        budget: Budget(GO_ALLOWED),
        orphans: Orphans::Fail,
    };
    gate.run();
}

#[test]
fn every_deleter_of_data_the_history_may_still_name_asks_the_one_gate() {
    let gate = Gate {
        name: "retired_delete_gate",
        roots: ROOTS,
        rules: rules(),
        budget: Budget(ALLOWED),
        orphans: Orphans::Fail,
    };
    gate.run();
}

/// A planted deleter that does not ask is found by the rules - one function the old code had, a
/// `collect_retired` that took the configured mode - and the one that asks is not.
#[test]
fn a_deleter_that_does_not_ask_is_found_and_one_that_does_is_not() {
    let hits = |source: &str, rule: &str| -> usize {
        let blanked = azul_srcgate::code(source);
        rules()
            .into_iter()
            .find(|r| r.name == rule)
            .expect("the rule")
            .find(&blanked)
            .len()
    };
    // The upkeep of the first fix: the configured mode goes to the deleter.
    let old = "fn collect_retired(&self, now: u64) { let mode = self.reclaim; \
               match delete_reclaimable(&self.inner, mode, proof) { _ => {} } }";
    assert_eq!(hits(old, UNASKED), 1, "the call carries no gate");
    assert_eq!(hits(old, NO_GATE), 1, "and the function got none");
    // A new deleter that goes round the two functions.
    let raw = "fn sweep(&self, pack: &PackRef) { self.inner.delete(&pack.id.bucket_key()); }";
    assert_eq!(hits(raw, RAW), 1);
    // The upkeep as it is.
    let asked =
        "fn collect_retired(&self, now: u64) { let (live, gate, _) = self.live_objects(now)?; \
                 match delete_reclaimable(&self.inner, &gate, proof) { _ => {} } }";
    assert_eq!(hits(asked, UNASKED), 0);
    assert_eq!(hits(asked, NO_GATE), 0);
    assert_eq!(hits(asked, RAW), 0);
    // A licence made by hand is found.
    assert_eq!(
        hits(
            "fn f() -> DeleteLicence { DeleteLicence { _sealed: () } }",
            MADE
        ),
        1
    );
    // A test's own is not read.
    let test_only = "#[cfg(test)]\nfn lab() -> DeleteLicence { DeleteLicence { _sealed: () } }\n";
    assert_eq!(hits(test_only, MADE), 0);
}
