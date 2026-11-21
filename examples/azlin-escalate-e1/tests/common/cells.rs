//! The cells of check 1 of the brief: every op, every signer role, every place of the subject (in scope, out
//! of scope, the signer itself), every rank of the subject (the six roles) and the quorum met or short, as
//! entries over the ids of one fixture team, each with the outcome the authority table v0 gives it (the data
//! of `tests/table_data.rs`, typed from the brief's table, not from the model).
//!
//! The fixture team (planning/architecture.md section 7.2): six SIGNERS, one of each role, all in group 1;
//! twelve SUBJECTS, one of each role in group 2 (inside the subtree of group 1, which is the scope of the
//! group admin and of the helpdesk member) and one of each role in group 3 (outside it); a member of no
//! group. The admin quorum is 2. Three owners exist, so no cell is about the last owner. Two copies of the
//! team exist: one whose subjects are private and one whose subjects are managed.

#![allow(dead_code)]

use std::collections::BTreeSet;

use azlin_escalate_e1::model::{
    AppId, CosigClaim, Dev, Grp, Key, MAssertion, MKind, MOp, MPatch, MSigner, Mem, ModelEntry,
    OpKind, PScope, Reason, Role,
};

use super::table_data::{self, Exp, Rel};

/// The place of a role in the fixture.
pub fn idx(r: Role) -> u16 {
    match r {
        Role::Owner => 0,
        Role::Admin => 1,
        Role::GroupAdmin => 2,
        Role::Helpdesk => 3,
        Role::Member => 4,
        Role::Guest => 5,
    }
}

/// The signer of a role.
pub fn signer_mem(r: Role) -> Mem {
    Mem(10 + idx(r))
}

/// The subject of a role, inside or outside the scope of group 1.
pub fn subject_mem(r: Role, in_scope: bool) -> Mem {
    Mem(30 + idx(r) * 2 + if in_scope { 0 } else { 1 })
}

/// The device of a member of the fixture.
pub fn dev_of(m: Mem) -> Dev {
    Dev(m.0 * 10)
}

/// A device that is a cosigner of a quorum entry of this signer: an admin, or the owner for an admin.
pub fn cosigner(signer: Role) -> Dev {
    if signer == Role::Admin {
        dev_of(signer_mem(Role::Owner))
    } else {
        dev_of(signer_mem(Role::Admin))
    }
}

fn entry(signer: Role, op: MOp, q_met: bool) -> ModelEntry {
    let mut e = ModelEntry::new(1, MSigner::Device(dev_of(signer_mem(signer))), op);
    if q_met {
        e.cosigs.push((cosigner(signer), CosigClaim::Valid));
    }
    e
}

/// Every subject of a signer: the six roles in scope, the six out of scope, the signer itself.
fn subjects(signer: Role) -> Vec<(Role, Rel, Mem)> {
    let mut out = Vec::new();
    for sr in Role::ALL {
        out.push((sr, Rel::InScope, subject_mem(sr, true)));
        out.push((sr, Rel::OutOfScope, subject_mem(sr, false)));
    }
    out.push((signer, Rel::SelfSubject, signer_mem(signer)));
    out
}

/// One cell: an entry over the fixture, the table's outcome for it and the copy of the team it runs on.
pub struct Cell {
    /// What the cell is, in words.
    pub label: String,
    /// Whether the entry runs on the team whose subjects are managed.
    pub managed_world: bool,
    /// The entry.
    pub entry: ModelEntry,
    /// What the table says.
    pub exp: Exp,
}

