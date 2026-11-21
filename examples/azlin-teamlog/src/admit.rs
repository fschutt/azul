//! `admit`: THE PERMISSION CHECK of the team log (planning/architecture.md section 5.9 "The metadata
//! group rejects an entry with a reason for a wrong `prev`, a signer without the role or scope, a missing
//! quorum or a bad signature. The server enforces only what was signed"; section 7.2; section 12 E1 "no
//! entry is accepted unless its signer may sign it; a quorum counts distinct people, never one person's
//! devices"; build unit S17, lane W1-S17c2).
//!
//! `admit(state, proposal, now)` says whether an entry may enter the account's log. It is a pure
//! function of the state, the proposal (an entry whose signatures hold) and the leader's stamp `now`
//! (section 2.4): it changes nothing, reads no clock, and never needs the seal key. The metadata group
//! runs it before it proposes an entry to Raft ("No entry can stop the group") and again when it
//! applies; every device runs it on every entry it verifies.
//!
//! THE ORDER OF THE CHECKS (the first that fails is the reason; the matrix test fixes it):
//!
//! 1. the chain: the account, the position, `prev`, the leader's stamp, the restore epoch (below the
//!    floor: `Refusal::Chain`; above it: `Refusal::EpochNotAnnounced`, only an announced restore moves
//!    the floor); the first entry is a genesis and nothing but the first entry is;
//! 2. the signer: its device exists and is Active (or Pending with its hold over) and its member is
//!    Active (`SignerUnknown`, `SignerNotActive`, `MemberNotActive`); a delegate key has a live
//!    delegation that names the op (`DelegationDoesNotCover`, `DelegationExpired`); the payments key
//!    signs an `entitlement` and nothing else;
//! 3. the op, in this order: the role gate (`RoleMaySignNo`), the subject (`SubjectUnknown`, a state the
//!    op cannot act on), the scope (`OutOfScope`), the rank rules (`RoleMaySignNo`), the conditions of
//!    the op (`LastOwner`, `GroupCycle`, `PolicyForbids`, `InviteInvalid`, `Invalid`), and last the
//!    quorum (`QuorumShort`, `AssertionMissing`).
//!
//! The table of who may sign what is `authority.rs` (data); this file is the procedure that reads it and
//! the conditions no row can say. Every function below quotes the row of the table it implements.

use azlin_chain::{ChainRefusal, Placement, RefusalClass};
use azlin_policy::{
    apply as apply_patch, check_tightens, effective, next_epoch, quorum_met, DeviceApproval,
    Loosened, PolicyPatch, QuorumClass, Signatory,
};
use azlin_seams::{
    mutant, ByteCount, DeviceId, GroupId, MemberId, MemberKind, RestoreEpoch, Stamp,
};

use crate::apply::{Actor, Admitted, EntryMeta, Link};
use crate::authority::{role_assignable, role_touchable, row, Grant, QuorumFacts, Reach, Row};
use crate::delegate::{is_delegable, live_delegation};
use crate::devices::{Device, DeviceState, PublicKeys};
use crate::invite::{check_join, JoinArgs};
use crate::members::{Member, MemberState};
use crate::ops::{
    AppName, EntitlementVersion, EscrowReason, Op, OpName, PolicyScope, UnknownOp, APP_BODY_MAX,
};
use crate::proposal::{Proposal, RawEntry, VerifiedSigner};
use crate::roles::{at_most, is_admin_class, needs_scope, Role};
use crate::state::{
    Delegation, TeamState, MAX_DELEGATIONS, MAX_DEVICES, MAX_ESCROW_HOLDERS, MAX_GROUPS,
    MAX_GROUPS_PER_MEMBER, MAX_GROUP_DEPTH, MAX_MEMBERS, MAX_PENDING_ESCROW_OPENS,
};

/// The ids of the twelve mutation guards of this crate (`azlin_seams::mutant!`, the feature
/// `azlin-seams/mutants`), each at the one decision that a privilege escalation would turn off, with the
/// test of this crate that must fail when the guard is on. The independent tests of lane S47a must catch
/// the same twelve (`e1_catches_every_teamlog_mutant`); the coordinator registers them (GATES-REG).
/// `determinism_tests` reads the source and checks that these are exactly the guards in it.
///
/// | id | what the guard turns off | killer |
/// |---|---|---|
/// | `teamlog_quorum_counts_devices` | "a quorum counts distinct people, never one person's devices" | `quorum_of_two_devices_of_one_person_refused` |
/// | `teamlog_scope_ignores_subtree` | the scope of a group admin and of helpdesk (the subtree of `admin_scope`) | `group_admin_cannot_leave_its_subtree` |
/// | `teamlog_role_assign_unbounded` | who may assign which role, and whose role a signer may touch | `admin_cannot_make_or_touch_an_owner` |
/// | `teamlog_last_owner_check_off` | "at least one Active owner remains" | `last_owner_cannot_be_demoted_removed_or_suspended` |
/// | `teamlog_helpdesk_may_approve` | "never approve alone": helpdesk is no signer of `device_add` | `helpdesk_cannot_approve_a_device_alone` |
/// | `teamlog_delegate_any_op` | "the exact ops it may sign" of a delegation | `delegate_key_signs_only_its_ops` |
/// | `teamlog_pending_device_may_sign` | "a Pending device, one in its hold, signs nothing" | `pending_device_signs_nothing` |
/// | `teamlog_suspended_may_sign` | a suspended member's devices sign nothing | `suspended_member_signs_nothing` |
/// | `teamlog_group_cycle_check_off` | "no cycle" of `group_move` | `group_move_cycle_refused` |
/// | `teamlog_policy_loosen_allowed` | "groups inherit and may only tighten" at `policy_set` | `policy_set_loosening_a_group_refused` |
/// | `teamlog_prev_unchecked` | the `prev` of the chain | `wrong_prev_refused_with_reason` |
/// | `teamlog_cosig_hash_unchecked` | a cosignature counts only over THIS entry's hash | `cosig_over_another_hash_not_counted` |
///
/// Implements planning/architecture.md section 12 (E1) and section 1.4 rule 1 (the guard reads its
/// switch as a parameter of the process).
///
/// Seam: lane S47a (`e1_catches_every_teamlog_mutant`), the mutant hold of this lane, GATES-REG.
pub const MUTANT_IDS: [&str; 12] = [
    "teamlog_quorum_counts_devices",
    "teamlog_scope_ignores_subtree",
    "teamlog_role_assign_unbounded",
    "teamlog_last_owner_check_off",
    "teamlog_helpdesk_may_approve",
    "teamlog_delegate_any_op",
    "teamlog_pending_device_may_sign",
    "teamlog_suspended_may_sign",
    "teamlog_group_cycle_check_off",
    "teamlog_policy_loosen_allowed",
    "teamlog_prev_unchecked",
    "teamlog_cosig_hash_unchecked",
];

/// Why the policy forbids an entry.
///
/// Implements planning/architecture.md section 5.10 (the account policy) and section 5.9 ("a missing
/// quorum").
///
/// Caller: `Refusal::PolicyForbids`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PolicyReason {
    /// A group's policy must tighten and these fields do not: a `policy_set` of a group that loosens what
    /// it inherits, or a `group_move` that makes the effective policy of a group of the moved subtree
    /// looser, signed by a group admin (X-DELTA-W1S47A-2).
    #[error("a group's policy must tighten: {} field(s) do not", .0.len())]
    Loosens(Vec<Loosened>),
    /// `device_approval` is `self`: a quorum of admins cannot enrol a device.
    #[error("device_approval is self: a quorum cannot enrol a device")]
    DeviceApprovalSelfOnly,
    /// The member has the most devices `max_devices_per_member` allows.
    #[error("the member has the most devices the policy allows ({max})")]
    MaxDevices {
        /// The policy's maximum.
        max: u8,
    },
    /// The policy epoch is at its largest number: no further change can be counted.
    #[error("the policy epoch is at its largest number")]
    EpochExhausted,
    /// The entitlement's version is not above the one the account holds.
    #[error("the entitlement is not newer than the one held")]
    EntitlementNotNewer,
    /// The member is not managed: there is no escrow to open.
    #[error("the member is not managed: there is no escrow to open")]
    NotManaged,
}

