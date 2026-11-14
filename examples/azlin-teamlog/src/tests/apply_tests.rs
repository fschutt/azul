//! Tests of `apply` (lane W1-S17c1): it is deterministic, it makes the rows and the notices of
//! planning/architecture.md section 5.9 for every op, and an entry that does not fit the state it is
//! applied to changes nothing. The entries are made without the permission check (see `kit.rs`): what
//! `admit` decides is `admit_tests.rs`'s.

use std::collections::BTreeSet;

use azlin_policy::{AdminQuorum, PolicyPatch};
use azlin_seams::{
    AccountKind, ByteCount, ChainSeq, Hash32, MemberKind, PolicyEpoch, RestoreEpoch, Span,
};

use super::kit::{at, by, device, group, hash, key, keys, member, Kit};
use crate::apply::{apply, Actor, Admitted};
use crate::devices::{DeviceState, EnrolledBy};
use crate::effects::{
    Audience, Effects, Notice, RegistryDelta, RevocationReason, RevocationSubject, Subject,
};
use crate::members::{AppSpec, Invite, MemberState};
use crate::ops::{
    AppName, EntitlementState, EntitlementVersion, EscrowReason, InviteSecret, Op, OpName,
    PolicyScope,
};
use crate::roles::Role;
use crate::state::{MAX_DELEGATIONS, MAX_PENDING_ESCROW_OPENS};

/// One entry of a script: who acted, which members cosigned, the op and its sealed field.
struct Step {
    actor: Actor,
    cosigners: Vec<azlin_seams::MemberId>,
    op: Op,
    sealed: Option<Vec<u8>>,
}

fn step(actor: Actor, op: Op) -> Step {
    Step {
        actor,
        cosigners: Vec::new(),
        op,
        sealed: None,
    }
}

/// A script that uses every op of the table once (the genesis is made by `Kit::with_genesis`), on a
/// business account: the owner is member 1.
fn script() -> Vec<Step> {
    let mut suspend_only = BTreeSet::new();
    suspend_only.insert(OpName::MemberSuspend);
    let mut groups_of_m2 = BTreeSet::new();
    groups_of_m2.insert(group(1));
    let mut holders = BTreeSet::new();
    holders.insert(member(1));
    holders.insert(member(2));
    vec![
        Step {
            sealed: Some(vec![1]),
            ..step(
                by(1, 1),
                Op::GroupCreate {
                    group: group(1),
                    parent: None,
                },
            )
        },
        step(
            by(1, 1),
            Op::GroupCreate {
                group: group(2),
                parent: Some(group(1)),
            },
        ),
        step(
            by(1, 1),
            Op::GroupCreate {
                group: group(3),
                parent: None,
            },
        ),
        step(
            by(1, 1),
            Op::MemberInvite {
                member: member(2),
                kind: MemberKind::Person,
                role: Role::Member,
                invite_hash: hash(2),
                groups: groups_of_m2,
                app: None,
            },
        ),
        step(
            by(2, 2),
            Op::MemberJoin {
                member: member(2),
                device: device(2),
                member_keys: keys(2),
                device_keys: keys(52),
                secret: InviteSecret([2; 32]),
            },
        ),
        step(
            by(1, 1),
            Op::MemberRole {
                member: member(2),
                role: Role::Admin,
                admin_scope: None,
            },
        ),
        step(
            by(2, 2),
            Op::DeviceAdd {
                member: member(2),
                device: device(7),
                keys: keys(7),
            },
        ),
        step(by(2, 2), Op::DeviceRemove { device: device(7) }),
        step(
            by(1, 1),
            Op::GroupMember {
                group: group(1),
                member: member(2),
                present: false,
            },
        ),
        step(
            by(1, 1),
            Op::GroupMove {
                group: group(2),
                new_parent: group(3),
            },
        ),
        step(
            by(1, 1),
            Op::MemberManaged {
                member: member(2),
                managed: false,
            },
        ),
        step(
            by(1, 1),
            Op::PolicySet {
                scope: PolicyScope::Account,
                patch: PolicyPatch {
                    credential_ttl: Some(Span::minutes(30)),
                    ..PolicyPatch::none()
                },
            },
        ),
        step(
            by(1, 1),
            Op::EscrowSet {
                escrow_pub: [0xE5; 32],
                holders,
            },
        ),
        step(
            by(1, 1),
            Op::AuditKeySet {
                audit_pub: [0xA7; 32],
            },
        ),
        step(
            by(1, 1),
            Op::AdminQuorumSet {
                quorum: AdminQuorum::new(3).expect("a quorum of three"),
            },
        ),
        Step {
            cosigners: vec![member(2)],
            ..step(
                by(1, 1),
                Op::EscrowOpen {
                    member: member(2),
                    reason: EscrowReason::LegalRequest,
                },
            )
        },
        step(
            by(1, 1),
            Op::Delegate {
                key: key(40),
                ops: suspend_only,
                expires: Some(at(9_000_000)),
            },
        ),
        step(
            Actor::Payments,
            Op::Entitlement {
                version: EntitlementVersion(1),
                state: EntitlementState::Active,
                quota: ByteCount::gib(100),
                seats: 25,
            },
        ),
        step(
            by(2, 2),
            Op::App {
                name: AppName::new("notes").expect("valid"),
                body: vec![1, 2],
            },
        ),
        step(by(1, 1), Op::MemberSuspend { member: member(2) }),
        step(by(1, 1), Op::MemberRemove { member: member(2) }),
    ]
}

/// Runs the script on a fresh business account; the effects of every entry, the genesis first.
fn run_script() -> (Kit, Vec<Effects>) {
    let (mut kit, genesis) = Kit::with_genesis(AccountKind::Business);
    let mut all = vec![genesis];
    for one in script() {
        let effects = kit.step_with(one.actor, one.cosigners, one.op, one.sealed);
        assert!(effects.applied);
        all.push(effects);
    }
    (kit, all)
}

