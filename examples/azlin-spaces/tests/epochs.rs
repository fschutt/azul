//! Check 6 of the brief (epochs): an epoch counts only if a signed `space_rekey` (or `space_create`) chain
//! entry from an authorized device commits to its key id. The attack the plan names (a removed member who
//! knows epoch n plus any writer plants an epoch n+1 they also know) is `false`; a rekey is the current
//! epoch plus one, by a manager who is still one, with a key id never used before; old epochs stay
//! authorized for reading; only the committed key id of an epoch is authorized.
//!
//! Implements planning/architecture.md section 5.5 ("Epochs must be authorized: an epoch counts only if a
//! signed `space_rekey` (or `space_create`) chain entry from an authorized device commits to its key id.
//! Envelopes carry the signer. Clients reject every other epoch.").

mod common;

use azlin_seams::AccountKind;
use azlin_spaces::{
    epoch_authorized, HistoryMode, RekeyReason, SpaceKind, SpaceOp, SpaceOpName, SpaceRefusal,
    SpaceRole,
};
use common::*;

fn created() -> World {
    World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full)
}

#[test]
fn planted_epoch_without_a_chain_entry_is_not_authorized() {
    let mut w = created();
    w.do_by(1, grant_op(m(5), SpaceRole::Viewer));
    w.do_by(1, rekey_op(2, 2));
    assert!(epoch_authorized(&w.state, epoch(1), &key(1)));
    assert!(epoch_authorized(&w.state, epoch(2), &key(2)));

    // The member 5 is removed. It knows epoch 2 and, with any writer, plants an epoch 3 that they both know.
    w.do_by(1, revoke_op(m(5)));
    w.do_by(1, grant_op(m(6), SpaceRole::Editor));
    let planted = key(99);
    assert!(!epoch_authorized(&w.state, epoch(3), &planted));
    // A planted epoch 2 (the one the removed member knows) with another key is no better.
    assert!(!epoch_authorized(&w.state, epoch(2), &planted));

    // Their attempts to commit the plant through the chain are refused: the removed member holds no grant,
    // the writer holds no manage capability.
    assert_eq!(
        w.refused_by(5, rekey_op(3, 99)),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Rekey
        }
    );
    assert_eq!(
        w.refused_by(6, rekey_op(3, 99)),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Rekey
        }
    );
    // An entry that skips the signers' check is no entry: a forged signature is refused before it is judged.
    let mut forged = w.build(device(5), rekey_op(3, 99), &[]);
    forged.sig = azlin_seams::FakeScheme::sign(&device_key(6), b"nothing");
    assert_eq!(
        refusal_of(w.admit_entry(&forged)),
        SpaceRefusal::BadSignature
    );
    assert!(!epoch_authorized(&w.state, epoch(3), &planted));
    assert_eq!(w.state.key_epoch(), Some(epoch(2)));

    // The remaining manager's rotation is what makes epoch 3: its own key id, committed by its entry.
    w.do_by(1, rekey_op(3, 3));
    assert!(epoch_authorized(&w.state, epoch(3), &key(3)));
    assert!(!epoch_authorized(&w.state, epoch(3), &planted));
    // The plant is no epoch 4 either.
    assert!(!epoch_authorized(&w.state, epoch(4), &planted));
}

#[test]
fn rekey_must_be_current_plus_one() {
    let mut w = created();
    // The current epoch is 1: the next is 2, and nothing else.
    for (wrong, name) in [
        (1u32, "the same epoch"),
        (3, "an epoch skipped"),
        (9, "far ahead"),
    ] {
        assert_eq!(
            w.refused_by(1, rekey_op(wrong, 2)),
            SpaceRefusal::EpochNotNext {
                expected: epoch(2),
                got: epoch(wrong)
            },
            "{name}"
        );
    }
    w.do_by(1, rekey_op(2, 2));
    // A replay of the same rekey is not the next epoch any more.
    assert_eq!(
        w.refused_by(1, rekey_op(2, 3)),
        SpaceRefusal::EpochNotNext {
            expected: epoch(3),
            got: epoch(2)
        }
    );
    // Going back to epoch 1 is refused too.
    assert_eq!(
        w.refused_by(1, rekey_op(1, 4)),
        SpaceRefusal::EpochNotNext {
            expected: epoch(3),
            got: epoch(1)
        }
    );
    w.do_by(1, rekey_op(3, 3));
    assert_eq!(w.state.key_epoch(), Some(epoch(3)));
    assert_eq!(w.state.epochs().count(), 3);
}

#[test]
fn rekey_by_a_removed_manager_refused() {
    let mut w = created();
    w.do_by(1, grant_op(m(2), SpaceRole::Manager));
    // While it is a manager it rotates.
    w.do_by(2, rekey_op(2, 2));
    // Removed (revoked by the other manager), it does not.
    w.do_by(1, revoke_op(m(2)));
    assert_eq!(
        w.refused_by(2, rekey_op(3, 3)),
        SpaceRefusal::NotAManager {
            op: SpaceOpName::Rekey
        }
    );
    // Nor does a manager whose member was suspended or removed from the account.
    let mut w = created();
    w.do_by(1, grant_op(m(7), SpaceRole::Manager));
    w.account.suspend(7);
    assert_eq!(
        w.refused_by(7, rekey_op(2, 2)),
        SpaceRefusal::MemberNotActive
    );
    // Nor a device that was removed from the account.
    let mut w = created();
    w.do_by(1, grant_op(m(7), SpaceRole::Manager));
    w.account.deactivate_device(7);
    assert_eq!(
        w.refused_by(7, rekey_op(2, 2)),
        SpaceRefusal::SignerNotActive
    );
}