/// Why an entry is well-formed and signed and still cannot be applied: a structural or state reason
/// that no role or quorum can change.
///
/// Implements planning/architecture.md section 5.9 ("rejects an entry with a reason").
///
/// Caller: `Refusal::Invalid`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Invalid {
    /// The id (member, device, group) is taken: ids are never reused.
    #[error("the id is taken (ids are never reused)")]
    IdTaken,
    /// The subject member is in a state the op cannot act on.
    #[error("the member is {0:?}, which the op cannot act on")]
    WrongMemberState(MemberState),
    /// The device is already revoked.
    #[error("the device is already revoked")]
    DeviceRevoked,
    /// A top-level group never moves.
    #[error("a top-level group never moves")]
    RootGroup,
    /// An argument of the op is malformed or inconsistent.
    #[error("{0}")]
    Args(&'static str),
    /// A limit of the log is reached.
    #[error("a limit of the log is reached: {0}")]
    Limit(&'static str),
    /// The app principal's `expires_at` has passed.
    #[error("the app principal has expired")]
    AppExpired,
    /// The state is not one the entries can have made: a bug or a corrupt snapshot.
    #[error("the state is damaged: {0}")]
    StateDamaged(&'static str),
}

/// The word of a signer's role for a message: a key that is no member has none.
fn role_word(role: &Option<Role>) -> &'static str {
    match role {
        Some(role) => role.as_str(),
        None => "a key that is no member",
    }
}

/// Why an entry is refused (planning/architecture.md section 5.9: "The metadata group rejects an entry
/// with a reason"). The variants of the brief are the first seventeen; `ConsentMissing` and `Invalid`
/// are added for the reasons the table gives no word for (drift row of lane W1-S17c), and
/// `EpochNotAnnounced` by lane W1-S17c3 (X-DELTA-W1S47A-3).
///
/// Implements planning/architecture.md section 5.9 and section 12 E1.
///
/// Caller: `admit`, `verify::verify_signatures`, `replay`; the registry glue maps it to a status (the
/// reason words of `azlin-regbase`) and reacts by `Refusal::class`; lane S47a maps it to its classes.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// The entry does not continue the chain (a wrong `prev`, a gap, a fork, a stamp that runs
    /// backwards, an entry of the lost timeline of a restore, a second genesis).
    #[error("the entry does not continue the chain: {0}")]
    Chain(ChainRefusal),
    /// The entry claims a restore epoch the chain does not stand in. Only a restore the metadata group
    /// announced (a verified restore record: `TeamState::raise_floor`) moves the epoch, and every entry
    /// carries exactly the floor, so no entry of a device can raise it: an entry that did would make
    /// every honest entry that still claims the old epoch a `BelowRestoreEpoch` alarm, and would use up
    /// the epochs for ever (`RestoreEpoch::next` of the largest is `None`: no later restore could be
    /// announced). An entry BELOW the floor is `Chain(BelowRestoreEpoch)`, the lost timeline of a
    /// restore. An alarm (X-DELTA-W1S47A-3).
    #[error(
        "the entry claims restore epoch {got} and the chain stands in {floor}: only an announced restore moves it"
    )]
    EpochNotAnnounced {
        /// The restore epoch the entry carries.
        got: RestoreEpoch,
        /// The restore epoch the chain stands in: the entry's must be exactly this.
        floor: RestoreEpoch,
    },
    /// A signature of the entry does not verify, or the entry is not what was signed: its preimage is
    /// not a team-log entry's bytes, or a typed field (the account, the position, the claimed time, the
    /// restore epoch, the signer, the op) differs from what the signed bytes say (X-DELTA-W1S47A-1).
    #[error("a signature of the entry does not verify, or the entry is not what was signed")]
    BadSignature,
    /// The signer is no device of this account.
    #[error("the signer is no device of this account")]
    SignerUnknown,
    /// The signer's device is in its hold or revoked: it signs nothing.
    #[error("the signer's device is {} and signs nothing", .state.word())]
    SignerNotActive {
        /// The device's state.
        state: DeviceState,
    },
    /// The signer's member is not Active: none of its devices signs anything.
    #[error("the signer's member is {} and signs nothing", .state.word())]
    MemberNotActive {
        /// The member's state.
        state: MemberState,
    },
    /// The signer's role (or kind of key) may not sign this op, or not with these arguments.
    #[error("{op} may not be signed by {}", role_word(.role))]
    RoleMaySignNo {
        /// The op.
        op: OpName,
        /// The signer's role; `None` for a key that is no member.
        role: Option<Role>,
    },
    /// The subject is out of the signer's scope.
    #[error("{op} is out of the signer's scope")]
    OutOfScope {
        /// The op.
        op: OpName,
    },
    /// Too few different people signed.
    #[error("quorum short: {have} different people signed, {need} are needed")]
    QuorumShort {
        /// The number of different people among the signatures.
        have: u8,
        /// The number the change needs.
        need: u8,
    },
    /// A passkey assertion with user verification over this entry's hash is missing.
    #[error("a passkey assertion with user verification over this entry is missing")]
    AssertionMissing,
    /// The policy forbids the entry.
    #[error("the policy forbids it: {0}")]
    PolicyForbids(PolicyReason),
    /// The entry would leave the account without an Active owner.
    #[error("the account would be left without an active owner")]
    LastOwner,
    /// The move would make a group its own ancestor.
    #[error("a group cannot move into its own subtree")]
    GroupCycle,
    /// The delegation does not cover the op (no delegation, or the op is not named, or no delegation
    /// may name it).
    #[error("no delegation covers {op}")]
    DelegationDoesNotCover {
        /// The op.
        op: OpName,
    },
    /// The delegation is over.
    #[error("the delegation is over")]
    DelegationExpired,
    /// The invitation does not hold (no invitation, a wrong secret, no confirmation by the inviter).
    #[error("the invitation does not hold")]
    InviteInvalid,
    /// Not an op of the team log.
    #[error("not an op of the team log")]
    UnknownOp,
    /// The subject (a member, a device, a group, a delegation) does not exist.
    #[error("the subject does not exist")]
    SubjectUnknown,
    /// A private member is made managed only with its own device's cosignature.
    #[error("the member's own device has not confirmed")]
    ConsentMissing,
    /// The entry cannot be applied for a reason no role or quorum changes.
    #[error("{0}")]
    Invalid(Invalid),
}

impl Refusal {
    /// What to do about the refusal, in the words of `azlin-chain`: refuse the entry and say why; refuse
    /// it and raise an alarm (a fork, a second genesis, the lost timeline of a restore, a restore epoch
    /// nobody announced); or fetch the entry again from another voter (a copy whose hash or signature
    /// does not hold, or that is not what was signed). Every variant is named: a new reason does not
    /// compile until it is classified.
    ///
    /// Implements planning/architecture.md section 5.9 ("refused; refused and alarmed; fetched again from
    /// another voter") and X-DELTA-W1S47A-3 ("a typed class, Alarm").
    ///
    /// Caller: the registry glue (the reaction to a refused entry), the sync client, lane S47a's
    /// classes, the tests.
    pub fn class(&self) -> RefusalClass {
        match self {
            Refusal::Chain(why) => why.class(),
            Refusal::EpochNotAnnounced { .. } => RefusalClass::Alarm,
            Refusal::BadSignature => RefusalClass::Refetch,
            Refusal::SignerUnknown
            | Refusal::SignerNotActive { .. }
            | Refusal::MemberNotActive { .. }
            | Refusal::RoleMaySignNo { .. }
            | Refusal::OutOfScope { .. }
            | Refusal::QuorumShort { .. }
            | Refusal::AssertionMissing
            | Refusal::PolicyForbids(_)
            | Refusal::LastOwner
            | Refusal::GroupCycle
            | Refusal::DelegationDoesNotCover { .. }
            | Refusal::DelegationExpired
            | Refusal::InviteInvalid
            | Refusal::UnknownOp
            | Refusal::SubjectUnknown
            | Refusal::ConsentMissing
            | Refusal::Invalid(_) => RefusalClass::Refuse,
        }
    }
}

