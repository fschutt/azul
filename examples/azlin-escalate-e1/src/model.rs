//! THE REFERENCE MODEL of the team log's permission check (planning/architecture.md section 12 E1: "one pure
//! function, checked against a separate reference model on random sequences of ops, group moves and
//! delegations"; section 5.9 the table of ops and who may sign; section 7.2 the roles and the group
//! hierarchy; section 7.3 the identity service's delegation; section 7.5 the quorum paths; section 5.10 the
//! policy; lane W1-S47a).
//!
//! DELIBERATELY NAIVE. The model imports NOTHING from `azlin-teamlog` (a self-check of the tests greps this
//! file for the crate's name): its ids, roles, entries and states are its own plain data (`Vec`s and
//! `BTreeMap`s), and every check re-derives its facts from scratch from the state it is given (no cached
//! subtree, no incremental count). It is written op by op from the AUTHORITY TABLE v0 of the lane's brief
//! (the table is the owner's to ratify, ARCH29 9.13 X-D-54) and the plan sentences quoted at each rule.
//!
//! THE READINGS. Where the table is silent the model takes the PROVISIONAL ANSWER that lane S17c registered
//! in its drift rows (`delta/W1S17C.tsv`), named `[S17c-n]` at the rule, because ARCH29 9.13 says the lanes
//! follow the provisional answers until the owner answers: the rank rule (a signer never acts on somebody
//! above its own rank, [S17c-8]), the group admin's reach ([S17c-9]), who counts in a quorum ([S17c-10]),
//! delegation ([S17c-11]), invitations by a delegate ([S17c-12]), the owner's escrow ([S17c-14]), the
//! quorum path of `device_add` under policy `self` ([S17c-17]) and an entry that changes nothing
//! ([S17c-28]). Where the TABLE ITSELF would let a lower role act on a higher one, the model refuses and the
//! drift file of this lane (`delta/W1S47A.tsv`) says so: that is the finding.
//!
//! WHAT A REFUSAL CLASS MEANS. The plan gives no order in which the checks run. [`model_violations`] therefore
//! returns EVERY class whose rule the entry breaks (each rule is evaluated on its own, never skipped
//! because an earlier one failed), in the model's own order; the production check reports the first rule it
//! meets, and the differential accepts any class of the set (it demands equality only when the set has one
//! member). [`model_admit`] says `Admit` exactly when the set is empty.

#![allow(missing_docs)]

use std::collections::{BTreeMap, BTreeSet};

use azlin_seams::{Span, Stamp};

/// A member (a person or an app principal) of the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Mem(pub u16);

/// A device of the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dev(pub u16);

/// A group of the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Grp(pub u16);

/// A delegate key of the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(pub u16);

/// An app (`app:<name>/...`) of the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppId(pub u8);

/// The roles by privilege: owner > admin > group_admin > helpdesk > member > guest (the table's preamble;
/// planning/architecture.md section 7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    Owner,
    Admin,
    GroupAdmin,
    Helpdesk,
    Member,
    Guest,
}

impl Role {
    /// The six roles.
    pub const ALL: [Role; 6] = [
        Role::Owner,
        Role::Admin,
        Role::GroupAdmin,
        Role::Helpdesk,
        Role::Member,
        Role::Guest,
    ];

    /// The place in the order of privilege: owner 5 ... guest 0.
    pub const fn rank(self) -> u8 {
        match self {
            Role::Owner => 5,
            Role::Admin => 4,
            Role::GroupAdmin => 3,
            Role::Helpdesk => 2,
            Role::Member => 1,
            Role::Guest => 0,
        }
    }

    /// "Owner and admin act on the whole account" (the table's preamble).
    pub const fn is_admin_class(self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }

    /// "group_admin and helpdesk need an `admin_scope` group, others none" (`member_role`).
    pub const fn needs_scope(self) -> bool {
        matches!(self, Role::GroupAdmin | Role::Helpdesk)
    }

    /// The word of the role.
    pub const fn name(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::GroupAdmin => "group_admin",
            Role::Helpdesk => "helpdesk",
            Role::Member => "member",
            Role::Guest => "guest",
        }
    }
}

/// The kind of an account (planning/architecture.md section 5.10's four columns).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AcctKind {
    Personal,
    Family,
    Team,
    Business,
}

impl AcctKind {
    /// The four kinds.
    pub const ALL: [AcctKind; 4] = [
        AcctKind::Personal,
        AcctKind::Family,
        AcctKind::Team,
        AcctKind::Business,
    ];
}

/// A member is a person or an app principal (planning/architecture.md section 5.3 `reg_member.kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MKind {
    Person,
    App,
}

/// The state of a member (planning/architecture.md section 5.3 `reg_member.state`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MState {
    Invited,
    Pending,
    Active,
    Suspended,
    Removed,
}

/// The state of a device: in its hold, active, or revoked (planning/architecture.md section 5.3
/// `reg_device.state`; section 7.5 "The group withholds the wrap until `hold_until`").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DState {
    Active,
    Pending(Stamp),
    Revoked,
}

impl DState {
    /// Whether a device in this state signs at the leader's stamp `now`: an active one, and one in its hold
    /// only once the hold is over (the stamp is at or after `hold_until`). "A Pending device, one in its
    /// hold, signs nothing" (the table's preconditions); "until `hold_until`" is read as: at `hold_until`
    /// the hold is over (drift row of this lane, reading R-hold).
    pub fn is_active_at(&self, now: Stamp) -> bool {
        match *self {
            DState::Active => true,
            DState::Pending(until) => now >= until,
            DState::Revoked => false,
        }
    }
}

/// How a device was enrolled (planning/architecture.md section 5.3 `reg_device.enrolled_by`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Enrolled {
    Genesis,
    Invite,
    OwnDevice,
    Quorum,
}

/// `device_approval` of planning/architecture.md section 5.10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DevApproval {
    SelfOnly,
    SelfOrQuorum,
}

/// `external_sharing` of planning/architecture.md section 5.10.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtSharing {
    Allowed,
    SameWorkspaceOnly,
}

/// The reason of an `escrow_open` (planning/architecture.md section 7.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    LeaverHandover,
    LegalRequest,
    Other,
}

/// The twenty ops of the table of planning/architecture.md section 5.9 (`app` stands for every
/// `app:<name>/...`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpKind {
    Genesis,
    DeviceAdd,
    DeviceRemove,
    MemberInvite,
    MemberJoin,
    MemberRole,
    MemberSuspend,
    MemberRemove,
    MemberManaged,
    GroupCreate,
    GroupMember,
    GroupMove,
    PolicySet,
    EscrowSet,
    AuditKeySet,
    AdminQuorumSet,
    EscrowOpen,
    Delegate,
    Entitlement,
    App,
}

impl OpKind {
    /// The twenty ops, in the order of the table.
    pub const ALL: [OpKind; 20] = [
        OpKind::Genesis,
        OpKind::DeviceAdd,
        OpKind::DeviceRemove,
        OpKind::MemberInvite,
        OpKind::MemberJoin,
        OpKind::MemberRole,
        OpKind::MemberSuspend,
        OpKind::MemberRemove,
        OpKind::MemberManaged,
        OpKind::GroupCreate,
        OpKind::GroupMember,
        OpKind::GroupMove,
        OpKind::PolicySet,
        OpKind::EscrowSet,
        OpKind::AuditKeySet,
        OpKind::AdminQuorumSet,
        OpKind::EscrowOpen,
        OpKind::Delegate,
        OpKind::Entitlement,
        OpKind::App,
    ];

    /// The wire word of the op.
    pub const fn name(self) -> &'static str {
        match self {
            OpKind::Genesis => "genesis",
            OpKind::DeviceAdd => "device_add",
            OpKind::DeviceRemove => "device_remove",
            OpKind::MemberInvite => "member_invite",
            OpKind::MemberJoin => "member_join",
            OpKind::MemberRole => "member_role",
            OpKind::MemberSuspend => "member_suspend",
            OpKind::MemberRemove => "member_remove",
            OpKind::MemberManaged => "member_managed",
            OpKind::GroupCreate => "group_create",
            OpKind::GroupMember => "group_member",
            OpKind::GroupMove => "group_move",
            OpKind::PolicySet => "policy_set",
            OpKind::EscrowSet => "escrow_set",
            OpKind::AuditKeySet => "audit_key_set",
            OpKind::AdminQuorumSet => "admin_quorum_set",
            OpKind::EscrowOpen => "escrow_open",
            OpKind::Delegate => "delegate",
            OpKind::Entitlement => "entitlement",
            OpKind::App => "app",
        }
    }
}

/// The classes both sides map their refusals to (the table in `compare.rs`). The first fourteen are the
/// classes of the brief; `PolicyForbids`, `ConsentMissing` and `Invalid` are the three reasons the brief's
/// list has no word for (drift rows 20 of lane S17c and 1 of this lane).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    Chain,
    Signature,
    SignerNotActive,
    RoleOrScope,
    QuorumShort,
    AssertionMissing,
    PolicyLoosens,
    LastOwner,
    GroupCycle,
    DelegationDoesNotCover,
    DelegationExpired,
    InviteInvalid,
    UnknownOp,
    SubjectUnknown,
    PolicyForbids,
    ConsentMissing,
    Invalid,
}

impl Class {
    /// Every class.
    pub const ALL: [Class; 17] = [
        Class::Chain,
        Class::Signature,
        Class::SignerNotActive,
        Class::RoleOrScope,
        Class::QuorumShort,
        Class::AssertionMissing,
        Class::PolicyLoosens,
        Class::LastOwner,
        Class::GroupCycle,
        Class::DelegationDoesNotCover,
        Class::DelegationExpired,
        Class::InviteInvalid,
        Class::UnknownOp,
        Class::SubjectUnknown,
        Class::PolicyForbids,
        Class::ConsentMissing,
        Class::Invalid,
    ];
}

/// The verdict on one entry: admitted, or refused with a class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Admit,
    Refuse(Class),
}

/// What the generator claims about the signer's own signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SigClaim {
    /// The signature verifies under the signer's key over this entry.
    Valid,
    /// It does not (made by somebody else, or over other bytes).
    Invalid,
}

