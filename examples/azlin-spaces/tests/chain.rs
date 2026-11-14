//! Check 1 of the brief (the chain): a space's membership chain is `azlin-chain`'s one set of link rules
//! (position, `prev`, stamp, restore epoch, fork), the first entry is a `space_create` that commits the
//! first epoch's key id, a second create is refused, the root kind exists only at account creation; and
//! the signature check, the limits and the witnesses around the chain.
//!
//! Implements planning/architecture.md section 5.9 ("two different entries at the same position are a
//! fork, refused and alarmed"; "an entry that fails its hash or signature check is fetched again from
//! another voter") and section 5.5 (a `space_create` entry commits the first epoch's key id).

mod common;

use azlin_chain::{Alarm, ChainId, ChainRefusal, RefusalClass};
use azlin_keys::{CommitKind, DirectoryError};
use azlin_seams::{AccountKind, FakeScheme, Hash32, RestoreEpoch};
use azlin_spaces::{
    admit, apply, epoch_authorized, verify_entry, Admitted, HistoryMode, SpaceDelta, SpaceKind,
    SpaceNotice, SpaceProposal, SpaceRefusal, SpaceRole, WrapNeed, COSIGS_MAX, PREIMAGE_MAX,
    SIG_MAX_LEN,
};
use common::*;

fn created() -> World {
    World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full)
}

#[test]
fn space_chain_uses_azlin_chain_for_prev_seq_and_forks() {
    let mut w = created();
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    let head = w.state.chain_head().expect("the chain has a head");
    assert_eq!(head.seq, seq(1));

    // A wrong `prev`.
    let wrong_prev = signed_entry(
        space(1),
        seq(2),
        hash_of_fill(9),
        device(1),
        grant_op(m(8), SpaceRole::Viewer),
        &[],
        RestoreEpoch::ZERO,
    );
    match refusal_of(w.admit_entry(&wrong_prev)) {
        SpaceRefusal::Chain(ChainRefusal::WrongPrev { expected, got }) => {
            assert_eq!(expected, head.hash);
            assert_eq!(got, hash_of_fill(9));
        }
        other => panic!("a wrong prev: {other:?}"),
    }

    // A gap: entries are missing before it.
    let gap = signed_entry(
        space(1),
        seq(5),
        head.hash,
        device(1),
        grant_op(m(8), SpaceRole::Viewer),
        &[],
        RestoreEpoch::ZERO,
    );
    match refusal_of(w.admit_entry(&gap)) {
        SpaceRefusal::Chain(ChainRefusal::Gap { expected, got }) => {
            assert_eq!(expected, seq(2));
            assert_eq!(got, seq(5));
        }
        other => panic!("a gap: {other:?}"),
    }

    // A fork: two different entries for the position 2, built against the same head.
    let first = w.build(device(1), grant_op(m(8), SpaceRole::Viewer), &[]);
    let second = w.build(device(1), grant_op(m(9), SpaceRole::Viewer), &[]);
    assert_ne!(first.hash, second.hash);
    let admitted = w.admit_entry(&first).expect("the first entry is admitted");
    apply(&mut w.state, admitted);
    w.clock += 10;
    match refusal_of(w.admit_entry(&second)) {
        SpaceRefusal::Chain(why @ ChainRefusal::Fork { at, have, got }) => {
            assert_eq!(at, seq(2));
            assert_eq!(have, first.hash);
            assert_eq!(got, second.hash);
            // A fork is an alarm, never a plain refusal.
            assert_eq!(why.class(), RefusalClass::Alarm);
            assert!(matches!(
                why.alarm(ChainId::space(space(1))),
                Some(Alarm::Fork { .. })
            ));
        }
        other => panic!("a fork: {other:?}"),
    }

    // The same entry again is not a fork: it does not extend the chain.
    match refusal_of(w.admit_entry(&first)) {
        SpaceRefusal::Chain(ChainRefusal::Gap { expected, got }) => {
            assert_eq!(expected, seq(3));
            assert_eq!(got, seq(2));
        }
        other => panic!("the same entry again: {other:?}"),
    }
}