impl From<UnknownOp> for Refusal {
    fn from(_: UnknownOp) -> Refusal {
        Refusal::UnknownOp
    }
}

/// Who signed, as `admit` resolved it against the state.
enum Who<'a> {
    /// An Active device of an Active member.
    Member {
        device: &'a Device,
        member: &'a Member,
    },
    /// The new device that signs its own `genesis` or `member_join`.
    NewDevice { device: DeviceId, member: MemberId },
    /// A delegate key with a live delegation that covers the op.
    Delegate(&'a Delegation),
    /// The payments key.
    Payments,
}

/// What an op is checked with.
struct Ctx<'a> {
    state: &'a TeamState,
    now: Stamp,
    entry: &'a RawEntry,
    proposal: &'a Proposal,
    who: Who<'a>,
}

impl<'a> Ctx<'a> {
    fn name(&self) -> OpName {
        self.entry.op.name()
    }

    fn signer_member(&self) -> Option<&'a Member> {
        match &self.who {
            Who::Member { member, .. } => Some(*member),
            Who::NewDevice { .. } | Who::Delegate(_) | Who::Payments => None,
        }
    }

    fn signer_role(&self) -> Option<Role> {
        self.signer_member().map(|member| member.role)
    }

    fn actor(&self) -> Actor {
        match &self.who {
            Who::Member { device, member } => Actor::Device {
                device: device.id,
                member: member.id,
            },
            Who::NewDevice { device, member } => Actor::Device {
                device: *device,
                member: *member,
            },
            Who::Delegate(delegation) => Actor::Delegate(delegation.key),
            Who::Payments => Actor::Payments,
        }
    }

    /// The refusal of a signer whose role (or kind of key) may not sign the op.
    fn no_role(&self) -> Refusal {
        Refusal::RoleMaySignNo {
            op: self.name(),
            role: self.signer_role(),
        }
    }

    fn out_of_scope(&self) -> Refusal {
        Refusal::OutOfScope { op: self.name() }
    }

    /// The member with this id, or the refusal of an unknown subject.
    fn subject(&self, id: &MemberId) -> Result<&'a Member, Refusal> {
        self.state.member(id).ok_or(Refusal::SubjectUnknown)
    }

    fn group_known(&self, id: &GroupId) -> Result<(), Refusal> {
        if self.state.groups().contains(id) {
            Ok(())
        } else {
            Err(Refusal::SubjectUnknown)
        }
    }
}

/// How the signer may sign the op.
enum Gate {
    /// A role of the table, with its reach.
    Role(Grant),
    /// An Active device of the subject member itself.
    Own,
    /// A delegate key.
    Delegate,
}

/// The role gate: may the signer sign this op at all, and how? An Active device of the subject member
/// itself where the row allows it (`device_add`, `device_remove`); a role the row names; a delegate key
/// where the row is delegable. Anything else is `RoleMaySignNo`.
///
/// Implements planning/architecture.md section 5.9 (the table "Who may sign") and the authority table
/// v0.
fn gate(c: &Ctx<'_>, row: &Row, subject: Option<&MemberId>) -> Result<Gate, Refusal> {
    match &c.who {
        Who::Member { member, .. } => {
            if row.own_member && subject == Some(&member.id) {
                return Ok(Gate::Own);
            }
            if let Some(grant) = row.signers.iter().find(|grant| grant.role == member.role) {
                return Ok(Gate::Role(*grant));
            }
            if mutant!("teamlog_helpdesk_may_approve")
                && row.op == OpName::DeviceAdd
                && member.role == Role::Helpdesk
            {
                return Ok(Gate::Role(Grant {
                    role: Role::Helpdesk,
                    reach: Reach::Account,
                }));
            }
            Err(c.no_role())
        }
        Who::Delegate(_) => {
            if row.delegable {
                Ok(Gate::Delegate)
            } else {
                Err(c.no_role())
            }
        }
        Who::NewDevice { .. } | Who::Payments => Err(c.no_role()),
    }
}

/// Whether the signer reaches the member `subject`: the whole account for owner and admin, the subtree of
/// `admin_scope` for a group admin and helpdesk (a member in no group of it, or a signer without a scope
/// group, is out of scope).
///
/// Implements planning/architecture.md section 7.2 ("Admin roles, scoped to a group subtree") and the
/// authority table v0 ("'In scope' = the subject member belongs, directly or through nesting, to a group
/// of the signer's subtree").
fn reach_member(c: &Ctx<'_>, gate: &Gate, subject: &MemberId) -> bool {
    match gate {
        Gate::Own | Gate::Delegate => true,
        Gate::Role(grant) => match grant.reach {
            Reach::Account => true,
            Reach::InScope => {
                if mutant!("teamlog_scope_ignores_subtree") {
                    return true;
                }
                match c.signer_member().and_then(|member| member.admin_scope) {
                    Some(root) => c.state.groups().member_in_subtree(subject, &root),
                    None => false,
                }
            }
        },
    }
}

/// Whether the signer reaches the group `subject`: the group lies in the subtree of its `admin_scope`.
///
/// Implements planning/architecture.md section 7.2 and the authority table v0 ("for a group as subject,
/// the group lies in the subtree").
fn reach_group(c: &Ctx<'_>, gate: &Gate, subject: &GroupId) -> bool {
    match gate {
        Gate::Own | Gate::Delegate => true,
        Gate::Role(grant) => match grant.reach {
            Reach::Account => true,
            Reach::InScope => {
                if mutant!("teamlog_scope_ignores_subtree") {
                    return true;
                }
                match c.signer_member().and_then(|member| member.admin_scope) {
                    Some(root) => c.state.groups().is_in_subtree(subject, &root),
                    None => false,
                }
            }
        },
    }
}

/// Whether the signer reaches the member, or the member belongs to no group at all (a member that is
/// nobody's yet may be taken into a department; one of another department may not: the v0 rule against
/// a group admin who widens its own scope by taking members out of other departments).
fn reach_member_or_unassigned(c: &Ctx<'_>, gate: &Gate, subject: &MemberId) -> bool {
    c.state.groups().groups_of(subject).is_none() || reach_member(c, gate, subject)
}

/// Whether the signer may act on a subject of this role: it has no higher rank; a delegate key never
/// acts on an owner.
///
/// Implements planning/architecture.md section 7.2 and the authority table v0 ("never an owner unless
/// by another owner with quorum", made general: drift row of lane W1-S17c).
fn may_touch(c: &Ctx<'_>, subject_role: Role) -> bool {
    match &c.who {
        Who::Member { member, .. } => at_most(subject_role, member.role),
        Who::Delegate(_) => subject_role != Role::Owner,
        Who::NewDevice { .. } | Who::Payments => false,
    }
}