/// What the generator claims about one cosignature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CosigClaim {
    /// A valid signature of the device over THIS entry.
    Valid,
    /// A valid signature of the device over ANOTHER entry (it counts for nothing).
    OverOtherEntry,
    /// Bytes that are no signature.
    Garbage,
}

/// The position an entry claims: the next one, or a number of places away from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeqSpec {
    Next,
    Off(i8),
}

/// The `prev` an entry claims: the head's hash, the zero hash, the hash of an entry `k` places before the
/// head, or bytes no entry hashes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrevSpec {
    Head,
    Zero,
    Stale(u8),
    Garbage(u64),
}

/// The restore epoch an entry claims: the chain's, one above it (a restore happened), one below, the
/// largest there is (a device that claims an epoch from the far future), or a number (a device that has not
/// heard of the newest epoch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpochSpec {
    Current,
    Bump,
    Below,
    Far,
    At(u64),
}

/// The account an entry names: this one, or another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountSpec {
    Own,
    Other,
}

/// Who signed: a device, a delegate key, the payments key (planning/architecture.md section 5.9 `signer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MSigner {
    Device(Dev),
    Delegate(Key),
    Payments,
}

/// A passkey assertion over an entry's hash (planning/architecture.md section 5.9 `webauthn`, section 7.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MAssertion {
    pub dev: Dev,
    pub user_verified: bool,
    pub challenge_ok: bool,
    pub rp_ok: bool,
}

impl MAssertion {
    /// An assertion that counts: user verified, over this entry's hash, for our relying party.
    pub const fn holds(&self) -> bool {
        self.user_verified && self.challenge_ok && self.rp_ok
    }
}

/// The columns of an app principal (planning/architecture.md section 5.3: `scopes`, `expires_at`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MApp {
    pub scopes: BTreeSet<AppId>,
    pub expires: Option<Stamp>,
}

/// The scope of a `policy_set`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PScope {
    Account,
    Group(Grp),
}

/// The fields of the policy the model tracks (the other 16 leaves of planning/architecture.md section 5.10
/// are never set by a model entry). `ManagedDefault`, `Updates` and `Padding` are account-level only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PField {
    CredentialTtl,
    DeviceApproval,
    MaxDevices,
    ManagedDefault,
    SsoRequired,
    EnrollHold,
    PasskeyRecovery,
    EscrowOpenWait,
    AuditLevel,
    AdminQuorum,
    BrowserDevices,
    ExternalSharing,
    Updates,
    Padding,
}

impl PField {
    /// The tracked fields.
    pub const ALL: [PField; 14] = [
        PField::CredentialTtl,
        PField::DeviceApproval,
        PField::MaxDevices,
        PField::ManagedDefault,
        PField::SsoRequired,
        PField::EnrollHold,
        PField::PasskeyRecovery,
        PField::EscrowOpenWait,
        PField::AuditLevel,
        PField::AdminQuorum,
        PField::BrowserDevices,
        PField::ExternalSharing,
        PField::Updates,
        PField::Padding,
    ];
}

/// The tracked part of the account policy (planning/architecture.md section 5.10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MPolicy {
    pub credential_ttl: Span,
    pub device_approval: DevApproval,
    pub max_devices: u8,
    pub managed_default: bool,
    pub sso_required: bool,
    pub enroll_hold: Span,
    pub passkey_recovery: bool,
    pub escrow_open_wait: Span,
    /// 0 none, 1 admin, 2 access summary, 3 access full.
    pub audit_level: u8,
    pub admin_quorum: u8,
    pub browser_devices: bool,
    pub external_sharing: ExtSharing,
    /// 0 auto, 1 notify, 2 managed by MDM.
    pub updates: u8,
    /// 0 none, 1 buckets, 2 full.
    pub padding: u8,
}

impl MPolicy {
    /// The column of planning/architecture.md section 5.10 for a kind of account, typed from the table
    /// of the plan (team's `admin_quorum` is 1: the plan gives none, drift row X-DELTA-W1S17B-2).
    pub fn defaults(kind: AcctKind) -> MPolicy {
        let business = kind == AcctKind::Business;
        let team_or_business = matches!(kind, AcctKind::Team | AcctKind::Business);
        MPolicy {
            credential_ttl: if business {
                Span::hours(1)
            } else {
                Span::hours(12)
            },
            device_approval: if team_or_business {
                DevApproval::SelfOrQuorum
            } else {
                DevApproval::SelfOnly
            },
            max_devices: 10,
            managed_default: business,
            sso_required: false,
            enroll_hold: Span::hours(48),
            passkey_recovery: !team_or_business,
            escrow_open_wait: Span::hours(72),
            audit_level: match kind {
                AcctKind::Personal | AcctKind::Family => 0,
                AcctKind::Team => 1,
                AcctKind::Business => 2,
            },
            admin_quorum: if business { 2 } else { 1 },
            browser_devices: true,
            external_sharing: if business {
                ExtSharing::SameWorkspaceOnly
            } else {
                ExtSharing::Allowed
            },
            updates: if business { 2 } else { 0 },
            padding: 1,
        }
    }

    /// How tight a field's value is: larger is tighter. `None` for the three fields that only the account
    /// sets ("a group may not change them, in either direction", drift row X-DELTA-W1S17B-1).
    fn score(&self, f: PField) -> Option<i128> {
        Some(match f {
            PField::CredentialTtl => -i128::from(self.credential_ttl.as_secs()),
            PField::DeviceApproval => match self.device_approval {
                DevApproval::SelfOnly => 1,
                DevApproval::SelfOrQuorum => 0,
            },
            PField::MaxDevices => -i128::from(self.max_devices),
            PField::SsoRequired => i128::from(self.sso_required),
            PField::EnrollHold => i128::from(self.enroll_hold.as_secs()),
            PField::PasskeyRecovery => i128::from(!self.passkey_recovery),
            PField::EscrowOpenWait => i128::from(self.escrow_open_wait.as_secs()),
            PField::AuditLevel => i128::from(self.audit_level),
            PField::AdminQuorum => i128::from(self.admin_quorum),
            PField::BrowserDevices => i128::from(!self.browser_devices),
            PField::ExternalSharing => match self.external_sharing {
                ExtSharing::Allowed => 0,
                ExtSharing::SameWorkspaceOnly => 1,
            },
            PField::ManagedDefault | PField::Updates | PField::Padding => return None,
        })
    }

    /// Whether the value of `f` is the same in both policies.
    fn same_value(&self, other: &MPolicy, f: PField) -> bool {
        match f {
            PField::ManagedDefault => self.managed_default == other.managed_default,
            PField::Updates => self.updates == other.updates,
            PField::Padding => self.padding == other.padding,
            _ => self.score(f) == other.score(f),
        }
    }

    /// Whether this policy is at least as tight as `parent` in `f` ("subgroups inherit and may tighten,
    /// never loosen", planning/architecture.md section 7.2).
    pub fn tighter_or_equal(&self, parent: &MPolicy, f: PField) -> bool {
        match (self.score(f), parent.score(f)) {
            (Some(mine), Some(theirs)) => mine >= theirs,
            _ => self.same_value(parent, f),
        }
    }

    /// Puts the value of `f` of `from` into this policy.
    fn copy_field(&mut self, from: &MPolicy, f: PField) {
        match f {
            PField::CredentialTtl => self.credential_ttl = from.credential_ttl,
            PField::DeviceApproval => self.device_approval = from.device_approval,
            PField::MaxDevices => self.max_devices = from.max_devices,
            PField::ManagedDefault => self.managed_default = from.managed_default,
            PField::SsoRequired => self.sso_required = from.sso_required,
            PField::EnrollHold => self.enroll_hold = from.enroll_hold,
            PField::PasskeyRecovery => self.passkey_recovery = from.passkey_recovery,
            PField::EscrowOpenWait => self.escrow_open_wait = from.escrow_open_wait,
            PField::AuditLevel => self.audit_level = from.audit_level,
            PField::AdminQuorum => self.admin_quorum = from.admin_quorum,
            PField::BrowserDevices => self.browser_devices = from.browser_devices,
            PField::ExternalSharing => self.external_sharing = from.external_sharing,
            PField::Updates => self.updates = from.updates,
            PField::Padding => self.padding = from.padding,
        }
    }

    /// The policy with the fields a patch sets put on it.
    pub fn applied(&self, patch: &MPatch) -> MPolicy {
        let mut p = self.clone();
        if let Some(v) = patch.credential_ttl {
            p.credential_ttl = v;
        }
        if let Some(v) = patch.device_approval {
            p.device_approval = v;
        }
        if let Some(v) = patch.max_devices {
            p.max_devices = v;
        }
        if let Some(v) = patch.managed_default {
            p.managed_default = v;
        }
        if let Some(v) = patch.sso_required {
            p.sso_required = v;
        }
        if let Some(v) = patch.enroll_hold {
            p.enroll_hold = v;
        }
        if let Some(v) = patch.passkey_recovery {
            p.passkey_recovery = v;
        }
        if let Some(v) = patch.escrow_open_wait {
            p.escrow_open_wait = v;
        }
        if let Some(v) = patch.audit_level {
            p.audit_level = v;
        }
        if let Some(v) = patch.admin_quorum {
            p.admin_quorum = v;
        }
        if let Some(v) = patch.browser_devices {
            p.browser_devices = v;
        }
        if let Some(v) = patch.external_sharing {
            p.external_sharing = v;
        }
        if let Some(v) = patch.updates {
            p.updates = v;
        }
        if let Some(v) = patch.padding {
            p.padding = v;
        }
        p
    }

    /// The policy of a group: this candidate, each field clamped to the level above (the tighter of the two;
    /// the account-only fields follow the parent).
    pub fn clamped_to(&self, parent: &MPolicy) -> MPolicy {
        let mut out = self.clone();
        for f in PField::ALL {
            if !self.tighter_or_equal(parent, f) {
                out.copy_field(parent, f);
            }
        }
        out
    }
}

