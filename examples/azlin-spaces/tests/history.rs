//! Check 7 of the brief (history): a grant in a `forward` space makes a rekey due (the key rotates on add)
//! and a rekey clears it; a grant in a `full` space makes none and the joiner gets the epoch chain;
//! `chain_visible_to` follows the history mode at the joiner's position; a switch of the mode applies to
//! later joiners only. And what a removal makes due, in either mode.
//!
//! Implements planning/architecture.md section 5.4 (`history`: "`full` (joiners get the epoch chain) or
//! `forward` (rotate on add, no chain for joiners)"), section 5.5 (rotation, "plus the chain wrap (epoch n
//! under epoch n+1)") and section 5.6 (what a removal costs).

mod common;

use azlin_seams::AccountKind;
use azlin_spaces::{
    chain_visible_to, ChainWrapNeed, HistoryMode, Obligation, RekeyReason, SpaceKind, SpaceNotice,
    SpaceOpName, SpaceRole, StopServing, WrapNeed,
};
use common::*;

fn world(history: HistoryMode) -> World {
    World::created(AccountKind::Team, SpaceKind::Shared, history)
}

#[test]
fn forward_space_grant_creates_a_rekey_due_obligation_and_a_rekey_clears_it() {
    let mut w = world(HistoryMode::Forward);
    assert_eq!(w.state.rekey_due(), None);

    // A joiner in a forward space: the key rotates on add, so the joiner gets no wrap of the current key.
    let effects = w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    assert_eq!(
        effects.obligations,
        vec![Obligation::RekeyDue {
            space: space(1),
            reason: RekeyReason::Joiner
        }]
    );
    assert!(
        effects.wraps.is_empty(),
        "no wrap of the current epoch for a forward joiner"
    );
    assert_eq!(w.state.rekey_due(), Some(RekeyReason::Joiner));

    // A second joiner while the rekey is still due: already due, no second obligation.
    let effects = w.do_by(1, grant_op(m(8), SpaceRole::Editor));
    assert!(effects.obligations.is_empty());
    assert_eq!(w.state.rekey_due(), Some(RekeyReason::Joiner));

    // A grant that cannot read (an uploader) adds no obligation of its own.
    let mut quiet = world(HistoryMode::Forward);
    let effects = quiet.do_by(1, grant_op(m(7), SpaceRole::Uploader));
    assert!(effects.obligations.is_empty());
    assert_eq!(quiet.state.rekey_due(), None);

    // The rekey clears it, and wraps the new epoch to every reader there is: the joiners included.
    let effects = w.do_by(1, rekey_op(2, 2));
    assert_eq!(
        effects.obligations,
        vec![Obligation::RekeyCleared { space: space(1) }]
    );
    assert_eq!(w.state.rekey_due(), None);
    for who in [m(1), m(7), m(8)] {
        assert!(
            effects.wraps.contains(&WrapNeed {
                to: who,
                epoch: epoch(2)
            }),
            "{who:?}"
        );
    }
    // A forward space makes no chain wrap: there is no chain for joiners.
    assert!(effects.chain_wraps.is_empty());
    assert!(!w.state.epoch_record(epoch(2)).expect("committed").chained);
    // A rekey that was not due clears nothing.
    let effects = w.do_by(1, rekey_op(3, 3));
    assert!(effects.obligations.is_empty());
}

#[test]
fn full_space_grant_creates_none_and_the_joiner_gets_the_chain() {
    let mut w = world(HistoryMode::Full);
    w.do_by(1, rekey_op(2, 2));
    w.do_by(1, rekey_op(3, 3));
    // A joiner of a full space reads the key of the epoch in force, and the chain wraps give it the past.
    let effects = w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    assert!(effects.obligations.is_empty());
    assert_eq!(w.state.rekey_due(), None);
    assert_eq!(
        effects.wraps,
        vec![WrapNeed {
            to: m(7),
            epoch: epoch(3)
        }]
    );
    let view = chain_visible_to(&w.state, seq(3));
    assert_eq!(view.mode(), Some(HistoryMode::Full));
    assert_eq!(view.first(), Some(epoch(1)));
    assert_eq!(view.through(), Some(epoch(3)));
    for n in 1..=3 {
        assert!(view.contains(epoch(n)), "epoch {n}");
    }
    assert!(!view.contains(epoch(4)));
    // The rotations of a full space made their chain wraps.
    assert!(w.state.epoch_record(epoch(2)).expect("2").chained);
    assert!(w.state.epoch_record(epoch(3)).expect("3").chained);
    assert!(!w.state.epoch_record(epoch(1)).expect("1").chained);
}

