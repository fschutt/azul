//! What `apply` produces: the registry's rows and the notices of section 5.9 (planning/architecture.md
//! section 5.9 "Notices come from the metadata group, not from the acting device. Every entry emits
//! `log_appended` (op, subject) over `watch` and push to every admin and to the subject's devices";
//! section 5.3 the rows; build unit S17, lane W1-S17c1).
//!
//! [`Effects`] is plain data. The registry's own types (`DeviceAddFacts`, `RevokeMember`, the rows of
//! `reg_member`, `reg_device`, `reg_group`) live in the iso crate `azlin-registry`, and an azul crate
//! never names an iso type (COMMON-W1R R5): the glue unit TEAMLOG-APPLY builds them from the
//! [`RegistryDelta`]s, in order. A delta carries the FULL row after the change (an upsert), so the
//! glue needs no knowledge of the state it came from.

use azlin_policy::{Policy, PolicyPatch};
use azlin_seams::{AccountKind, DeviceId, GroupId, Hash32, KeyId, MemberId, PolicyEpoch, Stamp};

use crate::devices::Device;
use crate::groups::Group;
use crate::members::Member;
use crate::ops::{AppName, Op, OpName, PolicyScope};
use crate::state::{Delegation, EntitlementFacts, PendingOpen};

/// Whom a notice goes to: "every admin and the subject's devices" (planning/architecture.md section
/// 5.9; section 7.5: "every enrollment emits `log_appended` ... to every admin and to the person's
/// devices").
///
/// Implements planning/architecture.md section 5.9 (notices).
///
/// Caller: `Notice::audience`; S33 (derives the `log_appended` event from `Effects::notices` and may
/// narrow an audience).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Audience {
    /// Every admin of the account.
    AllAdmins,
    /// The devices of the subject only.
    SubjectDevices,
    /// Every admin and the subject's devices.
    Both,
}

/// What an entry is about, for the notice.
///
/// Implements planning/architecture.md section 5.9 (`log_appended` carries the op and the subject).
///
/// Caller: `Notice::subject`, `subject_of`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Subject {
    /// The entry is about no one in particular (a policy, a key, an entitlement).
    None,
    /// A member.
    Member(MemberId),
    /// A device.
    Device(DeviceId),
    /// A group.
    Group(GroupId),
    /// A delegate key.
    Key(KeyId),
    /// An app.
    App(AppName),
}

/// The `log_appended` notice of an entry: its op, its subject, and whom it goes to.
///
/// Implements planning/architecture.md section 5.9 ("Every entry emits `log_appended` (op, subject)
/// over `watch` and push to every admin and to the subject's devices").
///
/// Caller: `Effects::notices`; S33 derives the event from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// The op of the entry.
    pub op: OpName,
    /// What the entry is about.
    pub subject: Subject,
    /// Whom the notice goes to.
    pub audience: Audience,
}

/// The subject of an op and the audience of its notice: a member or a device is a person with
/// devices, so the notice goes to every admin AND to those devices; every other subject has no
/// devices, so it goes to every admin.
///
/// Implements planning/architecture.md section 5.9 ("to every admin and to the subject's devices").
///
/// Caller: `apply.rs` (the notice of every applied entry).
pub fn subject_of(op: &Op) -> (Subject, Audience) {
    let subject = match op {
        Op::Genesis { owner, .. } => Subject::Member(*owner),
        Op::DeviceAdd { device, .. } | Op::DeviceRemove { device } => Subject::Device(*device),
        Op::MemberInvite { member, .. }
        | Op::MemberJoin { member, .. }
        | Op::MemberRole { member, .. }
        | Op::MemberSuspend { member }
        | Op::MemberRemove { member }
        | Op::MemberManaged { member, .. }
        | Op::EscrowOpen { member, .. } => Subject::Member(*member),
        Op::GroupCreate { group, .. }
        | Op::GroupMember { group, .. }
        | Op::GroupMove { group, .. } => Subject::Group(*group),
        Op::PolicySet { scope, .. } => match scope {
            PolicyScope::Account => Subject::None,
            PolicyScope::Group(group) => Subject::Group(*group),
        },
        Op::EscrowSet { .. }
        | Op::AuditKeySet { .. }
        | Op::AdminQuorumSet { .. }
        | Op::Entitlement { .. } => Subject::None,
        Op::Delegate { key, .. } => Subject::Key(*key),
        Op::App { name, .. } => Subject::App(name.clone()),
    };
    let audience = match subject {
        Subject::Member(_) | Subject::Device(_) => Audience::Both,
        Subject::None | Subject::Group(_) | Subject::Key(_) | Subject::App(_) => {
            Audience::AllAdmins
        }
    };
    (subject, audience)
}

