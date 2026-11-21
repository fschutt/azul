//! The authority table v0 as DATA, typed from the brief's table (section 2, "AUTHORITY TABLE v0") and the
//! plan sentences it quotes, NOT from the model (planning/architecture.md section 5.9 "Who may sign", 7.2,
//! 7.3, 7.5; check 1 of the brief).
//!
//! Each function answers, for one op, whether an entry of that op is admitted, for a signer of a role and a
//! subject of a role in a place (in scope, out of scope, the signer itself), with the quorum met or short.
//! `Exp::Hole(class)` marks a cell where the LITERAL table admits the entry and the provisional reading of
//! lane S17c ([S17c-8] a signer never acts on a member above its own rank, [S17c-9] a group admin's reach)
//! refuses it: the model follows the reading and the cell is a finding about the table.

#![allow(dead_code)]

use azlin_escalate_e1::model::{Class, OpKind, Role};

/// Where the subject is relative to the signer's scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rel {
    /// The subject belongs to a group of the signer's subtree.
    InScope,
    /// It belongs to a group outside it.
    OutOfScope,
    /// The subject is the signer.
    SelfSubject,
}

/// What the table says about a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exp {
    /// The entry is admitted.
    Admit,
    /// The entry is refused, and the reason is (among) this class.
    Refuse(Class),
    /// The literal table admits it, the reading refuses it with this class.
    Hole(Class),
}

fn admin_class(r: Role) -> bool {
    matches!(r, Role::Owner | Role::Admin)
}

/// The roles that may sign an op when they act on somebody else ("Who may sign").
pub fn may_sign(op: OpKind, role: Role) -> bool {
    use Role::*;
    match op {
        OpKind::MemberRemove | OpKind::GroupCreate => matches!(role, Owner | Admin | GroupAdmin),
        // "admins by quorum [plan 7.5]": the table names admins, not group admins, here
        OpKind::DeviceAdd
        | OpKind::DeviceRemove
        | OpKind::MemberManaged
        | OpKind::EscrowSet
        | OpKind::AuditKeySet
        | OpKind::AdminQuorumSet
        | OpKind::EscrowOpen
        | OpKind::Delegate => matches!(role, Owner | Admin),
        OpKind::MemberInvite
        | OpKind::MemberRole
        | OpKind::GroupMember
        | OpKind::GroupMove
        | OpKind::PolicySet => matches!(role, Owner | Admin | GroupAdmin),
        OpKind::MemberSuspend => matches!(role, Owner | Admin | GroupAdmin | Helpdesk),
        OpKind::App => !matches!(role, Guest),
        OpKind::Genesis | OpKind::MemberJoin | OpKind::Entitlement => false,
    }
}

/// `member_role`: which roles a signer may assign: "owner: any role incl. owner/admin; admin: group_admin,
/// helpdesk, member, guest; group_admin: member, guest".
pub fn assignable(signer: Role, new: Role) -> bool {
    use Role::*;
    match signer {
        Owner => true,
        Admin => matches!(new, GroupAdmin | Helpdesk | Member | Guest),
        GroupAdmin => matches!(new, Member | Guest),
        _ => false,
    }
}

