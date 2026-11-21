//! Tests of the state: the plain view, the group tree and its subtree queries, the device states and
//! the accessors (lane W1-S17c1).

use azlin_policy::{Policy, PolicyPatch};
use azlin_seams::{AccountKind, ChainSeq, Span};

use super::kit::{account, at, device, group, hash, key, keys, member, rich_business, Kit};
use crate::devices::DeviceState;
use crate::groups::{Group, Groups};
use crate::members::MemberState;
use crate::ops::{Op, PolicyScope};
use crate::roles::Role;
use crate::state::{TeamState, MAX_GROUP_DEPTH};

#[test]
fn view_is_plain_data_and_equal_for_equal_states() {
    let a = rich_business();
    let b = rich_business();
    // Two replicas that applied the same entries hold equal states and equal views.
    assert_eq!(a.state, b.state);
    assert_eq!(a.state.view(), b.state.view());
    assert_eq!(a.state.state_hash(), b.state.state_hash());
    // The view is a copy: it holds the rows, in order, with no behaviour.
    let view = a.state.view();
    assert_eq!(view.account, account());
    assert_eq!(view.kind, AccountKind::Business);
    assert_eq!(view.members.len(), 7);
    assert_eq!(view.devices.len(), 7);
    assert_eq!(view.groups.len(), 3);
    assert!(view.members.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert!(view.devices.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert!(view.groups.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert_eq!(view.group_members, a.state.groups().membership_pairs());
    assert_eq!(view.head, a.state.head());
    assert_eq!(view.policy_epoch, a.state.policy_epoch());
    assert_eq!(view.genesis_hash, a.state.genesis_hash());
    assert_eq!(view.escrow_pub, Some([0xE5; 32]));
    assert_eq!(view.audit_pub, Some([0xA7; 32]));
    assert_eq!(view.delegations.len(), 1);
    assert_eq!(view.pending_escrow_opens.len(), 1);
    assert!(view.entitlement.is_some());
    // The view of a clone is the view; the view of a different state is different.
    assert_eq!(a.state.clone().view(), view);
    let mut other = rich_business();
    assert!(
        other
            .step(1, 1, Op::MemberSuspend { member: member(2) })
            .applied
    );
    assert_ne!(other.state.view(), view);
}

#[test]
fn an_empty_state_has_no_head_and_the_default_policy_of_its_kind() {
    for kind in [
        AccountKind::Personal,
        AccountKind::Family,
        AccountKind::Team,
        AccountKind::Business,
    ] {
        let state = TeamState::empty(account(), kind);
        assert!(state.is_empty());
        assert_eq!(state.kind(), kind);
        assert_eq!(state.account(), account());
        assert_eq!(state.head(), None);
        assert_eq!(state.genesis_hash(), None);
        assert_eq!(state.members().count(), 0);
        assert_eq!(state.devices().count(), 0);
        assert!(state.groups().is_empty());
        assert_eq!(state.policy(), &Policy::defaults(kind));
        assert_eq!(state.tip().next_seq(), Some(ChainSeq::MIN));
    }
}

#[test]
fn subtree_of_nested_groups_is_transitive() {
    let mut kit = Kit::new(AccountKind::Team);
    // 1 > 2 > 3 > 4, and 5 beside 2 under 1, and 6 alone at the top.
    assert!(kit.make_group(1, None).applied);
    assert!(kit.make_group(2, Some(1)).applied);
    assert!(kit.make_group(3, Some(2)).applied);
    assert!(kit.make_group(4, Some(3)).applied);
    assert!(kit.make_group(5, Some(1)).applied);
    assert!(kit.make_group(6, None).applied);
    let groups = kit.state.groups();
    let ids = |numbers: &[u8]| -> std::collections::BTreeSet<_> {
        numbers.iter().map(|n| group(*n)).collect()
    };
    assert_eq!(groups.subtree(&group(1)), ids(&[1, 2, 3, 4, 5]));
    assert_eq!(groups.subtree(&group(2)), ids(&[2, 3, 4]));
    assert_eq!(groups.subtree(&group(3)), ids(&[3, 4]));
    assert_eq!(groups.subtree(&group(4)), ids(&[4]));
    assert_eq!(groups.subtree(&group(5)), ids(&[5]));
    assert_eq!(groups.subtree(&group(6)), ids(&[6]));
    // Transitive: 4 is in the subtree of 1 through 3 and 2; a sibling's subtree is not.
    assert!(groups.is_in_subtree(&group(4), &group(1)));
    assert!(groups.is_in_subtree(&group(4), &group(2)));
    assert!(groups.is_in_subtree(&group(4), &group(4)));
    assert!(!groups.is_in_subtree(&group(4), &group(5)));
    assert!(!groups.is_in_subtree(&group(1), &group(4)), "never upwards");
    assert!(!groups.is_in_subtree(&group(6), &group(1)));
    // An unknown group is in no subtree, and the subtree of an unknown group is empty.
    assert!(!groups.is_in_subtree(&group(99), &group(1)));
    assert!(!groups.is_in_subtree(&group(1), &group(99)));
    assert!(groups.subtree(&group(99)).is_empty());
    // Depth and the path from the top.
    assert_eq!(groups.depth_of(&group(1)), Some(0));
    assert_eq!(groups.depth_of(&group(4)), Some(3));
    assert_eq!(groups.depth_of(&group(99)), None);
    assert_eq!(
        groups.path_from_top(&group(4)),
        Some(vec![group(1), group(2), group(3), group(4)])
    );
    assert_eq!(groups.path_from_top(&group(6)), Some(vec![group(6)]));
    assert_eq!(groups.height_below(&group(1)), Some(3));
    assert_eq!(groups.height_below(&group(3)), Some(1));
    assert_eq!(groups.height_below(&group(4)), Some(0));
    assert_eq!(groups.height_below(&group(99)), None);
}

#[test]
fn a_member_is_in_the_subtree_through_any_of_its_groups() {
    let mut kit = Kit::new(AccountKind::Team);
    assert!(kit.make_group(1, None).applied);
    assert!(kit.make_group(2, Some(1)).applied);
    assert!(kit.make_group(3, Some(2)).applied);
    assert!(kit.make_group(4, None).applied);
    kit.enrol(2, Role::Member, &[3]);
    kit.enrol(3, Role::Member, &[4]);
    kit.enrol(4, Role::Member, &[3, 4]);
    kit.enrol(5, Role::Member, &[]);
    let groups = kit.state.groups();
    assert!(groups.member_in_subtree(&member(2), &group(1)));
    assert!(groups.member_in_subtree(&member(2), &group(2)));
    assert!(groups.member_in_subtree(&member(2), &group(3)));
    assert!(!groups.member_in_subtree(&member(2), &group(4)));
    assert!(!groups.member_in_subtree(&member(3), &group(1)));
    assert!(groups.member_in_subtree(&member(4), &group(1)));
    assert!(groups.member_in_subtree(&member(4), &group(4)));
    assert!(
        !groups.member_in_subtree(&member(5), &group(1)),
        "no group: no scope"
    );
    assert!(!groups.member_in_subtree(&member(99), &group(1)));
}

#[test]
fn a_damaged_tree_grants_no_scope_and_has_no_depth() {
    // A tree no entry can make: a chain longer than MAX_GROUP_DEPTH, and a cycle.
    let mut groups = Groups::new();
    for n in 0..=(MAX_GROUP_DEPTH as u8 + 3) {
        assert!(groups.insert(Group {
            id: group(n),
            parent: if n == 0 { None } else { Some(group(n - 1)) },
            name_sealed: None,
            log_seq: ChainSeq::MIN,
        }));
    }
    let deepest = group(MAX_GROUP_DEPTH as u8 + 3);
    assert_eq!(groups.depth_of(&deepest), None, "deeper than the bound");
    assert!(
        !groups.is_in_subtree(&deepest, &group(0)),
        "too deep: no scope"
    );
    assert_eq!(groups.path_from_top(&deepest), None);
    // The bound itself is fine.
    assert_eq!(
        groups.depth_of(&group(MAX_GROUP_DEPTH as u8)),
        Some(MAX_GROUP_DEPTH)
    );
    assert!(groups.is_in_subtree(&group(MAX_GROUP_DEPTH as u8), &group(0)));
    // A cycle: 100 under 101 under 100.
    assert!(groups.insert(Group {
        id: group(100),
        parent: Some(group(101)),
        name_sealed: None,
        log_seq: ChainSeq::MIN,
    }));
    assert!(groups.insert(Group {
        id: group(101),
        parent: Some(group(100)),
        name_sealed: None,
        log_seq: ChainSeq::MIN,
    }));
    assert_eq!(groups.depth_of(&group(100)), None);
    assert!(!groups.is_in_subtree(&group(100), &group(0)));
    assert_eq!(groups.path_from_top(&group(101)), None);
}

#[test]
fn a_group_is_inserted_once_and_a_membership_is_kept_in_both_directions() {
    let mut groups = Groups::new();
    let row = Group {
        id: group(1),
        parent: None,
        name_sealed: None,
        log_seq: ChainSeq::MIN,
    };
    assert!(groups.insert(row.clone()));
    assert!(!groups.insert(row), "an id is taken once");
    assert!(groups.add_member(&group(1), member(2)));
    assert!(
        !groups.add_member(&group(9), member(2)),
        "an unknown group takes no member"
    );
    assert_eq!(groups.members_of(&group(1)).map(|set| set.len()), Some(1));
    assert_eq!(groups.groups_of(&member(2)).map(|set| set.len()), Some(1));
    assert!(groups.remove_member(&group(1), &member(2)));
    assert_eq!(groups.members_of(&group(1)).map(|set| set.len()), Some(0));
    assert_eq!(
        groups.groups_of(&member(2)),
        None,
        "an empty set is no entry"
    );
    assert!(!groups.remove_member(&group(9), &member(2)));
    assert!(groups.add_member(&group(1), member(3)));
    assert_eq!(groups.remove_all_memberships(&member(3)), vec![group(1)]);
    assert!(groups.remove_all_memberships(&member(3)).is_empty());
    assert!(groups.membership_pairs().is_empty());
}

#[test]
fn the_effective_policy_of_a_group_is_clamped_to_every_level_above() {
    let mut kit = Kit::new(AccountKind::Team);
    assert!(kit.make_group(1, None).applied);
    assert!(kit.make_group(2, Some(1)).applied);
    // The group 1 tightens the credential lifetime to 6 hours; the group 2 asks for 10 hours.
    let tight = PolicyPatch {
        credential_ttl: Some(Span::hours(6)),
        ..PolicyPatch::none()
    };
    assert!(
        kit.step(
            1,
            1,
            Op::PolicySet {
                scope: PolicyScope::Group(group(1)),
                patch: tight,
            },
        )
        .applied
    );
    let state = &kit.state;
    assert_eq!(
        state
            .effective_policy_of(&group(1))
            .map(|p| p.credential_ttl),
        Some(Span::hours(6))
    );
    assert_eq!(
        state
            .effective_policy_of(&group(2))
            .map(|p| p.credential_ttl),
        Some(Span::hours(6)),
        "a subgroup inherits"
    );
    assert_eq!(
        state.policy().credential_ttl,
        Span::hours(12),
        "the account's own is unchanged"
    );
    assert_eq!(state.effective_policy_of(&group(99)), None);
    assert_eq!(
        state.group_patch(&group(1)).map(|p| p.sets().len()),
        Some(1)
    );
    assert_eq!(state.group_patch(&group(2)), None);
}

#[test]
fn device_states_say_whether_a_device_may_sign_at_a_stamp() {
    let hold = DeviceState::Pending {
        hold_until: at(100),
    };
    assert!(!hold.is_active_at(at(99)));
    assert!(hold.is_active_at(at(100)), "the hold ends AT hold_until");
    assert!(hold.is_active_at(at(101)));
    assert!(DeviceState::Active.is_active_at(at(0)));
    assert!(!DeviceState::Revoked.is_active_at(at(0)));
    assert!(!DeviceState::Revoked.is_active_at(at(u64::MAX)));
    assert_eq!(hold.word(), "pending");
    assert_eq!(DeviceState::Active.word(), "active");
    assert_eq!(DeviceState::Revoked.word(), "revoked");
}

#[test]
fn device_key_finds_every_state_and_nothing_for_an_unknown_device() {
    let kit = rich_business();
    let state = &kit.state;
    // device(1) Active, device(8) Pending, device(3) Revoked, device(6) revoked with its member.
    assert_eq!(state.device_key(device(1)), Some(&keys(101).sign));
    assert_eq!(state.device_key(device(8)), Some(&keys(8).sign));
    assert_eq!(state.device_key(device(3)), Some(&keys(53).sign));
    assert_eq!(state.device_key(device(99)), None);
    assert!(state.device_is_active_at(&device(1), at(0)));
    assert!(
        !state.device_is_active_at(&device(3), at(u64::MAX)),
        "revoked"
    );
    assert!(
        !state.device_is_active_at(&device(99), at(u64::MAX)),
        "unknown"
    );
    let hold_until = match state.device(&device(8)).map(|d| d.state) {
        Some(DeviceState::Pending { hold_until }) => hold_until,
        other => panic!("device 8 must be pending, was {other:?}"),
    };
    assert!(!state.device_is_active_at(&device(8), at(hold_until.as_secs() - 1)));
    assert!(state.device_is_active_at(&device(8), hold_until));
    assert_eq!(state.devices_of(&member(2)).count(), 2);
    assert_eq!(state.devices_of(&member(99)).count(), 0);
}

#[test]
fn the_active_owners_are_counted_without_the_one_excluded() {
    let mut kit = Kit::new(AccountKind::Team);
    assert_eq!(kit.state.active_owner_count(None), 1);
    assert_eq!(kit.state.active_owner_count(Some(&member(1))), 0);
    kit.enrol(2, Role::Member, &[]);
    assert!(
        kit.step(
            1,
            1,
            Op::MemberRole {
                member: member(2),
                role: Role::Owner,
                admin_scope: None,
            },
        )
        .applied
    );
    assert_eq!(kit.state.active_owner_count(None), 2);
    assert_eq!(kit.state.active_owner_count(Some(&member(1))), 1);
    // A suspended owner is no active owner.
    assert!(
        kit.step(1, 1, Op::MemberSuspend { member: member(2) })
            .applied
    );
    assert_eq!(kit.state.active_owner_count(None), 1);
    assert_eq!(
        kit.state.member(&member(2)).map(|m| m.state),
        Some(MemberState::Suspended)
    );
}

#[test]
fn the_state_after_a_rejected_op_is_the_state_before() {
    // `apply` does not re-decide the permission, but an op that does not fit the state changes
    // nothing at all (the witness of an unknown subject, a taken id, a second genesis).
    let mut kit = rich_business();
    let before = kit.state.clone();
    let hash_before = before.state_hash();
    let misfits = [
        // member(6) was removed: it has no role to change, nothing to manage, no group to join.
        Op::MemberRole {
            member: member(6),
            role: Role::Admin,
            admin_scope: None,
        },
        Op::MemberManaged {
            member: member(6),
            managed: true,
        },
        Op::GroupMember {
            group: group(12),
            member: member(6),
            present: true,
        },
        Op::MemberRole {
            member: member(77),
            role: Role::Admin,
            admin_scope: None,
        },
        Op::MemberSuspend { member: member(77) },
        Op::MemberSuspend { member: member(5) },
        Op::MemberRemove { member: member(6) },
        Op::DeviceRemove { device: device(77) },
        Op::DeviceRemove { device: device(3) },
        Op::GroupCreate {
            group: group(11),
            parent: None,
        },
        Op::GroupCreate {
            group: group(70),
            parent: Some(group(77)),
        },
        Op::GroupMove {
            group: group(11),
            new_parent: group(12),
        },
        Op::GroupMove {
            group: group(12),
            new_parent: group(77),
        },
        Op::GroupMember {
            group: group(77),
            member: member(2),
            present: true,
        },
        Op::GroupMember {
            group: group(12),
            member: member(77),
            present: true,
        },
        Op::MemberInvite {
            member: member(2),
            kind: azlin_seams::MemberKind::Person,
            role: Role::Member,
            invite_hash: hash(2),
            groups: Default::default(),
            app: None,
        },
        Op::MemberJoin {
            member: member(2),
            device: device(60),
            member_keys: keys(60),
            device_keys: keys(61),
            secret: crate::ops::InviteSecret([2; 32]),
        },
        Op::Delegate {
            key: key(77),
            ops: Default::default(),
            expires: None,
        },
        Op::DeviceAdd {
            member: member(77),
            device: device(61),
            keys: keys(61),
        },
        Op::DeviceAdd {
            member: member(2),
            device: device(1),
            keys: keys(61),
        },
    ];
    for op in misfits {
        let effects = kit.step(1, 1, op.clone());
        assert!(!effects.applied, "{op:?}");
        assert!(effects.registry.is_empty() && effects.notices.is_empty());
        assert!(!effects.credentials_invalidated);
        assert_eq!(kit.state, before, "{op:?}");
        assert_eq!(kit.state.state_hash(), hash_before);
    }
}
