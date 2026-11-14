//! Check 9 of the brief (the access view): a revocation is immediate; the view equals a naive model over
//! 5,000 seeded random chains (the model is written here independently of the crate: a flat list of
//! grants and a fold); a replay of a chain is the state that was built incrementally (equal `state_hash`),
//! and a damaged chain does not replay.
//!
//! Implements planning/architecture.md section 2.4 ("the space grant backed by a signed entry"), section
//! 5.9 ("the chain authenticates itself, any copy can restore it") and section 12 E1 (every device runs
//! its own check).

mod common;

use azlin_seams::sim::Rng;
use azlin_seams::{AccountKind, FakeScheme};
use azlin_spaces::{
    access, apply, capability_hash, replay, verify_entry, Grantee, HistoryMode, Payer, Principal,
    ReplayRefusal, ReplayStep, SignerFacts, SpaceEntry, SpaceKind, SpaceOp, SpaceRefusal,
    SpaceRole,
};
use common::*;

fn created() -> World {
    World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full)
}

fn flags(caps: azlin_spaces::Caps) -> (bool, bool, bool) {
    (caps.can_read(), caps.can_write(), caps.can_manage())
}

/// The table of the brief, typed here as (read, write, manage).
fn table(role: SpaceRole) -> (bool, bool, bool) {
    match role {
        SpaceRole::Manager => (true, true, true),
        SpaceRole::Editor => (true, true, false),
        SpaceRole::Viewer => (true, false, false),
        SpaceRole::Uploader => (false, true, false),
    }
}

#[test]
fn revoke_is_immediate_in_the_access_view() {
    let mut w = created();
    let cases: Vec<(Grantee, Principal)> = vec![
        (
            m(5),
            Principal::Member {
                member: member(5),
                groups: vec![],
            },
        ),
        (
            g(1),
            Principal::Member {
                member: member(6),
                groups: vec![group(1)],
            },
        ),
        (
            x(1),
            Principal::External {
                account: account(2),
                member: ext_member(1),
            },
        ),
        (cap(1), Principal::Capability(secret(1))),
    ];
    for (grantee, principal) in &cases {
        assert!(
            access(&w.state, principal, at(5_000)).is_empty(),
            "{grantee:?} before"
        );
        w.do_by(1, grant_op(*grantee, SpaceRole::Editor));
        let caps = access(&w.state, principal, at(5_000));
        assert!(
            caps.can_read() && caps.can_write(),
            "{grantee:?} after the grant"
        );
        // The entry that revokes it is applied, and the very next question is answered without it.
        w.do_by(1, revoke_op(*grantee));
        assert!(
            access(&w.state, principal, at(5_000)).is_empty(),
            "{grantee:?} after the revoke"
        );
    }
}

#[test]
fn an_archived_space_is_read_only_in_the_access_view() {
    let mut w = created();
    w.do_by(1, grant_op(m(5), SpaceRole::Editor));
    w.do_by(1, grant_op(m(6), SpaceRole::Uploader));
    let manager = Principal::Member {
        member: member(1),
        groups: vec![],
    };
    let editor = Principal::Member {
        member: member(5),
        groups: vec![],
    };
    let uploader = Principal::Member {
        member: member(6),
        groups: vec![],
    };
    w.state.archive(at(9_000));
    // Before the archive time nothing changed.
    assert_eq!(
        flags(access(&w.state, &manager, at(8_999))),
        (true, true, true)
    );
    // From it on, reading only: the manager manages nothing, the editor writes nothing, an uploader (which
    // only writes) has nothing left.
    assert_eq!(
        flags(access(&w.state, &manager, at(9_000))),
        (true, false, false)
    );
    assert_eq!(
        flags(access(&w.state, &editor, at(9_001))),
        (true, false, false)
    );
    assert_eq!(
        flags(access(&w.state, &uploader, at(9_001))),
        (false, false, false)
    );
}

// ---------------------------------------------------------------------------------------------
// The naive model
// ---------------------------------------------------------------------------------------------