/// All cells.
pub fn cells() -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::new();
    let mut add = |label: String, managed_world: bool, entry: ModelEntry, exp: Exp| {
        out.push(Cell {
            label,
            managed_world,
            entry,
            exp,
        });
    };
    for signer in Role::ALL {
        for q_met in [true, false] {
            let q = if q_met { "quorum met" } else { "quorum short" };
            for (sr, rel, subj) in subjects(signer) {
                // member_role, to each of the six roles
                for new in Role::ALL {
                    let op = MOp::MemberRole {
                        member: subj,
                        role: new,
                        scope: if new.needs_scope() {
                            Some(Grp(2))
                        } else {
                            None
                        },
                    };
                    add(
                        format!("member_role {signer:?} -> {sr:?} {rel:?} to {new:?}, {q}"),
                        false,
                        entry(signer, op, q_met),
                        table_data::member_role(signer, sr, rel, new, q_met),
                    );
                }
                if rel != Rel::SelfSubject {
                    add(
                        format!("member_suspend {signer:?} -> {sr:?} {rel:?}, {q}"),
                        false,
                        entry(signer, MOp::MemberSuspend { member: subj }, q_met),
                        table_data::member_suspend(signer, sr, rel, q_met),
                    );
                    add(
                        format!("member_remove {signer:?} -> {sr:?} {rel:?}, {q}"),
                        false,
                        entry(signer, MOp::MemberRemove { member: subj }, q_met),
                        table_data::member_remove(signer, sr, rel, q_met),
                    );
                    // private -> managed, with and without the member's own device
                    for consent in [true, false] {
                        let mut e = entry(
                            signer,
                            MOp::MemberManaged {
                                member: subj,
                                managed: true,
                            },
                            false,
                        );
                        if consent {
                            e.cosigs.push((dev_of(subj), CosigClaim::Valid));
                        }
                        add(
                            format!(
                                "member_managed {signer:?} -> {sr:?} {rel:?} to managed, consent {consent}, {q}"
                            ),
                            false,
                            e,
                            table_data::member_managed(signer, sr, true, consent, true),
                        );
                    }
                    // managed -> private needs the quorum
                    add(
                        format!("member_managed {signer:?} -> {sr:?} {rel:?} to private, {q}"),
                        true,
                        entry(
                            signer,
                            MOp::MemberManaged {
                                member: subj,
                                managed: false,
                            },
                            q_met,
                        ),
                        table_data::member_managed(signer, sr, false, true, q_met),
                    );
                }
                // device_add (the quorum path for another's member, the own path for the signer)
                add(
                    format!("device_add {signer:?} -> {sr:?} {rel:?}, {q}"),
                    false,
                    entry(
                        signer,
                        MOp::DeviceAdd {
                            member: subj,
                            device: Dev(5000),
                        },
                        q_met,
                    ),
                    table_data::device_add(signer, sr, rel, q_met),
                );
                // device_remove of the subject's device
                let exp = if rel == Rel::SelfSubject {
                    Exp::Admit
                } else {
                    table_data::device_remove(signer, sr, q_met)
                };
                add(
                    format!("device_remove {signer:?} -> {sr:?} {rel:?}, {q}"),
                    false,
                    entry(
                        signer,
                        MOp::DeviceRemove {
                            device: dev_of(subj),
                        },
                        q_met,
                    ),
                    exp,
                );
            }
            // the account's own ops
            for (op, mop) in [
                (
                    OpKind::PolicySet,
                    MOp::PolicySet {
                        scope: PScope::Account,
                        patch: MPatch {
                            audit_level: Some(2),
                            ..MPatch::default()
                        },
                    },
                ),
                (
                    OpKind::EscrowSet,
                    MOp::EscrowSet {
                        key: 1,
                        holders: BTreeSet::new(),
                    },
                ),
                (OpKind::AuditKeySet, MOp::AuditKeySet { key: 1 }),
                (OpKind::AdminQuorumSet, MOp::AdminQuorumSet { quorum: 3 }),
                (
                    OpKind::Delegate,
                    MOp::Delegate {
                        key: Key(1),
                        ops: [OpKind::MemberSuspend].into_iter().collect(),
                        expires: None,
                    },
                ),
            ] {
                add(
                    format!("{} {signer:?}, {q}", op.name()),
                    false,
                    entry(signer, mop, q_met),
                    table_data::account_op(op, signer, q_met),
                );
            }
            // escrow_open needs the signers' passkey assertions too
            let mut e = entry(
                signer,
                MOp::EscrowOpen {
                    member: subject_mem(Role::Member, true),
                    reason: Reason::LegalRequest,
                },
                q_met,
            );
            let signing: Vec<Dev> = std::iter::once(dev_of(signer_mem(signer)))
                .chain(e.cosigs.iter().map(|c| c.0))
                .collect();
            for dev in signing {
                e.assertions.push(MAssertion {
                    dev,
                    user_verified: true,
                    challenge_ok: true,
                    rp_ok: true,
                });
            }
            add(
                format!("escrow_open {signer:?}, {q}"),
                true,
                e,
                table_data::account_op(OpKind::EscrowOpen, signer, q_met),
            );
            // the group ops, in and out of the signer's scope
            for rel in [Rel::InScope, Rel::OutOfScope] {
                let (parent, group) = if rel == Rel::InScope {
                    (Grp(2), Grp(2))
                } else {
                    (Grp(3), Grp(3))
                };
                add(
                    format!("group_create {signer:?} {rel:?}, {q}"),
                    false,
                    entry(
                        signer,
                        MOp::GroupCreate {
                            group: Grp(9),
                            parent: Some(parent),
                        },
                        q_met,
                    ),
                    table_data::group_op(OpKind::GroupCreate, signer, rel),
                );
                add(
                    format!("group_member {signer:?} {rel:?}, {q}"),
                    false,
                    entry(
                        signer,
                        MOp::GroupMember {
                            group,
                            member: Mem(90),
                            present: true,
                        },
                        q_met,
                    ),
                    table_data::group_op(OpKind::GroupMember, signer, rel),
                );
                // group_move: group 2 (inside group 1) to a new parent in or out of the scope
                let new_parent = if rel == Rel::InScope { Grp(1) } else { Grp(3) };
                add(
                    format!("group_move {signer:?} {rel:?}, {q}"),
                    false,
                    entry(
                        signer,
                        MOp::GroupMove {
                            group: Grp(2),
                            new_parent,
                        },
                        q_met,
                    ),
                    table_data::group_op(OpKind::GroupMove, signer, rel),
                );
                // policy_set of a group: tighten
                let exp = match table_data::group_op(OpKind::PolicySet, signer, rel) {
                    Exp::Admit if !q_met => {
                        Exp::Refuse(azlin_escalate_e1::model::Class::QuorumShort)
                    }
                    other => other,
                };
                add(
                    format!("policy_set(group) {signer:?} {rel:?}, {q}"),
                    false,
                    entry(
                        signer,
                        MOp::PolicySet {
                            scope: PScope::Group(if rel == Rel::InScope { Grp(2) } else { Grp(3) }),
                            patch: MPatch {
                                audit_level: Some(2),
                                ..MPatch::default()
                            },
                        },
                        q_met,
                    ),
                    exp,
                );
                // member_invite into a group in or out of the scope
                add(
                    format!("member_invite {signer:?} {rel:?}, {q}"),
                    false,
                    entry(
                        signer,
                        MOp::MemberInvite {
                            member: Mem(95),
                            kind: MKind::Person,
                            role: Role::Member,
                            secret: 1,
                            groups: [if rel == Rel::InScope { Grp(2) } else { Grp(3) }]
                                .into_iter()
                                .collect(),
                            app: None,
                        },
                        q_met,
                    ),
                    table_data::group_op(OpKind::MemberInvite, signer, rel),
                );
            }
            // app ops: everybody but a guest
            add(
                format!("app {signer:?}, {q}"),
                false,
                entry(
                    signer,
                    MOp::App {
                        name: AppId(1),
                        body_len: 4,
                    },
                    false,
                ),
                if table_data::may_sign(OpKind::App, signer) {
                    Exp::Admit
                } else {
                    Exp::Refuse(azlin_escalate_e1::model::Class::RoleOrScope)
                },
            );
        }
    }
    out
}
