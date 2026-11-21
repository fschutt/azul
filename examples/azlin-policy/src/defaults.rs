//! The defaults of the account policy: the table of planning/architecture.md section 5.10, one
//! column for each kind of account, written row by row in the order and the words of the table. The
//! doc-table test of this crate (`defaults_match_architecture_5_10_table`) reads the plan's own
//! table and compares each cell with [`Policy::defaults`], and a copy of the plan with one cell
//! edited must fail it.
//!
//! The cells of the table that are not a plain default value are read as follows (each is a row of
//! the drift register of lane W1-S17b): "optional" (`sso_required`, `sso_max_age_s`) is a choice an
//! admin may make, so the default is off and no maximum age; "self or quorum" (`device_approval`
//! of a team and a business) is the value of that kind, read literally, so that a changed cell is
//! seen by the doc-table test; `admin_quorum` has no value for a `team`, so it is 1 (the signer
//! alone); the elided middle of `updates` (auto / ... / `managed_by_mdm`) is `auto`;
//! `link_policy` and `request_policy` have no value for any kind, so they bound nothing; `padding`
//! is `buckets`, "the default" of planning/architecture.md section 5.15 (M4).
//!
//! Implements planning/architecture.md section 5.10 (the account policy) and section 5.15 (M4,
//! `padding`).

use azlin_seams::Span;

use crate::fields::{
    AdminQuorum, AuditLevel, DeviceApproval, ExternalSharing, ManagedDefault, PaddingPolicy,
    Policy, RequestPolicy, UpdatePolicy,
};
use crate::kind::{by_kind, AccountKind};
use crate::link_policy::LinkPolicy;

impl Policy {
    /// The default policy of an account of this kind: the column of the table of
    /// planning/architecture.md section 5.10 for personal, family, team or business. Every row is
    /// one line below, with the four cells in the order of the table (personal, family, team,
    /// business) where the row has four.
    ///
    /// Implements planning/architecture.md section 5.10 (the table, "personal / family / team /
    /// business default").
    ///
    /// Seam: S17c1 (`genesis` starts an account with it), S9 (the registry glue of
    /// COMMON-W1R R11), S47a, S47b (the escalation stories start from it).
    pub fn defaults(kind: AccountKind) -> Policy {
        Policy {
            // credential_ttl_s: 12 h / 12 h / 12 h / 1 h
            credential_ttl: by_kind(
                kind,
                Span::hours(12),
                Span::hours(12),
                Span::hours(12),
                Span::hours(1),
            ),
            // device_approval: self / self / self or quorum / self or quorum
            device_approval: by_kind(
                kind,
                DeviceApproval::SelfOnly,
                DeviceApproval::SelfOnly,
                DeviceApproval::SelfOrQuorum,
                DeviceApproval::SelfOrQuorum,
            ),
            // max_devices_per_member: 10
            max_devices_per_member: 10,
            // managed_default: no / no (private spaces never) / no / yes
            managed_default: by_kind(
                kind,
                ManagedDefault::No,
                ManagedDefault::No,
                ManagedDefault::No,
                ManagedDefault::Yes,
            ),
            // sso_required, sso_max_age_s: no / no / optional / optional ("optional" is a choice:
            // off, with no maximum age, until an admin turns it on)
            sso_required: false,
            sso_max_age: None,
            // enroll_hold_s: 48 h for any enrollment without the person's own device (D42)
            enroll_hold: Span::hours(48),
            // passkey_recovery: on / on / off / off
            passkey_recovery: by_kind(kind, true, true, false, false),
            // escrow_open_wait_s: 72 h for an active member
            escrow_open_wait: Span::hours(72),
            // audit_level: none / none / admin / access summary
            audit_level: by_kind(
                kind,
                AuditLevel::None,
                AuditLevel::None,
                AuditLevel::Admin,
                AuditLevel::AccessSummary,
            ),
            // admin_quorum: 1 for personal and family; business: 2 (a team: 1, the table gives none)
            admin_quorum: by_kind(
                kind,
                AdminQuorum::ONE,
                AdminQuorum::ONE,
                AdminQuorum::ONE,
                AdminQuorum::TWO,
            ),
            // link_policy and request_policy: the table gives no value for any kind
            link_policy: LinkPolicy::allow_all(),
            request_policy: RequestPolicy::unbounded(),
            // browser_devices: allowed / allowed / allowed / allowed
            browser_devices: true,
            // external_sharing: allowed / allowed / allowed / same workspace only
            external_sharing: by_kind(
                kind,
                ExternalSharing::Allowed,
                ExternalSharing::Allowed,
                ExternalSharing::Allowed,
                ExternalSharing::SameWorkspaceOnly,
            ),
            // updates: auto / ... / managed_by_mdm (the elided middle is auto)
            updates: by_kind(
                kind,
                UpdatePolicy::Auto,
                UpdatePolicy::Auto,
                UpdatePolicy::Auto,
                UpdatePolicy::ManagedByMdm,
            ),
            // padding: none, buckets (the default) or full (planning/architecture.md section 5.15)
            padding: PaddingPolicy::Buckets,
        }
    }
}