/// Who asks, as a plain description the model reads.
#[derive(Clone, Copy, Debug)]
enum Who {
    /// A member of the account with the groups it is in.
    Member(u8, &'static [u8]),
    /// A member of the account number `.1` (2 is the account the externals come from).
    External(u8, u8),
    /// The holder of the secret `n`.
    Holder(u8),
}

fn principal_of(who: Who) -> Principal {
    match who {
        Who::Member(n, groups) => Principal::Member {
            member: member(n),
            groups: groups.iter().map(|id| group(*id)).collect(),
        },
        Who::External(n, from) => Principal::External {
            account: account(from),
            member: ext_member(n),
        },
        Who::Holder(n) => Principal::Capability(secret(n)),
    }
}

/// What the flat list of grants gives `who`: every grant in force that names it, capability by capability.
fn naive_caps(rows: &[(Grantee, SpaceRole, bool)], who: Who) -> (bool, bool, bool) {
    let mut caps = (false, false, false);
    for (grantee, role, active) in rows {
        if !*active {
            continue;
        }
        let applies = match (who, grantee) {
            (Who::Member(n, _), Grantee::Member(other)) => *other == member(n),
            (Who::Member(_, groups), Grantee::Group(other)) => {
                groups.iter().any(|id| group(*id) == *other)
            }
            (
                Who::External(n, from),
                Grantee::External {
                    account: other_account,
                    member: other_member,
                },
            ) => *other_account == account(from) && *other_member == ext_member(n),
            (Who::Holder(n), Grantee::Capability(hash)) => *hash == capability_hash(&secret(n)),
            _ => false,
        };
        if applies {
            let (read, write, manage) = table(*role);
            caps = (caps.0 || read, caps.1 || write, caps.2 || manage);
        }
    }
    caps
}

const GROUP_SETS: [&[u8]; 5] = [&[], &[1], &[2], &[1, 2, 3], &[3]];

fn principals() -> Vec<Who> {
    let mut all = Vec::new();
    for n in 1..=6u8 {
        for groups in GROUP_SETS {
            all.push(Who::Member(n, groups));
        }
    }
    all.push(Who::Member(7, &[1, 2, 3]));
    all.push(Who::Member(8, &[]));
    for n in 1..=3u8 {
        all.push(Who::External(n, 2));
    }
    // The member 1 of ANOTHER account than the one the grants name.
    all.push(Who::External(1, 3));
    for n in [1u8, 2, 3, 9] {
        all.push(Who::Holder(n));
    }
    all
}

/// One seeded chain of grants and revokes by the space's one standing manager, checked against the model.
fn run_chain(seed: u64) {
    let mut rng = Rng::new(seed);
    let mut w = created();
    let mut rows: Vec<(Grantee, SpaceRole, bool)> = vec![(m(1), SpaceRole::Manager, true)];
    // The grantees the chain plays with: the member 1 (the standing manager) is not among them.
    let grantees = [
        m(2),
        m(3),
        m(4),
        m(5),
        m(6),
        g(1),
        g(2),
        g(3),
        x(1),
        x(2),
        cap(1),
        cap(2),
    ];
    let roles = [
        SpaceRole::Manager,
        SpaceRole::Editor,
        SpaceRole::Viewer,
        SpaceRole::Uploader,
    ];
    let capability_roles = [SpaceRole::Editor, SpaceRole::Viewer, SpaceRole::Uploader];
    let steps = rng.range(4, 21).expect("a non-empty range");
    for step in 0..steps {
        let target = *rng.pick(&grantees).expect("a grantee");
        if rng.chance(7, 10) {
            let role = if matches!(target, Grantee::Capability(_)) {
                *rng.pick(&capability_roles).expect("a role")
            } else {
                *rng.pick(&roles).expect("a role")
            };
            w.submit_by(1, grant_op(target, role))
                .unwrap_or_else(|why| {
                    panic!("seed {seed} step {step}: {target:?} {role:?}: {why}")
                });
            match rows.iter_mut().find(|row| row.0 == target) {
                Some(row) => {
                    row.1 = role;
                    row.2 = true;
                }
                None => rows.push((target, role, true)),
            }
        } else {
            let in_force = rows.iter().any(|row| row.0 == target && row.2);
            let result = w.submit_by(1, revoke_op(target));
            if in_force {
                result.unwrap_or_else(|why| {
                    panic!("seed {seed} step {step}: revoke {target:?}: {why}")
                });
                if let Some(row) = rows.iter_mut().find(|row| row.0 == target) {
                    row.2 = false;
                }
            } else {
                assert_eq!(
                    result.expect_err("nothing to revoke"),
                    SpaceRefusal::NoSuchGrant,
                    "seed {seed} step {step}"
                );
            }
        }
        if step == steps / 2 {
            compare(&w, &rows, seed);
        }
    }
    compare(&w, &rows, seed);
}

fn compare(w: &World, rows: &[(Grantee, SpaceRole, bool)], seed: u64) {
    for who in principals() {
        let got = flags(access(&w.state, &principal_of(who), at(5_000)));
        assert_eq!(got, naive_caps(rows, who), "seed {seed}: {who:?}");
    }
}

#[test]
fn access_view_equals_a_naive_model_over_5000_seeded_random_chains() {
    for chain in 0..5_000u64 {
        run_chain(chain.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1));
    }
}

// ---------------------------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------------------------

/// One step of a recorded chain: the entry, the facts of its signer and the leader's stamp.
struct Recorded {
    entry: SpaceEntry,
    facts: SignerFacts,
    now: azlin_seams::Stamp,
}

