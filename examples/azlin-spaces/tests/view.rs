//! Check 11 of the brief (planner P3's seam): `SpaceChains` implements `azlin_keys::SpaceChainView`:
//! `commitment(space, epoch)` names the key id, the signer, the position and the kind of the entry that
//! committed it; an unreadable chain is an error and never "no commitment" or "not authorized";
//! `is_authorized_signer(space, device, at)` answers for a position of the chain.
//!
//! Implements planning/architecture.md section 5.5 ("an epoch counts only if a signed `space_rekey` (or
//! `space_create`) chain entry from an authorized device commits to its key id") and section 5.9 (each
//! space has its own signed membership chain).

mod common;

use azlin_keys::{ChainError, CommitKind, Commitment, SpaceChainView};
use azlin_seams::AccountKind;
use azlin_spaces::{HistoryMode, SpaceChains, SpaceKind, SpaceRole, SpaceState};
use common::*;

fn created() -> World {
    World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full)
}

#[test]
fn space_chains_implement_azlin_keys_space_chain_view() {
    let w = created();
    let mut chains = SpaceChains::new();
    assert!(chains.is_empty());
    chains.insert(w.state.clone());
    assert_eq!(chains.len(), 1);
    assert!(chains.state(&space(1)).is_some());
    // Used the way the key code uses it: through the trait object.
    let view: &dyn SpaceChainView = &chains;
    let commitment = view
        .commitment(&space(1), epoch(1))
        .expect("the chain is readable")
        .expect("the first epoch is committed");
    assert_eq!(commitment.key_id, key(1));
    assert_eq!(
        view.is_authorized_signer(&space(1), &device(1), seq(0)),
        Ok(true)
    );
}

#[test]
fn commitment_names_key_id_signer_seq_and_kind() {
    let mut w = created();
    w.do_by(1, grant_op(m(2), SpaceRole::Manager)); // 1
    w.do_by(2, rekey_op(2, 2)); // 2, signed by the second manager
    w.do_by(1, rekey_op(3, 3)); // 3
    let mut chains = SpaceChains::new();
    chains.insert(w.state.clone());
    let view: &dyn SpaceChainView = &chains;

    assert_eq!(
        view.commitment(&space(1), epoch(1)),
        Ok(Some(Commitment {
            space: space(1),
            epoch: epoch(1),
            key_id: key(1),
            signer: device(1),
            seq: seq(0),
            kind: CommitKind::Create,
        }))
    );
    assert_eq!(
        view.commitment(&space(1), epoch(2)),
        Ok(Some(Commitment {
            space: space(1),
            epoch: epoch(2),
            key_id: key(2),
            signer: device(2),
            seq: seq(2),
            kind: CommitKind::Rekey,
        }))
    );
    assert_eq!(
        view.commitment(&space(1), epoch(3)),
        Ok(Some(Commitment {
            space: space(1),
            epoch: epoch(3),
            key_id: key(3),
            signer: device(1),
            seq: seq(3),
            kind: CommitKind::Rekey,
        }))
    );
    // An epoch nobody committed: a readable chain with no commitment.
    assert_eq!(view.commitment(&space(1), epoch(4)), Ok(None));
    // The state answers the same question without the collection.
    assert_eq!(
        w.state.commitment(epoch(2)),
        view.commitment(&space(1), epoch(2)).expect("readable")
    );
}

#[test]
fn unreadable_chain_is_err_not_none() {
    let w = created();
    let mut chains = SpaceChains::new();
    // A space the collection knows nothing of.
    {
        let view: &dyn SpaceChainView = &chains;
        assert_eq!(
            view.commitment(&space(1), epoch(1)),
            Err(ChainError::UnknownSpace)
        );
        assert_eq!(
            view.is_authorized_signer(&space(1), &device(1), seq(0)),
            Err(ChainError::UnknownSpace)
        );
    }
    // A chain that cannot be read, or that fails its own verification: an error, whatever it held.
    for why in [ChainError::Unreadable, ChainError::Corrupt] {
        chains.insert(w.state.clone());
        chains.mark_unreadable(space(1), why);
        assert!(chains.state(&space(1)).is_none());
        let view: &dyn SpaceChainView = &chains;
        assert_eq!(view.commitment(&space(1), epoch(1)), Err(why));
        assert_eq!(view.commitment(&space(1), epoch(9)), Err(why));
        assert_eq!(
            view.is_authorized_signer(&space(1), &device(1), seq(0)),
            Err(why)
        );
    }
    // A readable chain that holds no commitment for the epoch is `Ok(None)`: a different answer.
    chains.insert(w.state.clone());
    let view: &dyn SpaceChainView = &chains;
    assert_eq!(view.commitment(&space(1), epoch(9)), Ok(None));
    // A readable chain that was not created yet commits nothing and authorizes nobody.
    chains.insert(SpaceState::new(space(2), account(1)));
    let view: &dyn SpaceChainView = &chains;
    assert_eq!(view.commitment(&space(2), epoch(1)), Ok(None));
    assert_eq!(
        view.is_authorized_signer(&space(2), &device(1), seq(0)),
        Ok(false)
    );
}