/// What a `policy_set` carries (planning/architecture.md section 5.9): every field an option.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MPatch {
    pub credential_ttl: Option<Span>,
    pub device_approval: Option<DevApproval>,
    pub max_devices: Option<u8>,
    pub managed_default: Option<bool>,
    pub sso_required: Option<bool>,
    pub enroll_hold: Option<Span>,
    pub passkey_recovery: Option<bool>,
    pub escrow_open_wait: Option<Span>,
    pub audit_level: Option<u8>,
    pub admin_quorum: Option<u8>,
    pub browser_devices: Option<bool>,
    pub external_sharing: Option<ExtSharing>,
    pub updates: Option<u8>,
    pub padding: Option<u8>,
}

impl MPatch {
    /// The fields the patch sets.
    pub fn fields_set(&self) -> Vec<PField> {
        let mut out = Vec::new();
        if self.credential_ttl.is_some() {
            out.push(PField::CredentialTtl);
        }
        if self.device_approval.is_some() {
            out.push(PField::DeviceApproval);
        }
        if self.max_devices.is_some() {
            out.push(PField::MaxDevices);
        }
        if self.managed_default.is_some() {
            out.push(PField::ManagedDefault);
        }
        if self.sso_required.is_some() {
            out.push(PField::SsoRequired);
        }
        if self.enroll_hold.is_some() {
            out.push(PField::EnrollHold);
        }
        if self.passkey_recovery.is_some() {
            out.push(PField::PasskeyRecovery);
        }
        if self.escrow_open_wait.is_some() {
            out.push(PField::EscrowOpenWait);
        }
        if self.audit_level.is_some() {
            out.push(PField::AuditLevel);
        }
        if self.admin_quorum.is_some() {
            out.push(PField::AdminQuorum);
        }
        if self.browser_devices.is_some() {
            out.push(PField::BrowserDevices);
        }
        if self.external_sharing.is_some() {
            out.push(PField::ExternalSharing);
        }
        if self.updates.is_some() {
            out.push(PField::Updates);
        }
        if self.padding.is_some() {
            out.push(PField::Padding);
        }
        out
    }

    /// Whether the patch sets nothing.
    pub fn is_empty(&self) -> bool {
        self.fields_set().is_empty()
    }
}

/// One op of the model, with its arguments (planning/architecture.md section 5.9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MOp {
    Genesis {
        owner: Mem,
        device: Dev,
    },
    DeviceAdd {
        member: Mem,
        device: Dev,
    },
    DeviceRemove {
        device: Dev,
    },
    MemberInvite {
        member: Mem,
        kind: MKind,
        role: Role,
        secret: u32,
        groups: BTreeSet<Grp>,
        app: Option<MApp>,
    },
    MemberJoin {
        member: Mem,
        device: Dev,
        secret: u32,
    },
    MemberRole {
        member: Mem,
        role: Role,
        scope: Option<Grp>,
    },
    MemberSuspend {
        member: Mem,
    },
    MemberRemove {
        member: Mem,
    },
    MemberManaged {
        member: Mem,
        managed: bool,
    },
    GroupCreate {
        group: Grp,
        parent: Option<Grp>,
    },
    GroupMember {
        group: Grp,
        member: Mem,
        present: bool,
    },
    GroupMove {
        group: Grp,
        new_parent: Grp,
    },
    PolicySet {
        scope: PScope,
        patch: MPatch,
    },
    EscrowSet {
        key: u16,
        holders: BTreeSet<Mem>,
    },
    AuditKeySet {
        key: u16,
    },
    AdminQuorumSet {
        quorum: u8,
    },
    EscrowOpen {
        member: Mem,
        reason: Reason,
    },
    Delegate {
        key: Key,
        ops: BTreeSet<OpKind>,
        expires: Option<Stamp>,
    },
    Entitlement {
        version: u64,
        active: bool,
        quota: u64,
        seats: u32,
    },
    App {
        name: AppId,
        body_len: usize,
    },
    /// A wire word that is no op of the table.
    Unknown {
        word: u8,
    },
}

impl MOp {
    /// The kind of the op; `None` for a word that is no op.
    pub fn kind(&self) -> Option<OpKind> {
        Some(match self {
            MOp::Genesis { .. } => OpKind::Genesis,
            MOp::DeviceAdd { .. } => OpKind::DeviceAdd,
            MOp::DeviceRemove { .. } => OpKind::DeviceRemove,
            MOp::MemberInvite { .. } => OpKind::MemberInvite,
            MOp::MemberJoin { .. } => OpKind::MemberJoin,
            MOp::MemberRole { .. } => OpKind::MemberRole,
            MOp::MemberSuspend { .. } => OpKind::MemberSuspend,
            MOp::MemberRemove { .. } => OpKind::MemberRemove,
            MOp::MemberManaged { .. } => OpKind::MemberManaged,
            MOp::GroupCreate { .. } => OpKind::GroupCreate,
            MOp::GroupMember { .. } => OpKind::GroupMember,
            MOp::GroupMove { .. } => OpKind::GroupMove,
            MOp::PolicySet { .. } => OpKind::PolicySet,
            MOp::EscrowSet { .. } => OpKind::EscrowSet,
            MOp::AuditKeySet { .. } => OpKind::AuditKeySet,
            MOp::AdminQuorumSet { .. } => OpKind::AdminQuorumSet,
            MOp::EscrowOpen { .. } => OpKind::EscrowOpen,
            MOp::Delegate { .. } => OpKind::Delegate,
            MOp::Entitlement { .. } => OpKind::Entitlement,
            MOp::App { .. } => OpKind::App,
            MOp::Unknown { .. } => return None,
        })
    }
}

/// One entry as the model sees it: the claims about its chain fields and its signatures, and the op. The
/// converter in `compare.rs` turns it into the bytes the production check reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelEntry {
    /// The identity of the entry's content: two entries with different ids differ in their bytes.
    pub eid: u64,
    pub seq: SeqSpec,
    pub prev: PrevSpec,
    pub epoch: EpochSpec,
    pub account: AccountSpec,
    /// The time the signer's device claims: informational, never read by the check.
    pub at: Stamp,
    pub signer: MSigner,
    pub sig: SigClaim,
    pub cosigs: Vec<(Dev, CosigClaim)>,
    pub assertions: Vec<MAssertion>,
    /// A copy whose hash is not the hash of its bytes.
    pub corrupt: bool,
    pub op: MOp,
}

impl ModelEntry {
    /// An entry signed by a device, at the next position, with every claim valid and no cosignature.
    pub fn new(eid: u64, signer: MSigner, op: MOp) -> ModelEntry {
        ModelEntry {
            eid,
            seq: SeqSpec::Next,
            prev: PrevSpec::Head,
            epoch: EpochSpec::Current,
            account: AccountSpec::Own,
            at: Stamp::EPOCH,
            signer,
            sig: SigClaim::Valid,
            cosigs: Vec::new(),
            assertions: Vec::new(),
            corrupt: false,
            op,
        }
    }
}

/// An invitation that is open (planning/architecture.md section 5.11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MInvite {
    /// The secret's number (the production side keeps its hash).
    pub secret: u32,
    /// The member whose device confirms the join; `None` for an invitation made by a delegate key.
    pub inviter: Option<Mem>,
}

/// A member of the model (planning/architecture.md section 5.3 `reg_member`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MMember {
    pub id: Mem,
    pub kind: MKind,
    pub role: Role,
    pub scope: Option<Grp>,
    pub managed: bool,
    pub state: MState,
    pub has_keys: bool,
    pub invite: Option<MInvite>,
    pub app: Option<MApp>,
    pub added_at: Stamp,
    pub removed_at: Option<Stamp>,
}

/// A device of the model (planning/architecture.md section 5.3 `reg_device`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MDevice {
    pub id: Dev,
    pub member: Mem,
    pub state: DState,
    pub enrolled: Enrolled,
    pub added_at: Stamp,
    pub revoked_at: Option<Stamp>,
}

/// A delegation (planning/architecture.md section 7.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MDelegation {
    pub ops: BTreeSet<OpKind>,
    pub expires: Option<Stamp>,
    pub granted_by: Mem,
}

/// The latest entitlement (planning/architecture.md section 5.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MEntitlement {
    pub version: u64,
    pub active: bool,
    pub quota: u64,
    pub seats: u32,
    pub applied_at: Stamp,
}

/// An escrow opening that waits (planning/architecture.md section 7.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MOpen {
    pub seq: u64,
    pub member: Mem,
    pub reason: Reason,
    pub requested_at: Stamp,
    pub ready_at: Option<Stamp>,
    pub signers: BTreeSet<Mem>,
}

/// The state of one account's log in the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelState {
    pub kind: AcctKind,
    /// The position and the content id of the newest entry; `None` before the genesis.
    pub head: Option<(u64, u64)>,
    pub last_at: Option<Stamp>,
    pub epoch: u64,
    pub members: BTreeMap<Mem, MMember>,
    pub devices: BTreeMap<Dev, MDevice>,
    /// Every group with its parent.
    pub groups: BTreeMap<Grp, Option<Grp>>,
    /// Every (group, member) pair.
    pub memberships: BTreeSet<(Grp, Mem)>,
    pub policy: MPolicy,
    pub group_patches: BTreeMap<Grp, MPatch>,
    pub policy_epoch: u32,
    pub delegations: BTreeMap<Key, MDelegation>,
    pub escrow_pub: Option<u16>,
    pub escrow_holders: BTreeSet<Mem>,
    pub audit_pub: Option<u16>,
    pub entitlement: Option<MEntitlement>,
    pub pending_opens: Vec<MOpen>,
    /// A bug planted into the MODEL on purpose, for the tests of the harness itself (the shrinker and the
    /// replay of a failing seed need a run that fails). `None` in every run that tests the production.
    pub planted: Option<Planted>,
}

/// A bug that a test plants into the model to see the harness catch and shrink it (planning/architecture.md
/// section 12 E1: "plant a divergence ... shrink, assert the minimal length").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Planted {
    /// The model forgets that "none of a suspended member's devices signs anything".
    SuspendedMembersMaySign,
}

/// `t + d`, saturating.
pub fn plus(t: Stamp, d: Span) -> Stamp {
    Stamp::from_secs(t.as_secs().saturating_add(d.as_secs()))
}

