//! Check 4 of the brief: the stories. Each is a numbered sequence in which the author wrote, from the plan,
//! the outcome of EVERY step; every step runs on the model, on the metadata group's check and on a device's
//! replay, which must agree with each other and with the author (planning/architecture.md section 12 E1,
//! ARCH29 9.13 X-D-54: "S47a's stories attack the table itself"). A story that the production fails because
//! the TABLE lets the attack through is a finding: its test is marked `#[ignore = "X-DELTA-W1S47A-<n>"]` and
//! the row of `delta/W1S47A.tsv` says what the plan expects instead.

use std::collections::BTreeSet;

use azlin_escalate_e1::model::{
    AcctKind, AppId, Class, DState, Dev, ExtSharing, Grp, Key, MApp, MKind, MOp, MPatch, MState,
    Mem, OpKind, PScope, PrevSpec, Role, SeqSpec, SigClaim, Verdict,
};
use azlin_escalate_e1::stories::{Story, World};
use azlin_seams::Span;

type R = Result<(), String>;

fn set<T: Ord + Copy>(items: &[T]) -> BTreeSet<T> {
    items.iter().copied().collect()
}

fn audit_patch(level: u8) -> MPatch {
    MPatch {
        audit_level: Some(level),
        ..MPatch::default()
    }
}