/// The signatory a device makes: its member (a person counts, an app never does). With the guard on,
/// every device is its own person.
///
/// Implements planning/architecture.md section 12 E1 ("a quorum counts distinct people, never one
/// person's devices").
fn signatory(member: &Member, device: &Device) -> Signatory {
    if mutant!("teamlog_quorum_counts_devices") {
        return Signatory::person(MemberId::from_bytes(*device.id.as_bytes()));
    }
    match member.kind {
        MemberKind::Person => Signatory::person(member.id),
        MemberKind::App => Signatory::app(member.id),
    }
}

/// The cosigners that count: their cosignature verified, their device is Active at `now`, their member
/// is Active, and their role is an admin's (planning/architecture.md section 5.9: `cosigs` are "further
/// admin signatures for a quorum"). A cosignature of a suspended admin counts for nothing.
fn counted_cosigners<'a>(c: &Ctx<'a>) -> Vec<(&'a Device, &'a Member)> {
    let mut counted = Vec::new();
    for id in c.proposal.cosigners() {
        let Some(device) = c.state.device(id) else {
            continue;
        };
        if !device.state.is_active_at(c.now) {
            continue;
        }
        let Some(member) = c.state.member(&device.member) else {
            continue;
        };
        if member.state != MemberState::Active || !is_admin_class(member.role) {
            continue;
        }
        counted.push((device, member));
    }
    counted
}

/// Whether the entry carries an assertion that holds for this device's key.
fn has_assertion(c: &Ctx<'_>, device: &Device) -> bool {
    c.entry
        .assertions
        .iter()
        .any(|assertion| assertion.key == device.id && assertion.holds())
}

/// The quorum of `class`: the signer and the counted cosigners, distinct people
/// (`azlin_policy::quorum_met` under the account's policy). With `uv`, every one of them must also have
/// a passkey assertion with user verification over this entry's hash (planning/architecture.md section
/// 7.5, `escrow_open`): a quorum that would be met without the assertions is `AssertionMissing`. The
/// members whose cosignature counted.
///
/// Implements planning/architecture.md section 5.9 ("with the policy's quorum"), section 5.10 (row
/// `admin_quorum`), section 7.5 and section 12 E1.
fn quorum(c: &Ctx<'_>, class: QuorumClass, uv: bool) -> Result<Vec<MemberId>, Refusal> {
    let mut everyone: Vec<Signatory> = Vec::new();
    let mut verified: Vec<Signatory> = Vec::new();
    let mut cosigners: Vec<MemberId> = Vec::new();
    if let Who::Member { device, member } = &c.who {
        let signer = signatory(member, device);
        everyone.push(signer);
        if has_assertion(c, device) {
            verified.push(signer);
        }
    }
    for (device, member) in counted_cosigners(c) {
        let cosigner = signatory(member, device);
        everyone.push(cosigner);
        if has_assertion(c, device) {
            verified.push(cosigner);
        }
        cosigners.push(member.id);
    }
    quorum_met(c.state.policy(), class, &everyone).map_err(|short| Refusal::QuorumShort {
        have: short.have,
        need: short.need,
    })?;
    if uv && quorum_met(c.state.policy(), class, &verified).is_err() {
        return Err(Refusal::AssertionMissing);
    }
    cosigners.sort();
    cosigners.dedup();
    Ok(cosigners)
}

/// The quorum the row asks for in this case (the table's `quorum` column, read with the facts of the
/// case), and its check. No quorum: nobody beyond the signer is needed.
fn require(c: &Ctx<'_>, row: &Row, facts: QuorumFacts, uv: bool) -> Result<Vec<MemberId>, Refusal> {
    match row.quorum.class(facts) {
        Some(class) => quorum(c, class, uv),
        None => Ok(Vec::new()),
    }
}

/// A refusal of a limit.
fn limit(what: &'static str) -> Refusal {
    Refusal::Invalid(Invalid::Limit(what))
}

/// A refusal of a malformed argument.
fn args(what: &'static str) -> Refusal {
    Refusal::Invalid(Invalid::Args(what))
}

/// The keys of an op must be of a size the state can hold.
fn check_keys(keys: &PublicKeys) -> Result<(), Refusal> {
    if keys
        .pq
        .as_ref()
        .is_some_and(|pq| pq.len() > PublicKeys::PQ_MAX_LEN)
    {
        return Err(args("a post-quantum key is too long"));
    }
    Ok(())
}

/// The state the chain stands in against the entry: its place in the chain at the leader's stamp, and its
/// restore epoch. A position the chain already holds with the same hash is an entry applied before: it
/// does not extend the chain, so it is a gap. An entry of an epoch BELOW the floor is the lost timeline of
/// a restore (the chain refuses it); an entry of an epoch ABOVE it claims a restore nobody announced
/// (`EpochNotAnnounced`): only `TeamState::raise_floor`, driven by a verified restore record, moves the
/// epoch, never an entry (X-DELTA-W1S47A-3).
fn check_chain(state: &TeamState, meta: &EntryMeta, now: Stamp) -> Result<(), Refusal> {
    let link = Link { meta, stamp: now };
    match state.tip().check(&link) {
        Ok(Placement::Next) => {
            let floor = state.tip().epoch();
            if meta.epoch != floor {
                return Err(Refusal::EpochNotAnnounced {
                    got: meta.epoch,
                    floor,
                });
            }
            Ok(())
        }
        Ok(Placement::Held { .. }) => match state.tip().next_seq() {
            Some(expected) => Err(Refusal::Chain(ChainRefusal::Gap {
                expected,
                got: meta.seq,
            })),
            None => Err(Refusal::Chain(ChainRefusal::Full)),
        },
        Err(ChainRefusal::WrongPrev { .. }) if mutant!("teamlog_prev_unchecked") => Ok(()),
        Err(why) => Err(Refusal::Chain(why)),
    }
}

/// The first entry of a log is its genesis, and a genesis is nothing but the first entry.
///
/// Implements planning/architecture.md section 5.9 (`genesis`: "the first owner device"; the authority
/// table v0: "seq 0, prev zero, account empty; pins `genesis_hash`").
fn check_first_entry(state: &TeamState, e: &RawEntry) -> Result<(), Refusal> {
    let is_genesis = matches!(e.op, Op::Genesis { .. });
    match (state.is_empty(), is_genesis) {
        (true, true) | (false, false) => Ok(()),
        (true, false) => Err(Refusal::Chain(ChainRefusal::NotGenesis)),
        (false, true) => match state.genesis_hash() {
            Some(have) => Err(Refusal::Chain(ChainRefusal::GenesisAgain {
                have,
                got: e.hash,
            })),
            None => Err(Refusal::Invalid(Invalid::StateDamaged(
                "a log that has entries and no genesis hash",
            ))),
        },
    }
}

