//! Check 1 of the brief on the PRODUCTION: the cells of `common/cells.rs` (every op, signer role, place and
//! rank of the subject, quorum met or short) run on a team built through real entries, and the production's
//! verdict equals the authority table v0's (typed as data in `table_data.rs`), the model's and a device's
//! replay's. This is E1's claim for the whole grid at once: "no entry is accepted unless its signer may sign
//! it; a quorum counts distinct people, never one person's devices" (planning/architecture.md section 12 E1).
//!
//! A cell where the LITERAL table admits and the reading of lane S17c refuses (`Exp::Hole`) must be refused by
//! the production too: the production implements the stricter reading (drift row X-DELTA-W1S47A-4).

mod common;

use std::collections::BTreeSet;

use azlin_escalate_e1::model::{AcctKind, Grp, MKind, MOp, Mem, Role, Verdict};
use azlin_escalate_e1::stories::Story;
use azlin_seams::Stamp;
use common::cells::{cells, dev_of, signer_mem, subject_mem};
use common::table_data::Exp;

/// The fixture team of `common/cells.rs` as a real account: made by the genesis owner through invitations,
/// joins and role changes (an account of kind team starts with the admin quorum 1, so one owner can make
/// admins and owners), then the quorum is raised to 2 and, for the second copy, the subjects are made managed
/// with their own devices' consent.
fn build_world(managed_subjects: bool) -> Result<Story, String> {
    let mut s = Story::new(AcctKind::Team);
    let owner = signer_mem(Role::Owner);
    let owner_dev = dev_of(owner);
    let e = s.e(
        owner_dev,
        MOp::Genesis {
            owner,
            device: owner_dev,
        },
    );
    s.admit("the genesis", e)?;
    for (g, parent) in [(Grp(1), None), (Grp(2), Some(Grp(1))), (Grp(3), None)] {
        let e = s.e(owner_dev, MOp::GroupCreate { group: g, parent });
        s.admit("a group", e)?;
    }
    let mut people: Vec<(Mem, Role, Option<Grp>)> = Vec::new();
    for r in Role::ALL {
        if r != Role::Owner {
            people.push((signer_mem(r), r, Some(Grp(1))));
        }
        people.push((subject_mem(r, true), r, Some(Grp(2))));
        people.push((subject_mem(r, false), r, Some(Grp(3))));
    }
    people.push((Mem(90), Role::Member, None));
    for (n, (m, role, group)) in people.iter().enumerate() {
        let secret = 5000 + n as u32;
        let invite_role = if *role == Role::Guest {
            Role::Guest
        } else {
            Role::Member
        };
        let groups: BTreeSet<Grp> = group.iter().copied().collect();
        let e = s.e(
            owner_dev,
            MOp::MemberInvite {
                member: *m,
                kind: MKind::Person,
                role: invite_role,
                secret,
                groups,
                app: None,
            },
        );
        s.admit("an invitation", e)?;
        let e = s.e(
            dev_of(*m),
            MOp::MemberJoin {
                member: *m,
                device: dev_of(*m),
                secret,
            },
        );
        let e = Story::with_cosigs(e, &[owner_dev]);
        s.admit("a join", e)?;
        if !matches!(role, Role::Member | Role::Guest) {
            let scope = if role.needs_scope() {
                Some(Grp(1))
            } else {
                None
            };
            let e = s.e(
                owner_dev,
                MOp::MemberRole {
                    member: *m,
                    role: *role,
                    scope,
                },
            );
            s.admit("a role", e)?;
        }
    }
    let e = s.e(owner_dev, MOp::AdminQuorumSet { quorum: 2 });
    s.admit("the admin quorum 2", e)?;
    if managed_subjects {
        for r in Role::ALL {
            for in_scope in [true, false] {
                let m = subject_mem(r, in_scope);
                let e = s.e(
                    owner_dev,
                    MOp::MemberManaged {
                        member: m,
                        managed: true,
                    },
                );
                let e = Story::with_cosigs(e, &[dev_of(m)]);
                s.admit("a subject is made managed", e)?;
            }
        }
    }
    Ok(s)
}

#[test]
fn production_matches_the_v0_table_cell_by_cell() {
    let private = build_world(false).expect("the team of private subjects");
    let managed = build_world(true).expect("the team of managed subjects");
    let mut bad: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut holes = 0usize;
    for cell in cells() {
        let base = if cell.managed_world {
            &managed
        } else {
            &private
        };
        let mut rig = base.rig.clone();
        let now = Stamp::from_secs(base.now.as_secs() + 10);
        let r = rig.step(&cell.entry, now);
        checked += 1;
        if !r.agrees() {
            bad.push(format!(
                "{}: model, production and replay disagree: {}",
                cell.label,
                r.problems.join("; ")
            ));
            continue;
        }
        match cell.exp {
            Exp::Admit => {
                if r.prod != Verdict::Admit {
                    bad.push(format!(
                        "{}: the table admits, the production says {:?} ({:?})",
                        cell.label, r.prod, r.prod_refusal
                    ));
                }
            }
            Exp::Refuse(c) | Exp::Hole(c) => {
                if matches!(cell.exp, Exp::Hole(_)) {
                    holes += 1;
                }
                if !matches!(r.prod, Verdict::Refuse(_)) || !r.model_classes.contains(&c) {
                    bad.push(format!(
                        "{}: the table refuses with {c:?}, the production says {:?} ({:?}), the model {:?}",
                        cell.label, r.prod, r.prod_refusal, r.model_classes
                    ));
                }
            }
        }
    }
    println!(
        "production: {checked} cells checked, {holes} of them cells where the literal table admits and the production refuses (the rank rule)"
    );
    assert!(
        bad.is_empty(),
        "{} of {checked} cells differ from the table:\n{}",
        bad.len(),
        bad.iter().take(60).cloned().collect::<Vec<_>>().join("\n")
    );
}