/// The next entry by member `n`, applied, and recorded.
fn step(w: &mut World, recorded: &mut Vec<Recorded>, n: u8, op: SpaceOp) {
    let entry = w.build(device(n), op, &[]);
    let facts = w.facts(device(n));
    let now = w.now();
    let admitted = w
        .admit_entry(&entry)
        .unwrap_or_else(|why| panic!("member {n}: {why}"));
    apply(&mut w.state, admitted);
    w.clock += 10;
    recorded.push(Recorded { entry, facts, now });
}

/// A chain with every op, several grantees of every kind, a rotation in each mode and a switch.
fn recorded_chain() -> (World, Vec<Recorded>) {
    let mut w = World::new(AccountKind::Team);
    let mut r = Vec::new();
    step(
        &mut w,
        &mut r,
        1,
        create_op(SpaceKind::Shared, HistoryMode::Full, 1, 1),
    );
    step(&mut w, &mut r, 1, grant_op(m(2), SpaceRole::Manager));
    step(&mut w, &mut r, 1, grant_op(m(5), SpaceRole::Viewer));
    step(&mut w, &mut r, 2, grant_op(m(6), SpaceRole::Editor));
    step(&mut w, &mut r, 1, grant_op(g(1), SpaceRole::Uploader));
    step(&mut w, &mut r, 1, grant_op(x(1), SpaceRole::Viewer));
    step(
        &mut w,
        &mut r,
        1,
        grant_full(cap(1), SpaceRole::Editor, Some(4_096), Payer::Owner),
    );
    step(&mut w, &mut r, 2, rekey_op(2, 2));
    step(&mut w, &mut r, 1, revoke_op(m(5)));
    step(&mut w, &mut r, 1, history_op(HistoryMode::Forward));
    step(&mut w, &mut r, 2, grant_op(m(7), SpaceRole::Viewer));
    step(&mut w, &mut r, 1, rekey_op(3, 3));
    step(
        &mut w,
        &mut r,
        1,
        grant_full(x(2), SpaceRole::Editor, Some(100), Payer::Grantee),
    );
    step(&mut w, &mut r, 2, revoke_op(cap(1)));
    step(&mut w, &mut r, 1, history_op(HistoryMode::Full));
    step(&mut w, &mut r, 1, grant_op(m(5), SpaceRole::Uploader));
    step(&mut w, &mut r, 1, rekey_op(4, 4));
    step(&mut w, &mut r, 2, revoke_op(m(1)));
    step(&mut w, &mut r, 2, grant_op(m(1), SpaceRole::Viewer));
    step(&mut w, &mut r, 2, rekey_op(5, 5));
    (w, r)
}

fn steps_of(w: &World, recorded: &[Recorded]) -> Vec<ReplayStep> {
    recorded
        .iter()
        .map(|record| ReplayStep {
            proposal: verify_entry(&record.entry, &w.dir, &FakeScheme, record.now)
                .expect("the signatures hold"),
            who: record.facts,
            now: record.now,
        })
        .collect()
}

#[test]
fn replay_equals_incremental_state_hash() {
    let (w, recorded) = recorded_chain();
    assert_eq!(recorded.len(), 20);
    let rebuilt = replay(space(1), account(1), steps_of(&w, &recorded), &w.account)
        .expect("the chain replays");
    assert_eq!(rebuilt.state_hash(), w.state.state_hash());
    // Not only the hash: the state a device rebuilds is the state the node holds.
    assert_eq!(rebuilt.key_epoch(), w.state.key_epoch());
    assert_eq!(rebuilt.history(), w.state.history());
    assert_eq!(rebuilt.rekey_due(), w.state.rekey_due());
    assert_eq!(rebuilt.chain_head(), w.state.chain_head());
    assert_eq!(
        rebuilt.grants().cloned().collect::<Vec<_>>(),
        w.state.grants().cloned().collect::<Vec<_>>()
    );
    assert_eq!(
        rebuilt.epochs().map(|(e, r)| (e, *r)).collect::<Vec<_>>(),
        w.state.epochs().map(|(e, r)| (e, *r)).collect::<Vec<_>>()
    );
    // The same access view for every principal.
    for who in principals() {
        assert_eq!(
            flags(access(&rebuilt, &principal_of(who), at(5_000))),
            flags(access(&w.state, &principal_of(who), at(5_000))),
            "{who:?}"
        );
    }
    // A prefix of the chain is another state.
    let prefix = replay(
        space(1),
        account(1),
        steps_of(&w, &recorded).into_iter().take(10).collect(),
        &w.account,
    )
    .expect("a prefix replays");
    assert_ne!(prefix.state_hash(), w.state.state_hash());
    // Two chains with the same effect but another signer are other states: the hash covers the entries.
    let mut a = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    let mut b = World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full);
    a.do_by(1, grant_op(m(2), SpaceRole::Manager));
    b.do_by(1, grant_op(m(2), SpaceRole::Manager));
    a.do_by(1, grant_op(m(5), SpaceRole::Viewer));
    b.do_by(2, grant_op(m(5), SpaceRole::Viewer));
    assert_ne!(a.state.state_hash(), b.state.state_hash());
}