#[test]
fn an_admin_who_is_not_a_manager_cannot_rekey() {
    // Wrapping the new key to the remaining members needs the old key: the admin does not hold it, and an
    // override of the manager's check would be a backdoor for the next rotation.
    let mut w = created();
    for admin in [1u8, 2] {
        let mut w2 = World::new(AccountKind::Team);
        w2.do_by(1, create_op(SpaceKind::Shared, HistoryMode::Full, 7, 1));
        assert_eq!(
            w2.refused_by(admin, rekey_op(2, 2)),
            SpaceRefusal::NotAManager {
                op: SpaceOpName::Rekey
            },
            "member {admin}"
        );
    }
    // The manager does.
    w.do_by(1, rekey_op(2, 2));
}

#[test]
fn rekey_key_id_reuse_refused() {
    let mut w = created();
    // The key id of epoch 1 cannot be committed again as epoch 2.
    assert_eq!(w.refused_by(1, rekey_op(2, 1)), SpaceRefusal::KeyIdReused);
    w.do_by(1, rekey_op(2, 2));
    // Nor the key id of epoch 2 as epoch 3, nor the first one.
    assert_eq!(w.refused_by(1, rekey_op(3, 2)), SpaceRefusal::KeyIdReused);
    assert_eq!(w.refused_by(1, rekey_op(3, 1)), SpaceRefusal::KeyIdReused);
    w.do_by(1, rekey_op(3, 3));
    // A refused rekey committed nothing.
    assert!(!epoch_authorized(&w.state, epoch(3), &key(1)));
    assert!(!epoch_authorized(&w.state, epoch(3), &key(2)));
}

#[test]
fn old_epochs_stay_authorized_for_reading() {
    let mut w = created();
    w.do_by(1, rekey_op(2, 2));
    w.do_by(1, rekey_op(3, 3));
    w.do_by(1, rekey_op(4, 4));
    // The members read the past with the old epochs: each stays authorized for its own key id.
    for n in 1..=4u8 {
        assert!(
            epoch_authorized(&w.state, epoch(u32::from(n)), &key(n)),
            "epoch {n}"
        );
    }
    assert_eq!(w.state.key_epoch(), Some(epoch(4)));
}

#[test]
fn epoch_authorized_only_for_committed_key_ids() {
    let mut w = created();
    // Nothing is authorized before the space exists.
    let fresh = World::new(AccountKind::Team);
    assert!(!epoch_authorized(&fresh.state, epoch(1), &key(1)));

    w.do_by(1, rekey_op(2, 2));
    // The committed pairs, and no other pairing of the epochs and the key ids.
    assert!(epoch_authorized(&w.state, epoch(1), &key(1)));
    assert!(epoch_authorized(&w.state, epoch(2), &key(2)));
    assert!(!epoch_authorized(&w.state, epoch(1), &key(2)));
    assert!(!epoch_authorized(&w.state, epoch(2), &key(1)));
    // An epoch nobody committed, whatever its key.
    for n in [1u8, 2, 3, 9] {
        assert!(
            !epoch_authorized(&w.state, epoch(3), &key(n)),
            "epoch 3 / key {n}"
        );
        assert!(
            !epoch_authorized(&w.state, epoch(4), &key(n)),
            "epoch 4 / key {n}"
        );
    }
    // The key id is the whole 32 bytes: one bit off is another key.
    let mut raw = *key(2).as_bytes();
    raw[31] ^= 0x01;
    assert!(!epoch_authorized(
        &w.state,
        epoch(2),
        &azlin_seams::KeyId::from_bytes(raw)
    ));
}

#[test]
fn a_rekey_by_an_app_or_a_group_manager_commits_the_same_way() {
    let mut w = created();
    w.account.add_person(
        20,
        azlin_seams::AccountRole::Member,
        azlin_seams::MemberKind::App,
    );
    w.do_by(1, grant_op(m(20), SpaceRole::Manager));
    w.account.put_in_groups(5, &[2]);
    w.do_by(1, grant_op(g(2), SpaceRole::Manager));
    w.do_by(20, rekey_op(2, 2));
    w.do_by(5, rekey_op(3, 3));
    assert!(epoch_authorized(&w.state, epoch(2), &key(2)));
    assert!(epoch_authorized(&w.state, epoch(3), &key(3)));
    let commit = |epoch_n: u32| {
        w.state
            .epoch_record(epoch(epoch_n))
            .expect("committed")
            .signer
    };
    assert_eq!(commit(2), device(20));
    assert_eq!(commit(3), device(5));
    // The reason of a rekey is the signer's word: it changes no rule of the epoch.
    let op = SpaceOp::Rekey {
        epoch: epoch(4),
        key_id: key(4),
        reason: RekeyReason::Removal,
    };
    assert!(w.admit_by(1, op).is_ok());
}
