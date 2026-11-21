//! The ops of the team log, typed (planning/architecture.md section 5.9 "The team log and the space
//! chains": the table of ops and who may sign; build unit S17, lane W1-S17c1).
//!
//! `Op` is one variant for each row of the table of section 5.9, with the arguments that row needs:
//! `genesis`, `device_add`, `device_remove`, `member_invite`, `member_join`, `member_role`,
//! `member_suspend`, `member_remove`, `member_managed`, `group_create`, `group_member`, `group_move`,
//! `policy_set`, `escrow_set`, `audit_key_set`, `admin_quorum_set`, `escrow_open`, `delegate`,
//! `entitlement` and the `app:<name>/...` ops, which are "stored and served, never interpreted" by the
//! storage layer (`Op::App` carries the app's own bytes and changes no row).
//!
//! The wire form of an entry (azlin-cbor, S5's contract types) is mapped to `RawEntry` by a glue unit;
//! `OpName::from_wire` is the one place the words of the table are read, and a word that is not in it is
//! `UnknownOp` ("unknown wire op -> `Refusal::UnknownOp`"). A person with two members in one account
//! defeats a quorum of distinct people: the authority table counts members, not people (drift row of
//! lane S47a).

use core::fmt;
use std::collections::BTreeSet;

use azlin_policy::{AdminQuorum, PolicyPatch};
use azlin_seams::{ByteCount, DeviceId, GroupId, Hash32, KeyId, MemberId, MemberKind, Stamp};

use crate::devices::PublicKeys;
use crate::members::AppSpec;
use crate::roles::Role;

/// The longest `app:<name>` an app name may have, in bytes.
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up front).
///
/// Caller: `AppName::new`.
pub const APP_NAME_MAX_LEN: usize = 48;

/// The largest `body` of an `app:<name>/...` op: a few hundred bytes are typical (planning/
/// architecture.md section 5.9 "Size"), and the cap keeps one app from filling the log (est.).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.9 (`app:<name>/...`: stored
/// and served).
///
/// Caller: `admit.rs` (`app_op_over_size_cap_refused`).
pub const APP_BODY_MAX: ByteCount = ByteCount::kib(16);

/// The largest `sealed` field of an entry (names and emails are short; est.).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.9 (the `sealed` field).
///
/// Caller: `admit.rs`, `sealed.rs` (`seal_field` refuses a longer plaintext).
pub const SEALED_MAX: ByteCount = ByteCount::kib(4);

/// Why a text is no op of the table.
///
/// Implements planning/architecture.md section 5.9 ("unknown wire op -> `Refusal::UnknownOp`").
///
/// Caller: `OpName::from_wire`; `Refusal::UnknownOp` is made from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("not an op of the team log")]
pub struct UnknownOp;

/// Why a text is no app name.
///
/// Implements planning/architecture.md section 5.9 (`app:<name>/...`).
///
/// Caller: `AppName::new`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadAppName;

impl fmt::Display for BadAppName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "an app name is 1 to {APP_NAME_MAX_LEN} lower-case letters, digits, '.', '_' or '-', and starts with a letter or a digit"
        )
    }
}

impl std::error::Error for BadAppName {}

/// The name of an app in `app:<name>/...`: 1 to [`APP_NAME_MAX_LEN`] bytes of lower-case ASCII letters,
/// digits, `.`, `_` and `-`, starting with a letter or a digit. Ordered, so it can sit in a set.
///
/// Implements planning/architecture.md section 5.9 (`app:<name>/...`).
///
/// Caller: `Op::App`, `AppSpec::scopes`, `OpName::from_wire`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppName(String);

impl AppName {
    /// The app name of this text; `BadAppName` for any other text.
    pub fn new(name: &str) -> Result<AppName, BadAppName> {
        let bytes = name.as_bytes();
        let starts_ok = matches!(bytes.first(), Some(b'a'..=b'z' | b'0'..=b'9'));
        let rest_ok = bytes
            .iter()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-'));
        if bytes.is_empty() || bytes.len() > APP_NAME_MAX_LEN || !starts_ok || !rest_ok {
            return Err(BadAppName);
        }
        Ok(AppName(name.to_string()))
    }