/// Story 1. An admin tries to become an owner in two steps: by himself, then with the other admin's help
/// (and by inviting an owner). 7.2: owner "everything, billing, owners"; admin "members, devices, groups,
/// spaces, policies in its scope". Only an owner makes owners.
#[test]
fn story_admin_makes_himself_owner_in_two_steps() -> R {
    let mut w = World::team()?;
    let (o, a, b) = (w.owner, w.admin_a, w.admin_b);
    let e = w.s.e(
        a.dev,
        MOp::MemberRole {
            member: a.mem,
            role: Role::Owner,
            scope: None,
        },
    );
    w.s.refuse(
        "admin A changes his own role to owner (nobody changes their own role)",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e(
        b.dev,
        MOp::MemberRole {
            member: a.mem,
            role: Role::Owner,
            scope: None,
        },
    );
    let e = Story::with_cosigs(e, &[a.dev]);
    w.s.refuse(
        "admin B makes admin A an owner and A cosigns (an admin never assigns the owner role)",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e(
        a.dev,
        MOp::MemberInvite {
            member: Mem(50),
            kind: MKind::Person,
            role: Role::Owner,
            secret: 5,
            groups: BTreeSet::new(),
            app: None,
        },
    );
    w.s.refuse(
        "admin A invites a new member straight in as owner (an invitation makes members and guests)",
        Class::RoleOrScope,
        e,
    )?;
    assert_eq!(w.role_of(a.mem), Some(Role::Admin), "A is still an admin");
    let e = w.s.e(
        o.dev,
        MOp::MemberRole {
            member: a.mem,
            role: Role::Owner,
            scope: None,
        },
    );
    w.s.admit("control: the owner makes admin A an owner", e)?;
    assert_eq!(w.role_of(a.mem), Some(Role::Owner));
    Ok(())
}

/// Story 2. A group admin whose scope is `eng` tries to leave his subtree, to pull other groups into it and
/// to widen his reach by moving groups. 5.9 (group_move: "admin or group admin in scope"); table v0: scope
/// over BOTH the group and the new parent. An admin may move `eng`; the group admin's scope is the subtree
/// of `eng` wherever it hangs.
#[test]
fn story_group_admin_moves_his_group_out_and_back_to_escape_scope() -> R {
    let mut w = World::team()?;
    let (ga, a) = (w.group_admin, w.admin_a);
    let (root, eng, ops, other) = (w.root, w.eng, w.ops, w.other);
    let e = w.s.e(
        ga.dev,
        MOp::GroupMove {
            group: eng,
            new_parent: other,
        },
    );
    w.s.refuse(
        "the group admin moves his own group under a group outside his scope",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e(
        ga.dev,
        MOp::GroupMove {
            group: ops,
            new_parent: eng,
        },
    );
    w.s.refuse(
        "he pulls the sibling department `ops` into his subtree",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e(
        ga.dev,
        MOp::GroupMove {
            group: other,
            new_parent: eng,
        },
    );
    w.s.refuse(
        "he pulls the top-level group `other` into his subtree",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e(
        ga.dev,
        MOp::GroupCreate {
            group: Grp(10),
            parent: Some(eng),
        },
    );
    w.s.admit("control: he makes a group inside his subtree", e)?;
    let e = w.s.e(
        a.dev,
        MOp::GroupMove {
            group: eng,
            new_parent: other,
        },
    );
    w.s.admit(
        "an admin moves `eng` under `other` (admins act everywhere)",
        e,
    )?;
    let e = w.s.e(
        ga.dev,
        MOp::GroupCreate {
            group: Grp(11),
            parent: Some(eng),
        },
    );
    w.s.admit(
        "the group admin's scope is the subtree of `eng` wherever it hangs: he still makes a group there",
        e,
    )?;
    let e = w.s.e(
        ga.dev,
        MOp::GroupCreate {
            group: Grp(12),
            parent: Some(root),
        },
    );
    w.s.refuse(
        "but `root` is no longer his neighbour: he makes no group under it",
        Class::RoleOrScope,
        e,
    )?;
    Ok(())
}

/// Story 3. Helpdesk and an admin try to suspend the only owner; the owner suspends himself. 7.2 helpdesk:
/// "open device requests and suspend; never approve alone"; 7.5: the owner cannot be suspended by anybody
/// but another owner; the account keeps an Active owner.
#[test]
fn story_helpdesk_suspends_the_only_owner() -> R {
    let mut w = World::team()?;
    let (h, o, a, m, g) = (w.helpdesk, w.owner, w.admin_a, w.member, w.guest);
    let e = w.s.e(h.dev, MOp::MemberSuspend { member: o.mem });
    w.s.refuse("helpdesk suspends the only owner", Class::RoleOrScope, e)?;
    let e = w.s.e(a.dev, MOp::MemberSuspend { member: o.mem });
    w.s.refuse("an admin suspends the only owner", Class::RoleOrScope, e)?;
    let e = w.s.e(o.dev, MOp::MemberSuspend { member: o.mem });
    w.s.refuse(
        "the owner suspends himself (the account keeps an Active owner)",
        Class::LastOwner,
        e,
    )?;
    let e = w.s.e(h.dev, MOp::MemberSuspend { member: g.mem });
    w.s.refuse(
        "helpdesk suspends the guest of another department (out of its scope)",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e(h.dev, MOp::MemberSuspend { member: m.mem });
    w.s.admit(
        "control: helpdesk suspends a member of its own department",
        e,
    )?;
    let suspended =
        w.s.rig
            .model
            .members
            .get(&m.mem)
            .map(|x| x.state == MState::Suspended);
    assert_eq!(suspended, Some(true));
    let owner_state = w.s.rig.model.members.get(&o.mem).map(|x| x.state);
    assert_eq!(owner_state, Some(MState::Active), "the owner is untouched");
    Ok(())
}

/// Story 4. The identity service's key names `member_suspend` and `member_invite` (7.3). It tries to
/// delegate, to name itself more ops and to change a role. 7.3: "Never a grant, a role or a key wrap"; the
/// delegate names "the exact ops".
#[test]
fn story_delegate_key_re_delegates() -> R {
    let mut w = World::team()?;
    let (o, m) = (w.owner, w.member);
    let key = Key(1);
    let e = w.s.e(
        o.dev,
        MOp::Delegate {
            key,
            ops: set(&[OpKind::MemberSuspend, OpKind::MemberInvite]),
            expires: None,
        },
    );
    w.s.admit(
        "the owner delegates `member_suspend` and `member_invite` to key 1",
        e,
    )?;
    let e = w.s.e_delegate(
        key,
        MOp::Delegate {
            key: Key(2),
            ops: set(&[OpKind::MemberSuspend]),
            expires: None,
        },
    );
    w.s.refuse(
        "key 1 delegates to a second key (nobody but an admin delegates)",
        Class::DelegationDoesNotCover,
        e,
    )?;
    let e = w.s.e_delegate(
        key,
        MOp::Delegate {
            key,
            ops: set(&[OpKind::MemberSuspend, OpKind::MemberRole]),
            expires: None,
        },
    );
    w.s.refuse(
        "key 1 widens its own delegation with `member_role`",
        Class::DelegationDoesNotCover,
        e,
    )?;
    let e = w.s.e_delegate(
        key,
        MOp::MemberRole {
            member: m.mem,
            role: Role::Admin,
            scope: None,
        },
    );
    w.s.refuse(
        "key 1 makes a member an admin",
        Class::DelegationDoesNotCover,
        e,
    )?;
    let e = w.s.e_delegate(
        key,
        MOp::GroupMember {
            group: w.root,
            member: m.mem,
            present: true,
        },
    );
    w.s.refuse(
        "key 1 puts a member into a group",
        Class::DelegationDoesNotCover,
        e,
    )?;
    assert!(w.delegation_names(key, OpKind::MemberSuspend));
    assert!(!w.delegation_names(key, OpKind::Delegate));
    Ok(())
}

/// Story 5. With an admin quorum of 2, one admin who owns two devices signs with both. 12 E1: "a quorum
/// counts distinct people, never one person's devices".
#[test]
fn story_one_admin_with_two_devices_satisfies_a_quorum() -> R {
    let mut w = World::team()?;
    w.set_quorum(2)?;
    let (a, b) = (w.admin_a, w.admin_b);
    w.add_own_device(a, Dev(22))?;
    let e = w.s.e(
        a.dev,
        MOp::PolicySet {
            scope: PScope::Account,
            patch: audit_patch(3),
        },
    );
    let e = Story::with_cosigs(e, &[Dev(22)]);
    w.s.refuse(
        "admin A signs a policy change with his phone and his laptop",
        Class::QuorumShort,
        e,
    )?;
    let e = w.s.e(
        a.dev,
        MOp::PolicySet {
            scope: PScope::Account,
            patch: audit_patch(3),
        },
    );
    let e = Story::with_cosigs(e, &[Dev(22), Dev(22)]);
    w.s.refuse(
        "the same device listed twice is still one person",
        Class::QuorumShort,
        e,
    )?;
    let e = w.s.e(
        a.dev,
        MOp::PolicySet {
            scope: PScope::Account,
            patch: audit_patch(3),
        },
    );
    let e = Story::with_cosigs(e, &[Dev(22), b.dev]);
    w.s.admit("control: admin A and admin B", e)?;
    assert_eq!(w.s.rig.model.policy.audit_level, 3);
    Ok(())
}

/// Story 6. A removed admin's (revoked) device cosigns a quorum entry. 5.9: "every entry is signed by a
/// device whose role allows it, with a quorum where policy asks"; a removed member's devices are revoked.
#[test]
fn story_removed_members_device_cosigns() -> R {
    let mut w = World::team()?;
    w.set_quorum(2)?;
    let (o, a, b) = (w.owner, w.admin_a, w.admin_b);
    let e = w.s.e(o.dev, MOp::MemberRemove { member: b.mem });
    let e = Story::with_cosigs(e, &[a.dev]);
    w.s.admit("the owner removes admin B, admin A cosigns", e)?;
    let e = w.s.e(
        a.dev,
        MOp::PolicySet {
            scope: PScope::Account,
            patch: audit_patch(3),
        },
    );
    let e = Story::with_cosigs(e, &[b.dev]);
    w.s.refuse(
        "admin A asks the removed admin B's device to cosign a policy change",
        Class::QuorumShort,
        e,
    )?;
    let e = w.s.e(
        b.dev,
        MOp::PolicySet {
            scope: PScope::Account,
            patch: audit_patch(3),
        },
    );
    let e = Story::with_cosigs(e, &[a.dev]);
    w.s.refuse(
        "the removed admin B signs and admin A cosigns",
        Class::SignerNotActive,
        e,
    )?;
    Ok(())
}

/// Story 7. A recovery device that admins enrolled by quorum signs a role change while it is in its hold.
/// 7.5: "Notice and hold: the group withholds the wrap until `hold_until` (48 h)"; table v0: "a Pending
/// device, one in its hold, signs nothing". At `hold_until` the hold is over (the reading of 'until').
#[test]
fn story_recovery_device_still_in_its_hold_signs_a_role_change() -> R {
    let mut w = World::team()?;
    let (o, a, m) = (w.owner, w.admin_a, w.member);
    let fresh = Dev(29);
    let e = w.s.e(
        o.dev,
        MOp::DeviceAdd {
            member: a.mem,
            device: fresh,
        },
    );
    w.s.admit(
        "the owner enrols a replacement device for admin A (quorum path)",
        e,
    )?;
    let until = match w.s.rig.model.devices.get(&fresh).map(|d| d.state) {
        Some(DState::Pending(t)) => t,
        other => return Err(format!("the new device is not in its hold: {other:?}")),
    };
    let role_change = |w: &mut World| {
        w.s.e(
            fresh,
            MOp::MemberRole {
                member: m.mem,
                role: Role::Guest,
                scope: None,
            },
        )
    };
    let e = role_change(&mut w);
    w.s.refuse(
        "inside the hold the new device makes a member a guest",
        Class::SignerNotActive,
        e,
    )?;
    w.s.set_clock(until.as_secs() - 2);
    let e = role_change(&mut w);
    w.s.refuse(
        "one second before the hold ends the new device tries again",
        Class::SignerNotActive,
        e,
    )?;
    w.s.set_clock(until.as_secs() - 1);
    let e = role_change(&mut w);
    w.s.admit("at `hold_until` the hold is over and the device signs", e)?;
    Ok(())
}

/// Story 8a. An old, valid entry is offered again at its old position: refused, it does not continue the
/// chain. 5.9: "The metadata group rejects an entry with a reason for a wrong `prev`".
#[test]
fn story_old_valid_entry_honest_replay_is_refused() -> R {
    let mut w = World::team()?;
    let (o, m) = (w.owner, w.member);
    let e = w.s.e(
        o.dev,
        MOp::MemberRole {
            member: m.mem,
            role: Role::Helpdesk,
            scope: Some(w.eng),
        },
    );
    w.s.admit("E: the owner makes M a helpdesk member", e)?;
    let (old, _) = w.s.rig.log.last().cloned().ok_or("no entry in the log")?;
    let e = w.s.e(
        o.dev,
        MOp::MemberRole {
            member: m.mem,
            role: Role::Member,
            scope: None,
        },
    );
    w.s.admit("the owner takes the role away again", e)?;
    w.s.tick(1);
    let verdict = w.s.probe_production(&old, w.s.now);
    if !matches!(verdict, Verdict::Refuse(Class::Chain)) {
        return Err(format!("the old entry at its old position: {verdict:?}"));
    }
    Ok(())
}

/// Story 8. The same old, valid entry with its signatures, and the position fields of a fresh entry. The
/// signatures cover the entry's position (5.9: `seq` and `prev` are fields of the signed entry; "every entry
/// is signed by a device whose role allows it"), so an old signature must not make a new entry. A device
/// that checks the typed fields only and never binds them to the signed bytes would admit it: the production
/// does (finding X-DELTA-W1S47A-1, class BUG).
#[test]
#[ignore = "X-DELTA-W1S47A-1: the production admits an old signed entry with the position fields of a new one"]
fn story_old_valid_entry_replayed_at_a_new_seq() -> R {
    let mut w = World::team()?;
    let (o, m) = (w.owner, w.member);
    let e = w.s.e(
        o.dev,
        MOp::MemberRole {
            member: m.mem,
            role: Role::Helpdesk,
            scope: Some(w.eng),
        },
    );
    w.s.admit("E: the owner makes M a helpdesk member", e)?;
    let (old, _) = w.s.rig.log.last().cloned().ok_or("no entry in the log")?;
    let e = w.s.e(
        o.dev,
        MOp::MemberRole {
            member: m.mem,
            role: Role::Member,
            scope: None,
        },
    );
    w.s.admit("the owner takes the role away again", e)?;
    let head = w.s.rig.prod.head().ok_or("no head")?;
    let mut forged = old.clone();
    forged.seq = head.seq.next().ok_or("the position overflows")?;
    forged.prev = head.hash;
    w.s.tick(1);
    let verdict = w.s.probe_production(&forged, w.s.now);
    if verdict == Verdict::Admit {
        return Err(
            "the production ADMITS the old signed entry with the position of a new one: the signatures \
             do not bind the typed fields (X-DELTA-W1S47A-1)"
                .to_string(),
        );
    }
    Ok(())
}

/// Story 9. Two replicas of the metadata group receive two different valid entries for one position; a
/// device that sees both keeps the first and refuses the second. 5.9: "two different entries at the same
/// position are a fork, refused and alarmed".
#[test]
fn story_two_replicas_get_two_entries_at_one_position() -> R {
    let w = World::team()?;
    let (o, m) = (w.owner, w.member);
    let eng = w.eng;
    let mut a = w.s.clone();
    let mut b = w.s.clone();
    let x = a.e(
        o.dev,
        MOp::MemberRole {
            member: m.mem,
            role: Role::Helpdesk,
            scope: Some(eng),
        },
    );
    a.admit("replica A admits X at position n", x)?;
    let mut y = b.e(
        o.dev,
        MOp::MemberRole {
            member: m.mem,
            role: Role::Guest,
            scope: None,
        },
    );
    y.eid += 1_000;
    b.admit("replica B admits Y at position n", y.clone())?;
    let mut y_on_a = y;
    y_on_a.seq = SeqSpec::Off(-1);
    y_on_a.prev = PrevSpec::Stale(1);
    a.refuse(
        "replica A is offered Y at the position where it holds X (a fork)",
        Class::Chain,
        y_on_a,
    )?;
    assert_eq!(
        a.rig.model.members.get(&m.mem).map(|x| x.role),
        Some(Role::Helpdesk),
        "A keeps X"
    );
    assert_eq!(
        b.rig.model.members.get(&m.mem).map(|x| x.role),
        Some(Role::Guest),
        "B keeps Y"
    );
    Ok(())
}

/// Story 10. Two admins lower the quorum together and one of them then acts alone. 5.10/5.9: "lowering
/// needs the CURRENT quorum": one admin alone cannot; two can, by design, and the quorum of 1 that follows
/// lets one admin make admin changes alone. (The plan leaves the owner no veto: see the drift row.)
#[test]
fn story_admin_lowers_the_quorum_then_acts_alone() -> R {
    let mut w = World::team()?;
    w.set_quorum(2)?;
    let (a, b) = (w.admin_a, w.admin_b);
    let e = w.s.e(a.dev, MOp::AdminQuorumSet { quorum: 1 });
    w.s.refuse(
        "admin A alone lowers the admin quorum from 2 to 1",
        Class::QuorumShort,
        e,
    )?;
    let e = w.s.e(a.dev, MOp::AdminQuorumSet { quorum: 1 });
    let e = Story::with_cosigs(e, &[b.dev]);
    w.s.admit(
        "admin A and admin B together lower it (the current quorum is met)",
        e,
    )?;
    assert_eq!(w.s.rig.model.policy.admin_quorum, 1);
    let e = w.s.e(a.dev, MOp::MemberRemove { member: b.mem });
    w.s.admit(
        "with a quorum of 1, admin A alone removes admin B (an admin change)",
        e,
    )?;
    Ok(())
}

/// Story 11. An app op whose body says "make me owner": it is "stored and served, never interpreted" (5.9),
/// so no role changes; a guest cannot send one; an app principal sends only for the apps its scopes name.
#[test]
fn story_app_op_used_as_a_role_change() -> R {
    let mut w = World::team()?;
    let (o, m, g) = (w.owner, w.member, w.guest);
    let before = azlin_escalate_e1::project::project_model(&w.s.rig.model).members;
    let e = w.s.e(
        m.dev,
        MOp::App {
            name: AppId(1),
            body_len: 64,
        },
    );
    w.s.admit("a member sends an app op that says 'make me owner'", e)?;
    let after = azlin_escalate_e1::project::project_model(&w.s.rig.model).members;
    assert_eq!(before, after, "no member changed");
    assert_eq!(w.role_of(m.mem), Some(Role::Member));
    let e = w.s.e(
        g.dev,
        MOp::App {
            name: AppId(1),
            body_len: 8,
        },
    );
    w.s.refuse("a guest sends an app op", Class::RoleOrScope, e)?;
    let principal = Mem(60);
    let principal_dev = Dev(601);
    let e = w.s.e(
        o.dev,
        MOp::MemberInvite {
            member: principal,
            kind: MKind::App,
            role: Role::Member,
            secret: 77,
            groups: BTreeSet::new(),
            app: Some(MApp {
                scopes: set(&[AppId(2)]),
                expires: None,
            }),
        },
    );
    w.s.admit("the owner invites an app principal for app 2", e)?;
    let e = w.s.e(
        principal_dev,
        MOp::MemberJoin {
            member: principal,
            device: principal_dev,
            secret: 77,
        },
    );
    let e = Story::with_cosigs(e, &[o.dev]);
    w.s.admit("the app principal joins", e)?;
    let e = w.s.e(
        principal_dev,
        MOp::App {
            name: AppId(1),
            body_len: 8,
        },
    );
    w.s.refuse(
        "the app principal sends an op for app 1 (its scopes name app 2)",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e(
        principal_dev,
        MOp::App {
            name: AppId(2),
            body_len: 8,
        },
    );
    w.s.admit("the app principal sends an op for app 2", e)?;
    let e = w.s.e(
        principal_dev,
        MOp::MemberRole {
            member: principal,
            role: Role::Admin,
            scope: None,
        },
    );
    w.s.refuse(
        "the app principal makes itself an admin",
        Class::RoleOrScope,
        e,
    )?;
    Ok(())
}

/// Story 12. The identity service (a delegate key) tries to create an admin: by inviting one, by promoting
/// a member, and by confirming its own invitation. 7.3: "Allowed: member_suspend and pending-member creation.
/// Never a grant, a role or a key wrap"; 7.3: "an admin approves in AzWorkspace".
#[test]
fn story_identity_service_creates_an_admin() -> R {
    let mut w = World::team()?;
    let o = w.owner;
    let key = Key(1);
    let e = w.s.e(
        o.dev,
        MOp::Delegate {
            key,
            ops: set(&[OpKind::MemberSuspend, OpKind::MemberInvite]),
            expires: None,
        },
    );
    w.s.admit(
        "the owner delegates suspend and invite to the identity service",
        e,
    )?;
    let invite = |role: Role, member: Mem, secret: u32| MOp::MemberInvite {
        member,
        kind: MKind::Person,
        role,
        secret,
        groups: BTreeSet::new(),
        app: None,
    };
    let e = w.s.e_delegate(key, invite(Role::Admin, Mem(51), 51));
    w.s.refuse(
        "the identity service invites a new member as an admin",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e_delegate(key, invite(Role::Member, Mem(52), 52));
    w.s.admit("the identity service creates a pending member", e)?;
    let e = w.s.e(
        Dev(521),
        MOp::MemberJoin {
            member: Mem(52),
            device: Dev(521),
            secret: 52,
        },
    );
    w.s.refuse(
        "the pending member joins with nobody's confirmation",
        Class::InviteInvalid,
        e,
    )?;
    let e = w.s.e(
        Dev(521),
        MOp::MemberJoin {
            member: Mem(52),
            device: Dev(521),
            secret: 52,
        },
    );
    let e = Story::with_cosigs(e, &[o.dev]);
    w.s.admit("an admin approves: the owner confirms the join", e)?;
    let e = w.s.e_delegate(
        key,
        MOp::MemberRole {
            member: Mem(52),
            role: Role::Admin,
            scope: None,
        },
    );
    w.s.refuse(
        "the identity service makes the new member an admin",
        Class::DelegationDoesNotCover,
        e,
    )?;
    assert_eq!(w.role_of(Mem(52)), Some(Role::Member));
    Ok(())
}

/// Story 13. The payments key adds a member, changes a role, and an untrusted payments key signs an
/// entitlement. 5.9: `entitlement` "replayed from the payments service's submission"; "the payments service
/// and the operator cannot add a member".
#[test]
fn story_payments_key_adds_a_member() -> R {
    let mut w = World::team()?;
    let m = w.member;
    let e = w.s.e_payments(MOp::MemberInvite {
        member: Mem(53),
        kind: MKind::Person,
        role: Role::Member,
        secret: 53,
        groups: BTreeSet::new(),
        app: None,
    });
    w.s.refuse(
        "the payments service invites a member",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e_payments(MOp::MemberRole {
        member: m.mem,
        role: Role::Admin,
        scope: None,
    });
    w.s.refuse(
        "the payments service makes a member an admin",
        Class::RoleOrScope,
        e,
    )?;
    let mut e = w.s.e_payments(MOp::Entitlement {
        version: 2,
        active: true,
        quota: 1 << 30,
        seats: 1_000_000,
    });
    e.sig = SigClaim::Invalid;
    w.s.refuse(
        "somebody signs an entitlement with a key that is not the payments service's",
        Class::Signature,
        e,
    )?;
    let e = w.s.e(
        w.owner.dev,
        MOp::Entitlement {
            version: 2,
            active: true,
            quota: 1 << 30,
            seats: 1_000_000,
        },
    );
    w.s.refuse(
        "an owner's device signs an entitlement",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e_payments(MOp::Entitlement {
        version: 2,
        active: true,
        quota: 1 << 30,
        seats: 25,
    });
    w.s.admit("control: the payments service submits the entitlement", e)?;
    let e = w.s.e_payments(MOp::Entitlement {
        version: 2,
        active: true,
        quota: 1 << 40,
        seats: 25,
    });
    w.s.refuse(
        "the same version again, with a bigger quota (R10: the higher version wins)",
        Class::PolicyForbids,
        e,
    )?;
    Ok(())
}

/// Story 14. An admin tightens a department (`hr`); a group admin whose scope contains `hr` and `eng` moves a
/// group out from under `hr` to the loose part of his subtree, where the admin's restriction no longer
/// applies. 7.2: "subgroups inherit and may tighten, never loosen"; a group admin only tightens. The table
/// v0 lets a group admin with scope over both groups move it, and the production admits the move (finding
/// X-DELTA-W1S47A-2, class BUG).
#[test]
#[ignore = "X-DELTA-W1S47A-2: a group admin loosens an admin-set policy by moving a group"]
fn story_policy_loosened_by_moving_a_group_under_a_looser_parent() -> R {
    let mut w = World::team()?;
    let (a, ga) = (w.admin_a, w.group_admin);
    let eng = w.eng;
    let (hr, sub) = (Grp(20), Grp(21));
    let e = w.s.e(
        a.dev,
        MOp::GroupCreate {
            group: hr,
            parent: Some(eng),
        },
    );
    w.s.admit("admin A makes the department `hr` under `eng`", e)?;
    let e = w.s.e(
        a.dev,
        MOp::GroupCreate {
            group: sub,
            parent: Some(hr),
        },
    );
    w.s.admit("admin A makes a team `sub` under `hr`", e)?;
    let e = w.s.e(
        a.dev,
        MOp::PolicySet {
            scope: PScope::Group(hr),
            patch: MPatch {
                external_sharing: Some(ExtSharing::SameWorkspaceOnly),
                credential_ttl: Some(Span::hours(1)),
                ..MPatch::default()
            },
        },
    );
    w.s.admit(
        "admin A tightens `hr`: no external sharing, one-hour credentials",
        e,
    )?;
    let tight =
        w.s.rig
            .model
            .effective_policy(sub)
            .ok_or("no policy for sub")?;
    assert_eq!(tight.external_sharing, ExtSharing::SameWorkspaceOnly);
    let e = w.s.e(
        ga.dev,
        MOp::GroupMove {
            group: sub,
            new_parent: eng,
        },
    );
    w.s.refuse(
        "the group admin moves `sub` out from under `hr` to `eng`, where the restriction does not apply",
        Class::PolicyLoosens,
        e,
    )?;
    Ok(())
}

/// Extra. An admin (alone, quorum 1) enrols a device for the OWNER. 7.5: "nobody else can 'recover' an
/// employee: not an attacker, not Azlin, not the IdP, not SCIM, not helpdesk, not one admin"; after its hold
/// the device would sign as the owner. The literal table lets an admin enrol for any member (finding: the
/// rank rule of lane S17c closes it).
#[test]
fn story_admin_enrolls_a_device_for_the_owner() -> R {
    let mut w = World::team()?;
    let (a, o) = (w.admin_a, w.owner);
    let e = w.s.e(
        a.dev,
        MOp::DeviceAdd {
            member: o.mem,
            device: Dev(19),
        },
    );
    w.s.refuse(
        "admin A enrols a new device for the owner",
        Class::RoleOrScope,
        e,
    )?;
    Ok(())
}

/// Extra. A group admin pulls an admin of another department into his group and then demotes him. The
/// literal table has no rank rule for a group admin's `member_role` subjects and no limit on `group_member`
/// (finding: the rules of lane S17c close both).
#[test]
fn story_group_admin_pulls_an_admin_into_scope_and_demotes_him() -> R {
    let mut w = World::team()?;
    let (ga, a) = (w.group_admin, w.admin_a);
    let e = w.s.e(
        ga.dev,
        MOp::GroupMember {
            group: w.eng,
            member: a.mem,
            present: true,
        },
    );
    w.s.refuse(
        "the group admin takes admin A (of `other`) into his department",
        Class::RoleOrScope,
        e,
    )?;
    let e = w.s.e(
        ga.dev,
        MOp::MemberRole {
            member: a.mem,
            role: Role::Member,
            scope: None,
        },
    );
    w.s.refuse(
        "the group admin demotes admin A to member",
        Class::RoleOrScope,
        e,
    )?;
    assert_eq!(w.role_of(a.mem), Some(Role::Admin));
    Ok(())
}

/// Extra. A new business account has one person, and its admin quorum is 2 from the first entry: the owner
/// can make no admin, no second owner and no policy (a deadlock of the plan's table, not an escalation).
#[test]
fn story_business_owner_cannot_make_the_first_admin() -> R {
    let mut s = Story::new(AcctKind::Business);
    let owner = Dev(11);
    let e = s.e(
        owner,
        MOp::Genesis {
            owner: Mem(1),
            device: owner,
        },
    );
    s.admit("the genesis of a business", e)?;
    let e = s.e(
        owner,
        MOp::MemberInvite {
            member: Mem(2),
            kind: MKind::Person,
            role: Role::Member,
            secret: 1,
            groups: BTreeSet::new(),
            app: None,
        },
    );
    s.admit("the owner invites a colleague", e)?;
    let e = s.e(
        Dev(21),
        MOp::MemberJoin {
            member: Mem(2),
            device: Dev(21),
            secret: 1,
        },
    );
    let e = Story::with_cosigs(e, &[owner]);
    s.admit("the colleague joins", e)?;
    let e = s.e(
        owner,
        MOp::MemberRole {
            member: Mem(2),
            role: Role::Admin,
            scope: None,
        },
    );
    s.refuse(
        "the owner makes the colleague an admin (a business asks two people for an admin change)",
        Class::QuorumShort,
        e,
    )?;
    let e = s.e(owner, MOp::AdminQuorumSet { quorum: 1 });
    s.refuse(
        "the owner lowers the quorum to 1 (lowering needs the current quorum)",
        Class::QuorumShort,
        e,
    )?;
    let e = s.e(
        owner,
        MOp::PolicySet {
            scope: PScope::Account,
            patch: audit_patch(3),
        },
    );
    s.refuse(
        "the owner changes a policy (a policy change needs the quorum)",
        Class::QuorumShort,
        e,
    )?;
    Ok(())
}
