//! The stories: hand-written attack sequences, each step with the outcome the PLAN says it must have, run on
//! the model and on the production check (planning/architecture.md section 12 E1 and ARCH29 9.13 X-D-54:
//! "S47a's stories attack the table itself"; lane W1-S47a).
//!
//! A [`Story`] is a [`Rig`] with a clock and a numbered log. Every step is run through the model, the
//! metadata group's check and a device's replay (they must agree, [`Rig::step`]) and then compared with the
//! outcome the author wrote down from the plan: `Expect::Admit`, or `Expect::Refuse(class)` where the class is
//! one the model finds (the plan gives no order of checks). A [`World`] is the team every story starts from:
//! an owner, two admins, a group admin, a helpdesk member, a member and a guest, and a small department tree.
//!
//! The functions return `Err(text)` instead of panicking; a test returns it, so the numbered log of the story
//! is the failure message.

use std::collections::BTreeSet;

use azlin_seams::Stamp;

use crate::compare::{Rig, StepReport};
use crate::gen::Step;
use crate::model::{
    AcctKind, Class, CosigClaim, Dev, Grp, Key, MKind, MOp, MSigner, Mem, ModelEntry, OpKind, Role,
    Verdict,
};

/// The outcome a story step must have, from the plan.
///
/// Implements planning/architecture.md section 12 E1; Caller: `Story::run`, the story tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// The entry is accepted.
    Admit,
    /// The entry is refused, for a reason of this class.
    Refuse(Class),
}

/// A rig with a clock and a numbered log.
///
/// Implements planning/architecture.md section 12 E1; Caller: the story tests.
#[derive(Clone)]
pub struct Story {
    /// The model, the production and the device's log.
    pub rig: Rig,
    /// The leader's stamp of the next step.
    pub now: Stamp,
    eid: u64,
    /// The numbered log of the steps run so far.
    pub lines: Vec<String>,
}

impl Story {
    /// A story on an empty account of the kind.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: the story tests, `World`.
    pub fn new(kind: AcctKind) -> Story {
        Story {
            rig: Rig::new(kind, 1),
            now: Stamp::from_secs(1_000_000),
            eid: 0,
            lines: Vec::new(),
        }
    }

    /// Moves the clock on by `secs`.
    ///
    /// Implements planning/architecture.md section 2.4 (time is the leader's stamp); Caller: the story
    /// tests.
    pub fn tick(&mut self, secs: u64) {
        self.now = Stamp::from_secs(self.now.as_secs().saturating_add(secs));
    }

    /// Sets the clock.
    ///
    /// Implements planning/architecture.md section 2.4; Caller: the story tests.
    pub fn set_clock(&mut self, secs: u64) {
        self.now = Stamp::from_secs(secs);
    }

    fn next_eid(&mut self) -> u64 {
        self.eid = self.eid.saturating_add(1);
        self.eid
    }

    /// An entry signed by a device.
    ///
    /// Implements planning/architecture.md section 5.9; Caller: the story tests.
    pub fn e(&mut self, signer: Dev, op: MOp) -> ModelEntry {
        let id = self.next_eid();
        ModelEntry::new(id, MSigner::Device(signer), op)
    }

    /// An entry signed by a delegate key.
    ///
    /// Implements planning/architecture.md section 7.3; Caller: the story tests.
    pub fn e_delegate(&mut self, key: Key, op: MOp) -> ModelEntry {
        let id = self.next_eid();
        ModelEntry::new(id, MSigner::Delegate(key), op)
    }

    /// An entry signed by the payments key.
    ///
    /// Implements planning/architecture.md section 5.9 (`entitlement`); Caller: the story tests.
    pub fn e_payments(&mut self, op: MOp) -> ModelEntry {
        let id = self.next_eid();
        ModelEntry::new(id, MSigner::Payments, op)
    }

    /// An entry with cosignatures by devices, every one valid.
    ///
    /// Implements planning/architecture.md section 5.9 (`cosigs`); Caller: the story tests.
    pub fn with_cosigs(mut e: ModelEntry, devices: &[Dev]) -> ModelEntry {
        for d in devices {
            e.cosigs.push((*d, CosigClaim::Valid));
        }
        e
    }

