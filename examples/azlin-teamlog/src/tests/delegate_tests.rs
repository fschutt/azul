//! Tests of delegations (check 9 of the brief; planning/architecture.md section 5.9 "`delegate` | admin:
//! names a key and the exact ops it may sign", section 7.3 "Allowed: `member_suspend` and pending-member
//! creation. Never a grant, a role or a key wrap"; lane W1-S17c2). Every entry goes through
//! `verify_signatures` and `admit` (`SignedKit`).

use std::collections::BTreeSet;

use azlin_seams::Stamp;

use super::kit::{at, device, group, hash, key, member};
use super::signed::{invite_op, universe, Shape, SignedKit};
use crate::admit::{Invalid, Refusal};
use crate::delegate::{delegable_ops, is_delegable};
use crate::ops::{AppName, Op, OpName};
use crate::proposal::{RawEntry, Signer};
use crate::roles::Role;
use crate::state::MAX_DELEGATIONS;

/// A `delegate` entry: the key `key_n` may sign `ops` until `expires`.
fn delegate_to(key_n: u8, ops: &[OpName], expires: Option<Stamp>) -> Op {
    Op::Delegate {
        key: key(key_n),
        ops: ops.iter().copied().collect::<BTreeSet<_>>(),
        expires,
    }
}

/// The universe with the delegation of key 40 made (by the owner and an admin: an admin change).
fn with_delegation(ops: &[OpName], expires: Option<Stamp>) -> SignedKit {
    let mut kit = universe();
    kit.must_with(1, &[4], delegate_to(40, ops, expires));
    kit
}

/// An entry signed by the delegate key `key_n`, with cosigners (device numbers).
fn by_delegate(kit: &SignedKit, key_n: u8, cosigners: &[u8], op: Op) -> RawEntry {
    kit.entry(
        Signer::DelegateKey(key(key_n)),
        op,
        cosigners,
        Vec::new(),
        None,
    )
}

fn suspend(n: u8) -> Op {
    Op::MemberSuspend { member: member(n) }
}

#[test]
fn delegate_names_key_and_exact_ops() {
    let mut kit = universe();
    // The delegation is an admin change: the quorum of two, and only owner and admin make it.
    let make = delegate_to(40, &[OpName::MemberSuspend], None);
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], make.clone())),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    assert!(matches!(
        kit.verdict(&kit.entry_by(5, &[4], make.clone())),
        Err(Refusal::RoleMaySignNo { .. })
    ));
    kit.must_with(1, &[4], make);
    let delegation = kit.state.delegation(&key(40)).expect("a delegation");
    assert_eq!(
        delegation.ops,
        [OpName::MemberSuspend].into_iter().collect::<BTreeSet<_>>()
    );
    assert_eq!(delegation.expires, None);
    assert_eq!(delegation.granted_by, member(1));
    // A second delegate entry for the key REPLACES the delegation.
    kit.must_with(
        3,
        &[4],
        delegate_to(
            40,
            &[OpName::MemberSuspend, OpName::MemberInvite],
            Some(at(u64::MAX)),
        ),
    );
    assert_eq!(
        kit.state
            .delegation(&key(40))
            .expect("a delegation")
            .ops
            .len(),
        2
    );
    // Ops outside the delegable set cannot be named, one by one.
    for op in OpName::ALL {
        let named = delegate_to(41, &[op], None);
        let verdict = kit.verdict(&kit.entry_by(1, &[4], named));
        if is_delegable(op) {
            assert_eq!(verdict, Ok(()), "{op}");
        } else {
            assert_eq!(verdict, Err(Refusal::DelegationDoesNotCover { op }), "{op}");
        }
    }
    // A delegation that has ended before it began is no delegation.
    let over = delegate_to(41, &[OpName::MemberSuspend], Some(at(1)));
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], over)),
        Err(Refusal::Invalid(Invalid::Args(
            "the expiry of a delegation is already over"
        )))
    );
}