/// Resolves who signed: the preconditions of every entry except a genesis, a join and an entitlement
/// (planning/architecture.md section 5.9 and the authority table v0): the signer's device is Active (a
/// Pending device, one in its hold, signs nothing) and its member is Active (not Invited, Suspended or
/// Removed); a delegate key only for the ops its live delegation names; the payments key only for an
/// `entitlement`.
fn resolve_signer<'a>(
    state: &'a TeamState,
    p: &'a Proposal,
    now: Stamp,
) -> Result<Who<'a>, Refusal> {
    let op = &p.entry().op;
    match p.signer() {
        VerifiedSigner::Payments => {
            if matches!(op, Op::Entitlement { .. }) {
                Ok(Who::Payments)
            } else {
                Err(Refusal::RoleMaySignNo {
                    op: op.name(),
                    role: None,
                })
            }
        }
        VerifiedSigner::Delegate(key) => {
            Ok(Who::Delegate(live_delegation(state, key, op.name(), now)?))
        }
        VerifiedSigner::Device { device, key } => {
            // The new device signs its own genesis or join with the key the entry registers.
            if let Op::Genesis {
                owner,
                device: new,
                device_keys,
                ..
            } = op
            {
                return if new == device && device_keys.sign == *key {
                    Ok(Who::NewDevice {
                        device: *device,
                        member: *owner,
                    })
                } else {
                    Err(Refusal::SignerUnknown)
                };
            }
            if let Op::MemberJoin {
                member,
                device: new,
                device_keys,
                ..
            } = op
            {
                return if new == device && device_keys.sign == *key {
                    Ok(Who::NewDevice {
                        device: *device,
                        member: *member,
                    })
                } else {
                    Err(Refusal::SignerUnknown)
                };
            }
            let found = state.device(device).ok_or(Refusal::SignerUnknown)?;
            if state.device_key(*device) != Some(key) {
                // The signature was verified against another state of the log.
                return Err(Refusal::BadSignature);
            }
            match found.state {
                DeviceState::Revoked => {
                    return Err(Refusal::SignerNotActive { state: found.state });
                }
                DeviceState::Pending { hold_until } => {
                    if now < hold_until && !mutant!("teamlog_pending_device_may_sign") {
                        return Err(Refusal::SignerNotActive { state: found.state });
                    }
                }
                DeviceState::Active => {}
            }
            let member =
                state
                    .member(&found.member)
                    .ok_or(Refusal::Invalid(Invalid::StateDamaged(
                        "a device whose member does not exist",
                    )))?;
            match member.state {
                MemberState::Active => {}
                MemberState::Suspended if mutant!("teamlog_suspended_may_sign") => {}
                other => return Err(Refusal::MemberNotActive { state: other }),
            }
            if let Some(expires) = member.app.as_ref().and_then(|app| app.expires_at) {
                if now >= expires {
                    return Err(Refusal::Invalid(Invalid::AppExpired));
                }
            }
            Ok(Who::Member {
                device: found,
                member,
            })
        }
    }
}

/// THE PERMISSION CHECK: whether the entry of `p` may enter the account's log at the leader's stamp
/// `now`, as the authority table says. Pure: it changes nothing. The `Admitted` it returns is the
/// witness `apply` takes.
///
/// Implements planning/architecture.md section 5.9 ("The metadata group rejects an entry with a reason
/// for a wrong `prev`, a signer without the role or scope, a missing quorum or a bad signature"),
/// section 7.2, section 7.3, section 7.5 and section 12 E1.
///
/// Caller: the registry glue (the leader's dry run before it proposes to Raft, and `MetaDb::apply`),
/// `replay::replay`, lane S47a.
pub fn admit(state: &TeamState, p: &Proposal, now: Stamp) -> Result<Admitted, Refusal> {
    let e = p.entry();
    let meta = EntryMeta {
        account: e.account,
        seq: e.seq,
        prev: e.prev,
        hash: e.hash,
        epoch: e.epoch,
    };
    check_chain(state, &meta, now)?;
    check_first_entry(state, e)?;
    let who = resolve_signer(state, p, now)?;
    let c = Ctx {
        state,
        now,
        entry: e,
        proposal: p,
        who,
    };
    let cosigners = authorize(&c)?;
    Ok(Admitted::new(
        meta,
        now,
        c.actor(),
        cosigners,
        e.op.clone(),
        e.sealed.clone(),
    ))
}

/// The conditions of the op: the table's row and what no row can say. The members whose cosignature
/// counted.
fn authorize(c: &Ctx<'_>) -> Result<Vec<MemberId>, Refusal> {
    match &c.entry.op {
        Op::Genesis {
            member_keys,
            device_keys,
            ..
        } => {
            check_keys(member_keys)?;
            check_keys(device_keys)?;
            Ok(Vec::new())
        }
        Op::DeviceAdd {
            member,
            device,
            keys,
        } => device_add(c, member, device, keys),
        Op::DeviceRemove { device } => device_remove(c, device),
        Op::MemberInvite {
            member,
            kind,
            role,
            groups,
            app,
            ..
        } => member_invite(c, member, *kind, *role, groups, app.is_some()),
        Op::MemberJoin {
            member,
            device,
            member_keys,
            device_keys,
            secret,
        } => check_join(
            c.state,
            &JoinArgs {
                member,
                device,
                member_keys,
                device_keys,
                secret,
            },
            c.proposal.cosigners(),
            c.now,
        ),
        Op::MemberRole {
            member,
            role,
            admin_scope,
        } => member_role(c, member, *role, admin_scope.as_ref()),
        Op::MemberSuspend { member } => member_suspend(c, member),
        Op::MemberRemove { member } => member_remove(c, member),
        Op::MemberManaged { member, managed } => member_managed(c, member, *managed),
        Op::GroupCreate { group, parent } => group_create(c, group, parent.as_ref()),
        Op::GroupMember {
            group,
            member,
            present,
        } => group_member(c, group, member, *present),
        Op::GroupMove { group, new_parent } => group_move(c, group, new_parent),
        Op::PolicySet { scope, patch } => policy_set(c, scope, patch),
        Op::EscrowSet { holders, .. } => escrow_set(c, holders),
        Op::AuditKeySet { .. } => {
            gate(c, row(OpName::AuditKeySet), None)?;
            require(c, row(OpName::AuditKeySet), QuorumFacts::default(), false)
        }
        Op::AdminQuorumSet { quorum: wanted } => admin_quorum_set(c, wanted),
        Op::EscrowOpen { member, reason } => escrow_open(c, member, *reason),
        Op::Delegate { key, ops, expires } => delegate(c, key, ops, *expires),
        Op::Entitlement { version, .. } => entitlement(c, *version),
        Op::App { name, body } => app_op(c, name, body),
    }
}

/// `device_add` | an Active device of the same member [plan]; or admins by quorum [plan 7.5] | the new
/// device starts Pending until `hold_until` when the signer is not the member's own device (policy
/// `enroll_hold`); `max_devices_per_member` | QuorumEnrollment (quorum path only). An admin's quorum
/// enrols a device only for a member of no higher rank than its own, and only where `device_approval` is
/// `self or quorum`.
fn device_add(
    c: &Ctx<'_>,
    member: &MemberId,
    device: &DeviceId,
    keys: &PublicKeys,
) -> Result<Vec<MemberId>, Refusal> {
    let g = gate(c, row(OpName::DeviceAdd), Some(member))?;
    let subject = c.subject(member)?;
    if subject.state != MemberState::Active {
        return Err(Refusal::Invalid(Invalid::WrongMemberState(subject.state)));
    }
    check_keys(keys)?;
    if c.state.device(device).is_some() {
        return Err(Refusal::Invalid(Invalid::IdTaken));
    }
    if c.state.devices().count() >= MAX_DEVICES {
        return Err(limit("devices"));
    }
    let live = c
        .state
        .devices_of(member)
        .filter(|found| found.state != DeviceState::Revoked)
        .count();
    let most = c.state.policy().max_devices_per_member;
    if live >= usize::from(most) {
        return Err(Refusal::PolicyForbids(PolicyReason::MaxDevices {
            max: most,
        }));
    }
    match g {
        Gate::Own => Ok(Vec::new()),
        Gate::Role(_) => {
            if c.state.policy().device_approval != DeviceApproval::SelfOrQuorum {
                return Err(Refusal::PolicyForbids(PolicyReason::DeviceApprovalSelfOnly));
            }
            if !may_touch(c, subject.role) {
                return Err(c.no_role());
            }
            if c.now.checked_add(c.state.policy().enroll_hold).is_none() {
                return Err(limit("the hold runs past the end of time"));
            }
            require(c, row(OpName::DeviceAdd), QuorumFacts::default(), false)
        }
        Gate::Delegate => Err(c.no_role()),
    }
}