impl ModelState {
    /// The state of an account with no entry yet: the default policy of its kind, the policy epoch 1.
    pub fn new(kind: AcctKind) -> ModelState {
        ModelState {
            kind,
            head: None,
            last_at: None,
            epoch: 0,
            members: BTreeMap::new(),
            devices: BTreeMap::new(),
            groups: BTreeMap::new(),
            memberships: BTreeSet::new(),
            policy: MPolicy::defaults(kind),
            group_patches: BTreeMap::new(),
            policy_epoch: 1,
            delegations: BTreeMap::new(),
            escrow_pub: None,
            escrow_holders: BTreeSet::new(),
            audit_pub: None,
            entitlement: None,
            pending_opens: Vec::new(),
            planted: None,
        }
    }

    /// The position the next entry must have.
    pub fn next_seq(&self) -> u64 {
        match self.head {
            Some((seq, _)) => seq.saturating_add(1),
            None => 0,
        }
    }

    /// How many Active owners there are, not counting `except`.
    pub fn active_owner_count(&self, except: Option<Mem>) -> usize {
        self.members
            .values()
            .filter(|m| m.role == Role::Owner && m.state == MState::Active && Some(m.id) != except)
            .count()
    }

    /// Whether `g` lies in the subtree of `root` (`root` included), by walking up the parents.
    pub fn in_subtree(&self, g: Grp, root: Grp) -> bool {
        let mut current = g;
        for _ in 0..=self.groups.len() {
            if !self.groups.contains_key(&current) {
                return false;
            }
            if current == root {
                return true;
            }
            match self.groups.get(&current) {
                Some(Some(parent)) => current = *parent,
                _ => return false,
            }
        }
        false
    }

    /// Every group of the subtree of `root`.
    pub fn subtree(&self, root: Grp) -> BTreeSet<Grp> {
        self.groups
            .keys()
            .copied()
            .filter(|g| self.in_subtree(*g, root))
            .collect()
    }

    /// The groups a member belongs to directly.
    pub fn groups_of(&self, m: Mem) -> Vec<Grp> {
        self.memberships
            .iter()
            .filter(|(_, member)| *member == m)
            .map(|(g, _)| *g)
            .collect()
    }

    /// "In scope": the member belongs, directly or through nesting, to a group of the subtree of `scope`
    /// (the table's preamble).
    pub fn member_in_scope(&self, scope: Grp, m: Mem) -> bool {
        self.groups_of(m).iter().any(|g| self.in_subtree(*g, scope))
    }

    /// The depth of a group: a top-level group is 0; `None` for an unknown group.
    pub fn depth_of(&self, g: Grp) -> Option<usize> {
        self.path_from_top(g).map(|p| p.len().saturating_sub(1))
    }

    /// How many levels lie below a group: 0 for a group without children.
    pub fn height_below(&self, g: Grp) -> usize {
        let Some(base) = self.depth_of(g) else {
            return 0;
        };
        self.groups
            .keys()
            .copied()
            .filter(|other| *other != g && self.in_subtree(*other, g))
            .filter_map(|other| self.depth_of(other))
            .map(|d| d.saturating_sub(base))
            .fold(0, usize::max)
    }

    /// The groups from the top of the tree down to `g`; `None` for an unknown group.
    pub fn path_from_top(&self, g: Grp) -> Option<Vec<Grp>> {
        let mut path = vec![g];
        let mut current = g;
        for _ in 0..=self.groups.len() {
            match self.groups.get(&current) {
                None => return None,
                Some(None) => {
                    path.reverse();
                    return Some(path);
                }
                Some(Some(parent)) => {
                    path.push(*parent);
                    current = *parent;
                }
            }
        }
        None
    }

    /// The policy `g` inherits: the account's with the patches of the groups ABOVE it, each clamped to the
    /// level above ("subgroups inherit and may tighten, never loosen", planning/architecture.md 7.2).
    pub fn inherited_policy(&self, g: Grp) -> Option<MPolicy> {
        let path = self.path_from_top(g)?;
        let mut level = self.policy.clone();
        for above in path.iter().filter(|x| **x != g) {
            if let Some(patch) = self.group_patches.get(above) {
                let candidate = level.applied(patch);
                level = candidate.clamped_to(&level);
            }
        }
        Some(level)
    }

    /// The policy that holds for `g` if its own patch were `own` (`None`: the stored one; an empty patch
    /// means none).
    pub fn effective_policy_with(&self, g: Grp, own: Option<&MPatch>) -> Option<MPolicy> {
        let inherited = self.inherited_policy(g)?;
        let patch = match own {
            Some(p) => Some(p),
            None => self.group_patches.get(&g),
        };
        Some(match patch {
            Some(p) if !p.is_empty() => inherited.applied(p).clamped_to(&inherited),
            _ => inherited,
        })
    }

    /// The policy that holds for `g` now.
    pub fn effective_policy(&self, g: Grp) -> Option<MPolicy> {
        self.effective_policy_with(g, None)
    }

    /// The devices that may sign at `now` and their members: the device is active (or its hold is over)
    /// and its member is Active.
    pub fn active_devices(&self, now: Stamp) -> Vec<(Dev, Mem)> {
        self.devices
            .values()
            .filter(|d| d.state.is_active_at(now))
            .filter(|d| {
                let planted_hole = self.planted == Some(Planted::SuspendedMembersMaySign);
                self.members.get(&d.member).is_some_and(|m| {
                    m.state == MState::Active || (planted_hole && m.state == MState::Suspended)
                })
            })
            .map(|d| (d.id, d.member))
            .collect()
    }
}

/// The deepest group tree: a top-level group has depth 0, so a tree has at most `MAX_GROUP_DEPTH + 1`
/// levels. The plan gives no number (planning/architecture.md section 1.4 rule 4: "everything has a limit,
/// stated up front"); lane S17c chose 16 (est., drift row X-DELTA-W1S17C-27) and the model adopts it (a test
/// compares it with the production constant).
pub const MAX_GROUP_DEPTH: usize = 16;

/// The largest `body` of an `app:<name>/...` op: the table says "size cap" and gives no number; lane S17c
/// chose 16 KiB (est., drift rows 6 and 27 of lane S17c) and the model adopts it (a test compares it with
/// the production constant).
pub const APP_BODY_CAP: usize = 16 * 1024;

/// The position an entry claims, resolved against the chain it is offered to.
pub fn resolved_seq(s: &ModelState, e: &ModelEntry) -> u64 {
    let next = i128::from(s.next_seq());
    let claimed = match e.seq {
        SeqSpec::Next => next,
        SeqSpec::Off(k) => next + i128::from(k),
    };
    claimed.max(0) as u64
}

/// The restore epoch an entry claims, resolved against the chain it is offered to.
pub fn resolved_epoch(s: &ModelState, e: &ModelEntry) -> u64 {
    match e.epoch {
        EpochSpec::Current => s.epoch,
        EpochSpec::Bump => s.epoch.saturating_add(1),
        EpochSpec::Below => s.epoch.saturating_sub(1),
        EpochSpec::Far => u64::MAX,
        EpochSpec::At(n) => n,
    }
}

/// Who signed, resolved against the state.
#[derive(Clone, Copy, Debug)]
enum Who {
    /// A device of a member of the state (whatever the device's and the member's state).
    Member {
        dev: Dev,
        mem: Mem,
        role: Role,
        scope: Option<Grp>,
        kind: MKind,
    },
    /// The new device that signs its own `genesis` or `member_join`.
    NewDevice,
    /// A delegate key.
    Delegate(Key),
    /// The payments key.
    Payments,
    /// A device the state does not know.
    Unknown,
}

fn who_of(s: &ModelState, e: &ModelEntry) -> Who {
    match e.signer {
        MSigner::Delegate(k) => Who::Delegate(k),
        MSigner::Payments => Who::Payments,
        MSigner::Device(d) => {
            let new_device = match &e.op {
                MOp::Genesis { device, .. } | MOp::MemberJoin { device, .. } => Some(*device),
                _ => None,
            };
            if new_device == Some(d) {
                return Who::NewDevice;
            }
            // "genesis: the first owner device; member_join: the invitee's new device": a device that
            // signs one of these two ops signs with the key the op registers, so any other signer is no
            // signer at all (reading R-new-device, drift row of this lane)
            if new_device.is_some() {
                return Who::Unknown;
            }
            match s.devices.get(&d).and_then(|dv| s.members.get(&dv.member)) {
                Some(m) => Who::Member {
                    dev: d,
                    mem: m.id,
                    role: m.role,
                    scope: m.scope,
                    kind: m.kind,
                },
                None => Who::Unknown,
            }
        }
    }
}

struct Ctx<'a> {
    s: &'a ModelState,
    e: &'a ModelEntry,
    now: Stamp,
    who: Who,
}

/// The classes an entry breaks and whether the op instance needs the policy's quorum.
struct Out {
    classes: Vec<Class>,
    quorum: bool,
}

impl Out {
    fn push(&mut self, c: Class) {
        if !self.classes.contains(&c) {
            self.classes.push(c);
        }
    }
}