#[test]
fn apply_is_deterministic_across_two_replicas() {
    let (first, first_effects) = run_script();
    let (second, second_effects) = run_script();
    assert_eq!(first_effects, second_effects);
    assert_eq!(first.state, second.state);
    assert_eq!(first.state.state_hash(), second.state.state_hash());
    assert_eq!(first.state.view(), second.state.view());
    // And step by step, not only at the end.
    let (mut a, _) = Kit::with_genesis(AccountKind::Business);
    let (mut b, _) = Kit::with_genesis(AccountKind::Business);
    for (one, two) in script().into_iter().zip(script()) {
        a.step_with(one.actor, one.cosigners, one.op, one.sealed);
        b.step_with(two.actor, two.cosigners, two.op, two.sealed);
        assert_eq!(a.state.state_hash(), b.state.state_hash());
    }
    // A different order is a different log (and here the first entry's parent is missing): the heads
    // differ.
    let (mut c, _) = Kit::with_genesis(AccountKind::Business);
    for one in script().into_iter().take(3).rev() {
        c.step_with(one.actor, one.cosigners, one.op, one.sealed);
    }
    let (mut d, _) = Kit::with_genesis(AccountKind::Business);
    for one in script().into_iter().take(3) {
        d.step_with(one.actor, one.cosigners, one.op, one.sealed);
    }
    assert_ne!(c.state.head(), d.state.head());
}

#[test]
fn apply_emits_log_appended_to_every_admin_and_the_subjects_devices() {
    // The notice of each entry, typed here from planning/architecture.md section 5.9 ("Every entry
    // emits `log_appended` (op, subject) ... to every admin and to the subject's devices"): a member
    // or a device is a person with devices, so admins AND those devices; every other subject has no
    // devices, so every admin.
    let m1 = Subject::Member(member(1));
    let m2 = Subject::Member(member(2));
    let expected: Vec<(OpName, Subject, Audience)> = vec![
        (OpName::Genesis, m1, Audience::Both),
        (
            OpName::GroupCreate,
            Subject::Group(group(1)),
            Audience::AllAdmins,
        ),
        (
            OpName::GroupCreate,
            Subject::Group(group(2)),
            Audience::AllAdmins,
        ),
        (
            OpName::GroupCreate,
            Subject::Group(group(3)),
            Audience::AllAdmins,
        ),
        (OpName::MemberInvite, m2.clone(), Audience::Both),
        (OpName::MemberJoin, m2.clone(), Audience::Both),
        (OpName::MemberRole, m2.clone(), Audience::Both),
        (
            OpName::DeviceAdd,
            Subject::Device(device(7)),
            Audience::Both,
        ),
        (
            OpName::DeviceRemove,
            Subject::Device(device(7)),
            Audience::Both,
        ),
        (
            OpName::GroupMember,
            Subject::Group(group(1)),
            Audience::AllAdmins,
        ),
        (
            OpName::GroupMove,
            Subject::Group(group(2)),
            Audience::AllAdmins,
        ),
        (OpName::MemberManaged, m2.clone(), Audience::Both),
        (OpName::PolicySet, Subject::None, Audience::AllAdmins),
        (OpName::EscrowSet, Subject::None, Audience::AllAdmins),
        (OpName::AuditKeySet, Subject::None, Audience::AllAdmins),
        (OpName::AdminQuorumSet, Subject::None, Audience::AllAdmins),
        (OpName::EscrowOpen, m2.clone(), Audience::Both),
        (OpName::Delegate, Subject::Key(key(40)), Audience::AllAdmins),
        (OpName::Entitlement, Subject::None, Audience::AllAdmins),
        (
            OpName::App,
            Subject::App(AppName::new("notes").expect("valid")),
            Audience::AllAdmins,
        ),
        (OpName::MemberSuspend, m2.clone(), Audience::Both),
        (OpName::MemberRemove, m2, Audience::Both),
    ];
    let (_, effects) = run_script();
    assert_eq!(effects.len(), expected.len());
    for (effects, (op, subject, audience)) in effects.iter().zip(expected.iter()) {
        // One notice, and only one, for every applied entry.
        assert_eq!(
            effects.notices,
            vec![Notice {
                op: *op,
                subject: subject.clone(),
                audience: *audience,
            }],
            "{op}"
        );
        assert!(effects.applied);
    }
    // Every op of the table is among them.
    let seen: BTreeSet<OpName> = expected.iter().map(|(op, _, _)| *op).collect();
    for op in OpName::ALL {
        assert!(seen.contains(&op), "{op} has a case");
    }
}

#[test]
fn apply_member_invite_creates_invited_member_with_chosen_id() {
    let mut kit = Kit::new(AccountKind::Business);
    assert!(kit.make_group(1, None).applied);
    let stamp = kit.next_stamp();
    let effects = kit.invite(7, Role::Guest, &[1]);
    assert!(effects.applied);
    let row = kit
        .state
        .member(&member(7))
        .expect("the invited member")
        .clone();
    assert_eq!(row.id, member(7), "the id the inviter chose");
    assert_eq!(row.state, MemberState::Invited);
    assert_eq!(row.role, Role::Guest);
    assert_eq!(row.kind, MemberKind::Person);
    assert_eq!(row.admin_scope, None);
    assert!(row.managed, "the business default is managed");
    assert_eq!(row.keys, None, "no keys before the join");
    assert_eq!(
        row.invite,
        Some(Invite {
            hash: hash(7),
            inviter: Some(member(1)),
        })
    );
    assert_eq!(row.app, None);
    assert_eq!(row.added_at, stamp);
    assert_eq!(row.removed_at, None);
    assert_eq!(
        kit.state.devices_of(&member(7)).count(),
        0,
        "no device before the join"
    );
    let groups: BTreeSet<_> = kit
        .state
        .groups()
        .groups_of(&member(7))
        .expect("in a group")
        .iter()
        .copied()
        .collect();
    let want: BTreeSet<_> = [group(1)].into_iter().collect();
    assert_eq!(groups, want);
    assert_eq!(
        effects.registry,
        vec![
            RegistryDelta::Member(row),
            RegistryDelta::GroupMember {
                group: group(1),
                member: member(7),
                present: true,
            },
        ]
    );
    // The same id is not invited twice, and an unknown group is no group.
    assert!(!kit.invite(7, Role::Member, &[]).applied);
    assert!(!kit.invite(8, Role::Member, &[99]).applied);
    assert!(kit.state.member(&member(8)).is_none(), "nothing half made");
    // A team account's default is private; an app is never managed; a delegate key made the invite
    // (no inviter).
    let mut team = Kit::new(AccountKind::Team);
    assert!(team.invite(2, Role::Member, &[]).applied);
    assert!(!team.state.member(&member(2)).expect("invited").managed);
    let app = Op::MemberInvite {
        member: member(3),
        kind: MemberKind::App,
        role: Role::Member,
        invite_hash: hash(3),
        groups: BTreeSet::new(),
        app: Some(AppSpec {
            scopes: BTreeSet::new(),
            expires_at: None,
        }),
    };
    assert!(kit.step(1, 1, app.clone()).applied);
    assert!(!kit.state.member(&member(3)).expect("invited").managed);
    let by_delegate = Op::MemberInvite {
        member: member(4),
        kind: MemberKind::Person,
        role: Role::Member,
        invite_hash: hash(4),
        groups: BTreeSet::new(),
        app: None,
    };
    assert!(
        kit.step_with(Actor::Delegate(key(40)), Vec::new(), by_delegate, None)
            .applied
    );
    assert_eq!(
        kit.state.member(&member(4)).expect("invited").invite,
        Some(Invite {
            hash: hash(4),
            inviter: None,
        })
    );
}