#[test]
fn the_leaders_stamp_never_runs_backwards_along_the_chain() {
    let mut w = created();
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    let entry = w.build(device(1), grant_op(m(8), SpaceRole::Viewer), &[]);
    let proposal = w.verify(&entry).expect("the signatures hold");
    let who = w.facts(device(1));
    match refusal_of(admit(&w.state, &proposal, &who, &w.account, at(5))) {
        SpaceRefusal::Chain(ChainRefusal::StampBackwards) => {}
        other => panic!("a stamp before the last one: {other:?}"),
    }
    // The same stamp as the last entry is fine.
    let last = w.clock;
    assert!(admit(&w.state, &proposal, &who, &w.account, at(last)).is_ok());
}

#[test]
fn an_entry_of_the_lost_timeline_of_a_restore_is_refused_and_alarmed() {
    let mut w = created();
    // The group restored once: every entry it writes now names the restore epoch 1.
    w.restore = RestoreEpoch::new(1);
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    // An entry of the old timeline (restore epoch 0) is refused, however validly signed.
    let old = w.build(device(1), grant_op(m(8), SpaceRole::Viewer), &[]);
    let old = signed_entry(
        space(1),
        old.seq,
        old.prev,
        device(1),
        grant_op(m(8), SpaceRole::Viewer),
        &[],
        RestoreEpoch::ZERO,
    );
    match refusal_of(w.admit_entry(&old)) {
        SpaceRefusal::Chain(why @ ChainRefusal::BelowRestoreEpoch { got, floor }) => {
            assert_eq!(got, RestoreEpoch::ZERO);
            assert_eq!(floor, RestoreEpoch::new(1));
            assert!(matches!(
                why.alarm(ChainId::space(space(1))),
                Some(Alarm::BelowRestoreEpoch { .. })
            ));
        }
        other => panic!("an entry of the lost timeline: {other:?}"),
    }
}

#[test]
fn space_create_commits_the_first_epoch_key_id() {
    let mut w = World::new(AccountKind::Team);
    assert!(!w.state.is_created());
    assert!(!epoch_authorized(
        &w.state,
        azlin_seams::Epoch::FIRST,
        &key(1)
    ));
    let effects = w.do_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 1, 1));
    assert!(w.state.is_created());
    assert!(epoch_authorized(
        &w.state,
        azlin_seams::Epoch::FIRST,
        &key(1)
    ));
    let record = w
        .state
        .epoch_record(azlin_seams::Epoch::FIRST)
        .expect("the first epoch is committed");
    assert_eq!(record.key_id, key(1));
    assert_eq!(record.seq, seq(0));
    assert_eq!(record.kind, CommitKind::Create);
    assert_eq!(record.signer, device(1));
    assert!(!record.chained);
    assert_eq!(w.state.key_epoch(), Some(azlin_seams::Epoch::FIRST));
    assert_eq!(w.state.kind(), Some(SpaceKind::Shared));
    assert_eq!(w.state.created_by(), Some(member(1)));
    // The creator's member is the first manager, and the first manager needs the first wrap.
    assert_eq!(
        effects.wraps,
        vec![WrapNeed {
            to: m(1),
            epoch: azlin_seams::Epoch::FIRST
        }]
    );
    assert!(effects.notices.contains(&SpaceNotice::KeyEpoch {
        space: space(1),
        epoch: azlin_seams::Epoch::FIRST
    }));
    match effects.rows.first() {
        Some(SpaceDelta::Space(row)) => {
            assert_eq!(row.kind, SpaceKind::Shared);
            assert_eq!(row.key_epoch, azlin_seams::Epoch::FIRST);
            assert_eq!(row.chain_head.seq, seq(0));
        }
        other => panic!("the first row is the reg_space row: {other:?}"),
    }
    assert!(effects.obligations.is_empty());
}