impl Ctx<'_> {
    fn role(&self) -> Option<Role> {
        match self.who {
            Who::Member { role, .. } => Some(role),
            _ => None,
        }
    }

    fn mem(&self) -> Option<Mem> {
        match self.who {
            Who::Member { mem, .. } => Some(mem),
            _ => None,
        }
    }

    /// The number of different people a quorum needs: the policy's `admin_quorum` (every class but
    /// `other` of planning/architecture.md section 5.10 uses it).
    fn need(&self) -> usize {
        usize::from(self.s.policy.admin_quorum)
    }

    /// The devices that sign this entry for the quorum: the signer's device (a person's, whatever its role:
    /// "the signer counts") and every cosigning device whose signature verifies over THIS entry, that is
    /// Active at the stamp, of an Active person who is an owner or an admin ([S17c-10]).
    fn signing_devices(&self) -> Vec<(Dev, Mem)> {
        let mut out = Vec::new();
        if let Who::Member {
            dev,
            mem,
            kind: MKind::Person,
            ..
        } = self.who
        {
            out.push((dev, mem));
        }
        for (d, claim) in &self.e.cosigs {
            if *claim != CosigClaim::Valid {
                continue;
            }
            let Some(dv) = self.s.devices.get(d) else {
                continue;
            };
            if !dv.state.is_active_at(self.now) {
                continue;
            }
            let Some(m) = self.s.members.get(&dv.member) else {
                continue;
            };
            if m.state != MState::Active || m.kind != MKind::Person || !m.role.is_admin_class() {
                continue;
            }
            out.push((*d, m.id));
        }
        out
    }

    /// The distinct people among the signatures ("a quorum counts distinct people, never one person's
    /// devices", planning/architecture.md section 12 E1).
    fn people(&self) -> BTreeSet<Mem> {
        self.signing_devices().into_iter().map(|(_, m)| m).collect()
    }

    /// The people among the signers whose device made a passkey assertion with user verification over this
    /// entry (planning/architecture.md section 7.5).
    fn assertion_people(&self) -> BTreeSet<Mem> {
        self.signing_devices()
            .into_iter()
            .filter(|(d, _)| self.e.assertions.iter().any(|a| a.dev == *d && a.holds()))
            .map(|(_, m)| m)
            .collect()
    }

    /// Whether a cosignature that counts as a confirmation was made by an Active device of `m` (an Active
    /// member), whatever the member's role.
    fn cosigned_by(&self, m: Mem) -> bool {
        self.e.cosigs.iter().any(|(d, claim)| {
            *claim == CosigClaim::Valid
                && self.s.devices.get(d).is_some_and(|dv| {
                    dv.member == m
                        && dv.state.is_active_at(self.now)
                        && self
                            .s
                            .members
                            .get(&m)
                            .is_some_and(|mm| mm.state == MState::Active)
                })
        })
    }

    /// Whether the signer's reach covers a member: owner and admin act on the whole account, group admin
    /// and helpdesk in the subtree of their `admin_scope`; a delegate key acts account-wide ([S17c-8]).
    fn reach_member(&self, m: Mem) -> bool {
        match self.who {
            Who::Member {
                role: Role::Owner | Role::Admin,
                ..
            } => true,
            Who::Member {
                role: Role::GroupAdmin | Role::Helpdesk,
                scope: Some(g),
                ..
            } => self.s.member_in_scope(g, m),
            Who::Delegate(_) => true,
            _ => false,
        }
    }

    /// Whether the signer's reach covers a group: owner and admin everywhere, group admin and helpdesk in
    /// the subtree of their scope.
    fn reach_group(&self, g: Grp) -> bool {
        match self.who {
            Who::Member {
                role: Role::Owner | Role::Admin,
                ..
            } => true,
            Who::Member {
                role: Role::GroupAdmin | Role::Helpdesk,
                scope: Some(root),
                ..
            } => self.s.in_subtree(g, root),
            _ => false,
        }
    }

    /// "A signer never acts on a member above its own rank" ([S17c-8]); a delegate key never acts on an
    /// owner.
    fn rank_ok(&self, subject: Role) -> bool {
        match self.who {
            Who::Member { role, .. } => subject.rank() <= role.rank(),
            Who::Delegate(_) => subject != Role::Owner,
            _ => false,
        }
    }
}

/// The role gate: whether the signer's kind of key may sign the op at all ("Who may sign" of the table).
fn gate(c: &Ctx, out: &mut Out, allowed: &[Role], delegate_ok: bool) -> bool {
    let pass = match c.who {
        Who::Member { role, .. } => allowed.contains(&role),
        Who::Delegate(_) => delegate_ok,
        Who::Payments | Who::NewDevice => false,
        Who::Unknown => true,
    };
    if !pass {
        out.push(Class::RoleOrScope);
    }
    pass
}

/// The subject member; `SubjectUnknown` when there is none.
fn subject_member<'a>(c: &Ctx<'a>, m: Mem, out: &mut Out) -> Option<&'a MMember> {
    match c.s.members.get(&m) {
        Some(found) => Some(found),
        None => {
            out.push(Class::SubjectUnknown);
            None
        }
    }
}

/// Marks the op instance as one that needs the quorum and records a short quorum.
fn need_quorum(c: &Ctx, out: &mut Out) {
    out.quorum = true;
    if c.people().len() < c.need() {
        out.push(Class::QuorumShort);
    }
}

/// The delegation of a key that signs: it must exist, name the op and still be live ("a delegate key only
/// for the ops its live delegation names", the table's preconditions; planning/architecture.md section
/// 7.3).
fn delegate_standing(c: &Ctx, key: Key, out: &mut Out) {
    let kind = c.e.op.kind();
    match c.s.delegations.get(&key) {
        None => out.push(Class::DelegationDoesNotCover),
        Some(d) => {
            let names = kind.is_some_and(|k| d.ops.contains(&k));
            if !names {
                out.push(Class::DelegationDoesNotCover);
            }
            let live = d.expires.is_none_or(|t| c.now < t);
            if !live {
                out.push(Class::DelegationExpired);
            }
        }
    }
}

/// The preconditions of every entry except `genesis`, `member_join`, `entitlement` and a recovery
/// `device_add` (the table's preamble): the signer's device is Active (a Pending device signs nothing) and
/// its member is Active (not Invited, Suspended or Removed); a delegate key has a live delegation.
fn standing(c: &Ctx, out: &mut Out) {
    match c.who {
        Who::Unknown => out.push(Class::SignerNotActive),
        Who::Member { dev, mem, .. } => {
            let device_ok =
                c.s.devices
                    .get(&dev)
                    .is_some_and(|d| d.state.is_active_at(c.now));
            let planted_hole = c.s.planted == Some(Planted::SuspendedMembersMaySign);
            let member_ok = c.s.members.get(&mem).is_some_and(|m| {
                m.state == MState::Active || (planted_hole && m.state == MState::Suspended)
            });
            if !device_ok || !member_ok {
                out.push(Class::SignerNotActive);
            }
            // an app principal signs nothing once its `expires_at` has passed, whatever the op (the
            // production checks it for every op of an app principal, not only for `app:` ops: reading
            // R-app-expiry, drift row of this lane)
            let expired =
                c.s.members
                    .get(&mem)
                    .and_then(|m| m.app.as_ref())
                    .is_some_and(|a| a.expires.is_some_and(|t| c.now >= t));
            if expired {
                out.push(Class::Invalid);
            }
        }
        Who::Delegate(k) => delegate_standing(c, k, out),
        Who::NewDevice | Who::Payments => {}
    }
}

/// The chain rules: "`seq`/`prev` right" (the table's preamble), the first entry is the genesis and nothing
/// else is, the leader's stamp does not run backwards, the restore epoch is not below the chain's
/// (planning/architecture.md section 5.9 "hash-chained"; the metadata group "rejects an entry with a reason
/// for a wrong `prev`").
fn chain_violated(s: &ModelState, e: &ModelEntry, now: Stamp) -> bool {
    if e.account == AccountSpec::Other {
        return true;
    }
    let is_genesis = matches!(e.op, MOp::Genesis { .. });
    let seq = resolved_seq(s, e);
    match s.head {
        None => {
            let prev_ok = matches!(e.prev, PrevSpec::Head | PrevSpec::Zero);
            !is_genesis || seq != 0 || !prev_ok
        }
        Some((head_seq, _)) => {
            let prev_ok = matches!(e.prev, PrevSpec::Head);
            let backwards = s.last_at.is_some_and(|t| now < t);
            is_genesis
                || seq != head_seq.saturating_add(1)
                || !prev_ok
                || backwards
                || resolved_epoch(s, e) < s.epoch
        }
    }
}

/// Whether the signer's own signature fails: a corrupt copy, or a signature that does not verify.
fn signature_violated(c: &Ctx) -> bool {
    c.e.corrupt || (c.e.sig == SigClaim::Invalid && !matches!(c.who, Who::Unknown))
}

/// The devices of a member that are not revoked.
fn live_device_count(s: &ModelState, m: Mem) -> usize {
    s.devices
        .values()
        .filter(|d| d.member == m && d.state != DState::Revoked)
        .count()
}

// ---------------------------------------------------------------------------------------------------
// The ops, one function each, each quoting its row of the authority table v0.
// ---------------------------------------------------------------------------------------------------

/// `device_add`: "an Active device of the same member [plan]; or admins by quorum [plan 7.5]"; "the new
/// device starts Pending until `hold_until` when the signer is not the member's own device; the policy's
/// `max_devices_per_member`"; quorum class QuorumEnrollment (quorum path only). Helpdesk is no signer
/// ("never approve alone", planning/architecture.md section 7.2). The quorum path enrols only for a member
/// of no higher rank ([S17c-8]) and only when `device_approval` allows a quorum ([S17c-17]).
fn rule_device_add(c: &Ctx, member: Mem, device: Dev, out: &mut Out) {
    if c.s.devices.contains_key(&device) {
        out.push(Class::Invalid);
    }
    let subject = subject_member(c, member, out);
    let own = c.mem() == Some(member);
    if !own {
        // "admins by quorum": owner and admin; a group admin is not named (reading R-device-add-roles,
        // drift row of this lane: planning/architecture.md section 7.2 lists "devices" among the group
        // admin's powers, the table's word here is "admins")
        gate(c, out, &[Role::Owner, Role::Admin], false);
        if let Some(subject) = subject {
            if !c.reach_member(member) || !c.rank_ok(subject.role) {
                out.push(Class::RoleOrScope);
            }
        }
        if c.s.policy.device_approval != DevApproval::SelfOrQuorum {
            out.push(Class::PolicyForbids);
        }
        need_quorum(c, out);
    }
    if let Some(subject) = subject {
        if subject.state != MState::Active {
            out.push(Class::Invalid);
        }
        if live_device_count(c.s, member) >= usize::from(c.s.policy.max_devices) {
            out.push(Class::PolicyForbids);
        }
    }
}