#[test]
fn apply_member_join_activates_the_member_and_uses_the_invitation_up() {
    let mut kit = Kit::new(AccountKind::Team);
    assert!(kit.invite(2, Role::Member, &[]).applied);
    let stamp = kit.next_stamp();
    let effects = kit.join(2);
    assert!(effects.applied);
    let row = kit.state.member(&member(2)).expect("joined").clone();
    assert_eq!(row.state, MemberState::Active);
    assert_eq!(row.keys, Some(keys(2)));
    assert_eq!(row.invite, None, "the invitation is used up");
    let first = kit
        .state
        .device(&device(2))
        .expect("the first device")
        .clone();
    assert_eq!(first.member, member(2));
    assert_eq!(first.state, DeviceState::Active);
    assert_eq!(first.enrolled_by, EnrolledBy::Invite);
    assert_eq!(first.keys, keys(52));
    assert_eq!(first.added_at, stamp);
    assert_eq!(
        effects.registry,
        vec![RegistryDelta::Member(row), RegistryDelta::Device(first)]
    );
    // A second join of the same member finds no invitation; a join of an unknown member finds none.
    assert!(!kit.join(2).applied);
    assert!(!kit.join(9).applied);
    // A device id that is taken cannot be joined with.
    assert!(kit.invite(3, Role::Member, &[]).applied);
    let taken = Op::MemberJoin {
        member: member(3),
        device: device(2),
        member_keys: keys(3),
        device_keys: keys(53),
        secret: InviteSecret([3; 32]),
    };
    assert!(!kit.step(3, 2, taken).applied);
    assert_eq!(
        kit.state.member(&member(3)).map(|m| m.state),
        Some(MemberState::Invited)
    );
}

#[test]
fn apply_group_move_updates_subtree_queries() {
    let mut kit = Kit::new(AccountKind::Team);
    assert!(kit.make_group(1, None).applied);
    assert!(kit.make_group(2, Some(1)).applied);
    assert!(kit.make_group(3, Some(2)).applied);
    assert!(kit.make_group(4, None).applied);
    assert!(kit.make_group(5, Some(4)).applied);
    kit.enrol(2, Role::Member, &[3]);
    {
        let groups = kit.state.groups();
        assert!(groups.member_in_subtree(&member(2), &group(1)));
        assert!(!groups.member_in_subtree(&member(2), &group(4)));
        assert!(groups.is_in_subtree(&group(3), &group(1)));
        assert_eq!(groups.subtree(&group(1)).len(), 3);
    }
    // Group 2 (with 3 below it) moves from group 1 to group 5.
    let effects = kit.step(
        1,
        1,
        Op::GroupMove {
            group: group(2),
            new_parent: group(5),
        },
    );
    assert!(effects.applied);
    let groups = kit.state.groups();
    assert!(
        !groups.member_in_subtree(&member(2), &group(1)),
        "no longer under 1"
    );
    assert!(groups.member_in_subtree(&member(2), &group(4)));
    assert!(groups.member_in_subtree(&member(2), &group(5)));
    assert!(
        groups.is_in_subtree(&group(3), &group(5)),
        "the subtree moved with it"
    );
    assert!(!groups.is_in_subtree(&group(3), &group(1)));
    assert_eq!(groups.subtree(&group(1)).len(), 1);
    assert_eq!(groups.subtree(&group(4)).len(), 4);
    assert_eq!(groups.depth_of(&group(3)), Some(3));
    let row = groups.get(&group(2)).expect("the group").clone();
    assert_eq!(row.parent, Some(group(5)));
    assert_eq!(effects.registry, vec![RegistryDelta::Group(row)]);
    // The tree stays a tree: no move into the group's own subtree, none of a top-level group, none to
    // or from an unknown group.
    for op in [
        Op::GroupMove {
            group: group(5),
            new_parent: group(3),
        },
        Op::GroupMove {
            group: group(2),
            new_parent: group(2),
        },
        Op::GroupMove {
            group: group(1),
            new_parent: group(5),
        },
        Op::GroupMove {
            group: group(2),
            new_parent: group(99),
        },
        Op::GroupMove {
            group: group(99),
            new_parent: group(5),
        },
    ] {
        let before = kit.state.clone();
        assert!(!kit.step(1, 1, op.clone()).applied, "{op:?}");
        assert_eq!(kit.state, before);
    }
}