    /// The name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AppName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The name of an op: the 19 fixed words of the table of planning/architecture.md section 5.9 and
/// `app` for every `app:<name>/...` op. Ordered and hashable, so it can sit in the set of ops a
/// delegation names.
///
/// Implements planning/architecture.md section 5.9 (the table of ops).
///
/// Caller: `Op::name`, `Delegation::ops`, `Notice::op`, `Refusal`, `authority.rs` (the table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpName {
    /// `genesis`: the first entry of the log.
    Genesis,
    /// `device_add`.
    DeviceAdd,
    /// `device_remove`.
    DeviceRemove,
    /// `member_invite`.
    MemberInvite,
    /// `member_join`.
    MemberJoin,
    /// `member_role`.
    MemberRole,
    /// `member_suspend`.
    MemberSuspend,
    /// `member_remove`.
    MemberRemove,
    /// `member_managed`.
    MemberManaged,
    /// `group_create`.
    GroupCreate,
    /// `group_member`.
    GroupMember,
    /// `group_move`.
    GroupMove,
    /// `policy_set`.
    PolicySet,
    /// `escrow_set`.
    EscrowSet,
    /// `audit_key_set`.
    AuditKeySet,
    /// `admin_quorum_set`.
    AdminQuorumSet,
    /// `escrow_open`.
    EscrowOpen,
    /// `delegate`.
    Delegate,
    /// `entitlement`.
    Entitlement,
    /// `app:<name>/...`.
    App,
}

impl OpName {
    /// Every op, in the order of the table.
    pub const ALL: [OpName; 20] = [
        OpName::Genesis,
        OpName::DeviceAdd,
        OpName::DeviceRemove,
        OpName::MemberInvite,
        OpName::MemberJoin,
        OpName::MemberRole,
        OpName::MemberSuspend,
        OpName::MemberRemove,
        OpName::MemberManaged,
        OpName::GroupCreate,
        OpName::GroupMember,
        OpName::GroupMove,
        OpName::PolicySet,
        OpName::EscrowSet,
        OpName::AuditKeySet,
        OpName::AdminQuorumSet,
        OpName::EscrowOpen,
        OpName::Delegate,
        OpName::Entitlement,
        OpName::App,
    ];

    /// The word of the op on the wire (`app` for every app op).
    pub const fn as_str(self) -> &'static str {
        match self {
            OpName::Genesis => "genesis",
            OpName::DeviceAdd => "device_add",
            OpName::DeviceRemove => "device_remove",
            OpName::MemberInvite => "member_invite",
            OpName::MemberJoin => "member_join",
            OpName::MemberRole => "member_role",
            OpName::MemberSuspend => "member_suspend",
            OpName::MemberRemove => "member_remove",
            OpName::MemberManaged => "member_managed",
            OpName::GroupCreate => "group_create",
            OpName::GroupMember => "group_member",
            OpName::GroupMove => "group_move",
            OpName::PolicySet => "policy_set",
            OpName::EscrowSet => "escrow_set",
            OpName::AuditKeySet => "audit_key_set",
            OpName::AdminQuorumSet => "admin_quorum_set",
            OpName::EscrowOpen => "escrow_open",
            OpName::Delegate => "delegate",
            OpName::Entitlement => "entitlement",
            OpName::App => "app",
        }
    }

    /// The op of a wire word: one of the 19 fixed words exactly, or `app:<name>/<anything>` with a
    /// valid app name (which is `OpName::App`). Any other text is `UnknownOp`: there is no default op.
    ///
    /// Implements planning/architecture.md section 5.9 ("unknown wire op -> `Refusal::UnknownOp`").
    ///
    /// Caller: the glue that decodes a wire entry (CHAIN-GLUE) before it reads the op's arguments; the
    /// tests.
    pub fn from_wire(text: &str) -> Result<OpName, UnknownOp> {
        if let Some(rest) = text.strip_prefix("app:") {
            let name = match rest.split_once('/') {
                Some((name, _)) => name,
                None => rest,
            };
            return match AppName::new(name) {
                Ok(_) => Ok(OpName::App),
                Err(_) => Err(UnknownOp),
            };
        }
        OpName::ALL
            .iter()
            .copied()
            .find(|op| *op != OpName::App && op.as_str() == text)
            .ok_or(UnknownOp)
    }
}

impl fmt::Display for OpName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The scope of a `policy_set`: the account's own policy, or the patch of one group (planning/
/// architecture.md section 7.2: "Policies sit on the account or a group").
///
/// Implements planning/architecture.md section 5.9 (`policy_set`) and section 7.2.
///
/// Caller: `Op::PolicySet`, `admit.rs`, `apply.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PolicyScope {
    /// The account's policy.
    Account,
    /// The patch of this group's policy: what the group sets on top of what it inherits.
    Group(GroupId),
}

/// Why an `escrow_open` is asked (planning/architecture.md section 7.5: "a leaver's handover, a legal
/// request"). A closed set of words: free text is server-readable and goes into the `sealed` field.
///
/// Implements planning/architecture.md section 7.5 ("an `escrow_open` entry with a reason").
///
/// Caller: `Op::EscrowOpen`, `PendingOpen::reason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EscrowReason {
    /// A leaver's handover.
    LeaverHandover,
    /// A legal request.
    LegalRequest,
    /// Any other reason (the details are in the entry's `sealed` field).
    Other,
}