/// The chain of the scenario of the next two tests: positions and epochs.
///
/// ```text
///  0 create (full)         epoch 1
///  1 grant member 7        (joins at 1, full)
///  2 rekey                 epoch 2, chained
///  3 rekey                 epoch 3, chained
///  4 history forward
///  5 grant member 8        (joins at 5, forward: a rekey is due)
///  6 rekey                 epoch 4, NOT chained (forward)
///  7 history full
///  8 grant member 9        (joins at 8, full)
/// ```
fn scenario() -> World {
    let mut w = world(HistoryMode::Full);
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    w.do_by(1, rekey_op(2, 2));
    w.do_by(1, rekey_op(3, 3));
    w.do_by(1, history_op(HistoryMode::Forward));
    w.do_by(1, grant_op(m(8), SpaceRole::Viewer));
    w.do_by(1, rekey_op(4, 4));
    w.do_by(1, history_op(HistoryMode::Full));
    w.do_by(1, grant_op(m(9), SpaceRole::Viewer));
    w
}

#[test]
fn chain_visible_to_follows_history_mode_at_the_joiners_seq() {
    let w = scenario();
    assert_eq!(w.state.chain_head().expect("head").seq, seq(8));

    // The member 7 joined at 1, in full history: it reads every epoch's wrap chain from the first.
    let first = chain_visible_to(&w.state, seq(1));
    assert_eq!(first.mode(), Some(HistoryMode::Full));
    assert_eq!(first.first(), Some(epoch(1)));
    assert_eq!(first.through(), Some(epoch(4)));
    for n in 1..=4 {
        assert!(first.contains(epoch(n)), "member 7 / epoch {n}");
    }

    // The member 8 joined at 5, in forward history: only from the rotation that followed its grant.
    let forward = chain_visible_to(&w.state, seq(5));
    assert_eq!(forward.mode(), Some(HistoryMode::Forward));
    assert_eq!(forward.first(), Some(epoch(4)));
    assert!(forward.contains(epoch(4)));
    for n in 1..=3 {
        assert!(!forward.contains(epoch(n)), "member 8 / epoch {n}");
    }

    // The member 9 joined at 8, in full history again, but the rotation to epoch 4 was made in forward
    // history, so there is no chain wrap of epoch 3 under epoch 4: its chain starts at epoch 4.
    let later = chain_visible_to(&w.state, seq(8));
    assert_eq!(later.mode(), Some(HistoryMode::Full));
    assert_eq!(later.first(), Some(epoch(4)));
    assert!(!later.contains(epoch(3)));

    // A joiner at 3 would have had the whole chain of epochs 3, 2, 1.
    let at_three = chain_visible_to(&w.state, seq(3));
    assert_eq!(at_three.first(), Some(epoch(1)));

    // The chain wraps of the rotations: made in full mode only.
    let chained: Vec<bool> = w.state.epochs().map(|(_, record)| record.chained).collect();
    assert_eq!(chained, vec![false, true, true, false]);

    // The forward joiner before its rotation: no epoch at all is visible to it yet.
    let mut pending = world(HistoryMode::Forward);
    pending.do_by(1, grant_op(m(8), SpaceRole::Viewer));
    let view = chain_visible_to(&pending.state, seq(1));
    assert_eq!(view.mode(), Some(HistoryMode::Forward));
    assert_eq!(view.first(), None);
    assert!(!view.contains(epoch(1)));
    pending.do_by(1, rekey_op(2, 2));
    let view = chain_visible_to(&pending.state, seq(1));
    assert_eq!(view.first(), Some(epoch(2)));

    // Before the space was created there is nothing to see.
    let empty = World::new(AccountKind::Team);
    let view = chain_visible_to(&empty.state, seq(0));
    assert_eq!(view.mode(), None);
    assert_eq!(view.first(), None);
    assert_eq!(view.through(), None);
}