#[test]
fn apply_device_add_by_quorum_is_pending_until_hold_until() {
    let mut kit = Kit::new(AccountKind::Business);
    kit.enrol(2, Role::Member, &[]);
    let stamp = kit.next_stamp();
    // The owner, who is not member 2, adds a device for member 2: the quorum path.
    let effects = kit.step_with(
        by(1, 1),
        vec![member(3)],
        Op::DeviceAdd {
            member: member(2),
            device: device(9),
            keys: keys(9),
        },
        Some(vec![7, 7]),
    );
    assert!(effects.applied);
    let hold_until = stamp.checked_add(Span::hours(48)).expect("no overflow");
    let row = kit
        .state
        .device(&device(9))
        .expect("the new device")
        .clone();
    assert_eq!(row.state, DeviceState::Pending { hold_until });
    assert_eq!(row.enrolled_by, EnrolledBy::Quorum);
    assert_eq!(row.member, member(2));
    assert_eq!(row.keys, keys(9));
    assert_eq!(row.label_sealed, Some(vec![7, 7]));
    assert_eq!(row.added_at, stamp);
    assert_eq!(effects.registry, vec![RegistryDelta::Device(row)]);
    let one_second = Span::secs(1);
    assert!(!kit.state.device_is_active_at(
        &device(9),
        hold_until.checked_sub(one_second).expect("after the epoch")
    ));
    assert!(kit.state.device_is_active_at(&device(9), hold_until));
    // The member's own device: no hold.
    let effects = kit.step(
        2,
        2,
        Op::DeviceAdd {
            member: member(2),
            device: device(10),
            keys: keys(10),
        },
    );
    assert!(effects.applied);
    let own = kit.state.device(&device(10)).expect("the new device");
    assert_eq!(own.state, DeviceState::Active);
    assert_eq!(own.enrolled_by, EnrolledBy::OwnDevice);
    assert_eq!(own.label_sealed, None);
    // A device id is never reused (not even a revoked one's), and an unknown member has no device.
    assert!(
        !kit.step(
            2,
            2,
            Op::DeviceAdd {
                member: member(2),
                device: device(9),
                keys: keys(9),
            },
        )
        .applied
    );
    assert!(
        !kit.step(
            1,
            1,
            Op::DeviceAdd {
                member: member(77),
                device: device(11),
                keys: keys(11),
            },
        )
        .applied
    );
    // A hold that would run past the end of time is not applied.
    kit.set_clock(u64::MAX - 100);
    let before = kit.state.clone();
    assert!(
        !kit.step(
            1,
            1,
            Op::DeviceAdd {
                member: member(2),
                device: device(12),
                keys: keys(12),
            },
        )
        .applied
    );
    assert_eq!(kit.state, before);
}

#[test]
fn apply_device_remove_revokes_and_notices() {
    let mut kit = Kit::new(AccountKind::Team);
    kit.enrol(2, Role::Member, &[]);
    let stamp = kit.next_stamp();
    let effects = kit.step(1, 1, Op::DeviceRemove { device: device(2) });
    assert!(effects.applied);
    let row = kit.state.device(&device(2)).expect("still a row").clone();
    assert_eq!(row.state, DeviceState::Revoked);
    assert_eq!(row.revoked_at, Some(stamp));
    assert!(!kit.state.device_is_active_at(&device(2), at(u64::MAX)));
    assert_eq!(
        effects.registry,
        vec![
            RegistryDelta::Device(row),
            RegistryDelta::Revocation {
                subject: RevocationSubject::Device(device(2)),
                at: stamp,
                reason: RevocationReason::DeviceRemoved,
            },
        ]
    );
    assert_eq!(
        effects.notices,
        vec![Notice {
            op: OpName::DeviceRemove,
            subject: Subject::Device(device(2)),
            audience: Audience::Both,
        }]
    );
    assert!(!effects.credentials_invalidated);
    // The member is still a member; a second removal changes nothing.
    assert_eq!(
        kit.state.member(&member(2)).map(|m| m.state),
        Some(MemberState::Active)
    );
    assert!(
        !kit.step(1, 1, Op::DeviceRemove { device: device(2) })
            .applied
    );
}

#[test]
fn apply_member_remove_revokes_devices_and_leaves_groups() {
    let mut kit = Kit::new(AccountKind::Team);
    assert!(kit.make_group(1, None).applied);
    assert!(kit.make_group(2, None).applied);
    kit.enrol(2, Role::Member, &[1, 2]);
    kit.enrol(3, Role::Member, &[1]);
    assert!(
        kit.step(
            2,
            2,
            Op::DeviceAdd {
                member: member(2),
                device: device(8),
                keys: keys(8),
            },
        )
        .applied
    );
    let stamp = kit.next_stamp();
    let effects = kit.step(1, 1, Op::MemberRemove { member: member(2) });
    assert!(effects.applied);
    let removed = kit.state.member(&member(2)).expect("a row").clone();
    assert_eq!(removed.state, MemberState::Removed);
    assert_eq!(removed.removed_at, Some(stamp));
    assert_eq!(removed.invite, None);
    for n in [2, 8] {
        let row = kit.state.device(&device(n)).expect("a device row");
        assert_eq!(row.state, DeviceState::Revoked, "device {n}");
        assert_eq!(row.revoked_at, Some(stamp));
    }
    assert_eq!(
        kit.state.groups().groups_of(&member(2)),
        None,
        "in no group"
    );
    assert!(
        kit.state.groups().groups_of(&member(3)).is_some(),
        "others stay"
    );
    assert_eq!(
        kit.state.member(&member(3)).map(|m| m.state),
        Some(MemberState::Active)
    );
    let device_rows: Vec<RegistryDelta> = [2, 8]
        .iter()
        .map(|n| RegistryDelta::Device(kit.state.device(&device(*n)).expect("row").clone()))
        .collect();
    let mut expected = vec![RegistryDelta::Member(removed)];
    expected.extend(device_rows);
    expected.push(RegistryDelta::GroupMember {
        group: group(1),
        member: member(2),
        present: false,
    });
    expected.push(RegistryDelta::GroupMember {
        group: group(2),
        member: member(2),
        present: false,
    });
    expected.push(RegistryDelta::Revocation {
        subject: RevocationSubject::Member(member(2)),
        at: stamp,
        reason: RevocationReason::MemberRemoved,
    });
    assert_eq!(effects.registry, expected);
    // Removing twice, or removing an unknown member, changes nothing.
    assert!(
        !kit.step(1, 1, Op::MemberRemove { member: member(2) })
            .applied
    );
    assert!(
        !kit.step(1, 1, Op::MemberRemove { member: member(77) })
            .applied
    );
    // An invited member who never joined goes too, with its invitation.
    assert!(kit.invite(4, Role::Member, &[1]).applied);
    assert!(
        kit.step(1, 1, Op::MemberRemove { member: member(4) })
            .applied
    );
    let never_joined = kit.state.member(&member(4)).expect("a row");
    assert_eq!(never_joined.state, MemberState::Removed);
    assert_eq!(never_joined.invite, None);
}