/// `device_remove` | an Active device of the same member, the device itself included [plan]; or an admin
/// in scope [v0] | removing a device of an admin or owner by someone else: quorum | AdminChange (that
/// case only).
fn device_remove(c: &Ctx<'_>, device: &DeviceId) -> Result<Vec<MemberId>, Refusal> {
    let found = c.state.device(device).ok_or(Refusal::SubjectUnknown)?;
    let subject = c.subject(&found.member)?;
    let g = gate(c, row(OpName::DeviceRemove), Some(&found.member))?;
    if found.state == DeviceState::Revoked {
        return Err(Refusal::Invalid(Invalid::DeviceRevoked));
    }
    match g {
        Gate::Own => Ok(Vec::new()),
        Gate::Role(_) => {
            if !may_touch(c, subject.role) {
                return Err(c.no_role());
            }
            let facts = QuorumFacts {
                subject_is_admin_class: is_admin_class(subject.role),
                ..QuorumFacts::default()
            };
            require(c, row(OpName::DeviceRemove), facts, false)
        }
        Gate::Delegate => Err(c.no_role()),
    }
}

/// `member_invite` | admin in scope; group_admin in scope [plan] | creates an Invited member with role
/// member or guest only [v0]; carries the hash of the invite secret and the pseudonymous member id the
/// inviter chose [v0] | none. A delegate key (the identity service) may create one when its delegation
/// names the op.
fn member_invite(
    c: &Ctx<'_>,
    member: &MemberId,
    kind: MemberKind,
    role: Role,
    groups: &std::collections::BTreeSet<GroupId>,
    has_app: bool,
) -> Result<Vec<MemberId>, Refusal> {
    let g = gate(c, row(OpName::MemberInvite), None)?;
    if !matches!(role, Role::Member | Role::Guest) {
        return Err(c.no_role());
    }
    if c.state.member(member).is_some() {
        return Err(Refusal::Invalid(Invalid::IdTaken));
    }
    if c.state.members().count() >= MAX_MEMBERS {
        return Err(limit("members"));
    }
    if (kind == MemberKind::App) != has_app {
        return Err(args(
            "an app principal has app columns and a person has none",
        ));
    }
    if groups.len() > MAX_GROUPS_PER_MEMBER {
        return Err(limit("groups of one member"));
    }
    for id in groups {
        c.group_known(id)?;
    }
    if let Gate::Role(grant) = &g {
        if grant.reach == Reach::InScope
            && (groups.is_empty() || !groups.iter().all(|id| reach_group(c, &g, id)))
        {
            return Err(c.out_of_scope());
        }
    }
    Ok(Vec::new())
}

/// `member_role` | owner: any role incl. owner/admin; admin: group_admin, helpdesk, member, guest for
/// subjects in scope whose role is below admin; group_admin: member, guest in scope [v0] | never the
/// signer's own role; at least one Active owner remains; group_admin and helpdesk need an `admin_scope`
/// group, others none | AdminChange when the old or new role is admin or owner.
fn member_role(
    c: &Ctx<'_>,
    member: &MemberId,
    new_role: Role,
    admin_scope: Option<&GroupId>,
) -> Result<Vec<MemberId>, Refusal> {
    let g = gate(c, row(OpName::MemberRole), None)?;
    let subject = c.subject(member)?;
    if subject.state == MemberState::Removed {
        return Err(Refusal::Invalid(Invalid::WrongMemberState(subject.state)));
    }
    if c.signer_member().map(|signer| signer.id) == Some(subject.id) {
        return Err(c.no_role());
    }
    if !reach_member(c, &g, member) {
        return Err(c.out_of_scope());
    }
    if !mutant!("teamlog_role_assign_unbounded") {
        let signer = c.signer_role().ok_or_else(|| c.no_role())?;
        if !role_touchable(signer, subject.role) || !role_assignable(signer, new_role) {
            return Err(c.no_role());
        }
    }
    match (needs_scope(new_role), admin_scope) {
        (true, Some(scope)) => {
            c.group_known(scope)?;
            if !reach_group(c, &g, scope) {
                return Err(c.out_of_scope());
            }
        }
        (true, None) => return Err(args("a group admin or helpdesk member needs a scope group")),
        (false, Some(_)) => {
            return Err(args(
                "only a group admin or a helpdesk member has a scope group",
            ));
        }
        (false, None) => {}
    }
    if subject.role == Role::Owner
        && subject.state == MemberState::Active
        && new_role != Role::Owner
        && c.state.active_owner_count(Some(&subject.id)) == 0
        && !mutant!("teamlog_last_owner_check_off")
    {
        return Err(Refusal::LastOwner);
    }
    let facts = QuorumFacts {
        role_change_is_admin_class: is_admin_class(subject.role) || is_admin_class(new_role),
        ..QuorumFacts::default()
    };
    require(c, row(OpName::MemberRole), facts, false)
}

/// Whether the member is an Active owner and no other Active owner is left.
fn is_last_owner(c: &Ctx<'_>, subject: &Member) -> bool {
    subject.role == Role::Owner
        && subject.state == MemberState::Active
        && c.state.active_owner_count(Some(&subject.id)) == 0
        && !mutant!("teamlog_last_owner_check_off")
}

/// `member_suspend` | admin, group_admin, helpdesk in scope [plan 7.2 'open device requests and
/// suspend']; the identity service's delegate key if its delegation names it [plan 7.3] | never an owner
/// unless by another owner with quorum | AdminChange for an admin or owner subject.
fn member_suspend(c: &Ctx<'_>, member: &MemberId) -> Result<Vec<MemberId>, Refusal> {
    let g = gate(c, row(OpName::MemberSuspend), None)?;
    let subject = c.subject(member)?;
    if subject.state != MemberState::Active {
        return Err(Refusal::Invalid(Invalid::WrongMemberState(subject.state)));
    }
    if !reach_member(c, &g, member) {
        return Err(c.out_of_scope());
    }
    let own_owner = subject.role == Role::Owner
        && c.signer_member().map(|signer| signer.id) == Some(subject.id);
    if !may_touch(c, subject.role) || own_owner {
        return Err(c.no_role());
    }
    if is_last_owner(c, subject) {
        return Err(Refusal::LastOwner);
    }
    let facts = QuorumFacts {
        subject_is_admin_class: is_admin_class(subject.role),
        ..QuorumFacts::default()
    };
    require(c, row(OpName::MemberSuspend), facts, false)
}

/// `member_remove` | admin, group_admin in scope (helpdesk never) [v0] | not the last owner |
/// AdminChange for an admin or owner subject.
fn member_remove(c: &Ctx<'_>, member: &MemberId) -> Result<Vec<MemberId>, Refusal> {
    let g = gate(c, row(OpName::MemberRemove), None)?;
    let subject = c.subject(member)?;
    if subject.state == MemberState::Removed {
        return Err(Refusal::Invalid(Invalid::WrongMemberState(subject.state)));
    }
    if !reach_member(c, &g, member) {
        return Err(c.out_of_scope());
    }
    if !may_touch(c, subject.role) {
        return Err(c.no_role());
    }
    if is_last_owner(c, subject) {
        return Err(Refusal::LastOwner);
    }
    let facts = QuorumFacts {
        subject_is_admin_class: is_admin_class(subject.role),
        ..QuorumFacts::default()
    };
    require(c, row(OpName::MemberRemove), facts, false)
}