/// `device_remove`: "an Active device of the same member, the device itself included [plan]; or an admin in
/// scope [v0]"; "removing a device of an admin or owner by someone else: quorum" (AdminChange, that case
/// only). The signer never removes the device of a member above its own rank ([S17c-8]).
fn rule_device_remove(c: &Ctx, device: Dev, out: &mut Out) {
    let Some(target) = c.s.devices.get(&device) else {
        out.push(Class::SubjectUnknown);
        return;
    };
    if target.state == DState::Revoked {
        out.push(Class::Invalid);
    }
    if c.mem() == Some(target.member) {
        return;
    }
    gate(c, out, &[Role::Owner, Role::Admin], false);
    if let Some(m) = c.s.members.get(&target.member) {
        if !c.rank_ok(m.role) {
            out.push(Class::RoleOrScope);
        }
        if m.role.is_admin_class() {
            need_quorum(c, out);
        }
    }
}

/// `member_invite`: "admin in scope; group_admin in scope [plan 'admin in its scope']; the identity
/// service's delegate key [plan 7.3: pending-member creation]"; "creates an Invited member with role member
/// or guest only [v0]; carries the hash of the invite secret and the pseudonymous member id the inviter
/// chose [v0]". A group admin names at least one group and every group lies in its subtree ([S17c-9]).
fn rule_member_invite(
    c: &Ctx,
    member: Mem,
    kind: MKind,
    role: Role,
    groups: &BTreeSet<Grp>,
    app: &Option<MApp>,
    out: &mut Out,
) {
    gate(c, out, &[Role::Owner, Role::Admin, Role::GroupAdmin], true);
    if !matches!(role, Role::Member | Role::Guest) {
        out.push(Class::RoleOrScope);
    }
    if c.s.members.contains_key(&member) {
        out.push(Class::Invalid);
    }
    match (kind, app) {
        (MKind::Person, Some(_)) | (MKind::App, None) => out.push(Class::Invalid),
        _ => {}
    }
    for g in groups {
        if !c.s.groups.contains_key(g) {
            out.push(Class::SubjectUnknown);
        }
    }
    if matches!(
        c.who,
        Who::Member {
            role: Role::GroupAdmin,
            ..
        }
    ) && (groups.is_empty() || groups.iter().any(|g| !c.reach_group(*g)))
    {
        out.push(Class::RoleOrScope);
    }
}

/// `member_join`: "the invitee's new device [plan 'the invitee']; the preimage of the invite hash; a
/// cosignature of the inviter's device = the fingerprint confirmation of 5.11 [v0]; the invite unused". An
/// invitation made by a delegate key has no inviter: an Active owner or admin confirms ([S17c-12]).
fn rule_member_join(c: &Ctx, member: Mem, device: Dev, secret: u32, out: &mut Out) {
    if c.s.devices.contains_key(&device) {
        out.push(Class::Invalid);
    }
    let Some(m) = c.s.members.get(&member) else {
        out.push(Class::InviteInvalid);
        return;
    };
    let Some(invite) = &m.invite else {
        out.push(Class::InviteInvalid);
        return;
    };
    if m.state != MState::Invited || invite.secret != secret {
        out.push(Class::InviteInvalid);
    }
    let confirmed = match invite.inviter {
        Some(inviter) => c.cosigned_by(inviter),
        // the signing devices of a join are the cosigners only (the signer is the new device): one
        // Active owner or admin among them confirms
        None => !c.signing_devices().is_empty(),
    };
    if !confirmed {
        out.push(Class::InviteInvalid);
    }
}

/// `member_role`: "owner: any role incl. owner/admin; admin: group_admin, helpdesk, member, guest for
/// subjects in scope whose role is below admin; group_admin: member, guest in scope [v0]; never the
/// signer's own role; at least one Active owner remains; group_admin and helpdesk need an `admin_scope`
/// group, others none"; quorum AdminChange when the old or new role is admin or owner. A non-owner signer
/// acts only on a subject strictly below its own rank ([S17c-8]).
fn rule_member_role(c: &Ctx, member: Mem, new_role: Role, scope: Option<Grp>, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin, Role::GroupAdmin], false);
    let subject = subject_member(c, member, out);
    if c.mem() == Some(member) {
        out.push(Class::RoleOrScope);
    }
    // "admin: group_admin, helpdesk, member, guest; group_admin: member, guest" (an owner: any role)
    let assignable = match c.role() {
        Some(Role::Admin) => matches!(
            new_role,
            Role::GroupAdmin | Role::Helpdesk | Role::Member | Role::Guest
        ),
        Some(Role::GroupAdmin) => matches!(new_role, Role::Member | Role::Guest),
        _ => true,
    };
    if !assignable {
        out.push(Class::RoleOrScope);
    }
    match (new_role.needs_scope(), scope) {
        (true, None) | (false, Some(_)) => out.push(Class::Invalid),
        (true, Some(g)) if !c.s.groups.contains_key(&g) => out.push(Class::SubjectUnknown),
        _ => {}
    }
    if let Some(subject) = subject {
        if let Some(signer_role @ (Role::Admin | Role::GroupAdmin)) = c.role() {
            if subject.role.rank() >= signer_role.rank() {
                out.push(Class::RoleOrScope);
            }
        }
        if !c.reach_member(member) {
            out.push(Class::RoleOrScope);
        }
        if subject.state != MState::Active {
            out.push(Class::Invalid);
        }
        if subject.role == Role::Owner
            && new_role != Role::Owner
            && c.s.active_owner_count(Some(member)) == 0
        {
            out.push(Class::LastOwner);
        }
        if subject.role.is_admin_class() || new_role.is_admin_class() {
            need_quorum(c, out);
        }
    }
}

/// `member_suspend`: "admin, group_admin, helpdesk in scope [plan 7.2 'open device requests and suspend'];
/// the identity service's delegate key if its delegation names it [plan 7.3]"; "never an owner unless by
/// another owner with quorum"; AdminChange for an admin or owner subject. The signer never suspends a
/// member above its own rank ([S17c-8]).
fn rule_member_suspend(c: &Ctx, member: Mem, out: &mut Out) {
    gate(
        c,
        out,
        &[Role::Owner, Role::Admin, Role::GroupAdmin, Role::Helpdesk],
        true,
    );
    let Some(subject) = subject_member(c, member, out) else {
        return;
    };
    if !c.reach_member(member) || !c.rank_ok(subject.role) {
        out.push(Class::RoleOrScope);
    }
    if subject.role == Role::Owner && c.mem() == Some(member) {
        out.push(Class::RoleOrScope);
    }
    if subject.role == Role::Owner && c.s.active_owner_count(Some(member)) == 0 {
        out.push(Class::LastOwner);
    }
    if subject.state != MState::Active {
        out.push(Class::Invalid);
    }
    if subject.role.is_admin_class() {
        need_quorum(c, out);
    }
}

/// `member_remove`: "admin, group_admin in scope (helpdesk never) [v0]; not the last owner"; AdminChange for
/// an admin or owner subject. The signer never removes a member above its own rank ([S17c-8]).
fn rule_member_remove(c: &Ctx, member: Mem, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin, Role::GroupAdmin], false);
    let Some(subject) = subject_member(c, member, out) else {
        return;
    };
    if !c.reach_member(member) || !c.rank_ok(subject.role) {
        out.push(Class::RoleOrScope);
    }
    if subject.role == Role::Owner && c.s.active_owner_count(Some(member)) == 0 {
        out.push(Class::LastOwner);
    }
    if subject.state == MState::Removed {
        out.push(Class::Invalid);
    }
    if subject.role.is_admin_class() {
        need_quorum(c, out);
    }
}

/// `member_managed`: "admin in scope [plan]; private to managed needs the member's own device as cosigner
/// [v0]; managed to private needs the quorum [v0]" (EscrowChange). The signer never changes the flag of a
/// member above its own rank ([S17c-8]).
fn rule_member_managed(c: &Ctx, member: Mem, managed: bool, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin], false);
    let Some(subject) = subject_member(c, member, out) else {
        return;
    };
    if !c.rank_ok(subject.role) {
        out.push(Class::RoleOrScope);
    }
    // "private to managed needs the member's own device as cosigner [v0]": a cosignature of the member's
    // device is required even when the signer is that device (reading R-consent, drift row of this lane)
    if managed && !subject.managed && !c.cosigned_by(member) {
        out.push(Class::ConsentMissing);
    }
    if !managed && subject.managed {
        need_quorum(c, out);
    }
    if subject.state != MState::Active {
        out.push(Class::Invalid);
    }
}

/// `group_create`: "admin; group_admin under a group of its subtree [plan 'admin or group admin in scope'];
/// the parent exists". A group admin makes no top-level group: it would lie in nobody's subtree ([S17c-9]).
fn rule_group_create(c: &Ctx, group: Grp, parent: Option<Grp>, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin, Role::GroupAdmin], false);
    if c.s.groups.contains_key(&group) {
        out.push(Class::Invalid);
    }
    let group_admin = matches!(c.role(), Some(Role::GroupAdmin));
    match parent {
        Some(p) => {
            if !c.s.groups.contains_key(&p) {
                out.push(Class::SubjectUnknown);
            } else {
                if group_admin && !c.reach_group(p) {
                    out.push(Class::RoleOrScope);
                }
                // everything has a limit (planning/architecture.md section 1.4 rule 4): the tree is at
                // most `MAX_GROUP_DEPTH + 1` levels deep
                if c.s.depth_of(p).is_some_and(|d| d + 1 > MAX_GROUP_DEPTH) {
                    out.push(Class::Invalid);
                }
            }
        }
        None => {
            if group_admin {
                out.push(Class::RoleOrScope);
            }
        }
    }
}

/// `group_member`: "admin; group_admin for a group of its subtree [plan]; the member exists and is not
/// Removed". A group admin takes into its group only a member who belongs to no group or to a group of its
/// subtree ([S17c-9]).
fn rule_group_member(c: &Ctx, group: Grp, member: Mem, present: bool, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin, Role::GroupAdmin], false);
    let group_known = c.s.groups.contains_key(&group);
    if !group_known {
        out.push(Class::SubjectUnknown);
    }
    let subject = subject_member(c, member, out);
    if let Some(subject) = subject {
        if subject.state == MState::Removed {
            out.push(Class::Invalid);
        }
    }
    if let Who::Member {
        role: Role::GroupAdmin,
        scope: Some(root),
        ..
    } = c.who
    {
        if group_known && !c.reach_group(group) {
            out.push(Class::RoleOrScope);
        }
        if present && subject.is_some() {
            let theirs = c.s.groups_of(member);
            let free = theirs.is_empty() || theirs.iter().any(|g| c.s.in_subtree(*g, root));
            if !free {
                out.push(Class::RoleOrScope);
            }
        }
    }
}