#[test]
fn second_create_refused() {
    let w = created();
    let genesis = w
        .state
        .chain()
        .entries()
        .first()
        .expect("the genesis")
        .hash();

    // At the next position: a create is nothing but the first entry.
    let again = w.build(
        device(1),
        create_op(SpaceKind::Shared, HistoryMode::Full, 1, 2),
        &[],
    );
    match refusal_of(w.admit_entry(&again)) {
        SpaceRefusal::Chain(ChainRefusal::GenesisAgain { have, got }) => {
            assert_eq!(have, genesis);
            assert_eq!(got, again.hash);
        }
        other => panic!("a second create at the next position: {other:?}"),
    }

    // At position 0 with another hash: a fork of the genesis, alarmed.
    let other_genesis = signed_entry(
        space(1),
        seq(0),
        Hash32::ZERO,
        device(1),
        create_op(SpaceKind::Team, HistoryMode::Forward, 1, 2),
        &[],
        RestoreEpoch::ZERO,
    );
    match refusal_of(w.admit_entry(&other_genesis)) {
        SpaceRefusal::Chain(why @ ChainRefusal::GenesisAgain { .. }) => {
            assert!(matches!(
                why.alarm(ChainId::space(space(1))),
                Some(Alarm::Fork { .. })
            ));
        }
        other => panic!("a second genesis: {other:?}"),
    }
    // Nothing changed: the first epoch is still the only one.
    assert_eq!(w.state.epochs().count(), 1);
    assert!(epoch_authorized(
        &w.state,
        azlin_seams::Epoch::FIRST,
        &key(1)
    ));
    assert!(!epoch_authorized(
        &w.state,
        azlin_seams::Epoch::FIRST,
        &key(2)
    ));
}

#[test]
fn root_kind_only_at_account_creation() {
    // At account creation (the account has no root space yet) the owner makes it.
    let mut w = World::new(AccountKind::Team);
    let effects = w.do_by(1, create_op(SpaceKind::Root, HistoryMode::Full, 1, 1));
    assert!(effects.applied);
    assert_eq!(w.state.kind(), Some(SpaceKind::Root));

    // Later, when the account has its root space, no second one.
    let mut w = World::new(AccountKind::Team);
    w.account.root_space = true;
    assert_eq!(
        w.refused_by(1, create_op(SpaceKind::Root, HistoryMode::Full, 1, 1)),
        SpaceRefusal::RootExists
    );

    // And only an owner makes it: an admin does not.
    let mut w = World::new(AccountKind::Team);
    assert_eq!(
        w.refused_by(2, create_op(SpaceKind::Root, HistoryMode::Full, 2, 1)),
        SpaceRefusal::CreateNotAllowed {
            kind: SpaceKind::Root
        }
    );
}

#[test]
fn a_non_create_first_entry_is_refused() {
    let mut w = World::new(AccountKind::Team);
    for op in [
        grant_op(m(7), SpaceRole::Viewer),
        revoke_op(m(7)),
        rekey_op(2, 2),
        history_op(HistoryMode::Forward),
    ] {
        match w.refused_by(1, op.clone()) {
            SpaceRefusal::Chain(ChainRefusal::NotGenesis) => {}
            other => panic!("{op:?} as the first entry: {other:?}"),
        }
    }
}

#[test]
fn an_entry_of_another_space_is_refused() {
    let w = created();
    let foreign = signed_entry(
        space(2),
        seq(1),
        w.state.chain_head().expect("a head").hash,
        device(1),
        grant_op(m(7), SpaceRole::Viewer),
        &[],
        RestoreEpoch::ZERO,
    );
    assert_eq!(
        refusal_of(w.admit_entry(&foreign)),
        SpaceRefusal::Chain(ChainRefusal::WrongChain)
    );
}