impl EscrowReason {
    /// The word of the reason.
    pub const fn word(self) -> &'static str {
        match self {
            EscrowReason::LeaverHandover => "leaver_handover",
            EscrowReason::LegalRequest => "legal_request",
            EscrowReason::Other => "other",
        }
    }
}

/// The `state` of an entitlement (COMMON-W1R R10: the words are `Active | Lapsed`).
///
/// Implements planning/architecture.md section 5.3 (`reg_account.entitlement`).
///
/// Caller: `Op::Entitlement`, `EntitlementFacts::state`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntitlementState {
    /// The account's plan is paid.
    Active,
    /// The plan lapsed.
    Lapsed,
}

impl EntitlementState {
    /// The word of the state.
    pub const fn word(self) -> &'static str {
        match self {
            EntitlementState::Active => "active",
            EntitlementState::Lapsed => "lapsed",
        }
    }
}

/// The version of an entitlement: monotone per account, the higher wins (COMMON-W1R R10).
///
/// Implements planning/architecture.md section 5.3 (`reg_account.entitlement`: "the latest
/// entitlement, signed by the payments key").
///
/// Caller: `Op::Entitlement`, `EntitlementFacts::version`, `admit.rs` (a version that is not newer is
/// refused).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntitlementVersion(pub u64);

/// The secret of an invitation, as it travels in a `member_join` entry. The invitation's secret is
/// SPENT by the entry that carries it: the entry is logged and public, so the value is no secret any
/// more (planning/architecture.md section 5.11). `Debug` prints no byte, so a log line never shows one
/// before the join.
///
/// Implements planning/architecture.md section 5.11 ("a `member_invite` entry carries the hash of a
/// secret").
///
/// Caller: `Op::MemberJoin`, `invite::invite_hash`.
#[derive(Clone, PartialEq, Eq)]
pub struct InviteSecret(pub [u8; 32]);

impl fmt::Debug for InviteSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InviteSecret(***)")
    }
}

