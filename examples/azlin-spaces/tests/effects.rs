//! Check 10 of the brief (effects): a grant names the wraps a new member needs (member or group, and the
//! epoch), a rekey emits the `key_epoch` notice and wraps the remaining readers, a revoke lists the wraps
//! to stop serving; and what every entry emits (the `log_appended` notice and the `reg_space` row), the
//! rows of the grants, and the determinism of the effects.
//!
//! Implements planning/architecture.md section 5.5 ("a new epoch key, wrapped only to the remaining members
//! and groups, plus the chain wrap"), section 5.6 (what a removal costs), section 5.9 ("Notices come from
//! the metadata group") and section 5.13 (the `log_appended` and `key_epoch` events).

mod common;

use azlin_seams::AccountKind;
use azlin_spaces::{
    Grantee, HistoryMode, Payer, RekeyReason, Source, SpaceDelta, SpaceKind, SpaceNotice,
    SpaceOpName, SpaceRole, StopServing, WrapNeed,
};
use common::*;

fn created() -> World {
    World::created(AccountKind::Team, SpaceKind::Shared, HistoryMode::Full)
}

#[test]
fn grant_effect_names_the_wraps_a_new_member_needs() {
    let mut w = created();
    // A member, a group, an external member and a capability: each is a recipient of a wrap of the
    // current epoch (wraps go to groups, not to each member of them).
    for who in [m(7), g(1), x(1), cap(1)] {
        let effects = w.do_by(1, grant_op(who, SpaceRole::Viewer));
        assert_eq!(
            effects.wraps,
            vec![WrapNeed {
                to: who,
                epoch: epoch(1)
            }],
            "{who:?}"
        );
        assert!(effects.stop_serving.is_empty());
    }
    // After a rotation the new member needs the wrap of the new epoch.
    w.do_by(1, rekey_op(2, 2));
    let effects = w.do_by(1, grant_op(m(8), SpaceRole::Editor));
    assert_eq!(
        effects.wraps,
        vec![WrapNeed {
            to: m(8),
            epoch: epoch(2)
        }]
    );
    // An uploader holds no key: no wrap.
    let effects = w.do_by(1, grant_op(m(9), SpaceRole::Uploader));
    assert!(effects.wraps.is_empty());
    // A change of role that keeps the reading changes no wrap.
    let effects = w.do_by(1, grant_op(m(8), SpaceRole::Viewer));
    assert!(effects.wraps.is_empty());
    // The row of the grant is in the effects, whole, with its source and stamp.
    let effects = w.do_by(
        1,
        grant_full(m(10), SpaceRole::Editor, Some(500), Payer::Grantee),
    );
    let grants: Vec<_> = effects
        .rows
        .iter()
        .filter_map(|row| match row {
            SpaceDelta::Grant(grant) => Some(grant),
            SpaceDelta::Space(_) => None,
        })
        .collect();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].grantee, m(10));
    assert_eq!(grants[0].role, SpaceRole::Editor);
    assert_eq!(grants[0].cap_bytes, Some(bytes(500)));
    assert_eq!(grants[0].payer, Payer::Grantee);
    assert!(grants[0].is_active());
    assert_eq!(grants[0].removed_at, None);
    assert_eq!(grants[0].source, Source::Chain(seq(9)));
}

#[test]
fn rekey_effect_emits_key_epoch_notice() {
    let mut w = created();
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer));
    w.do_by(1, grant_op(m(8), SpaceRole::Uploader));
    let effects = w.do_by(1, rekey_op(2, 2));
    // The `key_epoch` event, and the `log_appended` of every entry.
    assert!(effects.notices.contains(&SpaceNotice::KeyEpoch {
        space: space(1),
        epoch: epoch(2)
    }));
    assert!(effects.notices.contains(&SpaceNotice::LogAppended {
        space: space(1),
        op: SpaceOpName::Rekey
    }));
    // The new epoch is wrapped to the readers that remain: the manager and the viewer, not the uploader.
    let wrapped: Vec<Grantee> = effects.wraps.iter().map(|need| need.to).collect();
    assert_eq!(wrapped, vec![m(1), m(7)]);
    assert!(effects.wraps.iter().all(|need| need.epoch == epoch(2)));
    // The chain wrap of a full space: the old key under the new one.
    assert_eq!(effects.chain_wraps.len(), 1);
    assert_eq!(effects.chain_wraps[0].of, epoch(1));
    assert_eq!(effects.chain_wraps[0].under, epoch(2));
    // The `reg_space` row carries the new epoch.
    match effects.rows.first() {
        Some(SpaceDelta::Space(row)) => {
            assert_eq!(row.key_epoch, epoch(2));
            assert_eq!(row.chain_head.seq, seq(3));
        }
        other => panic!("{other:?}"),
    }
    // A grant or a revoke is no key epoch: only a create and a rekey emit the event.
    let effects = w.do_by(1, grant_op(m(9), SpaceRole::Viewer));
    assert!(effects
        .notices
        .iter()
        .all(|notice| !matches!(notice, SpaceNotice::KeyEpoch { .. })));
}