#[test]
fn a_copy_whose_hash_is_not_its_preimages_is_corrupt() {
    let w = created();
    let mut entry = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &[]);
    assert!(entry.is_intact());
    entry.hash = hash_of_fill(3);
    assert!(!entry.is_intact());
    let why = refusal_of(w.verify(&entry));
    assert_eq!(why, SpaceRefusal::Chain(ChainRefusal::Corrupt));
    // A corrupt copy is fetched again from another voter: never read as a fork.
    assert!(matches!(
        why,
        SpaceRefusal::Chain(ref refusal) if refusal.class() == RefusalClass::Refetch
    ));
}

#[test]
fn a_bad_unknown_or_unusable_signature_is_refused() {
    let w = created();
    let entry = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &[]);

    // The signature of another preimage.
    let mut forged = entry.clone();
    forged.sig = FakeScheme::sign(&device_key(1), b"another preimage");
    assert_eq!(refusal_of(w.verify(&forged)), SpaceRefusal::BadSignature);

    // The right bytes signed with another device's key.
    let mut stolen = entry.clone();
    stolen.sig = FakeScheme::sign(&device_key(2), &entry.preimage);
    assert_eq!(refusal_of(w.verify(&stolen)), SpaceRefusal::BadSignature);

    // A signature of the wrong length.
    let mut short = entry.clone();
    short.sig.truncate(5);
    assert_eq!(refusal_of(w.verify(&short)), SpaceRefusal::BadSignature);

    // A device the directory does not know, one that is revoked or not yet active, one it cannot read.
    let mut unknown = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &[]);
    unknown.signer = device(99);
    assert_eq!(refusal_of(w.verify(&unknown)), SpaceRefusal::SignerUnknown);
    for (error, expected) in [
        (DirectoryError::Revoked, SpaceRefusal::SignerNotActive),
        (DirectoryError::NotYetActive, SpaceRefusal::SignerNotActive),
        (DirectoryError::Unavailable, SpaceRefusal::KeyUnavailable),
        (DirectoryError::Unknown, SpaceRefusal::SignerUnknown),
    ] {
        let mut w = created();
        w.dir.errors.insert(device(1), error);
        let entry = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &[]);
        assert_eq!(refusal_of(w.verify(&entry)), expected, "{error:?}");
    }
}

#[test]
fn every_part_of_an_entry_has_a_limit() {
    let w = created();
    let entry = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &[]);

    let mut big_preimage = entry.clone();
    big_preimage
        .preimage
        .resize(PREIMAGE_MAX.get() as usize + 1, 0);
    assert!(matches!(
        refusal_of(w.verify(&big_preimage)),
        SpaceRefusal::TooBig(_)
    ));

    let mut big_sig = entry.clone();
    big_sig.sig = vec![0; SIG_MAX_LEN + 1];
    assert!(matches!(
        refusal_of(w.verify(&big_sig)),
        SpaceRefusal::TooBig(_)
    ));

    let cosigners: Vec<_> = (0..=COSIGS_MAX).map(|_| device(2)).collect();
    let many = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &cosigners);
    assert!(matches!(
        refusal_of(w.verify(&many)),
        SpaceRefusal::TooBig(_)
    ));

    // Exactly at the limit is fine.
    let at_limit: Vec<_> = (0..COSIGS_MAX).map(|_| device(2)).collect();
    let ok = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &at_limit);
    assert!(w.verify(&ok).is_ok());
}

#[test]
fn the_account_view_must_be_the_spaces_account() {
    let w = created();
    let entry = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &[]);
    let proposal = w.verify(&entry).expect("the signatures hold");
    let mut other = w.account.clone();
    other.id = account(9);
    let who = w.facts(device(1));
    assert_eq!(
        refusal_of(admit(&w.state, &proposal, &who, &other, w.now())),
        SpaceRefusal::WrongAccount
    );
}