#[test]
fn apply_member_role_and_state_changes_touch_only_their_member() {
    let mut kit = Kit::new(AccountKind::Team);
    assert!(kit.make_group(1, None).applied);
    kit.enrol(2, Role::Member, &[]);
    kit.enrol(3, Role::Member, &[]);
    let effects = kit.step(
        1,
        1,
        Op::MemberRole {
            member: member(2),
            role: Role::GroupAdmin,
            admin_scope: Some(group(1)),
        },
    );
    assert!(effects.applied);
    let row = kit.state.member(&member(2)).expect("row").clone();
    assert_eq!(row.role, Role::GroupAdmin);
    assert_eq!(row.admin_scope, Some(group(1)));
    assert_eq!(effects.registry, vec![RegistryDelta::Member(row.clone())]);
    assert_eq!(
        row.log_seq,
        ChainSeq::new(kit.state.head().expect("a head").seq.get())
    );
    // A role without a scope drops the scope, whatever the entry says.
    assert!(
        kit.step(
            1,
            1,
            Op::MemberRole {
                member: member(2),
                role: Role::Member,
                admin_scope: Some(group(1)),
            },
        )
        .applied
    );
    assert_eq!(
        kit.state.member(&member(2)).map(|m| m.admin_scope),
        Some(None)
    );
    // Suspend, and managed: only the subject's row changes.
    let other = kit.state.member(&member(3)).expect("row").clone();
    assert!(
        kit.step(1, 1, Op::MemberSuspend { member: member(2) })
            .applied
    );
    assert_eq!(
        kit.state.member(&member(2)).map(|m| m.state),
        Some(MemberState::Suspended)
    );
    assert!(
        !kit.step(1, 1, Op::MemberSuspend { member: member(2) })
            .applied,
        "not twice"
    );
    assert!(
        kit.step(
            1,
            1,
            Op::MemberManaged {
                member: member(3),
                managed: true,
            },
        )
        .applied
    );
    let after = kit.state.member(&member(3)).expect("row");
    assert!(after.managed);
    assert_eq!(after.role, other.role);
    assert_eq!(after.state, other.state);
    assert!(
        !kit.step(
            1,
            1,
            Op::MemberManaged {
                member: member(77),
                managed: true,
            },
        )
        .applied
    );
}

#[test]
fn apply_policy_set_bumps_epoch_and_flags_credentials() {
    let mut kit = Kit::new(AccountKind::Team);
    assert!(kit.make_group(1, None).applied);
    assert!(kit.make_group(2, Some(1)).applied);
    assert_eq!(kit.state.policy_epoch(), PolicyEpoch::new(1));
    let set = |ttl: Span| PolicyPatch {
        credential_ttl: Some(ttl),
        ..PolicyPatch::none()
    };
    // A change of the account's policy: the epoch moves and every older credential is invalid.
    let effects = kit.step(
        1,
        1,
        Op::PolicySet {
            scope: PolicyScope::Account,
            patch: set(Span::minutes(30)),
        },
    );
    assert!(effects.applied);
    assert!(effects.credentials_invalidated);
    assert_eq!(kit.state.policy_epoch(), PolicyEpoch::new(2));
    assert_eq!(kit.state.policy().credential_ttl, Span::minutes(30));
    assert_eq!(
        effects.registry,
        vec![RegistryDelta::AccountPolicy {
            policy: kit.state.policy().clone(),
            epoch: PolicyEpoch::new(2),
        }]
    );
    // A patch that sets what is already there, and one that sets nothing: applied, no new epoch.
    for patch in [set(Span::minutes(30)), PolicyPatch::none()] {
        let effects = kit.step(
            1,
            1,
            Op::PolicySet {
                scope: PolicyScope::Account,
                patch,
            },
        );
        assert!(effects.applied);
        assert!(!effects.credentials_invalidated);
        assert_eq!(kit.state.policy_epoch(), PolicyEpoch::new(2));
    }
    // A group that tightens: its patch is stored and the epoch moves.
    let tight = set(Span::minutes(10));
    let effects = kit.step(
        1,
        1,
        Op::PolicySet {
            scope: PolicyScope::Group(group(1)),
            patch: tight.clone(),
        },
    );
    assert!(effects.applied);
    assert!(effects.credentials_invalidated);
    assert_eq!(kit.state.policy_epoch(), PolicyEpoch::new(3));
    assert_eq!(kit.state.group_patch(&group(1)), Some(&tight));
    assert_eq!(
        effects.registry,
        vec![
            RegistryDelta::GroupPolicy {
                group: group(1),
                patch: Some(tight.clone()),
            },
            RegistryDelta::PolicyEpoch(PolicyEpoch::new(3)),
        ]
    );
    assert_eq!(
        kit.state.policy().credential_ttl,
        Span::minutes(30),
        "the account's own is unchanged"
    );
    // A subgroup asks for more than its parent allows: the patch is kept, the clamp holds the effective
    // policy where it was, and the epoch does not move.
    let loose = set(Span::minutes(20));
    let effects = kit.step(
        1,
        1,
        Op::PolicySet {
            scope: PolicyScope::Group(group(2)),
            patch: loose.clone(),
        },
    );
    assert!(effects.applied);
    assert!(!effects.credentials_invalidated);
    assert_eq!(kit.state.policy_epoch(), PolicyEpoch::new(3));
    assert_eq!(kit.state.group_patch(&group(2)), Some(&loose));
    assert_eq!(
        effects.registry,
        vec![RegistryDelta::GroupPolicy {
            group: group(2),
            patch: Some(loose),
        }]
    );
    assert_eq!(
        kit.state
            .effective_policy_of(&group(2))
            .map(|p| p.credential_ttl),
        Some(Span::minutes(10))
    );
    // An empty patch clears a group's patch: the group's effective policy is the account's again.
    let effects = kit.step(
        1,
        1,
        Op::PolicySet {
            scope: PolicyScope::Group(group(1)),
            patch: PolicyPatch::none(),
        },
    );
    assert!(effects.applied);
    assert!(effects.credentials_invalidated);
    assert_eq!(kit.state.group_patch(&group(1)), None);
    assert_eq!(
        effects.registry[0],
        RegistryDelta::GroupPolicy {
            group: group(1),
            patch: None,
        }
    );
    assert_eq!(kit.state.policy_epoch(), PolicyEpoch::new(4));
    // A group that does not exist has no policy.
    assert!(
        !kit.step(
            1,
            1,
            Op::PolicySet {
                scope: PolicyScope::Group(group(99)),
                patch: set(Span::minutes(1)),
            },
        )
        .applied
    );
    // `admin_quorum_set` is a change of the account's policy too.
    let effects = kit.step(
        1,
        1,
        Op::AdminQuorumSet {
            quorum: AdminQuorum::TWO,
        },
    );
    assert!(effects.applied);
    assert!(effects.credentials_invalidated);
    assert_eq!(kit.state.policy().admin_quorum, AdminQuorum::TWO);
    assert_eq!(kit.state.policy_epoch(), PolicyEpoch::new(5));
    let effects = kit.step(
        1,
        1,
        Op::AdminQuorumSet {
            quorum: AdminQuorum::TWO,
        },
    );
    assert!(
        effects.applied && !effects.credentials_invalidated,
        "the same quorum: no new epoch"
    );
}