#[test]
fn a_damaged_chain_does_not_replay() {
    let (w, recorded) = recorded_chain();

    // Two entries swapped: the first of them is not the next one.
    let mut swapped = steps_of(&w, &recorded);
    swapped.swap(4, 5);
    match replay(space(1), account(1), swapped, &w.account) {
        Err(ReplayRefusal::Refused { at, why }) => {
            assert_eq!(at, seq(5));
            assert!(matches!(why, SpaceRefusal::Chain(_)), "{why:?}");
        }
        other => panic!("a swapped chain: {other:?}"),
    }

    // An entry missing: a gap.
    let mut missing = steps_of(&w, &recorded);
    missing.remove(3);
    match replay(space(1), account(1), missing, &w.account) {
        Err(ReplayRefusal::Refused { at, why }) => {
            assert_eq!(at, seq(4));
            assert!(matches!(
                why,
                SpaceRefusal::Chain(azlin_chain::ChainRefusal::Gap { .. })
            ));
        }
        other => panic!("a chain with an entry missing: {other:?}"),
    }

    // An entry that was never allowed: the member 5 (a viewer) signs a grant at the right position.
    let mut forged = World::new(AccountKind::Team);
    let mut r = Vec::new();
    step(
        &mut forged,
        &mut r,
        1,
        create_op(SpaceKind::Shared, HistoryMode::Full, 1, 1),
    );
    step(&mut forged, &mut r, 1, grant_op(m(5), SpaceRole::Viewer));
    let entry = forged.build(device(5), grant_op(m(6), SpaceRole::Viewer), &[]);
    let mut steps = steps_of(&forged, &r);
    steps.push(ReplayStep {
        proposal: verify_entry(&entry, &forged.dir, &FakeScheme, forged.now()).expect("signed"),
        who: forged.facts(device(5)),
        now: forged.now(),
    });
    match replay(space(1), account(1), steps, &forged.account) {
        Err(ReplayRefusal::Refused { at, why }) => {
            assert_eq!(at, seq(2));
            assert!(matches!(why, SpaceRefusal::NotAManager { .. }), "{why:?}");
        }
        other => panic!("a chain with a forged entry: {other:?}"),
    }

    // A chain that does not start with its create.
    let tail: Vec<ReplayStep> = steps_of(&w, &recorded).into_iter().skip(1).collect();
    assert!(matches!(
        replay(space(1), account(1), tail, &w.account),
        Err(ReplayRefusal::Refused { .. })
    ));
}

#[test]
fn the_state_hash_covers_every_part_of_the_state() {
    let base = |tweak: &dyn Fn(&mut World)| {
        let mut w = created();
        w.do_by(1, grant_op(m(5), SpaceRole::Viewer));
        tweak(&mut w);
        w.state.state_hash()
    };
    let plain = base(&|_| {});
    assert_eq!(plain, base(&|_| {}), "the hash is deterministic");
    let variants: Vec<(&str, Hash)> = vec![
        (
            "one more entry",
            base(&|w| {
                w.do_by(1, grant_op(m(6), SpaceRole::Viewer));
            }),
        ),
        (
            "a rekey",
            base(&|w| {
                w.do_by(1, rekey_op(2, 2));
            }),
        ),
        (
            "a history switch",
            base(&|w| {
                w.do_by(1, history_op(HistoryMode::Forward));
            }),
        ),
        (
            "a revoke",
            base(&|w| {
                w.do_by(1, revoke_op(m(5)));
            }),
        ),
        ("an archive", base(&|w| w.state.archive(at(7)))),
        ("another role", {
            let mut w = created();
            w.do_by(1, grant_op(m(5), SpaceRole::Editor));
            w.state.state_hash()
        }),
        ("a cap", {
            let mut w = created();
            w.do_by(
                1,
                grant_full(m(5), SpaceRole::Editor, Some(10), Payer::Owner),
            );
            w.state.state_hash()
        }),
        ("another payer", {
            let mut w = created();
            w.do_by(1, grant_full(m(5), SpaceRole::Editor, None, Payer::Grantee));
            w.state.state_hash()
        }),
        ("another grantee", {
            let mut w = created();
            w.do_by(1, grant_op(m(6), SpaceRole::Viewer));
            w.state.state_hash()
        }),
    ];
    let mut seen = vec![plain];
    for (what, hash) in variants {
        assert!(!seen.contains(&hash), "{what} gives a hash that was seen");
        seen.push(hash);
    }
}

type Hash = azlin_seams::Hash32;