#[test]
fn delegate_key_signs_only_its_ops() {
    let kit = with_delegation(&[OpName::MemberSuspend], None);
    // It suspends a member (a one-person change)...
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], suspend(24))),
        Ok(())
    );
    // ...and nothing it was not given: an invitation, a role, a removal, a device, an app op.
    let invite = invite_op(70, &[3]);
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], invite)),
        Err(Refusal::DelegationDoesNotCover {
            op: OpName::MemberInvite
        })
    );
    let role = Op::MemberRole {
        member: member(24),
        role: Role::Guest,
        admin_scope: None,
    };
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], role)),
        Err(Refusal::DelegationDoesNotCover {
            op: OpName::MemberRole
        })
    );
    for (name, op) in [
        (
            OpName::MemberRemove,
            Op::MemberRemove { member: member(24) },
        ),
        (
            OpName::DeviceRemove,
            Op::DeviceRemove { device: device(24) },
        ),
        (
            OpName::App,
            Op::App {
                name: AppName::new("notes").expect("valid"),
                body: vec![1],
            },
        ),
        (
            OpName::GroupMember,
            Op::GroupMember {
                group: group(3),
                member: member(40),
                present: true,
            },
        ),
    ] {
        assert_eq!(
            kit.verdict(&by_delegate(&kit, 40, &[1, 4], op)),
            Err(Refusal::DelegationDoesNotCover { op: name }),
            "{name}"
        );
    }
    // A key with no delegation covers nothing.
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 41, &[], suspend(24))),
        Err(Refusal::DelegationDoesNotCover {
            op: OpName::MemberSuspend
        })
    );
    // It never suspends an owner...
    assert_eq!(
        why_role_none(kit.verdict(&by_delegate(&kit, 40, &[1, 4], suspend(2)))),
        Some(OpName::MemberSuspend)
    );
    // ...and an admin only with the quorum of two admins' cosignatures: a key is no person.
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], suspend(21))),
        Err(Refusal::QuorumShort { have: 0, need: 2 })
    );
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[3], suspend(21))),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[1, 4], suspend(21))),
        Ok(())
    );
    // The delegation that names the invitation lets the key invite members and guests, never an owner.
    let mut inviter = universe();
    inviter.must_with(1, &[4], delegate_to(42, &[OpName::MemberInvite], None));
    assert_eq!(
        inviter.verdict(&by_delegate(&inviter, 42, &[], invite_op(70, &[3]))),
        Ok(())
    );
    let owner_invite = Op::MemberInvite {
        member: member(70),
        kind: azlin_seams::MemberKind::Person,
        role: Role::Owner,
        invite_hash: hash(70),
        groups: Default::default(),
        app: None,
    };
    assert!(matches!(
        inviter.verdict(&by_delegate(&inviter, 42, &[], owner_invite)),
        Err(Refusal::RoleMaySignNo { role: None, .. })
    ));
}

/// The op of a `RoleMaySignNo` of a key (no role), if that is what the verdict is.
fn why_role_none(verdict: Result<(), Refusal>) -> Option<OpName> {
    match verdict {
        Err(Refusal::RoleMaySignNo { op, role: None }) => Some(op),
        _ => None,
    }
}

#[test]
fn identity_service_delegation_cannot_name_role_grant_or_wrap() {
    // The delegable set is the table's: member_suspend and member_invite (pending members). Everything
    // else, a role, a group membership (the grant), a device (the key wrap), a policy, an escrow, and
    // `delegate` itself, is refused by name.
    assert_eq!(
        delegable_ops(),
        vec![OpName::MemberInvite, OpName::MemberSuspend]
    );
    let kit = universe();
    for named in [
        OpName::MemberRole,
        OpName::GroupMember,
        OpName::GroupCreate,
        OpName::GroupMove,
        OpName::DeviceAdd,
        OpName::DeviceRemove,
        OpName::PolicySet,
        OpName::EscrowSet,
        OpName::EscrowOpen,
        OpName::AuditKeySet,
        OpName::AdminQuorumSet,
        OpName::MemberRemove,
        OpName::MemberManaged,
        OpName::MemberJoin,
        OpName::Genesis,
        OpName::Entitlement,
        OpName::App,
        OpName::Delegate,
    ] {
        assert!(!is_delegable(named));
        assert_eq!(
            kit.verdict(&kit.entry_by(1, &[4], delegate_to(40, &[named], None))),
            Err(Refusal::DelegationDoesNotCover { op: named }),
            "{named}"
        );
    }
    // One forbidden op among allowed ones refuses the whole delegation.
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[4],
            delegate_to(40, &[OpName::MemberSuspend, OpName::MemberRole], None)
        )),
        Err(Refusal::DelegationDoesNotCover {
            op: OpName::MemberRole
        })
    );
}