#[test]
fn the_facts_must_be_about_the_signing_device() {
    let w = created();
    let entry = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &[]);
    let proposal = w.verify(&entry).expect("the signatures hold");
    // The facts of the owner's second device, offered for the first one.
    let mut other_device = w.facts(device(1));
    other_device.device = device(2);
    assert_eq!(
        refusal_of(admit(
            &w.state,
            &proposal,
            &other_device,
            &w.account,
            w.now()
        )),
        SpaceRefusal::FactsOfAnotherDevice
    );
}

#[test]
fn an_archived_space_admits_nothing_and_applies_nothing() {
    let mut w = created();
    let stale: Admitted = w
        .try_admit_as(device(1), grant_op(m(7), SpaceRole::Viewer), &[])
        .expect("admitted while the space is live");
    w.state.archive(at(5_000));
    assert_eq!(w.state.archived(), Some(at(5_000)));
    // Archiving again keeps the first stamp.
    w.state.archive(at(9_000));
    assert_eq!(w.state.archived(), Some(at(5_000)));
    for op in [
        grant_op(m(7), SpaceRole::Viewer),
        revoke_op(m(1)),
        rekey_op(2, 2),
        history_op(HistoryMode::Forward),
    ] {
        assert_eq!(w.refused_by(1, op), SpaceRefusal::Archived);
    }
    // An admission made before the archive is not applied to the archived space.
    let before = w.state.state_hash();
    let effects = apply(&mut w.state, stale);
    assert!(!effects.applied);
    assert!(effects.rows.is_empty() && effects.wraps.is_empty() && effects.notices.is_empty());
    assert_eq!(w.state.state_hash(), before);
}

#[test]
fn an_admission_made_against_another_state_is_not_applied() {
    let mut w = created();
    let stale = w
        .try_admit_as(device(1), grant_op(m(7), SpaceRole::Viewer), &[])
        .expect("admitted");
    // Another entry takes the position first.
    w.do_by(1, grant_op(m(8), SpaceRole::Viewer));
    let before = w.state.state_hash();
    let effects = apply(&mut w.state, stale);
    assert!(!effects.applied);
    assert!(effects.obligations.is_empty());
    assert_eq!(w.state.state_hash(), before, "the state is untouched");
    assert!(w.state.grant(&m(7)).is_none());
}

#[test]
fn the_first_epoch_is_committed_by_a_creator_who_is_no_manager() {
    // An admin creates a team space for a team lead: the admin signs the create and holds no grant.
    let mut w = World::new(AccountKind::Team);
    let effects = w.do_by(2, create_op(SpaceKind::Team, HistoryMode::Full, 7, 1));
    assert!(effects.applied);
    let record = w
        .state
        .epoch_record(azlin_seams::Epoch::FIRST)
        .expect("epoch 1");
    assert_eq!(record.signer, device(2));
    assert_eq!(
        effects.wraps,
        vec![WrapNeed {
            to: m(7),
            epoch: azlin_seams::Epoch::FIRST
        }]
    );
    // The creator is authorized at the position it signed, and holds no manage capability after it.
    assert_eq!(
        w.refused_by(2, grant_op(m(8), SpaceRole::Viewer)),
        SpaceRefusal::NotAManager {
            op: azlin_spaces::SpaceOpName::Grant
        }
    );
}

#[test]
fn proposals_and_admissions_are_witnesses() {
    // Neither can be cloned or made by `Default`: `admit` takes a proposal whose signatures were checked
    // and `apply` takes what `admit` made, so nothing is applied without both checks.
    <SpaceProposal as AmbiguousIfClone<_>>::check();
    <SpaceProposal as AmbiguousIfDefault<_>>::check();
    <Admitted as AmbiguousIfClone<_>>::check();
    <Admitted as AmbiguousIfDefault<_>>::check();
    // `verify_entry` is the one way to a proposal, and it needs a directory and a verifier.
    let w = created();
    let entry = w.build(device(1), grant_op(m(7), SpaceRole::Viewer), &[]);
    assert!(verify_entry(&entry, &w.dir, &FakeScheme, w.now()).is_ok());
}
