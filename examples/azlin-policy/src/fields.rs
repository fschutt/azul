//! The account policy of planning/architecture.md section 5.10 as typed data. Every row of the table
//! is one field of [`Policy`]; the rows that are structures of their own (`link_policy`, section
//! 5.12, and `request_policy`) are [`LinkPolicy`] (link_policy.rs) and [`RequestPolicy`]. Durations
//! are [`Span`]s and sizes are [`ByteCount`]s: no raw seconds or bytes in a public field (the plan
//! writes `credential_ttl_s`; here the unit is the type).
//!
//! The values with a closed set of words are enums with strict words (`as_str`, `FromStr`: exactly
//! the spelling of the table in lower case with underscores, never a default for an unknown word).
//! [`Field`] names every leaf of the policy; the ONE table in `lib.rs` (`policy_fields!`) says, for
//! each leaf, where it lives in the policy and how it tightens, and it drives [`Field`], the patch
//! (`patch.rs`), the tightening check and the clamp (`tighten.rs`).
//!
//! Not a field: `allowed_networks`. The table says "later: an allow-list checked in RAM, never
//! stored": the field is reserved, and nothing here can carry it, so a policy never holds one
//! (drift register of lane W1-S17b).
//!
//! Implements planning/architecture.md section 5.10 (the account policy), section 5.15 (M4: policy
//! `padding` = none, buckets or full) and section 5.9 (`policy_set`, `admin_quorum_set`).

use core::fmt;

use azlin_seams::{ByteCount, Span};

use crate::link_policy::LinkPolicy;

/// The most people an `admin_quorum` may ask for. The plan gives no maximum (est.): a quorum of more
/// than a handful of admins cannot be met by any account and would lock the account out of
/// `policy_set` (lowering a quorum needs the current quorum), so a quorum above this is no value.
///
/// Implements planning/architecture.md section 5.10 (row `admin_quorum`) and section 1.4 rule 4
/// (everything has a limit, stated up front).
///
/// Caller: `AdminQuorum::new`.
pub const MAX_ADMIN_QUORUM: u8 = 8;

/// The number of different people an `admin_quorum` asks for: at least 1 (a quorum of 0 would ask for
/// no signature at all) and at most [`MAX_ADMIN_QUORUM`]. Made with [`AdminQuorum::new`]; there is
/// no way to make an invalid one.
///
/// Implements planning/architecture.md section 5.10 (row `admin_quorum`: 1 for personal and family;
/// business: 2 for admin changes, policy, escrow, escrow_open, quorum enrollments).
///
/// Caller: `Policy`, `required_people`; Seam: S17c1 (`admin_quorum_set` builds one from the log
/// entry's number and refuses the error).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdminQuorum(u8);

/// Why a number is no admin quorum.
///
/// Implements planning/architecture.md section 5.10 (row `admin_quorum`).
///
/// Caller: `AdminQuorum::new`; Seam: S17c1 (`admin_quorum_set` refuses the entry with it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("an admin quorum is 1 to {} people, not {people}", MAX_ADMIN_QUORUM)]
pub struct QuorumOutOfRange {
    /// The refused number.
    pub people: u8,
}

impl AdminQuorum {
    /// One person: personal, family and team accounts (the signer alone suffices).
    pub const ONE: AdminQuorum = AdminQuorum(1);

    /// Two different people: the default of a business for the five classes of changes that the
    /// quorum guards.
    pub const TWO: AdminQuorum = AdminQuorum(2);

    /// The quorum of `people` different people; an error for 0 and for more than
    /// [`MAX_ADMIN_QUORUM`].
    pub fn new(people: u8) -> Result<AdminQuorum, QuorumOutOfRange> {
        if !(1..=MAX_ADMIN_QUORUM).contains(&people) {
            return Err(QuorumOutOfRange { people });
        }
        Ok(AdminQuorum(people))
    }

    /// The number of different people.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// `device_approval`: who may approve a new device of a member.
///
/// Implements planning/architecture.md section 5.10 (row `device_approval`: self / self / self or
/// quorum / self or quorum) and section 7.5 ("A new device for an existing person").
///
/// Caller: `Policy`; Seam: S17e (`azlin-enroll` refuses a quorum enrollment under `SelfOnly`),
/// S17c1 (`device_add` by quorum).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceApproval {
    /// Only an existing device of the member approves ("self").
    SelfOnly,
    /// An existing device of the member, or a quorum of admins ("self or quorum").
    SelfOrQuorum,
}

words!(
    DeviceApproval,
    "device approval",
    [SelfOnly => "self", SelfOrQuorum => "self_or_quorum"]
);