/// `member_role` by a signer on a subject of a role, in a place, to a new role.
pub fn member_role(signer: Role, subject: Role, rel: Rel, new: Role, q_met: bool) -> Exp {
    use Role::*;
    if !matches!(signer, Owner | Admin | GroupAdmin) {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if rel == Rel::SelfSubject {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if !assignable(signer, new) {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if signer == GroupAdmin && rel == Rel::OutOfScope {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if signer == Admin && subject.rank() >= Admin.rank() {
        return Exp::Refuse(Class::RoleOrScope);
    }
    let quorum_short = (admin_class(subject) || admin_class(new)) && !q_met;
    if signer == GroupAdmin && subject.rank() >= GroupAdmin.rank() {
        return if quorum_short {
            Exp::Refuse(Class::QuorumShort)
        } else {
            Exp::Hole(Class::RoleOrScope)
        };
    }
    if quorum_short {
        return Exp::Refuse(Class::QuorumShort);
    }
    Exp::Admit
}

/// `member_suspend`: "admin, group_admin, helpdesk in scope; never an owner unless by another owner with
/// quorum"; AdminChange for an admin or owner subject.
pub fn member_suspend(signer: Role, subject: Role, rel: Rel, q_met: bool) -> Exp {
    use Role::*;
    if !may_sign(OpKind::MemberSuspend, signer) {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if matches!(signer, GroupAdmin | Helpdesk) && rel == Rel::OutOfScope {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if subject == Owner && signer != Owner {
        return Exp::Refuse(Class::RoleOrScope);
    }
    let quorum_short = admin_class(subject) && !q_met;
    if signer != Owner && subject.rank() > signer.rank() {
        return if quorum_short {
            Exp::Refuse(Class::QuorumShort)
        } else {
            Exp::Hole(Class::RoleOrScope)
        };
    }
    if quorum_short {
        return Exp::Refuse(Class::QuorumShort);
    }
    Exp::Admit
}

/// `member_remove`: "admin, group_admin in scope (helpdesk never); not the last owner"; AdminChange for an
/// admin or owner subject.
pub fn member_remove(signer: Role, subject: Role, rel: Rel, q_met: bool) -> Exp {
    use Role::*;
    if !may_sign(OpKind::MemberRemove, signer) {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if signer == GroupAdmin && rel == Rel::OutOfScope {
        return Exp::Refuse(Class::RoleOrScope);
    }
    let quorum_short = admin_class(subject) && !q_met;
    if signer != Owner && subject.rank() > signer.rank() {
        return if quorum_short {
            Exp::Refuse(Class::QuorumShort)
        } else {
            Exp::Hole(Class::RoleOrScope)
        };
    }
    if quorum_short {
        return Exp::Refuse(Class::QuorumShort);
    }
    Exp::Admit
}

/// `device_add` by a quorum: "admins by quorum [plan 7.5]; helpdesk never approves alone"; the quorum
/// path enrols for a member in scope.
pub fn device_add(signer: Role, subject: Role, rel: Rel, q_met: bool) -> Exp {
    use Role::*;
    if rel == Rel::SelfSubject {
        return Exp::Admit;
    }
    if !may_sign(OpKind::DeviceAdd, signer) {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if signer == GroupAdmin && rel == Rel::OutOfScope {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if signer != Owner && subject.rank() > signer.rank() {
        return if q_met {
            Exp::Hole(Class::RoleOrScope)
        } else {
            Exp::Refuse(Class::QuorumShort)
        };
    }
    if !q_met {
        return Exp::Refuse(Class::QuorumShort);
    }
    Exp::Admit
}

/// `device_remove` of somebody else's device: "or an admin in scope [v0]; removing a device of an admin or
/// owner by someone else: quorum".
pub fn device_remove(signer: Role, subject: Role, q_met: bool) -> Exp {
    use Role::*;
    if !may_sign(OpKind::DeviceRemove, signer) {
        return Exp::Refuse(Class::RoleOrScope);
    }
    let quorum_short = admin_class(subject) && !q_met;
    if signer != Owner && subject.rank() > signer.rank() {
        return if quorum_short {
            Exp::Refuse(Class::QuorumShort)
        } else {
            Exp::Hole(Class::RoleOrScope)
        };
    }
    if quorum_short {
        return Exp::Refuse(Class::QuorumShort);
    }
    Exp::Admit
}

/// `member_managed`: "admin in scope [plan]; private to managed needs the member's own device as cosigner;
/// managed to private needs the quorum". `to_managed` says which way.
pub fn member_managed(
    signer: Role,
    subject: Role,
    to_managed: bool,
    consent: bool,
    q_met: bool,
) -> Exp {
    use Role::*;
    if !may_sign(OpKind::MemberManaged, signer) {
        return Exp::Refuse(Class::RoleOrScope);
    }
    let consent_missing = to_managed && !consent;
    let quorum_short = !to_managed && !q_met;
    if signer != Owner && subject.rank() > signer.rank() {
        return if consent_missing {
            Exp::Refuse(Class::ConsentMissing)
        } else if quorum_short {
            Exp::Refuse(Class::QuorumShort)
        } else {
            Exp::Hole(Class::RoleOrScope)
        };
    }
    if consent_missing {
        return Exp::Refuse(Class::ConsentMissing);
    }
    if quorum_short {
        return Exp::Refuse(Class::QuorumShort);
    }
    Exp::Admit
}

/// The ops with no subject member: who may sign, and whether the quorum is needed ("policy_set, escrow_set,
/// audit_key_set, admin_quorum_set, escrow_open, delegate: with the policy's quorum").
pub fn account_op(op: OpKind, signer: Role, q_met: bool) -> Exp {
    if !may_sign(op, signer) {
        return Exp::Refuse(Class::RoleOrScope);
    }
    let needs = matches!(
        op,
        OpKind::PolicySet
            | OpKind::EscrowSet
            | OpKind::AuditKeySet
            | OpKind::AdminQuorumSet
            | OpKind::EscrowOpen
            | OpKind::Delegate
    );
    // a group admin sets the policy of a group only: the account's is out of its reach
    if op == OpKind::PolicySet && signer == Role::GroupAdmin {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if needs && !q_met {
        return Exp::Refuse(Class::QuorumShort);
    }
    Exp::Admit
}

/// The group ops of a group admin: in or out of the scope of the group (and the new parent).
pub fn group_op(op: OpKind, signer: Role, rel: Rel) -> Exp {
    if !may_sign(op, signer) {
        return Exp::Refuse(Class::RoleOrScope);
    }
    if signer == Role::GroupAdmin && rel == Rel::OutOfScope {
        return Exp::Refuse(Class::RoleOrScope);
    }
    Exp::Admit
}