#[test]
fn apply_does_not_wrap_the_policy_epoch() {
    let mut kit = Kit::new(AccountKind::Team);
    kit.state.policy_epoch = PolicyEpoch::new(u32::MAX);
    let before = kit.state.clone();
    let change = Op::PolicySet {
        scope: PolicyScope::Account,
        patch: PolicyPatch {
            credential_ttl: Some(Span::minutes(5)),
            ..PolicyPatch::none()
        },
    };
    assert!(
        !kit.step(1, 1, change).applied,
        "a change at the largest epoch is not applied"
    );
    assert_eq!(kit.state, before);
    let effects = kit.step(
        1,
        1,
        Op::PolicySet {
            scope: PolicyScope::Account,
            patch: PolicyPatch::none(),
        },
    );
    assert!(
        effects.applied && !effects.credentials_invalidated,
        "a no-op still counts"
    );
    assert_eq!(kit.state.policy_epoch(), PolicyEpoch::new(u32::MAX));
    assert!(
        !kit.step(
            1,
            1,
            Op::AdminQuorumSet {
                quorum: AdminQuorum::new(5).expect("five"),
            },
        )
        .applied
    );
}

#[test]
fn apply_escrow_open_records_the_wait_for_a_member_and_none_for_a_removed_one() {
    let mut kit = Kit::new(AccountKind::Business);
    kit.enrol(2, Role::Member, &[]);
    kit.enrol(3, Role::Member, &[]);
    assert!(kit.invite(4, Role::Member, &[]).applied);
    let stamp = kit.next_stamp();
    let effects = kit.step_with(
        by(1, 1),
        vec![member(3), member(1)],
        Op::EscrowOpen {
            member: member(2),
            reason: EscrowReason::LeaverHandover,
        },
        None,
    );
    assert!(effects.applied);
    let seq = kit.state.head().expect("a head").seq;
    let open = kit
        .state
        .pending_escrow_opens()
        .next()
        .expect("an opening")
        .clone();
    assert_eq!(open.seq, seq);
    assert_eq!(open.member, member(2));
    assert_eq!(open.reason, EscrowReason::LeaverHandover);
    assert_eq!(open.requested_at, stamp);
    assert_eq!(open.ready_at, stamp.checked_add(Span::hours(72)));
    assert_eq!(
        open.signers,
        vec![member(1), member(3)],
        "the signer and the cosigners, in order, once"
    );
    assert_eq!(effects.registry, vec![RegistryDelta::EscrowOpen(open)]);
    // A suspended member still waits.
    assert!(
        kit.step(1, 1, Op::MemberSuspend { member: member(3) })
            .applied
    );
    let stamp = kit.next_stamp();
    assert!(
        kit.step(
            1,
            1,
            Op::EscrowOpen {
                member: member(3),
                reason: EscrowReason::Other,
            },
        )
        .applied
    );
    let open = kit.state.pending_escrow_opens().last().expect("an opening");
    assert_eq!(open.ready_at, stamp.checked_add(Span::hours(72)));
    // A member who was removed has no wait.
    assert!(
        kit.step(1, 1, Op::MemberRemove { member: member(2) })
            .applied
    );
    assert!(
        kit.step(
            1,
            1,
            Op::EscrowOpen {
                member: member(2),
                reason: EscrowReason::LegalRequest,
            },
        )
        .applied
    );
    let open = kit.state.pending_escrow_opens().last().expect("an opening");
    assert_eq!(open.member, member(2));
    assert_eq!(
        open.ready_at, None,
        "no wait for a member already offboarded"
    );
    // An invited member and an unknown one have nothing to open.
    for subject in [4, 77] {
        assert!(
            !kit.step(
                1,
                1,
                Op::EscrowOpen {
                    member: member(subject),
                    reason: EscrowReason::Other,
                },
            )
            .applied
        );
    }
}

#[test]
fn apply_escrow_open_is_bounded() {
    let mut kit = Kit::new(AccountKind::Business);
    kit.enrol(2, Role::Member, &[]);
    for _ in 0..MAX_PENDING_ESCROW_OPENS {
        assert!(
            kit.step(
                1,
                1,
                Op::EscrowOpen {
                    member: member(2),
                    reason: EscrowReason::Other,
                },
            )
            .applied
        );
    }
    assert_eq!(
        kit.state.pending_escrow_opens().count(),
        MAX_PENDING_ESCROW_OPENS
    );
    let before = kit.state.clone();
    assert!(
        !kit.step(
            1,
            1,
            Op::EscrowOpen {
                member: member(2),
                reason: EscrowReason::Other,
            },
        )
        .applied
    );
    assert_eq!(kit.state, before);
}