    /// Runs one step one second after the last and compares it with the plan's outcome. `what` is the
    /// sentence of the story ("the admin makes himself owner"). The step is run on the model, on the
    /// metadata group's check and on a device's replay; any disagreement between them, or between them and
    /// `expect`, is an `Err` holding the numbered log.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: the story tests.
    pub fn run(&mut self, what: &str, e: ModelEntry, expect: Expect) -> Result<StepReport, String> {
        self.tick(1);
        let report = self.rig.step(&e, self.now);
        let n = self.lines.len() + 1;
        let line = format!(
            "{n}. {what}: the plan says {expect:?}; model {:?} {:?}, production {:?}{}, replay {:?}",
            report.model,
            report.model_classes,
            report.prod,
            match &report.prod_refusal {
                Some(r) => format!(" ({r})"),
                None => String::new(),
            },
            report.replay
        );
        self.lines.push(line.clone());
        if !report.agrees() {
            return Err(self.failure(&line, &report.problems.join("; ")));
        }
        let met = match expect {
            Expect::Admit => report.prod == Verdict::Admit,
            Expect::Refuse(c) => {
                matches!(report.prod, Verdict::Refuse(_)) && report.model_classes.contains(&c)
            }
        };
        if !met {
            return Err(self.failure(&line, "the outcome is not the one the plan gives"));
        }
        Ok(report)
    }

    /// Runs a step that the plan says is admitted.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: the story tests.
    pub fn admit(&mut self, what: &str, e: ModelEntry) -> Result<StepReport, String> {
        self.run(what, e, Expect::Admit)
    }

    /// Runs a step that the plan says is refused with a reason of the class.
    ///
    /// Implements planning/architecture.md section 12 E1; Caller: the story tests.
    pub fn refuse(
        &mut self,
        what: &str,
        class: Class,
        e: ModelEntry,
    ) -> Result<StepReport, String> {
        self.run(what, e, Expect::Refuse(class))
    }

    /// The numbered log with a verdict line, as the message of a failed story.
    pub fn failure(&self, line: &str, why: &str) -> String {
        format!(
            "STORY FAILED at {line}\n  because: {why}\nthe story so far:\n{}\n",
            self.lines.join("\n")
        )
    }

    /// Offers the production check (and nothing else) a converted entry that a test has changed after the
    /// signatures were made: a copy with other typed fields than the bytes that were signed. The model has
    /// no claim for such an entry, so the answer is the production's alone: `Admit`, or the class of its
    /// refusal. Nothing is applied.
    ///
    /// Implements planning/architecture.md section 5.9 ("every entry is signed by a device whose role
    /// allows it"; "an entry that fails its hash or signature check is fetched again"); Caller: the story
    /// test `story_old_valid_entry_replayed_at_a_new_seq`.
    pub fn probe_production(&self, raw: &azlin_teamlog::RawEntry, now: Stamp) -> Verdict {
        self.rig.probe(raw, now)
    }
}

/// A named actor of the world: its member and its first device.
///
/// Implements planning/architecture.md section 7.2; Caller: the story tests.
#[derive(Clone, Copy, Debug)]
pub struct Actor {
    /// The member.
    pub mem: Mem,
    /// The device the member joined with.
    pub dev: Dev,
}

/// The team every story starts from (an account of kind team: admin quorum 1, so admins can be made; a story
/// that needs more raises the quorum). Nine members could be made; seven are: an owner, two admins, a group
/// admin whose scope is the engineering group, a helpdesk member whose scope is the same group, a member
/// and a guest. The group tree: `root` with the children `eng` and `ops`, and a second top-level group
/// `other`; the group admin and the helpdesk member belong to `eng`, the member to `eng`, the guest to `ops`
/// and both admins to `other`.
///
/// Implements planning/architecture.md section 7.2 (roles scoped to a group subtree); Caller: the story
/// tests.
#[derive(Clone)]
pub struct World {
    /// The story (the rig and its log).
    pub s: Story,
    /// The first owner.
    pub owner: Actor,
    /// The first admin.
    pub admin_a: Actor,
    /// The second admin.
    pub admin_b: Actor,
    /// The group admin, scope `eng`.
    pub group_admin: Actor,
    /// The helpdesk member, scope `eng`.
    pub helpdesk: Actor,
    /// A member, in `eng`.
    pub member: Actor,
    /// A guest, in `ops`.
    pub guest: Actor,
    /// The top group.
    pub root: Grp,
    /// A department under `root`.
    pub eng: Grp,
    /// Another department under `root`.
    pub ops: Grp,
    /// A second top-level group.
    pub other: Grp,
}

impl World {
    /// Builds the team through real entries, each of which must be admitted (a failure to build is an
    /// `Err`).
    ///
    /// Implements planning/architecture.md section 7.2; Caller: the story tests.
    pub fn team() -> Result<World, String> {
        World::build(AcctKind::Team)
    }

