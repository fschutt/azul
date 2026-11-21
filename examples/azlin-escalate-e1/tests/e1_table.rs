//! Check 1 of the brief: THE MODEL AGAINST THE TABLE. For every op, every signer role, every place of the
//! subject (in scope, out of scope, the signer itself), every rank of the subject (the six roles) and the
//! quorum met or short, the model's verdict equals the table's. The table is typed as data in
//! `table_data.rs` from the brief's table, NOT from the model (planning/architecture.md section 5.9 "Who may
//! sign", 7.2, 7.3, 7.5; section 12 E1). The cells are listed in `common/cells.rs`; the same cells run on the
//! production in `e1_table_prod.rs`.
//!
//! The cells where the LITERAL table admits an entry that the provisional reading of lane S17c refuses
//! ([S17c-8] a signer never acts on a member above its own rank, [S17c-9] a group admin's reach) are printed
//! as the holes of the table: they are the findings of drift row X-DELTA-W1S47A-4.

mod common;

use std::collections::BTreeSet;

use azlin_escalate_e1::model::{
    model_admit, model_violations, AcctKind, DState, Enrolled, Grp, MDevice, MKind, MMember,
    MState, Mem, ModelEntry, ModelState, Role, Verdict,
};
use azlin_seams::Stamp;
use common::cells::{cells, dev_of, signer_mem, subject_mem};
use common::table_data::Exp;

const NOW: Stamp = Stamp::from_secs(10_000);

fn add(s: &mut ModelState, id: Mem, role: Role, group: Option<Grp>, managed: bool) {
    s.members.insert(
        id,
        MMember {
            id,
            kind: MKind::Person,
            role,
            scope: if role.needs_scope() {
                Some(Grp(1))
            } else {
                None
            },
            managed,
            state: MState::Active,
            has_keys: true,
            invite: None,
            app: None,
            added_at: Stamp::EPOCH,
            removed_at: None,
        },
    );
    s.devices.insert(
        dev_of(id),
        MDevice {
            id: dev_of(id),
            member: id,
            state: DState::Active,
            enrolled: Enrolled::Invite,
            added_at: Stamp::EPOCH,
            revoked_at: None,
        },
    );
    if let Some(g) = group {
        s.memberships.insert((g, id));
    }
}

/// The fixture team as a model state (see `common/cells.rs`).
fn fixture(managed_subjects: bool) -> ModelState {
    let mut s = ModelState::new(AcctKind::Team);
    s.policy.admin_quorum = 2;
    s.head = Some((0, 1));
    s.groups.insert(Grp(1), None);
    s.groups.insert(Grp(2), Some(Grp(1)));
    s.groups.insert(Grp(3), None);
    for r in Role::ALL {
        add(&mut s, signer_mem(r), r, Some(Grp(1)), false);
        add(
            &mut s,
            subject_mem(r, true),
            r,
            Some(Grp(2)),
            managed_subjects,
        );
        add(
            &mut s,
            subject_mem(r, false),
            r,
            Some(Grp(3)),
            managed_subjects,
        );
    }
    add(&mut s, Mem(90), Role::Member, None, false);
    s
}

struct Tally {
    cells: usize,
    bad: Vec<String>,
    holes: BTreeSet<String>,
}

impl Tally {
    fn check(&mut self, label: String, s: &ModelState, e: &ModelEntry, exp: Exp) {
        self.cells += 1;
        let got = model_violations(s, e, NOW);
        let verdict = model_admit(s, e, NOW);
        if (verdict == Verdict::Admit) != got.is_empty() {
            self.bad.push(format!(
                "{label}: model_admit and model_violations disagree"
            ));
        }
        match exp {
            Exp::Admit => {
                if !got.is_empty() {
                    self.bad.push(format!(
                        "{label}: the table admits, the model refuses {got:?}"
                    ));
                }
            }
            Exp::Refuse(c) => {
                if !got.contains(&c) {
                    self.bad.push(format!(
                        "{label}: the table refuses with {c:?}, the model says {got:?}"
                    ));
                }
            }
            Exp::Hole(c) => {
                if !got.contains(&c) {
                    self.bad.push(format!(
                        "{label}: the literal table admits, the reading refuses with {c:?}, the model says {got:?}"
                    ));
                }
                self.holes.insert(
                    label
                        .replace(" quorum met", "")
                        .replace(" quorum short", ""),
                );
            }
        }
    }
}

#[test]
fn model_matches_the_v0_table_cell_by_cell() {
    let mut t = Tally {
        cells: 0,
        bad: Vec::new(),
        holes: BTreeSet::new(),
    };
    let private = fixture(false);
    let managed = fixture(true);
    for cell in cells() {
        let state = if cell.managed_world {
            &managed
        } else {
            &private
        };
        t.check(cell.label, state, &cell.entry, cell.exp);
    }
    println!(
        "table: {} cells checked; {} cells where the literal table admits and the reading refuses:",
        t.cells,
        t.holes.len()
    );
    for h in &t.holes {
        println!("  hole: {h}");
    }
    assert!(
        t.bad.is_empty(),
        "{} of {} cells differ from the table:\n{}",
        t.bad.len(),
        t.cells,
        t.bad
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