#[test]
fn apply_delegate_sets_replaces_and_revokes() {
    let mut kit = Kit::new(AccountKind::Team);
    let ops = |names: &[OpName]| -> BTreeSet<OpName> { names.iter().copied().collect() };
    let stamp = kit.next_stamp();
    let effects = kit.step(
        1,
        1,
        Op::Delegate {
            key: key(40),
            ops: ops(&[OpName::MemberSuspend]),
            expires: Some(at(5_000)),
        },
    );
    assert!(effects.applied);
    let row = kit
        .state
        .delegation(&key(40))
        .expect("a delegation")
        .clone();
    assert_eq!(row.ops, ops(&[OpName::MemberSuspend]));
    assert_eq!(row.expires, Some(at(5_000)));
    assert_eq!(row.granted_by, member(1));
    assert_eq!(row.granted_at, stamp);
    assert_eq!(
        effects.registry,
        vec![RegistryDelta::Delegation {
            key: key(40),
            delegation: Some(row),
        }]
    );
    // A second `delegate` for the key REPLACES the delegation (it does not add to it).
    assert!(
        kit.step(
            1,
            1,
            Op::Delegate {
                key: key(40),
                ops: ops(&[OpName::MemberInvite]),
                expires: None,
            },
        )
        .applied
    );
    let replaced = kit.state.delegation(&key(40)).expect("a delegation");
    assert_eq!(replaced.ops, ops(&[OpName::MemberInvite]));
    assert_eq!(replaced.expires, None);
    assert_eq!(kit.state.delegations().count(), 1);
    // An empty set revokes it at once; revoking what is not there is not applied.
    let effects = kit.step(
        1,
        1,
        Op::Delegate {
            key: key(40),
            ops: BTreeSet::new(),
            expires: None,
        },
    );
    assert!(effects.applied);
    assert!(kit.state.delegation(&key(40)).is_none());
    assert_eq!(
        effects.registry,
        vec![RegistryDelta::Delegation {
            key: key(40),
            delegation: None,
        }]
    );
    assert!(
        !kit.step(
            1,
            1,
            Op::Delegate {
                key: key(40),
                ops: BTreeSet::new(),
                expires: None,
            },
        )
        .applied
    );
    // A delegate key cannot make a delegation (there is no member to grant it).
    assert!(
        !kit.step_with(
            Actor::Delegate(key(41)),
            Vec::new(),
            Op::Delegate {
                key: key(42),
                ops: ops(&[OpName::MemberSuspend]),
                expires: None,
            },
            None,
        )
        .applied
    );
}

#[test]
fn apply_delegations_are_bounded() {
    let mut kit = Kit::new(AccountKind::Team);
    for n in 0..MAX_DELEGATIONS {
        let mut ops = BTreeSet::new();
        ops.insert(OpName::MemberSuspend);
        assert!(
            kit.step(
                1,
                1,
                Op::Delegate {
                    key: key(n as u8 + 1),
                    ops,
                    expires: None,
                },
            )
            .applied
        );
    }
    assert_eq!(kit.state.delegations().count(), MAX_DELEGATIONS);
    let mut ops = BTreeSet::new();
    ops.insert(OpName::MemberSuspend);
    let before = kit.state.clone();
    assert!(
        !kit.step(
            1,
            1,
            Op::Delegate {
                key: key(200),
                ops: ops.clone(),
                expires: None,
            },
        )
        .applied
    );
    assert_eq!(kit.state, before);
    // A key that is already there can still be replaced, and a revocation makes room.
    assert!(
        kit.step(
            1,
            1,
            Op::Delegate {
                key: key(1),
                ops: ops.clone(),
                expires: Some(at(7)),
            },
        )
        .applied
    );
    assert!(
        kit.step(
            1,
            1,
            Op::Delegate {
                key: key(2),
                ops: BTreeSet::new(),
                expires: None,
            },
        )
        .applied
    );
    assert!(
        kit.step(
            1,
            1,
            Op::Delegate {
                key: key(200),
                ops,
                expires: None,
            },
        )
        .applied
    );
}

#[test]
fn apply_entitlement_keeps_the_newer_version_and_changes_no_member() {
    let mut kit = Kit::new(AccountKind::Team);
    kit.enrol(2, Role::Member, &[]);
    let members_before: Vec<_> = kit.state.members().cloned().collect();
    let entitlement = |version: u64, seats: u32| Op::Entitlement {
        version: EntitlementVersion(version),
        state: EntitlementState::Active,
        quota: ByteCount::gib(50),
        seats,
    };
    let stamp = kit.next_stamp();
    let effects = kit.step_with(Actor::Payments, Vec::new(), entitlement(2, 10), None);
    assert!(effects.applied);
    let facts = kit.state.entitlement().expect("an entitlement").clone();
    assert_eq!(facts.version, EntitlementVersion(2));
    assert_eq!(facts.state, EntitlementState::Active);
    assert_eq!(facts.quota, ByteCount::gib(50));
    assert_eq!(facts.seats, 10);
    assert_eq!(facts.applied_at, stamp);
    assert_eq!(
        effects.registry,
        vec![RegistryDelta::AccountEntitlement(facts)]
    );
    // An older one, and the same one again, do not replace it (the higher version wins).
    for (version, seats) in [(1, 99), (2, 98)] {
        let before = kit.state.clone();
        assert!(
            !kit.step_with(
                Actor::Payments,
                Vec::new(),
                entitlement(version, seats),
                None
            )
            .applied
        );
        assert_eq!(kit.state, before);
    }
    assert!(
        kit.step_with(Actor::Payments, Vec::new(), entitlement(3, 11), None)
            .applied
    );
    assert_eq!(kit.state.entitlement().map(|e| e.seats), Some(11));
    // It added no member and changed no role.
    let members_after: Vec<_> = kit.state.members().cloned().collect();
    assert_eq!(members_before, members_after);
}

#[test]
fn apply_app_op_changes_no_row_but_moves_the_chain() {
    let mut kit = Kit::new(AccountKind::Team);
    kit.enrol(2, Role::Member, &[]);
    let before = kit.state.clone();
    let head_before = before.head().expect("a head");
    let effects = kit.step(
        2,
        2,
        Op::App {
            name: AppName::new("notes").expect("valid"),
            body: vec![1, 2, 3],
        },
    );
    assert!(effects.applied);
    assert!(
        effects.registry.is_empty(),
        "stored and served, never interpreted"
    );
    assert!(!effects.credentials_invalidated);
    assert_eq!(effects.notices.len(), 1);
    let head = kit.state.head().expect("a head");
    assert_eq!(head.seq.get(), head_before.seq.get() + 1, "the chain moved");
    assert_ne!(head.hash, head_before.hash);
    // Nothing else did.
    let mut after = kit.state.clone();
    after.tip = before.tip.clone();
    assert_eq!(after, before);
}

#[test]
fn apply_moves_the_chain_head_and_the_stamp_and_nothing_else_does() {
    let mut kit = Kit::new(AccountKind::Team);
    let mut seq = kit.state.head().expect("a head").seq.get();
    for n in 1..=5u8 {
        let stamp = kit.next_stamp();
        assert!(kit.make_group(n, None).applied);
        seq += 1;
        let tip = kit.state.tip();
        assert_eq!(tip.head().map(|h| h.seq.get()), Some(seq));
        assert_eq!(
            tip.last_at(),
            Some(stamp),
            "the leader's stamp of the entry"
        );
        assert_eq!(tip.epoch(), RestoreEpoch::ZERO);
        assert_eq!(tip.next_seq(), Some(ChainSeq::new(seq + 1)));
    }
    // A refused entry moves nothing.
    let before = kit.state.tip().clone();
    assert!(!kit.make_group(5, None).applied);
    assert_eq!(kit.state.tip(), &before);
}