    /// Builds the world on an account of the given kind. A business cannot be built this way: its admin
    /// quorum is 2 from the first entry, and only one person exists (the story
    /// `story_business_owner_cannot_make_the_first_admin` shows it).
    ///
    /// Implements planning/architecture.md section 5.10; Caller: `World::team`.
    pub fn build(kind: AcctKind) -> Result<World, String> {
        let mut s = Story::new(kind);
        let actor = |n: u16| Actor {
            mem: Mem(n),
            dev: Dev(n * 10 + 1),
        };
        let owner = actor(1);
        let admin_a = actor(2);
        let admin_b = actor(3);
        let group_admin = actor(4);
        let helpdesk = actor(5);
        let member = actor(6);
        let guest = actor(7);
        let (root, eng, ops, other) = (Grp(1), Grp(2), Grp(3), Grp(4));

        let e = s.e(
            owner.dev,
            MOp::Genesis {
                owner: owner.mem,
                device: owner.dev,
            },
        );
        s.admit("the genesis makes the first owner", e)?;
        for (g, parent) in [
            (root, None),
            (eng, Some(root)),
            (ops, Some(root)),
            (other, None),
        ] {
            let e = s.e(owner.dev, MOp::GroupCreate { group: g, parent });
            s.admit("the owner makes a group", e)?;
        }
        let plan: [(Actor, Role, Vec<Grp>, Option<Grp>); 6] = [
            (admin_a, Role::Admin, vec![other], None),
            (admin_b, Role::Admin, vec![other], None),
            (group_admin, Role::GroupAdmin, vec![eng], Some(eng)),
            (helpdesk, Role::Helpdesk, vec![eng], Some(eng)),
            (member, Role::Member, vec![eng], None),
            (guest, Role::Guest, vec![ops], None),
        ];
        for (n, (who, role, groups, scope)) in plan.iter().enumerate() {
            let secret = 1000 + n as u32;
            let invite_role = if *role == Role::Guest {
                Role::Guest
            } else {
                Role::Member
            };
            let e = s.e(
                owner.dev,
                MOp::MemberInvite {
                    member: who.mem,
                    kind: MKind::Person,
                    role: invite_role,
                    secret,
                    groups: groups.iter().copied().collect::<BTreeSet<Grp>>(),
                    app: None,
                },
            );
            s.admit("the owner invites a member", e)?;
            let join = s.e(
                who.dev,
                MOp::MemberJoin {
                    member: who.mem,
                    device: who.dev,
                    secret,
                },
            );
            let join = Story::with_cosigs(join, &[owner.dev]);
            s.admit("the invitee joins, the inviter confirms", join)?;
            if !matches!(role, Role::Member | Role::Guest) {
                let e = s.e(
                    owner.dev,
                    MOp::MemberRole {
                        member: who.mem,
                        role: *role,
                        scope: *scope,
                    },
                );
                s.admit("the owner gives the role", e)?;
            }
        }
        Ok(World {
            s,
            owner,
            admin_a,
            admin_b,
            group_admin,
            helpdesk,
            member,
            guest,
            root,
            eng,
            ops,
            other,
        })
    }

    /// The owner sets the account's admin quorum (the owner is alone, so the current quorum must be 1).
    ///
    /// Implements planning/architecture.md section 5.10 (`admin_quorum`); Caller: the story tests.
    pub fn set_quorum(&mut self, quorum: u8) -> Result<(), String> {
        let e = self.s.e(self.owner.dev, MOp::AdminQuorumSet { quorum });
        let e = Story::with_cosigs(e, &[]);
        self.s.admit("the owner sets the admin quorum", e)?;
        Ok(())
    }

    /// A second device for a member by the member's own device (no hold).
    ///
    /// Implements planning/architecture.md section 7.5 (the person's own existing device approves); Caller:
    /// the story tests.
    pub fn add_own_device(&mut self, who: Actor, device: Dev) -> Result<(), String> {
        let e = self.s.e(
            who.dev,
            MOp::DeviceAdd {
                member: who.mem,
                device,
            },
        );
        self.s.admit("the member adds a device of its own", e)?;
        Ok(())
    }

    /// Whether the model's state holds this member in this role.
    ///
    /// Implements planning/architecture.md section 7.2; Caller: the story tests.
    pub fn role_of(&self, m: Mem) -> Option<Role> {
        self.s.rig.model.members.get(&m).map(|x| x.role)
    }

    /// Whether this op kind is among the ones the delegation of `key` names.
    ///
    /// Implements planning/architecture.md section 7.3; Caller: the story tests.
    pub fn delegation_names(&self, key: Key, op: OpKind) -> bool {
        self.s
            .rig
            .model
            .delegations
            .get(&key)
            .is_some_and(|d| d.ops.contains(&op))
    }
}

/// A one-step helper for the tests: the entry of a step.
///
/// Implements planning/architecture.md section 12 E1; Caller: the tests.
pub fn step_at(entry: ModelEntry, now: Stamp) -> Step {
    Step { entry, now }
}