/// `member_managed` | admin in scope [plan] | private to managed needs the member's own device as
/// cosigner [v0]; managed to private needs the quorum [v0] | EscrowChange (managed to private). An app
/// is never managed.
fn member_managed(c: &Ctx<'_>, member: &MemberId, managed: bool) -> Result<Vec<MemberId>, Refusal> {
    gate(c, row(OpName::MemberManaged), None)?;
    let subject = c.subject(member)?;
    if subject.kind != MemberKind::Person {
        return Err(args("an app principal is never managed"));
    }
    if subject.state == MemberState::Removed {
        return Err(Refusal::Invalid(Invalid::WrongMemberState(subject.state)));
    }
    if !may_touch(c, subject.role) {
        return Err(c.no_role());
    }
    if managed == subject.managed {
        return Ok(Vec::new());
    }
    if managed {
        // Private to managed: the member's own Active device confirms.
        let consented = c.proposal.cosigners().iter().any(|id| {
            c.state.device(id).is_some_and(|device| {
                device.member == subject.id && device.state.is_active_at(c.now)
            })
        });
        if subject.state != MemberState::Active || !consented {
            return Err(Refusal::ConsentMissing);
        }
        Ok(vec![subject.id])
    } else {
        let facts = QuorumFacts {
            managed_to_private: true,
            ..QuorumFacts::default()
        };
        require(c, row(OpName::MemberManaged), facts, false)
    }
}

/// `group_create` | admin; group_admin under a group of its subtree [plan 'admin or group admin in
/// scope'] | parent exists; names are `sealed` | none.
fn group_create(
    c: &Ctx<'_>,
    group: &GroupId,
    parent: Option<&GroupId>,
) -> Result<Vec<MemberId>, Refusal> {
    let g = gate(c, row(OpName::GroupCreate), None)?;
    if c.state.groups().contains(group) {
        return Err(Refusal::Invalid(Invalid::IdTaken));
    }
    if c.state.groups().len() >= MAX_GROUPS {
        return Err(limit("groups"));
    }
    match parent {
        Some(parent) => {
            c.group_known(parent)?;
            if !reach_group(c, &g, parent) {
                return Err(c.out_of_scope());
            }
            let depth = c.state.groups().depth_of(parent).ok_or(Refusal::Invalid(
                Invalid::StateDamaged("a damaged group tree"),
            ))?;
            if depth >= MAX_GROUP_DEPTH {
                return Err(limit("the depth of the group tree"));
            }
        }
        None => {
            // A group admin makes no top-level group: it would lie in nobody's subtree.
            if matches!(&g, Gate::Role(grant) if grant.reach == Reach::InScope) {
                return Err(c.out_of_scope());
            }
        }
    }
    Ok(Vec::new())
}

/// `group_member` | admin; group_admin for a group of its subtree [plan] | the member exists and is not
/// Removed | none. A group admin takes into its group nobody who belongs to another department.
fn group_member(
    c: &Ctx<'_>,
    group: &GroupId,
    member: &MemberId,
    present: bool,
) -> Result<Vec<MemberId>, Refusal> {
    let g = gate(c, row(OpName::GroupMember), None)?;
    c.group_known(group)?;
    let subject = c.subject(member)?;
    if present && subject.state == MemberState::Removed {
        return Err(Refusal::Invalid(Invalid::WrongMemberState(subject.state)));
    }
    if !reach_group(c, &g, group) {
        return Err(c.out_of_scope());
    }
    if present {
        if !reach_member_or_unassigned(c, &g, member) {
            return Err(c.out_of_scope());
        }
        let held = match c.state.groups().groups_of(member) {
            Some(set) => set.len(),
            None => 0,
        };
        if held >= MAX_GROUPS_PER_MEMBER {
            return Err(limit("groups of one member"));
        }
    }
    Ok(Vec::new())
}

/// The patches of the groups of a path, from the top down.
fn patches_of<'a>(c: &Ctx<'a>, path: &[GroupId]) -> Vec<&'a PolicyPatch> {
    path.iter()
        .filter_map(|id| c.state.group_patch(id))
        .collect()
}

/// The fields that get looser for the first group of the subtree of `group` whose effective policy
/// would be looser if `group` were moved under `new_parent`: for each group of the subtree, the policy
/// it has now (`effective`: the account's policy under the patches of its path, each clamped to the
/// level above) against the policy it would have on the path of the new parent followed by the rest of
/// its own path (`azlin_policy::check_tightens`: the new one must be at least as tight as the old one).
/// `None` when no group of the subtree gets looser. A move changes what a group inherits without a
/// `policy_set`, so it is the one other op by which a policy can be loosened (X-DELTA-W1S47A-2).
///
/// Implements planning/architecture.md section 7.2 ("subgroups inherit and may tighten, never
/// loosen") and section 5.10 ("groups inherit and may only tighten").
fn move_loosens(
    c: &Ctx<'_>,
    group: &GroupId,
    new_parent: &GroupId,
) -> Result<Option<Vec<Loosened>>, Refusal> {
    let damaged = || Refusal::Invalid(Invalid::StateDamaged("a damaged group tree"));
    let groups = c.state.groups();
    let above = groups.path_from_top(new_parent).ok_or_else(damaged)?;
    for member in groups.subtree(group) {
        let old_path = groups.path_from_top(&member).ok_or_else(damaged)?;
        let at = old_path
            .iter()
            .position(|id| id == group)
            .ok_or_else(damaged)?;
        let below = old_path.get(at..).ok_or_else(damaged)?;
        let new_path: Vec<GroupId> = above.iter().chain(below.iter()).copied().collect();
        let before = effective(c.state.policy(), &patches_of(c, &old_path));
        let after = effective(c.state.policy(), &patches_of(c, &new_path));
        if let Err(fields) = check_tightens(&before, &after) {
            return Ok(Some(fields));
        }
    }
    Ok(None)
}

/// `group_move` | admin; group_admin with scope over BOTH the group and the new parent [v0] | no cycle;
/// the root group never moves; no group of the moved subtree may get a looser effective policy, except by
/// an owner or an admin [D-W1S17C3, X-DELTA-W1S47A-2: a move changes what a group inherits, and "subgroups
/// inherit and may tighten, never loosen"] | PolicyChange (a loosening move only).
fn group_move(
    c: &Ctx<'_>,
    group: &GroupId,
    new_parent: &GroupId,
) -> Result<Vec<MemberId>, Refusal> {
    let g = gate(c, row(OpName::GroupMove), None)?;
    let found = c.state.groups().get(group).ok_or(Refusal::SubjectUnknown)?;
    c.group_known(new_parent)?;
    if !reach_group(c, &g, group) || !reach_group(c, &g, new_parent) {
        return Err(c.out_of_scope());
    }
    if found.parent.is_none() {
        return Err(Refusal::Invalid(Invalid::RootGroup));
    }
    if c.state.groups().is_in_subtree(new_parent, group)
        && !mutant!("teamlog_group_cycle_check_off")
    {
        return Err(Refusal::GroupCycle);
    }
    let below = c.state.groups().height_below(group);
    let above = c.state.groups().depth_of(new_parent);
    match (above, below) {
        (Some(above), Some(below)) => {
            if above.saturating_add(1).saturating_add(below) > MAX_GROUP_DEPTH {
                return Err(limit("the depth of the group tree"));
            }
        }
        _ => {
            return Err(Refusal::Invalid(Invalid::StateDamaged(
                "a damaged group tree",
            )))
        }
    }
    // A move that would loosen the effective policy of a group of the subtree is for an owner or an admin
    // with the quorum of a policy change: a group admin only tightens.
    let loosens = move_loosens(c, group, new_parent)?;
    let facts = QuorumFacts {
        move_loosens_policy: loosens.is_some(),
        ..QuorumFacts::default()
    };
    if let Some(fields) = loosens {
        if !matches!(&g, Gate::Role(grant) if grant.reach == Reach::Account) {
            return Err(Refusal::PolicyForbids(PolicyReason::Loosens(fields)));
        }
    }
    require(c, row(OpName::GroupMove), facts, false)
}