/// `managed_default`: whether a new member is managed (the company can recover the member's keys
/// through escrow) or private (only the member's own devices).
///
/// Implements planning/architecture.md section 5.10 (row `managed_default`: no / no (private spaces
/// never) / no / yes) and section 7.2 ("Members ... each managed ... or private").
///
/// Caller: `Policy`; Seam: S17c1 (`member_invite` and `member_managed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ManagedDefault {
    /// A new member is private.
    No,
    /// A new member is managed.
    Yes,
}

words!(ManagedDefault, "managed default", [No => "no", Yes => "yes"]);

impl ManagedDefault {
    /// Whether a new member is managed.
    pub const fn is_managed(self) -> bool {
        matches!(self, ManagedDefault::Yes)
    }
}

/// `audit_level`: what the access audit records (none, admin, access_summary, access_full).
///
/// Implements planning/architecture.md section 5.10 (row `audit_level`) and section 5.14 ("with
/// `audit_level` above `none`, the metadata group appends per-account events").
///
/// Caller: `Policy`; Seam: S9 (the registry's audit appends).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuditLevel {
    /// No access audit (the signed team log is the admin audit).
    None,
    /// Admin actions.
    Admin,
    /// A summary of accesses.
    AccessSummary,
    /// Every access.
    AccessFull,
}

words!(
    AuditLevel,
    "audit level",
    [
        None => "none",
        Admin => "admin",
        AccessSummary => "access_summary",
        AccessFull => "access_full",
    ]
);

/// `external_sharing`: whether the account's spaces may be shared with members of other accounts
/// (external members, `reg_grant`). The table's two values: allowed, and "same workspace only".
///
/// Implements planning/architecture.md section 5.10 (row `external_sharing`: allowed / allowed /
/// allowed / same workspace only) and section 5.9 ("external members only if `external_sharing`
/// allows").
///
/// Caller: `Policy`; Seam: S16b (`azlin-spaces` refuses an external grant under
/// `SameWorkspaceOnly`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExternalSharing {
    /// External members are allowed.
    Allowed,
    /// Only members of the same workspace: no external member.
    SameWorkspaceOnly,
}

words!(
    ExternalSharing,
    "external sharing",
    [Allowed => "allowed", SameWorkspaceOnly => "same_workspace_only"]
);

/// `updates`: how the apps of the account take updates. The plan lists "auto / ... /
/// `managed_by_mdm`": the elided middle is `Notify` (drift register of lane W1-S17b).
///
/// Implements planning/architecture.md section 5.10 (row `updates`) and section 7.7 ("with
/// `updates = managed_by_mdm` the app never updates itself").
///
/// Caller: `Policy`; Seam: the apps' update channel (S48, AzWorkspace).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UpdatePolicy {
    /// The app updates itself.
    Auto,
    /// The app says that an update is there and the person decides.
    Notify,
    /// The company's MDM updates the app; the app never updates itself.
    ManagedByMdm,
}

words!(
    UpdatePolicy,
    "update policy",
    [Auto => "auto", Notify => "notify", ManagedByMdm => "managed_by_mdm"]
);

/// `padding`: how much a pack's size is hidden (metadata privacy, M4).
///
/// Implements planning/architecture.md section 5.15 (M4: "policy `padding` = none, buckets (the
/// default) or full").
///
/// Caller: `Policy`; Seam: S14c (`azlin-content` pack padding).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaddingPolicy {
    /// No padding.
    None,
    /// Packs padded to buckets (the default).
    Buckets,
    /// Full padding.
    Full,
}

words!(
    PaddingPolicy,
    "padding policy",
    [None => "none", Buckets => "buckets", Full => "full"]
);

/// The `request_policy` row of planning/architecture.md section 5.10: "allowed, maximum expiry,
/// maximum files and bytes". A request (`reg_request`, section 5.12) is an upload-only door that a
/// member opens; the policy bounds what a member may open. `allowed: false` forbids requests, and
/// the other fields are then of no use; the maxima are ceilings of one request.
///
/// Implements planning/architecture.md section 5.10 (row `request_policy`) and section 5.12
/// (Requests: `expires_at`, `max_files`, `max_bytes`).
///
/// Caller: `Policy`, `check_tightens`, `clamp`; Seam: S16f (`azlin-requests` `validate_spec`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestPolicy {
    /// Whether requests may be opened at all.
    pub allowed: bool,
    /// The longest time a request may live; `None`: no bound.
    pub max_expiry: Option<Span>,
    /// The most files one request may take.
    pub max_files: u32,
    /// The most bytes one request may take.
    pub max_bytes: ByteCount,
}