/// The chain of the scenario of the next tests, positions and who signed:
///
/// ```text
///   0 create            member 1 (first manager)
///   1 grant viewer 7    member 1
///   2 grant viewer 8    member 1
///   3 grant viewer 9    member 1
///   4 grant viewer 10   member 1
///   5 grant MANAGER 2   member 1      member 2 may sign from 6
///   6 grant viewer 11   member 2
///   7 grant viewer 12   member 1
///   8 grant editor 7    member 1
///   9 revoke 2          member 1      member 2 may sign no more from 10
///  10 rekey             member 1
/// ```
fn scenario() -> World {
    let mut w = created();
    for n in 7..=10u8 {
        w.do_by(1, grant_op(m(n), SpaceRole::Viewer)); // 1 to 4
    }
    w.do_by(1, grant_op(m(2), SpaceRole::Manager)); // 5
    w.do_by(2, grant_op(m(11), SpaceRole::Viewer)); // 6
    w.do_by(1, grant_op(m(12), SpaceRole::Viewer)); // 7
    w.do_by(1, grant_op(m(7), SpaceRole::Editor)); // 8
    w.do_by(1, revoke_op(m(2))); // 9
    w.do_by(1, rekey_op(2, 2)); // 10
    assert_eq!(w.state.chain_head().expect("head").seq, seq(10));
    w
}

#[test]
fn is_authorized_signer_at_a_chain_position() {
    let w = scenario();
    let mut chains = SpaceChains::new();
    chains.insert(w.state.clone());
    let view: &dyn SpaceChainView = &chains;
    let authorized = |device_n: u8, at: u64| {
        view.is_authorized_signer(&space(1), &device(device_n), seq(at))
            .expect("the chain is readable")
    };

    // The member 2 was made a manager by the entry at 5 and removed by the entry at 9: it could sign
    // from 6 up to and including 9 (its own removal), and not before or after.
    for at in 0..=5 {
        assert!(!authorized(2, at), "member 2 at {at}");
    }
    for at in 6..=9 {
        assert!(authorized(2, at), "member 2 at {at}");
    }
    assert!(
        authorized(2, 7),
        "a manager at 5, removed at 9, is authorized at 7"
    );
    assert!(!authorized(2, 10), "and not at 10");
    assert!(!authorized(2, 11));

    // The member 1 (the first manager) signed the create at 0 and may sign from 1 on.
    for at in 0..=11 {
        assert!(authorized(1, at), "member 1 at {at}");
    }

    // Devices that never signed and were never told of: nobody the chain can vouch for.
    assert!(!authorized(7, 3));
    assert!(!authorized(5, 8));
    assert!(!authorized(33, 3));
}

#[test]
fn a_manager_who_never_signed_is_known_through_the_team_log() {
    let mut w = created();
    w.do_by(1, grant_op(m(3), SpaceRole::Manager)); // 1: member 3 may sign from 2
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer)); // 2
    w.do_by(1, revoke_op(m(3))); // 3: member 3 may sign no more from 4
    w.do_by(1, grant_op(m(8), SpaceRole::Viewer)); // 4
    let mut chains = SpaceChains::new();
    chains.insert(w.state.clone());
    {
        let view: &dyn SpaceChainView = &chains;
        // Not known as a device of member 3 yet: not a signer the chain can name.
        assert_eq!(
            view.is_authorized_signer(&space(1), &device(3), seq(2)),
            Ok(false)
        );
    }
    // The team log knows the device and tells the collection.
    chains.learn_device(device(3), account(1), member(3));
    let view: &dyn SpaceChainView = &chains;
    let authorized = |at: u64| {
        view.is_authorized_signer(&space(1), &device(3), seq(at))
            .expect("readable")
    };
    assert!(!authorized(1));
    assert!(authorized(2));
    assert!(authorized(3));
    assert!(!authorized(4));
    assert!(!authorized(5));
}