#[test]
fn delegate_cannot_delegate() {
    let kit = with_delegation(&[OpName::MemberSuspend], None);
    // The key signs a `delegate` entry (to widen its own delegation, or to make another key's).
    for target in [40u8, 43] {
        let widen = delegate_to(target, &[OpName::MemberSuspend, OpName::MemberInvite], None);
        assert_eq!(
            kit.verdict(&by_delegate(&kit, 40, &[1, 4], widen)),
            Err(Refusal::DelegationDoesNotCover {
                op: OpName::Delegate
            }),
            "key {target}"
        );
    }
    // A delegation cannot be revoked by the delegate either.
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[1, 4], delegate_to(40, &[], None))),
        Err(Refusal::DelegationDoesNotCover {
            op: OpName::Delegate
        })
    );
}

#[test]
fn delegation_expiry_uses_the_leader_stamp() {
    let mut kit = universe();
    let expires = at(kit.stamp().as_secs() + 1_000);
    kit.must_with(
        1,
        &[4],
        delegate_to(40, &[OpName::MemberSuspend], Some(expires)),
    );
    // Well before the end.
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], suspend(24))),
        Ok(())
    );
    // The stamp one second before the end: live. At the end: over ("expires" is the first second it is
    // over). After it: over.
    kit.advance(expires.as_secs() - 1 - kit.stamp().as_secs());
    assert_eq!(kit.stamp().as_secs() + 1, expires.as_secs());
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], suspend(24))),
        Ok(())
    );
    kit.advance(1);
    assert_eq!(kit.stamp(), expires);
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], suspend(24))),
        Err(Refusal::DelegationExpired)
    );
    kit.advance(10_000);
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], suspend(24))),
        Err(Refusal::DelegationExpired)
    );
    // The entry's own `at` decides nothing: the leader's stamp does, whatever the signer claims.
    let early = kit.reshape(
        &by_delegate(&kit, 40, &[], suspend(24)),
        Shape {
            at: Some(at(1)),
            ..Shape::default()
        },
    );
    assert_eq!(kit.verdict(&early), Err(Refusal::DelegationExpired));
    let mut fresh = universe();
    let end = at(fresh.stamp().as_secs() + 50);
    fresh.must_with(
        1,
        &[4],
        delegate_to(40, &[OpName::MemberSuspend], Some(end)),
    );
    let claims_late = fresh.reshape(
        &by_delegate(&fresh, 40, &[], suspend(24)),
        Shape {
            at: Some(at(u64::MAX)),
            ..Shape::default()
        },
    );
    assert_eq!(fresh.verdict(&claims_late), Ok(()));
    // An expired delegation says so before it says it does not cover: an op it never named is
    // `DelegationExpired` too (the delegation is over, whatever it named).
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], invite_op(70, &[3]))),
        Err(Refusal::DelegationExpired)
    );
}

#[test]
fn empty_delegate_op_set_revokes_at_once() {
    let mut kit = with_delegation(&[OpName::MemberSuspend], None);
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], suspend(24))),
        Ok(())
    );
    // The revocation needs the quorum like the delegation, and an owner or an admin.
    let revoke = delegate_to(40, &[], None);
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], revoke.clone())),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    kit.must_with(3, &[1], revoke.clone());
    assert!(kit.state.delegation(&key(40)).is_none());
    // At once: the very next entry of the key is refused.
    assert_eq!(
        kit.verdict(&by_delegate(&kit, 40, &[], suspend(24))),
        Err(Refusal::DelegationDoesNotCover {
            op: OpName::MemberSuspend
        })
    );
    // Revoking what is not there is no entry.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], revoke)),
        Err(Refusal::SubjectUnknown)
    );
}

#[test]
fn delegations_are_bounded_and_a_replacement_needs_no_room() {
    let mut kit = universe();
    for n in 0..MAX_DELEGATIONS {
        kit.must_with(
            1,
            &[4],
            delegate_to(100 + n as u8, &[OpName::MemberSuspend], None),
        );
    }
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], delegate_to(200, &[OpName::MemberSuspend], None))),
        Err(Refusal::Invalid(Invalid::Limit("delegations")))
    );
    // A key that is there is replaced, and a revocation makes room.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], delegate_to(100, &[OpName::MemberInvite], None))),
        Ok(())
    );
    kit.must_with(1, &[4], delegate_to(100, &[], None));
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], delegate_to(200, &[OpName::MemberSuspend], None))),
        Ok(())
    );
}