impl RequestPolicy {
    /// The policy that bounds nothing: requests allowed, no longest expiry, and the largest number
    /// of files and bytes the types hold. Planning/architecture.md section 5.10 gives no
    /// `request_policy` value for any kind, so this is the default of every kind (drift register of
    /// lane W1-S17b); the limits of the system itself apply whatever the policy says.
    ///
    /// Implements planning/architecture.md section 5.10 (row `request_policy`).
    ///
    /// Caller: `Policy::defaults`.
    pub const fn unbounded() -> RequestPolicy {
        RequestPolicy {
            allowed: true,
            max_expiry: None,
            max_files: u32::MAX,
            max_bytes: ByteCount::MAX,
        }
    }
}

/// The account policy: one field for every row of the table of planning/architecture.md section
/// 5.10. `Policy::defaults(kind)` is the table's column for a kind; [`apply`](crate::apply) puts a
/// `policy_set` patch on it; [`effective`](crate::effective) computes the policy of a group down a
/// tree (a group inherits and may only tighten); [`required_people`](crate::required_people) and
/// [`quorum_met`](crate::quorum_met) read `admin_quorum`.
///
/// Implements planning/architecture.md section 5.10 (the table) and section 7.2 ("Policies sit on
/// the account or a group; subgroups inherit and may tighten, never loosen").
///
/// Seam: S17c1 (`azlin-teamlog` `AccountView`, `policy_set`), S16c1 (`azlin-links`), S16f
/// (`azlin-requests`), S16b (`azlin-spaces`), S17e (`azlin-enroll`), the registry glue of S9
/// (`From<&Policy>`, COMMON-W1R R11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    /// `credential_ttl_s`: how long a credential lives (12 h; a business 1 h).
    pub credential_ttl: Span,
    /// `device_approval`: who approves a new device of a member.
    pub device_approval: DeviceApproval,
    /// `max_devices_per_member`: the most devices one member may have (10).
    pub max_devices_per_member: u8,
    /// `managed_default`: whether a new member is managed.
    pub managed_default: ManagedDefault,
    /// `sso_required`: a refresh needs a fresh SSO attestation of the identity service.
    pub sso_required: bool,
    /// `sso_max_age_s`: how old that attestation may be; `None`: no bound. A maximum age keeps
    /// sessions through identity-service outages.
    pub sso_max_age: Option<Span>,
    /// `enroll_hold_s`: how long the group withholds the key of any enrollment without the person's
    /// own device (48 h, D42).
    pub enroll_hold: Span,
    /// `passkey_recovery`: whether the passkey wrap may recover a member's keys.
    pub passkey_recovery: bool,
    /// `escrow_open_wait_s`: the wait before an escrow opening for an active member (72 h).
    pub escrow_open_wait: Span,
    /// `audit_level`: what the access audit records.
    pub audit_level: AuditLevel,
    /// `admin_quorum`: how many different people the guarded changes need.
    pub admin_quorum: AdminQuorum,
    /// `link_policy`: the bounds of the links members make.
    pub link_policy: LinkPolicy,
    /// `request_policy`: the bounds of the requests members open.
    pub request_policy: RequestPolicy,
    /// `browser_devices`: whether AzDrive in the browser (`drive.azlin.io`) may hold a device.
    pub browser_devices: bool,
    /// `external_sharing`: whether spaces may be shared with other accounts.
    pub external_sharing: ExternalSharing,
    /// `updates`: how the apps take updates.
    pub updates: UpdatePolicy,
    /// `padding`: how much a pack's size is hidden (M4).
    pub padding: PaddingPolicy,
}

/// Defines [`Field`] from the table of `policy_fields!`.
macro_rules! define_field {
    ($(($variant:ident, $name:literal, $($path:ident).+, $rule:ident)),+ $(,)?) => {
        /// One leaf of the policy: one row of `policy_fields!`. The name of a leaf is its path in
        /// [`Policy`] (`link_policy.uploads`), which is also the name the refusals of the team log
        /// show.
        ///
        /// Implements planning/architecture.md section 5.10 (every row of the table, the rows
        /// `link_policy` and `request_policy` by their items).
        ///
        /// Caller: `Loosened`, `PolicyPatch::sets`; Seam: S17c1 (`PolicyForbids` names the field).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Field {
            $(
                #[doc = $name]
                $variant
            ),+
        }

        impl Field {
            /// Every leaf of the policy, in the order of the table.
            pub const ALL: &'static [Field] = &[$(Field::$variant),+];

            /// The name of the leaf: its path in [`Policy`], for example `credential_ttl` or
            /// `link_policy.uploads`.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Field::$variant => $name),+
                }
            }
        }
    };
}

policy_fields!(define_field);

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