#[test]
fn revoke_effect_lists_the_wraps_to_stop_serving() {
    let mut w = created();
    for who in [m(7), g(1), x(1), cap(1)] {
        w.do_by(1, grant_op(who, SpaceRole::Editor));
    }
    w.do_by(1, rekey_op(2, 2));
    for who in [m(7), g(1), x(1), cap(1)] {
        let effects = w.do_by(1, revoke_op(who));
        // Every wrap of the recipient up to the epoch in force stops being served.
        assert_eq!(
            effects.stop_serving,
            vec![StopServing {
                to: who,
                through: epoch(2)
            }],
            "{who:?}"
        );
        // The row is the revoked grant, whole, with its end stamp.
        let ended: Vec<_> = effects
            .rows
            .iter()
            .filter_map(|row| match row {
                SpaceDelta::Grant(grant) => Some(grant),
                SpaceDelta::Space(_) => None,
            })
            .collect();
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0].grantee, who);
        assert!(!ended[0].is_active());
        assert!(ended[0].removed_at.is_some());
        // A removal makes a rekey due: the revoked grantee knows the key.
        assert_eq!(w.state.rekey_due(), Some(RekeyReason::Removal));
        assert!(effects.wraps.is_empty());
    }
    // An uploader held no wrap: nothing to stop.
    let effects = w.do_by(1, grant_op(m(8), SpaceRole::Uploader));
    assert!(effects.wraps.is_empty());
    let effects = w.do_by(1, revoke_op(m(8)));
    assert!(effects.stop_serving.is_empty());
}

#[test]
fn every_entry_emits_log_appended_and_the_space_row() {
    let mut w = World::new(AccountKind::Team);
    let ops = [
        (
            1u8,
            create_op(SpaceKind::Shared, HistoryMode::Full, 1, 1),
            SpaceOpName::Create,
        ),
        (1, grant_op(m(7), SpaceRole::Viewer), SpaceOpName::Grant),
        (1, rekey_op(2, 2), SpaceOpName::Rekey),
        (1, history_op(HistoryMode::Forward), SpaceOpName::History),
        (1, revoke_op(m(7)), SpaceOpName::Revoke),
    ];
    for (n, op, name) in ops {
        let effects = w.do_by(n, op);
        assert!(effects.applied);
        assert!(effects.notices.contains(&SpaceNotice::LogAppended {
            space: space(1),
            op: name
        }));
        // The first row is always the `reg_space` row, and its chain head is the entry just applied.
        match effects.rows.first() {
            Some(SpaceDelta::Space(row)) => {
                assert_eq!(Some(row.chain_head), w.state.chain_head());
                assert_eq!(row.space, space(1));
                assert_eq!(row.account, account(1));
            }
            other => panic!("{name:?}: {other:?}"),
        }
    }
    assert_eq!(w.state.row().expect("a row").chain_head.seq, seq(4));
}

#[test]
fn a_grant_update_keeps_where_the_access_began() {
    let mut w = created();
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer)); // seq 1
    let began = w.state.grant(&m(7)).expect("row").clone();
    assert_eq!(began.source, Source::Chain(seq(1)));
    // A new role that keeps the reading keeps the beginning (it is where a joiner joined).
    w.do_by(1, grant_op(m(7), SpaceRole::Editor));
    let updated = w.state.grant(&m(7)).expect("row").clone();
    assert_eq!(updated.role, SpaceRole::Editor);
    assert_eq!(updated.source, began.source);
    assert_eq!(updated.added_at, began.added_at);
    // An uploader that becomes a viewer begins to read: a new beginning.
    w.do_by(1, grant_op(m(8), SpaceRole::Uploader)); // seq 3
    w.do_by(1, grant_op(m(8), SpaceRole::Viewer)); // seq 4
    assert_eq!(
        w.state.grant(&m(8)).expect("row").source,
        Source::Chain(seq(4))
    );
    // A revoked row keeps its beginning and gets its end; a new grant after it is a new row.
    w.do_by(1, revoke_op(m(7))); // seq 5
    let ended = w.state.grant(&m(7)).expect("row").clone();
    assert!(!ended.is_active());
    assert_eq!(ended.source, began.source);
    assert!(ended.removed_at.expect("an end") >= ended.added_at);
    w.do_by(1, grant_op(m(7), SpaceRole::Viewer)); // seq 6
    let again = w.state.grant(&m(7)).expect("row").clone();
    assert!(again.is_active());
    assert_eq!(again.source, Source::Chain(seq(6)));
    assert_eq!(again.removed_at, None);
}

#[test]
fn the_effects_of_the_same_entries_are_the_same_on_every_replica() {
    let run = || {
        let mut w = created();
        let mut all = Vec::new();
        for (n, op) in [
            (1u8, grant_op(m(7), SpaceRole::Viewer)),
            (1, grant_op(g(1), SpaceRole::Editor)),
            (1, rekey_op(2, 2)),
            (1, revoke_op(m(7))),
            (1, history_op(HistoryMode::Forward)),
            (1, grant_op(x(1), SpaceRole::Viewer)),
            (1, rekey_op(3, 3)),
        ] {
            all.push(w.do_by(n, op));
        }
        (all, w.state.state_hash())
    };
    let (first, first_hash) = run();
    let (second, second_hash) = run();
    assert_eq!(first, second);
    assert_eq!(first_hash, second_hash);
}