#[test]
fn history_switch_applies_to_later_joiners_only() {
    let mut w = world(HistoryMode::Full);
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    w.do_by(1, rekey_op(2, 2));
    w.do_by(1, rekey_op(3, 3));
    let before = chain_visible_to(&w.state, seq(1));
    assert_eq!(before.mode(), Some(HistoryMode::Full));
    assert_eq!(before.first(), Some(epoch(1)));

    // The managers switch to forward, and a new member joins afterwards.
    w.do_by(1, history_op(HistoryMode::Forward));
    assert_eq!(w.state.history(), Some(HistoryMode::Forward));
    w.do_by(1, grant_op(m(8), SpaceRole::Viewer));
    w.do_by(1, rekey_op(4, 4));

    // The member who joined before the switch keeps what it had: its mode and its first epoch are the
    // same, the epochs it may read have only grown.
    let after = chain_visible_to(&w.state, seq(1));
    assert_eq!(after.mode(), Some(HistoryMode::Full));
    assert_eq!(after.first(), before.first());
    assert_eq!(after.through(), Some(epoch(4)));
    // The later joiner is under the new mode.
    let later = chain_visible_to(&w.state, seq(5));
    assert_eq!(later.mode(), Some(HistoryMode::Forward));
    assert_eq!(later.first(), Some(epoch(4)));
    assert!(!later.contains(epoch(3)));

    // Switching to the mode already in force changes nothing: no second switch is recorded.
    let hash = w.state.state_hash();
    let effects = w.do_by(1, history_op(HistoryMode::Forward));
    assert_eq!(effects.notices.len(), 1);
    assert_ne!(w.state.state_hash(), hash, "the chain moved by one entry");
    assert_eq!(chain_visible_to(&w.state, seq(5)).first(), Some(epoch(4)));
    // The switch is a notice of the chain, nothing more.
    assert!(matches!(
        effects.notices[0],
        SpaceNotice::LogAppended {
            op: SpaceOpName::History,
            ..
        }
    ));
}

#[test]
fn a_removal_makes_a_rekey_due_in_either_mode() {
    for mode in [HistoryMode::Full, HistoryMode::Forward] {
        let mut w = world(mode);
        w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
        w.do_by(1, grant_op(m(8), SpaceRole::Uploader));
        // Clear a due rekey of the joiner first, in a forward space.
        if mode == HistoryMode::Forward {
            w.do_by(1, rekey_op(2, 2));
        }
        let epoch_now = w.state.key_epoch().expect("an epoch");

        // An uploader never held the key: its removal makes no rekey due and stops no wrap.
        let effects = w.do_by(1, revoke_op(m(8)));
        assert!(effects.obligations.is_empty(), "{mode:?}");
        assert!(effects.stop_serving.is_empty());
        assert_eq!(w.state.rekey_due(), None);

        // A viewer did: the key rotates and its wraps stop being served.
        let effects = w.do_by(1, revoke_op(m(7)));
        assert_eq!(
            effects.obligations,
            vec![Obligation::RekeyDue {
                space: space(1),
                reason: RekeyReason::Removal
            }],
            "{mode:?}"
        );
        assert_eq!(
            effects.stop_serving,
            vec![StopServing {
                to: m(7),
                through: epoch_now
            }]
        );
        assert_eq!(w.state.rekey_due(), Some(RekeyReason::Removal));
    }
}