/// `group_move`: "admin; group_admin with scope over BOTH the group and the new parent [v0]; no cycle; the
/// root group never moves".
fn rule_group_move(c: &Ctx, group: Grp, new_parent: Grp, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin, Role::GroupAdmin], false);
    let group_known = c.s.groups.contains_key(&group);
    let parent_known = c.s.groups.contains_key(&new_parent);
    if !group_known || !parent_known {
        out.push(Class::SubjectUnknown);
    }
    if group_known && matches!(c.s.groups.get(&group), Some(None)) {
        out.push(Class::Invalid);
    }
    if group_known && parent_known && c.s.in_subtree(new_parent, group) {
        out.push(Class::GroupCycle);
    }
    // the moved subtree keeps its shape: its deepest group lands `height_below` levels under the group
    if group_known && parent_known && !c.s.in_subtree(new_parent, group) {
        let new_depth =
            c.s.depth_of(new_parent)
                .map(|d| d + 1 + c.s.height_below(group));
        if new_depth.is_some_and(|d| d > MAX_GROUP_DEPTH) {
            out.push(Class::Invalid);
        }
    }
    if matches!(
        c.who,
        Who::Member {
            role: Role::GroupAdmin,
            ..
        }
    ) && (!c.reach_group(group) || !c.reach_group(new_parent))
    {
        out.push(Class::RoleOrScope);
    }
}

/// `policy_set`: "admin; group_admin for a group of its subtree [plan 'policies in its scope']; a group's
/// policy must tighten; the account's may change anything" (PolicyChange).
fn rule_policy_set(c: &Ctx, scope: PScope, patch: &MPatch, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin, Role::GroupAdmin], false);
    let group_admin = matches!(c.role(), Some(Role::GroupAdmin));
    match scope {
        PScope::Account => {
            if group_admin {
                out.push(Class::RoleOrScope);
            }
        }
        PScope::Group(g) => match c.s.inherited_policy(g) {
            None => out.push(Class::SubjectUnknown),
            Some(inherited) => {
                if group_admin && !c.reach_group(g) {
                    out.push(Class::RoleOrScope);
                }
                let candidate = inherited.applied(patch);
                if patch
                    .fields_set()
                    .into_iter()
                    .any(|f| !candidate.tighter_or_equal(&inherited, f))
                {
                    out.push(Class::PolicyLoosens);
                }
            }
        },
    }
    need_quorum(c, out);
}

/// `escrow_set`: "admin [plan]; sets `escrow_pub` and the holder set" (EscrowChange).
fn rule_escrow_set(c: &Ctx, holders: &BTreeSet<Mem>, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin], false);
    if holders.iter().any(|h| !c.s.members.contains_key(h)) {
        out.push(Class::SubjectUnknown);
    }
    // an escrow holder is a person, never an app principal (reading R-holders, drift row of this lane)
    if holders
        .iter()
        .any(|h| c.s.members.get(h).is_some_and(|m| m.kind != MKind::Person))
    {
        out.push(Class::Invalid);
    }
    need_quorum(c, out);
}

/// `audit_key_set`: "admin [plan]; sets `audit_pub`" (PolicyChange).
fn rule_audit_key_set(c: &Ctx, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin], false);
    need_quorum(c, out);
}

/// `admin_quorum_set`: "admin [plan]; lowering needs the CURRENT quorum" (PolicyChange: the quorum that
/// holds before the entry, for a raise as well).
fn rule_admin_quorum_set(c: &Ctx, quorum: u8, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin], false);
    if !(1..=8).contains(&quorum) {
        out.push(Class::Invalid);
    }
    need_quorum(c, out);
}

/// `escrow_open`: "quorum of admins [plan 7.5: two different admins, hardware-bound keys, each with a
/// passkey assertion with user verification]; a `reason`; the subject is managed; records `ready_at` =
/// stamp + `escrow_open_wait` for an Active subject, none for one already offboarded" (EscrowOpen). Any
/// admin quorum may open any managed person's escrow, an owner's included ([S17c-14]).
fn rule_escrow_open(c: &Ctx, member: Mem, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin], false);
    if let Some(subject) = subject_member(c, member, out) {
        if subject.kind != MKind::Person || !subject.managed {
            out.push(Class::PolicyForbids);
        }
        if matches!(subject.state, MState::Invited | MState::Pending) {
            out.push(Class::Invalid);
        }
    }
    need_quorum(c, out);
    if c.assertion_people().len() < c.need() {
        out.push(Class::AssertionMissing);
    }
}

/// `delegate`: "admin [plan]: names a key and the exact ops it may sign, from the delegable set
/// {member_suspend, member_invite}; never a role, grant, key wrap or another delegation [plan 7.3]; an
/// empty op set revokes" (AdminChange). 'cosign of device_add' is not built ([S17c-11]).
fn rule_delegate(c: &Ctx, key: Key, ops: &BTreeSet<OpKind>, out: &mut Out) {
    gate(c, out, &[Role::Owner, Role::Admin], false);
    if ops
        .iter()
        .any(|k| !matches!(k, OpKind::MemberSuspend | OpKind::MemberInvite))
    {
        out.push(Class::DelegationDoesNotCover);
    }
    if ops.is_empty() && !c.s.delegations.contains_key(&key) {
        out.push(Class::SubjectUnknown);
    }
    need_quorum(c, out);
}

/// `entitlement`: "the payments key, not a device [plan 'replayed from the payments service's
/// submission']; verified against the trusted payments key; can neither add a member nor change a role". A
/// version that is not higher than the one held is refused (COMMON-W1R R10, [S17c-26]).
fn rule_entitlement(c: &Ctx, version: u64, out: &mut Out) {
    if !matches!(c.who, Who::Payments | Who::Unknown) {
        out.push(Class::RoleOrScope);
    }
    if let Some(held) = &c.s.entitlement {
        if version <= held.version {
            out.push(Class::PolicyForbids);
        }
    }
}

/// `app:<name>/...`: "an Active device of an Active person; or an app principal whose `scopes` name
/// `app:<name>` [v0]; stored and served, never interpreted; size cap". A guest signs no app op (the brief's
/// near-miss `app_op_by_a_guest`, [S17c-6]).
fn rule_app(c: &Ctx, name: AppId, body_len: usize, out: &mut Out) {
    // a guest signs no app op, whether it is a person or an app principal invited with the role guest
    // (reading R-guest-app, drift row of this lane)
    if matches!(
        c.who,
        Who::Member {
            role: Role::Guest,
            ..
        }
    ) {
        out.push(Class::RoleOrScope);
    }
    match c.who {
        Who::Member {
            role,
            kind: MKind::Person,
            ..
        } => {
            if role == Role::Guest {
                out.push(Class::RoleOrScope);
            }
        }
        Who::Member {
            mem,
            kind: MKind::App,
            ..
        } => match c.s.members.get(&mem).and_then(|m| m.app.as_ref()) {
            Some(app) => {
                if !app.scopes.contains(&name) {
                    out.push(Class::RoleOrScope);
                }
                if app.expires.is_some_and(|t| c.now >= t) {
                    out.push(Class::Invalid);
                }
            }
            None => out.push(Class::RoleOrScope),
        },
        Who::Delegate(_) | Who::Payments | Who::NewDevice => out.push(Class::RoleOrScope),
        Who::Unknown => {}
    }
    if body_len > APP_BODY_CAP {
        out.push(Class::Invalid);
    }
}

/// Every class whose rule the entry breaks, in the model's order, and whether the op instance needs the
/// policy's quorum. An entry that breaks none is admitted.
fn evaluate(s: &ModelState, e: &ModelEntry, now: Stamp) -> (Vec<Class>, bool) {
    let mut out = Out {
        classes: Vec::new(),
        quorum: false,
    };
    if matches!(e.op, MOp::Unknown { .. }) {
        out.push(Class::UnknownOp);
        return (out.classes, false);
    }
    let c = Ctx {
        s,
        e,
        now,
        who: who_of(s, e),
    };
    if chain_violated(s, e, now) {
        out.push(Class::Chain);
    }
    if signature_violated(&c) {
        out.push(Class::Signature);
    }
    standing(&c, &mut out);
    if !matches!(c.who, Who::Unknown) {
        match &e.op {
            MOp::Genesis { .. } | MOp::Unknown { .. } => {}
            MOp::DeviceAdd { member, device } => rule_device_add(&c, *member, *device, &mut out),
            MOp::DeviceRemove { device } => rule_device_remove(&c, *device, &mut out),
            MOp::MemberInvite {
                member,
                kind,
                role,
                groups,
                app,
                ..
            } => rule_member_invite(&c, *member, *kind, *role, groups, app, &mut out),
            MOp::MemberJoin {
                member,
                device,
                secret,
            } => rule_member_join(&c, *member, *device, *secret, &mut out),
            MOp::MemberRole {
                member,
                role,
                scope,
            } => rule_member_role(&c, *member, *role, *scope, &mut out),
            MOp::MemberSuspend { member } => rule_member_suspend(&c, *member, &mut out),
            MOp::MemberRemove { member } => rule_member_remove(&c, *member, &mut out),
            MOp::MemberManaged { member, managed } => {
                rule_member_managed(&c, *member, *managed, &mut out)
            }
            MOp::GroupCreate { group, parent } => rule_group_create(&c, *group, *parent, &mut out),
            MOp::GroupMember {
                group,
                member,
                present,
            } => rule_group_member(&c, *group, *member, *present, &mut out),
            MOp::GroupMove { group, new_parent } => {
                rule_group_move(&c, *group, *new_parent, &mut out)
            }
            MOp::PolicySet { scope, patch } => rule_policy_set(&c, *scope, patch, &mut out),
            MOp::EscrowSet { holders, .. } => rule_escrow_set(&c, holders, &mut out),
            MOp::AuditKeySet { .. } => rule_audit_key_set(&c, &mut out),
            MOp::AdminQuorumSet { quorum } => rule_admin_quorum_set(&c, *quorum, &mut out),
            MOp::EscrowOpen { member, .. } => rule_escrow_open(&c, *member, &mut out),
            MOp::Delegate { key, ops, .. } => rule_delegate(&c, *key, ops, &mut out),
            MOp::Entitlement { version, .. } => rule_entitlement(&c, *version, &mut out),
            MOp::App { name, body_len } => rule_app(&c, *name, *body_len, &mut out),
        }
    }
    (out.classes, out.quorum)
}