/// `policy_set` | admin; group_admin for a group of its subtree [plan 'policies in its scope'] | a
/// group's policy must tighten (`azlin_policy::check_tightens`); the account's may change anything;
/// bumps `policy_epoch` | PolicyChange.
fn policy_set(
    c: &Ctx<'_>,
    scope: &PolicyScope,
    patch: &PolicyPatch,
) -> Result<Vec<MemberId>, Refusal> {
    let g = gate(c, row(OpName::PolicySet), None)?;
    match scope {
        PolicyScope::Account => {
            // Only owner and admin change the account's own policy.
            if matches!(&g, Gate::Role(grant) if grant.reach == Reach::InScope) {
                return Err(c.no_role());
            }
            let after = apply_patch(c.state.policy(), patch);
            if next_epoch(c.state.policy_epoch(), c.state.policy(), &after).is_err() {
                return Err(Refusal::PolicyForbids(PolicyReason::EpochExhausted));
            }
        }
        PolicyScope::Group(group) => {
            c.group_known(group)?;
            if !reach_group(c, &g, group) {
                return Err(c.out_of_scope());
            }
            let damaged = || Refusal::Invalid(Invalid::StateDamaged("a damaged group tree"));
            let inherited = c.state.inherited_policy_of(group).ok_or_else(damaged)?;
            let child = apply_patch(&inherited, patch);
            if let Err(fields) = check_tightens(&inherited, &child) {
                if !mutant!("teamlog_policy_loosen_allowed") {
                    return Err(Refusal::PolicyForbids(PolicyReason::Loosens(fields)));
                }
            }
            let before = c.state.effective_policy_of(group).ok_or_else(damaged)?;
            let after = c
                .state
                .effective_with_patch(group, patch)
                .ok_or_else(damaged)?;
            if next_epoch(c.state.policy_epoch(), &before, &after).is_err() {
                return Err(Refusal::PolicyForbids(PolicyReason::EpochExhausted));
            }
        }
    }
    require(c, row(OpName::PolicySet), QuorumFacts::default(), false)
}

/// `escrow_set` | admin [plan] | sets `escrow_pub` and the holder set | EscrowChange. Every holder is a
/// person who is a member.
fn escrow_set(
    c: &Ctx<'_>,
    holders: &std::collections::BTreeSet<MemberId>,
) -> Result<Vec<MemberId>, Refusal> {
    gate(c, row(OpName::EscrowSet), None)?;
    if holders.len() > MAX_ESCROW_HOLDERS {
        return Err(limit("escrow holders"));
    }
    for id in holders {
        let holder = c.subject(id)?;
        if holder.kind != MemberKind::Person {
            return Err(args("an escrow holder is a person"));
        }
        if holder.state == MemberState::Removed {
            return Err(Refusal::Invalid(Invalid::WrongMemberState(holder.state)));
        }
    }
    require(c, row(OpName::EscrowSet), QuorumFacts::default(), false)
}

/// `admin_quorum_set` | admin [plan] | lowering needs the CURRENT quorum | PolicyChange. (Every change
/// needs the current quorum, the policy of the state the entry is checked against.)
fn admin_quorum_set(
    c: &Ctx<'_>,
    wanted: &azlin_policy::AdminQuorum,
) -> Result<Vec<MemberId>, Refusal> {
    gate(c, row(OpName::AdminQuorumSet), None)?;
    let mut after = c.state.policy().clone();
    after.admin_quorum = *wanted;
    if next_epoch(c.state.policy_epoch(), c.state.policy(), &after).is_err() {
        return Err(Refusal::PolicyForbids(PolicyReason::EpochExhausted));
    }
    require(
        c,
        row(OpName::AdminQuorumSet),
        QuorumFacts::default(),
        false,
    )
}

/// `escrow_open` | quorum of admins [plan 7.5: two different admins, hardware-bound keys, each with a
/// passkey assertion with user verification] | a `reason`; the subject is managed; records `ready_at` =
/// stamp + `escrow_open_wait` for an Active subject, none for one already offboarded [plan] | EscrowOpen.
fn escrow_open(
    c: &Ctx<'_>,
    member: &MemberId,
    _reason: EscrowReason,
) -> Result<Vec<MemberId>, Refusal> {
    gate(c, row(OpName::EscrowOpen), None)?;
    let subject = c.subject(member)?;
    if subject.kind != MemberKind::Person {
        return Err(args("an app principal has no escrow"));
    }
    if !subject.managed {
        return Err(Refusal::PolicyForbids(PolicyReason::NotManaged));
    }
    match subject.state {
        MemberState::Active | MemberState::Suspended => {
            if c.now
                .checked_add(c.state.policy().escrow_open_wait)
                .is_none()
            {
                return Err(limit("the wait runs past the end of time"));
            }
        }
        MemberState::Removed => {}
        MemberState::Invited | MemberState::Pending => {
            return Err(Refusal::Invalid(Invalid::WrongMemberState(subject.state)));
        }
    }
    if c.state.pending_escrow_opens().count() >= MAX_PENDING_ESCROW_OPENS {
        return Err(limit("escrow openings that wait"));
    }
    require(c, row(OpName::EscrowOpen), QuorumFacts::default(), true)
}

/// `delegate` | admin [plan] | names a key and the exact ops, from the delegable set [v0]; never a role,
/// grant, key wrap or another delegation [plan 7.3]; an empty op set revokes | AdminChange.
fn delegate(
    c: &Ctx<'_>,
    key: &azlin_seams::KeyId,
    ops: &std::collections::BTreeSet<OpName>,
    expires: Option<Stamp>,
) -> Result<Vec<MemberId>, Refusal> {
    gate(c, row(OpName::Delegate), None)?;
    for op in ops {
        if !is_delegable(*op) {
            return Err(Refusal::DelegationDoesNotCover { op: *op });
        }
    }
    if ops.is_empty() {
        if c.state.delegation(key).is_none() {
            return Err(Refusal::SubjectUnknown);
        }
    } else {
        if let Some(end) = expires {
            if end <= c.now {
                return Err(args("the expiry of a delegation is already over"));
            }
        }
        if c.state.delegation(key).is_none() && c.state.delegations().count() >= MAX_DELEGATIONS {
            return Err(limit("delegations"));
        }
    }
    require(c, row(OpName::Delegate), QuorumFacts::default(), false)
}

/// `entitlement` | the payments key, not a device [plan 'replayed from the payments service's
/// submission'] | verified against the trusted payments key; can neither add a member nor change a role
/// [plan] | none. The version must be above the one held (the higher wins).
fn entitlement(c: &Ctx<'_>, version: EntitlementVersion) -> Result<Vec<MemberId>, Refusal> {
    if !matches!(c.who, Who::Payments) {
        // A device, a delegate key: the row names no role, so this is always the refusal.
        gate(c, row(OpName::Entitlement), None)?;
        return Err(c.no_role());
    }
    if let Some(held) = c.state.entitlement() {
        if version <= held.version {
            return Err(Refusal::PolicyForbids(PolicyReason::EntitlementNotNewer));
        }
    }
    Ok(Vec::new())
}

/// `app:<name>/...` | an Active device of an Active person; or an app principal whose `scopes` name
/// `app:<name>` [v0] | stored and served, never interpreted [plan]; size cap | none. A guest signs no app
/// op.
fn app_op(c: &Ctx<'_>, name: &AppName, body: &[u8]) -> Result<Vec<MemberId>, Refusal> {
    gate(c, row(OpName::App), None)?;
    if let Some(signer) = c.signer_member() {
        if signer.kind == MemberKind::App
            && !signer
                .app
                .as_ref()
                .is_some_and(|spec| spec.scopes.contains(name))
        {
            return Err(c.out_of_scope());
        }
    }
    if ByteCount::of_len(body.len()) > APP_BODY_MAX {
        return Err(limit("the body of an app op"));
    }
    Ok(Vec::new())
}