#[test]
fn a_due_reason_only_gets_worse() {
    let mut w = world(HistoryMode::Forward);
    // Joiner first ...
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    assert_eq!(w.state.rekey_due(), Some(RekeyReason::Joiner));
    // ... then a removal: more severe, so the obligation is raised again.
    let effects = w.do_by(1, revoke_op(m(7)));
    assert_eq!(
        effects.obligations,
        vec![Obligation::RekeyDue {
            space: space(1),
            reason: RekeyReason::Removal
        }]
    );
    // A later joiner does not lower it, and a second removal is already covered.
    let effects = w.do_by(1, grant_op(m(8), SpaceRole::Viewer));
    assert!(effects.obligations.is_empty());
    assert_eq!(w.state.rekey_due(), Some(RekeyReason::Removal));
    let effects = w.do_by(1, revoke_op(m(8)));
    assert!(effects.obligations.is_empty());
    assert_eq!(w.state.rekey_due(), Some(RekeyReason::Removal));
    // The rekey clears whatever was due.
    let effects = w.do_by(1, rekey_op(2, 2));
    assert_eq!(
        effects.obligations,
        vec![Obligation::RekeyCleared { space: space(1) }]
    );
}

#[test]
fn a_role_change_that_gains_or_loses_reading_is_a_join_or_a_removal() {
    // Full space: an uploader that becomes a viewer reads from then on and needs a wrap; one that was
    // a viewer and becomes an uploader loses the key it holds.
    let mut w = world(HistoryMode::Full);
    w.do_by(1, grant_op(m(7), SpaceRole::Uploader));
    let effects = w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    assert_eq!(
        effects.wraps,
        vec![WrapNeed {
            to: m(7),
            epoch: epoch(1)
        }]
    );
    assert!(effects.obligations.is_empty());
    // Viewer to editor: reads before and after, nothing changes for the key.
    let effects = w.do_by(1, grant_op(m(7), SpaceRole::Editor));
    assert!(effects.wraps.is_empty() && effects.obligations.is_empty());
    // Editor to uploader: it knows the key and may not read any more: a removal.
    let effects = w.do_by(1, grant_op(m(7), SpaceRole::Uploader));
    assert_eq!(
        effects.stop_serving,
        vec![StopServing {
            to: m(7),
            through: epoch(1)
        }]
    );
    assert_eq!(w.state.rekey_due(), Some(RekeyReason::Removal));

    // Forward space: gaining read access is a join, so the rekey is due and no wrap is made now.
    let mut w = world(HistoryMode::Forward);
    w.do_by(1, grant_op(m(7), SpaceRole::Uploader));
    assert_eq!(w.state.rekey_due(), None);
    let effects = w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    assert!(effects.wraps.is_empty());
    assert_eq!(w.state.rekey_due(), Some(RekeyReason::Joiner));
}

#[test]
fn a_rekey_wraps_only_the_remaining_readers_and_chains_in_full_mode() {
    let mut w = world(HistoryMode::Full);
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    w.do_by(1, grant_op(m(8), SpaceRole::Uploader));
    w.do_by(1, grant_op(g(1), SpaceRole::Editor));
    w.do_by(1, grant_op(x(1), SpaceRole::Viewer));
    w.do_by(1, grant_op(cap(1), SpaceRole::Viewer));
    w.do_by(1, grant_op(m(9), SpaceRole::Viewer));
    w.do_by(1, revoke_op(m(9)));
    let effects = w.do_by(1, rekey_op(2, 2));
    let wrapped: Vec<_> = effects.wraps.iter().map(|need| need.to).collect();
    // Every reader that remains (the grants are in the order of their grantees), no uploader, no revoked.
    assert!(wrapped.contains(&m(1)));
    assert!(wrapped.contains(&m(7)));
    assert!(wrapped.contains(&g(1)));
    assert!(wrapped.contains(&x(1)));
    assert!(wrapped.contains(&cap(1)));
    assert!(!wrapped.contains(&m(8)), "an uploader holds no key");
    assert!(!wrapped.contains(&m(9)), "a revoked reader gets no wrap");
    assert_eq!(wrapped.len(), 5);
    assert!(effects.wraps.iter().all(|need| need.epoch == epoch(2)));
    // The chain wrap: epoch 1 under epoch 2.
    assert_eq!(
        effects.chain_wraps,
        vec![ChainWrapNeed {
            of: epoch(1),
            under: epoch(2)
        }]
    );
}
