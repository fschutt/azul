//! Members of the team log: the state, the invitation and the app principal of a member (planning/
//! architecture.md section 5.3 `reg_member`, section 5.11 "Invites", section 7.2; build unit S17,
//! lane W1-S17c1).
//!
//! A member is a person or an app. The log creates it (`genesis` for the first owner, `member_invite`
//! for every other one), activates it (`member_join`), and changes its role, state and managed flag.
//! The registry's `reg_member` columns that the log does not write (`quota_cap`) are the registry's
//! own.

use std::collections::BTreeSet;

use azlin_seams::{ChainSeq, GroupId, Hash32, MemberId, MemberKind, Stamp};

use crate::devices::PublicKeys;
use crate::ops::AppName;
use crate::roles::Role;

/// The state of a member (planning/architecture.md section 5.3 `reg_member.state`: "pending, invited,
/// active, suspended, removed").
///
/// A member made by `member_invite` is `Invited` until its `member_join`, and then `Active`. In v0 the
/// log never makes a member `Pending` (the registry's word for a proposal that an admin has not
/// approved: the authority table has no op for the approval; drift row of lane W1-S17c). `Suspended`
/// and `Removed` are made by `member_suspend` and `member_remove`; nothing leaves `Suspended` except
/// `member_remove` in v0 (the table has no unsuspend: drift row).
///
/// Implements planning/architecture.md section 5.3 (`reg_member.state`).
///
/// Caller: `Member`, `apply.rs` (the transitions), `admit.rs` (a member signs only while `Active`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemberState {
    /// Invited and not joined yet: it has no keys and no device.
    Invited,
    /// A proposal that an admin has not approved (never made by the log in v0).
    Pending,
    /// A member: its devices may sign.
    Active,
    /// Suspended: none of its devices signs anything.
    Suspended,
    /// Removed: its devices are revoked and it is in no group.
    Removed,
}

impl MemberState {
    /// The word of the state in the registry's column.
    ///
    /// Implements planning/architecture.md section 5.3 (`reg_member.state`).
    ///
    /// Caller: the registry glue (TEAMLOG-APPLY); the tests.
    pub const fn word(self) -> &'static str {
        match self {
            MemberState::Invited => "invited",
            MemberState::Pending => "pending",
            MemberState::Active => "active",
            MemberState::Suspended => "suspended",
            MemberState::Removed => "removed",
        }
    }
}

/// An open invitation: the hash of the invite secret and the member who invited (planning/
/// architecture.md section 5.11: "a `member_invite` entry carries the hash of a secret").
///
/// `inviter` is `None` when a delegate key (the identity service) made the invitation: no member
/// confirmed it, so the join must be confirmed by an Active device of an owner or an admin (drift row
/// of lane W1-S17c).
///
/// Implements planning/architecture.md section 5.11 (invites; "the inviter's device confirms").
///
/// Caller: `Member::invite`, `invite.rs` (`check_join`), `apply.rs` (`member_invite`, `member_join`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invite {
    /// The hash of the invite secret (`invite::invite_hash`).
    pub hash: Hash32,
    /// The member whose device confirms the join; `None` for an invitation made by a delegate key.
    pub inviter: Option<MemberId>,
}

/// What an app principal may do: the apps it may sign `app:<name>/...` ops for, and when it expires
/// (planning/architecture.md section 5.3: "For `kind = app`: `app_name`, `scopes`, `expires_at`,
/// `attestation_policy`"; the authority table v0: "an app principal whose `scopes` name `app:<name>`").
///
/// Implements planning/architecture.md section 5.3 (`reg_member` app columns) and section 5.9
/// (`app:<name>/...` ops).
///
/// Caller: `Member::app`, `Op::MemberInvite`, `admit.rs` (`app_op`: the scope and the expiry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppSpec {
    /// The apps the principal may sign ops for.
    pub scopes: BTreeSet<AppName>,
    /// The leader's stamp from which the principal signs nothing; `None`: it does not expire.
    pub expires_at: Option<Stamp>,
}

/// One member of the account, as the team log defines it: the columns of `reg_member` that the log
/// writes.
///
/// Implements planning/architecture.md section 5.3 (`reg_member`: `member_id`, `kind`, `role`,
/// `admin_scope`, `managed`, `state`, the keys, `log_seq`, `added_at`, `removed_at`, the app columns).
///
/// Caller: `TeamState::members`, `RegistryDelta::Member`, `admit.rs` (the signer's and the subject's
/// member).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// The pseudonymous id of the member (chosen by the inviter, or by the first owner's device).
    pub id: MemberId,
    /// A person or an app.
    pub kind: MemberKind,
    /// The role.
    pub role: Role,
    /// The group whose subtree a group admin or a helpdesk member acts in; `None` for every other
    /// role.
    pub admin_scope: Option<GroupId>,
    /// Whether the company can recover the member's keys through escrow (planning/architecture.md
    /// section 7.2: managed or private).
    pub managed: bool,
    /// Invited, pending, active, suspended or removed.
    pub state: MemberState,
    /// The member's keys; `None` until the member joins.
    pub keys: Option<PublicKeys>,
    /// The open invitation; `None` once the member joined (or for the first owner).
    pub invite: Option<Invite>,
    /// The app columns, for a member of kind `App`.
    pub app: Option<AppSpec>,
    /// The leader's stamp of the entry that made the member.
    pub added_at: Stamp,
    /// The leader's stamp of the entry that removed the member.
    pub removed_at: Option<Stamp>,
    /// The position of the newest entry that touched the member.
    pub log_seq: ChainSeq,
}