/// One op of the team log with its arguments (planning/architecture.md section 5.9, the table of
/// ops). Which signer may make which op, and under what conditions, is `authority.rs` and
/// `admit.rs`; what an op does to the state is `apply.rs`.
///
/// Implements planning/architecture.md section 5.9.
///
/// Caller: `RawEntry::op`, `Admitted`, `apply.rs`, `admit.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// `genesis`: the account's first entry. It makes the first owner (`owner`) and the owner's first
    /// device, and pins the genesis hash. The account's kind comes from the registry's account row
    /// (`TeamState::empty`).
    Genesis {
        /// The first owner's member id.
        owner: MemberId,
        /// The first owner device's id.
        device: DeviceId,
        /// The first owner's member keys.
        member_keys: PublicKeys,
        /// The first owner device's keys: the entry is signed with its `sign` key.
        device_keys: PublicKeys,
    },
    /// `device_add`: a new device for `member`. The member's own device approves at once; admins
    /// approve by quorum and the device starts in its hold.
    DeviceAdd {
        /// The member the device is for.
        member: MemberId,
        /// The new device's id.
        device: DeviceId,
        /// The new device's keys.
        keys: PublicKeys,
    },
    /// `device_remove`: revokes a device.
    DeviceRemove {
        /// The device to revoke.
        device: DeviceId,
    },
    /// `member_invite`: makes an Invited member with a role of member or guest.
    MemberInvite {
        /// The pseudonymous id the inviter chose.
        member: MemberId,
        /// A person or an app.
        kind: MemberKind,
        /// The role: member or guest.
        role: Role,
        /// The hash of the invite secret (`invite::invite_hash`).
        invite_hash: Hash32,
        /// The groups the member starts in.
        groups: BTreeSet<GroupId>,
        /// The app columns: `Some` exactly for a member of kind `App`.
        app: Option<AppSpec>,
    },
    /// `member_join`: the invitee's new device joins with the invite secret; the inviter's device
    /// confirms the fingerprint phrase by cosigning (planning/architecture.md section 5.11).
    MemberJoin {
        /// The invited member.
        member: MemberId,
        /// The joiner's first device.
        device: DeviceId,
        /// The member's keys.
        member_keys: PublicKeys,
        /// The device's keys: the entry is signed with its `sign` key.
        device_keys: PublicKeys,
        /// The preimage of the invite hash.
        secret: InviteSecret,
    },
    /// `member_role`: sets the role (and the `admin_scope` of a group admin or a helpdesk member).
    MemberRole {
        /// The subject.
        member: MemberId,
        /// The new role.
        role: Role,
        /// The scope group: `Some` exactly for a group admin or a helpdesk member.
        admin_scope: Option<GroupId>,
    },
    /// `member_suspend`.
    MemberSuspend {
        /// The subject.
        member: MemberId,
    },
    /// `member_remove`.
    MemberRemove {
        /// The subject.
        member: MemberId,
    },
    /// `member_managed`: makes a member managed (escrow-recoverable) or private.
    MemberManaged {
        /// The subject.
        member: MemberId,
        /// The new value.
        managed: bool,
    },
    /// `group_create`: a new group under `parent` (or a new top-level group).
    GroupCreate {
        /// The new group's id.
        group: GroupId,
        /// The parent; `None` for a top-level group.
        parent: Option<GroupId>,
    },
    /// `group_member`: adds a member to a group, or takes it out.
    GroupMember {
        /// The group.
        group: GroupId,
        /// The member.
        member: MemberId,
        /// `true` adds, `false` removes.
        present: bool,
    },
    /// `group_move`: moves a group (and its subtree) under another parent.
    GroupMove {
        /// The group to move.
        group: GroupId,
        /// The new parent.
        new_parent: GroupId,
    },
    /// `policy_set`: a patch of the account's policy, or of one group's.
    PolicySet {
        /// The account or one group.
        scope: PolicyScope,
        /// What the entry sets.
        patch: PolicyPatch,
    },
    /// `escrow_set`: the escrow public key and the set of holders.
    EscrowSet {
        /// The team's escrow public key.
        escrow_pub: [u8; 32],
        /// The members who hold shares of the escrow key.
        holders: BTreeSet<MemberId>,
    },
    /// `audit_key_set`: the team's audit public key.
    AuditKeySet {
        /// The audit public key.
        audit_pub: [u8; 32],
    },
    /// `admin_quorum_set`: the number of different people the guarded changes need.
    AdminQuorumSet {
        /// The new quorum.
        quorum: AdminQuorum,
    },
    /// `escrow_open`: a request to open a member's escrow, signed by a quorum of admins.
    EscrowOpen {
        /// The member whose data is opened.
        member: MemberId,
        /// Why.
        reason: EscrowReason,
    },
    /// `delegate`: names a key and the exact ops it may sign; an empty set revokes the key's
    /// delegation.
    Delegate {
        /// The delegate key (the bytes of its Ed25519 public key).
        key: KeyId,
        /// The ops the key may sign.
        ops: BTreeSet<OpName>,
        /// The leader's stamp from which the delegation is over; `None`: it does not expire.
        expires: Option<Stamp>,
    },
    /// `entitlement`: the payments service's entitlement, replayed into the log.
    Entitlement {
        /// The version: the higher wins.
        version: EntitlementVersion,
        /// Active or lapsed.
        state: EntitlementState,
        /// The quota the plan gives.
        quota: ByteCount,
        /// The seats the plan gives.
        seats: u32,
    },
    /// `app:<name>/...`: stored and served, never interpreted.
    App {
        /// The app.
        name: AppName,
        /// The app's own bytes.
        body: Vec<u8>,
    },
}

impl Op {
    /// The name of the op.
    ///
    /// Implements planning/architecture.md section 5.9 (the table of ops).
    ///
    /// Caller: `Notice::op` (`apply.rs`), `admit.rs`, `authority.rs`.
    pub fn name(&self) -> OpName {
        match self {
            Op::Genesis { .. } => OpName::Genesis,
            Op::DeviceAdd { .. } => OpName::DeviceAdd,
            Op::DeviceRemove { .. } => OpName::DeviceRemove,
            Op::MemberInvite { .. } => OpName::MemberInvite,
            Op::MemberJoin { .. } => OpName::MemberJoin,
            Op::MemberRole { .. } => OpName::MemberRole,
            Op::MemberSuspend { .. } => OpName::MemberSuspend,
            Op::MemberRemove { .. } => OpName::MemberRemove,
            Op::MemberManaged { .. } => OpName::MemberManaged,
            Op::GroupCreate { .. } => OpName::GroupCreate,
            Op::GroupMember { .. } => OpName::GroupMember,
            Op::GroupMove { .. } => OpName::GroupMove,
            Op::PolicySet { .. } => OpName::PolicySet,
            Op::EscrowSet { .. } => OpName::EscrowSet,
            Op::AuditKeySet { .. } => OpName::AuditKeySet,
            Op::AdminQuorumSet { .. } => OpName::AdminQuorumSet,
            Op::EscrowOpen { .. } => OpName::EscrowOpen,
            Op::Delegate { .. } => OpName::Delegate,
            Op::Entitlement { .. } => OpName::Entitlement,
            Op::App { .. } => OpName::App,
        }
    }
}