/// Every class whose rule the entry breaks, in the model's own order (the plan gives no order of checks:
/// any of them is a correct reason). Empty: the entry is admitted.
///
/// Implements planning/architecture.md section 5.9 (the table of ops and who may sign), section 7.2 (roles
/// and scopes), section 7.5 (quorum paths) and section 12 E1; Caller: `compare.rs` (the differential), the
/// generator (`gen.rs`) and the tests.
pub fn model_violations(s: &ModelState, e: &ModelEntry, now: Stamp) -> Vec<Class> {
    evaluate(s, e, now).0
}

/// The verdict on an entry: `Admit`, or `Refuse` with the first class of [`model_violations`].
///
/// Implements planning/architecture.md section 12 E1 ("no entry is accepted unless its signer may sign
/// it"); Caller: `compare.rs`, `stories.rs`, the tests.
pub fn model_admit(s: &ModelState, e: &ModelEntry, stamp: Stamp) -> Verdict {
    match evaluate(s, e, stamp).0.first() {
        Some(class) => Verdict::Refuse(*class),
        None => Verdict::Admit,
    }
}

/// The number of different people this op instance needs, when the table's quorum column applies to it
/// (the signer counts, devices of one person count once); `None` when no quorum is needed.
///
/// Implements planning/architecture.md section 5.9 (`cosigs`: "further admin signatures for a quorum") and
/// section 5.10 (`admin_quorum`); Caller: `gen.rs` (adds the cosignatures a valid entry needs).
pub fn quorum_requirement(s: &ModelState, e: &ModelEntry, now: Stamp) -> Option<usize> {
    let (_, needs) = evaluate(s, e, now);
    if needs {
        Some(usize::from(s.policy.admin_quorum))
    } else {
        None
    }
}

/// Whether a role may sign an op when it acts on somebody else, by the table's "Who may sign" column (the
/// own-device paths of `device_add` and `device_remove` need no role). Used to pick plausible signers.
///
/// Implements planning/architecture.md section 5.9 (the table of ops) and section 7.2; Caller: `gen.rs`,
/// the table test.
pub fn role_may_sign(op: OpKind, role: Role) -> bool {
    use Role::*;
    match op {
        OpKind::Genesis | OpKind::MemberJoin | OpKind::Entitlement => false,
        OpKind::MemberRemove | OpKind::GroupCreate => matches!(role, Owner | Admin | GroupAdmin),
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
    }
}

/// Applies an ADMITTED entry to the state at the leader's stamp `now` ("the metadata group applies each
/// entry once, as it arrives", planning/architecture.md section 5.9). The caller has checked
/// [`model_admit`]; an entry that was not admitted changes the state in ways no rule defines.
///
/// Implements planning/architecture.md section 5.9, section 5.10 (a policy change moves the epoch when the
/// effective policy changes) and section 7.5 (a new device of another's enrolment starts in its hold);
/// Caller: `compare.rs`, `stories.rs`.
pub fn model_apply(s: &mut ModelState, e: &ModelEntry, now: Stamp) {
    let seq = resolved_seq(s, e);
    let epoch = resolved_epoch(s, e);
    let who = who_of(s, e);
    let signer_mem = match who {
        Who::Member { mem, .. } => Some(mem),
        _ => None,
    };
    let counted: BTreeSet<Mem> = {
        let c = Ctx { s, e, now, who };
        c.people()
    };
    match &e.op {
        MOp::Unknown { .. } => return,
        MOp::Genesis { owner, device } => {
            s.members.insert(
                *owner,
                MMember {
                    id: *owner,
                    kind: MKind::Person,
                    role: Role::Owner,
                    scope: None,
                    managed: s.policy.managed_default,
                    state: MState::Active,
                    has_keys: true,
                    invite: None,
                    app: None,
                    added_at: now,
                    removed_at: None,
                },
            );
            s.devices.insert(
                *device,
                MDevice {
                    id: *device,
                    member: *owner,
                    state: DState::Active,
                    enrolled: Enrolled::Genesis,
                    added_at: now,
                    revoked_at: None,
                },
            );
        }
        MOp::DeviceAdd { member, device } => {
            let own = signer_mem == Some(*member);
            let (state, enrolled) = if own {
                (DState::Active, Enrolled::OwnDevice)
            } else {
                (
                    DState::Pending(plus(now, s.policy.enroll_hold)),
                    Enrolled::Quorum,
                )
            };
            s.devices.insert(
                *device,
                MDevice {
                    id: *device,
                    member: *member,
                    state,
                    enrolled,
                    added_at: now,
                    revoked_at: None,
                },
            );
        }
        MOp::DeviceRemove { device } => {
            if let Some(d) = s.devices.get_mut(device) {
                d.state = DState::Revoked;
                d.revoked_at = Some(now);
            }
        }
        MOp::MemberInvite {
            member,
            kind,
            role,
            secret,
            groups,
            app,
        } => {
            s.members.insert(
                *member,
                MMember {
                    id: *member,
                    kind: *kind,
                    role: *role,
                    scope: None,
                    // an app principal is never managed (planning/architecture.md section 7.6: only
                    // people are escrowed)
                    managed: s.policy.managed_default && *kind == MKind::Person,
                    state: MState::Invited,
                    has_keys: false,
                    invite: Some(MInvite {
                        secret: *secret,
                        inviter: signer_mem,
                    }),
                    app: app.clone(),
                    added_at: now,
                    removed_at: None,
                },
            );
            for g in groups {
                s.memberships.insert((*g, *member));
            }
        }
        MOp::MemberJoin { member, device, .. } => {
            if let Some(m) = s.members.get_mut(member) {
                m.state = MState::Active;
                m.has_keys = true;
                m.invite = None;
            }
            s.devices.insert(
                *device,
                MDevice {
                    id: *device,
                    member: *member,
                    state: DState::Active,
                    enrolled: Enrolled::Invite,
                    added_at: now,
                    revoked_at: None,
                },
            );
        }
        MOp::MemberRole {
            member,
            role,
            scope,
        } => {
            if let Some(m) = s.members.get_mut(member) {
                m.role = *role;
                m.scope = if role.needs_scope() { *scope } else { None };
            }
        }
        MOp::MemberSuspend { member } => {
            if let Some(m) = s.members.get_mut(member) {
                m.state = MState::Suspended;
            }
        }
        MOp::MemberRemove { member } => {
            if let Some(m) = s.members.get_mut(member) {
                m.state = MState::Removed;
                m.removed_at = Some(now);
                // an open invitation dies with the member
                m.invite = None;
            }
            for d in s.devices.values_mut().filter(|d| d.member == *member) {
                if d.state != DState::Revoked {
                    d.state = DState::Revoked;
                    d.revoked_at = Some(now);
                }
            }
            s.memberships.retain(|(_, m)| m != member);
        }
        MOp::MemberManaged { member, managed } => {
            if let Some(m) = s.members.get_mut(member) {
                m.managed = *managed;
            }
        }
        MOp::GroupCreate { group, parent } => {
            s.groups.insert(*group, *parent);
        }
        MOp::GroupMember {
            group,
            member,
            present,
        } => {
            if *present {
                s.memberships.insert((*group, *member));
            } else {
                s.memberships.remove(&(*group, *member));
            }
        }
        MOp::GroupMove { group, new_parent } => {
            s.groups.insert(*group, Some(*new_parent));
        }
        MOp::PolicySet { scope, patch } => match scope {
            PScope::Account => {
                let after = s.policy.applied(patch);
                if after != s.policy {
                    s.policy = after;
                    s.policy_epoch = s.policy_epoch.saturating_add(1);
                }
            }
            PScope::Group(g) => {
                let before = s.effective_policy(*g);
                if patch.is_empty() {
                    s.group_patches.remove(g);
                } else {
                    s.group_patches.insert(*g, patch.clone());
                }
                let after = s.effective_policy(*g);
                if before != after {
                    s.policy_epoch = s.policy_epoch.saturating_add(1);
                }
            }
        },
        MOp::EscrowSet { key, holders } => {
            s.escrow_pub = Some(*key);
            s.escrow_holders = holders.clone();
        }
        MOp::AuditKeySet { key } => {
            s.audit_pub = Some(*key);
        }
        MOp::AdminQuorumSet { quorum } => {
            if s.policy.admin_quorum != *quorum {
                s.policy.admin_quorum = *quorum;
                s.policy_epoch = s.policy_epoch.saturating_add(1);
            }
        }
        MOp::EscrowOpen { member, reason } => {
            let removed = s
                .members
                .get(member)
                .is_some_and(|m| m.state == MState::Removed);
            let ready_at = if removed {
                None
            } else {
                Some(plus(now, s.policy.escrow_open_wait))
            };
            s.pending_opens.push(MOpen {
                seq,
                member: *member,
                reason: *reason,
                requested_at: now,
                ready_at,
                signers: counted,
            });
        }
        MOp::Delegate { key, ops, expires } => {
            if ops.is_empty() {
                s.delegations.remove(key);
            } else if let Some(granted_by) = signer_mem {
                s.delegations.insert(
                    *key,
                    MDelegation {
                        ops: ops.clone(),
                        expires: *expires,
                        granted_by,
                    },
                );
            }
        }
        MOp::Entitlement {
            version,
            active,
            quota,
            seats,
        } => {
            s.entitlement = Some(MEntitlement {
                version: *version,
                active: *active,
                quota: *quota,
                seats: *seats,
                applied_at: now,
            });
        }
        MOp::App { .. } => {}
    }
    s.head = Some((seq, e.eid));
    s.last_at = Some(now);
    s.epoch = epoch;
}