#[test]
fn a_position_beyond_the_chain_is_unreadable_not_authorized() {
    let w = scenario();
    let mut chains = SpaceChains::new();
    chains.insert(w.state.clone());
    let view: &dyn SpaceChainView = &chains;
    // The head is at 10: the position after it is the next entry's, and the chain says nothing beyond.
    assert_eq!(
        view.is_authorized_signer(&space(1), &device(1), seq(11)),
        Ok(true)
    );
    assert_eq!(
        view.is_authorized_signer(&space(1), &device(1), seq(12)),
        Err(ChainError::Unreadable)
    );
    assert_eq!(
        view.is_authorized_signer(&space(1), &device(2), seq(500)),
        Err(ChainError::Unreadable)
    );
}

#[test]
fn a_creator_who_is_no_manager_signed_only_the_create() {
    // An admin makes a team space for a lead: the admin signed the create and holds no grant.
    let mut w = World::new(AccountKind::Team);
    w.do_by(2, create_op(SpaceKind::Team, HistoryMode::Full, 7, 1));
    w.do_by(7, grant_op(m(8), SpaceRole::Viewer));
    let mut chains = SpaceChains::new();
    chains.insert(w.state.clone());
    let view: &dyn SpaceChainView = &chains;
    assert_eq!(
        view.is_authorized_signer(&space(1), &device(2), seq(0)),
        Ok(true)
    );
    assert_eq!(
        view.is_authorized_signer(&space(1), &device(2), seq(1)),
        Ok(false)
    );
    // The lead is a manager from the entry after the create.
    assert_eq!(
        view.is_authorized_signer(&space(1), &device(7), seq(1)),
        Ok(true)
    );
    assert_eq!(
        view.is_authorized_signer(&space(1), &device(7), seq(0)),
        Ok(false)
    );
    // The commitment of the first epoch is the admin's entry: authorized at the position it signed.
    let commitment = view
        .commitment(&space(1), epoch(1))
        .expect("readable")
        .expect("committed");
    assert_eq!(commitment.signer, device(2));
    assert_eq!(
        view.is_authorized_signer(&space(1), &commitment.signer, commitment.seq),
        Ok(true)
    );
}

#[test]
fn an_external_manager_has_a_window_of_its_own_and_a_group_manager_only_its_own_entries() {
    let mut w = created();
    w.do_by(1, grant_op(x(1), SpaceRole::Manager)); // 1: the external member may sign from 2
    w.do_by_ext(1, grant_op(m(7), SpaceRole::Viewer)); // 2
    w.do_by(1, revoke_op(x(1))); // 3
                                 // A manager through the group 2: the chain cannot see the group's members, so it recognizes the
                                 // device at the positions it signed and nowhere else.
    w.account.put_in_groups(5, &[2]);
    w.do_by(1, grant_op(g(2), SpaceRole::Manager)); // 4
    w.do_by(5, grant_op(m(8), SpaceRole::Viewer)); // 5
    w.do_by(1, grant_op(m(9), SpaceRole::Viewer)); // 6
    let mut chains = SpaceChains::new();
    chains.insert(w.state.clone());
    let view: &dyn SpaceChainView = &chains;
    let authorized = |device_id, at: u64| {
        view.is_authorized_signer(&space(1), &device_id, seq(at))
            .expect("readable")
    };
    // The external device signed at 2 and is known by that; its window is 2 to 3.
    assert!(!authorized(ext_device(1), 1));
    assert!(authorized(ext_device(1), 2));
    assert!(authorized(ext_device(1), 3));
    assert!(!authorized(ext_device(1), 4));
    // The member of the group signed at 5, and only there is it recognized.
    assert!(authorized(device(5), 5));
    assert!(!authorized(device(5), 4));
    assert!(!authorized(device(5), 6));
    // An INTERNAL member whose id equals the external one's is somebody else: its device is not authorized
    // by the external member's window.
    let mut other = chains;
    other.learn_device(device(40), account(1), ext_member(1));
    let view: &dyn SpaceChainView = &other;
    assert_eq!(
        view.is_authorized_signer(&space(1), &device(40), seq(2)),
        Ok(false)
    );
}