/// Why a subject has a revocation row.
///
/// Implements planning/architecture.md section 5.3 (`reg_revocation`: `reason`).
///
/// Caller: `RegistryDelta::Revocation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RevocationReason {
    /// A `device_remove` entry revoked the device.
    DeviceRemoved,
    /// A `member_remove` entry removed the member (and so every one of its devices).
    MemberRemoved,
}

/// What a revocation row is about (planning/architecture.md section 5.3 `reg_revocation`: "subject
/// (family, device, member, credential nonce)"; the log knows devices and members).
///
/// Implements planning/architecture.md section 5.3.
///
/// Caller: `RegistryDelta::Revocation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RevocationSubject {
    /// A device.
    Device(DeviceId),
    /// A member.
    Member(MemberId),
}

/// One change of the registry that an applied entry makes. Every variant that carries a row carries
/// the whole row as it stands after the change.
///
/// Implements planning/architecture.md section 5.3 (`reg_account`, `reg_member`, `reg_device`,
/// `reg_group`, `reg_group_member`, `reg_revocation`) and section 5.9 ("the metadata group applies each
/// entry once, as it arrives, into the registry's tables").
///
/// Caller: `Effects::registry`; the registry glue (TEAMLOG-APPLY) turns each into a write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryDelta {
    /// `reg_account`: the account's kind and its genesis hash (the account's pin), at the genesis.
    AccountGenesis {
        /// The kind of the account.
        kind: AccountKind,
        /// The hash of the genesis entry.
        genesis_hash: Hash32,
    },
    /// `reg_account.policy` and `policy_epoch`: the account's policy after the change.
    AccountPolicy {
        /// The policy.
        policy: Policy,
        /// The policy epoch after the change.
        epoch: PolicyEpoch,
    },
    /// `reg_account.policy_epoch` alone: the epoch moved because a group's effective policy changed.
    PolicyEpoch(PolicyEpoch),
    /// The patch of one group's policy; `None` when the group no longer sets any field.
    GroupPolicy {
        /// The group.
        group: GroupId,
        /// The patch it sets now.
        patch: Option<PolicyPatch>,
    },
    /// `reg_account.escrow_pub` and the escrow holders.
    AccountEscrow {
        /// The escrow public key.
        escrow_pub: [u8; 32],
        /// The holders, in order.
        holders: Vec<MemberId>,
    },
    /// `reg_account.audit_pub`.
    AccountAudit {
        /// The audit public key.
        audit_pub: [u8; 32],
    },
    /// `reg_account.entitlement`, `quota_bytes` and `seats`.
    AccountEntitlement(EntitlementFacts),
    /// A `reg_member` row, whole.
    Member(Member),
    /// A `reg_device` row, whole.
    Device(Device),
    /// A `reg_group` row, whole.
    Group(Group),
    /// A `reg_group_member` row that exists (`present`) or no longer does.
    GroupMember {
        /// The group.
        group: GroupId,
        /// The member.
        member: MemberId,
        /// Whether the member is in the group now.
        present: bool,
    },
    /// A `reg_revocation` row.
    Revocation {
        /// What is revoked.
        subject: RevocationSubject,
        /// The leader's stamp of the entry.
        at: Stamp,
        /// Why.
        reason: RevocationReason,
    },
    /// A delegation after the change; `None` when the key's delegation is gone.
    Delegation {
        /// The delegate key.
        key: KeyId,
        /// The delegation now.
        delegation: Option<Delegation>,
    },
    /// An escrow opening that now waits.
    EscrowOpen(PendingOpen),
}

/// What `apply` produced for one entry.
///
/// `applied` is `false` only when the `Admitted` witness was made against another state of the chain
/// than the one it was applied to: nothing changed then and every list is empty. A caller that runs
/// `admit` and `apply` against the same state in one step never sees it.
///
/// Implements planning/architecture.md section 5.9 (the metadata group applies each entry once into
/// the registry's tables; notices come from the metadata group) and section 5.11 (a policy change
/// invalidates older credentials).
///
/// Caller: the registry glue (TEAMLOG-APPLY: persists `registry`, derives the events from `notices`),
/// `replay.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effects {
    /// The registry changes, in the order to apply them.
    pub registry: Vec<RegistryDelta>,
    /// The `log_appended` notices: one for each applied entry.
    pub notices: Vec<Notice>,
    /// Whether the policy epoch moved, so every credential of an older epoch is invalid.
    pub credentials_invalidated: bool,
    /// Whether the entry was applied.
    pub applied: bool,
}

impl Effects {
    /// The effects of an entry that was not applied: nothing.
    ///
    /// Implements planning/architecture.md section 5.9 (an entry is applied once, in order).
    ///
    /// Caller: `apply::apply`.
    pub(crate) fn not_applied() -> Effects {
        Effects {
            registry: Vec::new(),
            notices: Vec::new(),
            credentials_invalidated: false,
            applied: false,
        }
    }
}