#[test]
fn apply_of_a_witness_that_does_not_fit_the_chain_is_not_applied() {
    let mut kit = Kit::new(AccountKind::Team);
    let op = Op::GroupCreate {
        group: group(1),
        parent: None,
    };
    let meta = kit.next_meta();
    // The first witness fits; a second one for the same position (the same entry again) does not.
    let first = Admitted::new(
        meta.clone(),
        at(2_000),
        by(1, 1),
        Vec::new(),
        op.clone(),
        None,
    );
    let again = Admitted::new(
        meta.clone(),
        at(2_001),
        by(1, 1),
        Vec::new(),
        op.clone(),
        None,
    );
    let effects = apply(&mut kit.state, first);
    assert!(effects.applied);
    let snapshot = kit.state.clone();
    let effects = apply(&mut kit.state, again);
    assert!(!effects.applied);
    assert!(effects.registry.is_empty() && effects.notices.is_empty());
    assert_eq!(kit.state, snapshot);
    // A gap, a wrong `prev`, another account, a stamp that runs backwards, an entry of the lost
    // timeline: none of them is applied, and none leaves a trace.
    let head = snapshot.head().expect("a head");
    let next = ChainSeq::new(head.seq.get() + 1);
    let good = || crate::apply::EntryMeta {
        account: super::kit::account(),
        seq: next,
        prev: head.hash,
        hash: hash(0x42),
        epoch: RestoreEpoch::ZERO,
    };
    let other_op = |n: u8| Op::GroupCreate {
        group: group(n),
        parent: None,
    };
    let cases = [
        crate::apply::EntryMeta {
            seq: ChainSeq::new(next.get() + 3),
            ..good()
        },
        crate::apply::EntryMeta {
            prev: hash(0x99),
            ..good()
        },
        crate::apply::EntryMeta {
            account: azlin_seams::AccountId::from_bytes([0x11; 16]),
            ..good()
        },
        // An epoch nobody announced: an entry never moves the restore epoch (X-DELTA-W1S47A-3).
        crate::apply::EntryMeta {
            epoch: RestoreEpoch::new(7),
            ..good()
        },
    ];
    for (n, meta) in cases.into_iter().enumerate() {
        let effects = apply(
            &mut kit.state,
            Admitted::new(
                meta,
                at(5_000),
                by(1, 1),
                Vec::new(),
                other_op(30 + n as u8),
                None,
            ),
        );
        assert!(!effects.applied, "case {n}");
        assert_eq!(kit.state, snapshot, "case {n}");
    }
    let backwards = apply(
        &mut kit.state,
        Admitted::new(good(), at(1), by(1, 1), Vec::new(), other_op(40), None),
    );
    assert!(
        !backwards.applied,
        "a stamp earlier than the newest entry's"
    );
    assert_eq!(kit.state, snapshot);
    kit.state.tip.raise_floor(RestoreEpoch::new(2));
    let snapshot = kit.state.clone();
    let lost = apply(
        &mut kit.state,
        Admitted::new(good(), at(5_000), by(1, 1), Vec::new(), other_op(41), None),
    );
    assert!(
        !lost.applied,
        "an entry written before the restore this state accepted"
    );
    assert_eq!(kit.state, snapshot);
    let restored = crate::apply::EntryMeta {
        epoch: RestoreEpoch::new(2),
        ..good()
    };
    let fits = apply(
        &mut kit.state,
        Admitted::new(
            restored,
            at(5_000),
            by(1, 1),
            Vec::new(),
            other_op(42),
            None,
        ),
    );
    assert!(fits.applied);
    assert_eq!(kit.state.tip().epoch(), RestoreEpoch::new(2));
}

#[test]
fn genesis_is_the_first_entry_and_only_the_first_entry() {
    let genesis = || Op::Genesis {
        owner: member(1),
        device: device(1),
        member_keys: keys(1),
        device_keys: keys(101),
    };
    // On a started log a genesis is not applied, whatever its position.
    let mut kit = Kit::new(AccountKind::Team);
    let before = kit.state.clone();
    assert!(!kit.step(1, 1, genesis()).applied);
    assert_eq!(kit.state, before);
    // On an empty log nothing but a genesis is applied, even at the right position.
    let mut empty = crate::state::TeamState::empty(super::kit::account(), AccountKind::Team);
    let meta = crate::apply::EntryMeta {
        account: super::kit::account(),
        seq: ChainSeq::MIN,
        prev: Hash32::ZERO,
        hash: hash(1),
        epoch: RestoreEpoch::ZERO,
    };
    let effects = apply(
        &mut empty,
        Admitted::new(
            meta.clone(),
            at(10),
            by(1, 1),
            Vec::new(),
            Op::GroupCreate {
                group: group(1),
                parent: None,
            },
            None,
        ),
    );
    assert!(!effects.applied);
    assert!(empty.is_empty() && empty.groups().is_empty());
    let effects = apply(
        &mut empty,
        Admitted::new(meta, at(10), by(1, 1), Vec::new(), genesis(), None),
    );
    assert!(effects.applied);
    assert!(!empty.is_empty());
    assert_eq!(
        effects.registry.len(),
        4,
        "the account's pin and policy, the owner, its first device"
    );
    assert!(matches!(
        effects.registry[0],
        RegistryDelta::AccountGenesis {
            kind: AccountKind::Team,
            ..
        }
    ));
}

#[test]
fn the_witness_reads_back_what_it_was_made_from() {
    let kit = Kit::new(AccountKind::Team);
    let meta = kit.next_meta();
    let expected_hash = meta.hash;
    let op = Op::MemberSuspend { member: member(1) };
    let witness = Admitted::new(meta, at(77), by(1, 1), Vec::new(), op.clone(), None);
    assert_eq!(witness.op(), &op);
    assert_eq!(witness.seq(), ChainSeq::new(1));
    assert_eq!(witness.hash(), expected_hash);
    assert_eq!(witness.stamp(), at(77));
}
