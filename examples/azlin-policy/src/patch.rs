//! The patch: what `policy_set` carries (planning/architecture.md section 5.9: "`policy_set` | admin,
//! with the policy's quorum"), for the account or for one group. Every field is an `Option`:
//! `None` leaves the field as it is, `Some(value)` sets it, and nothing else changes. A field that
//! is itself optional in the policy (`sso_max_age`, `link_policy.max_expiry`,
//! `request_policy.max_expiry`) takes an `Option<Option<Span>>`: the outer `Option` says whether
//! the patch sets the field, the inner one is the value (`Some(None)` removes the bound).
//!
//! The patch has the shape of [`Policy`] (the structures of `link_policy` and `request_policy` have
//! a patch of their own), and [`apply`] puts it on a policy field by field with the ONE table of
//! `policy_fields!` (lib.rs), so a field of the table that the patch could not set does not compile.
//!
//! Implements planning/architecture.md section 5.9 (`policy_set`) and section 5.10 (the account
//! policy).

use azlin_seams::{ByteCount, Span};

use crate::fields::{
    AdminQuorum, AuditLevel, DeviceApproval, ExternalSharing, Field, ManagedDefault, PaddingPolicy,
    Policy, UpdatePolicy,
};
use crate::link_policy::AudienceSet;

/// The patch of the `link_policy` row: one `Option` for each field of
/// [`LinkPolicy`](crate::LinkPolicy).
///
/// Implements planning/architecture.md section 5.10 (row `link_policy`) and section 5.9
/// (`policy_set`).
///
/// Caller: `PolicyPatch`; Seam: S17c1 (the arguments of `policy_set`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkPolicyPatch {
    /// Sets `link_policy.audiences`.
    pub audiences: Option<AudienceSet>,
    /// Sets `link_policy.max_expiry` (`Some(None)`: no bound).
    pub max_expiry: Option<Option<Span>>,
    /// Sets `link_policy.password_for_anyone`.
    pub password_for_anyone: Option<bool>,
    /// Sets `link_policy.force_preview_only`.
    pub force_preview_only: Option<bool>,
    /// Sets `link_policy.force_watermark`.
    pub force_watermark: Option<bool>,
    /// Sets `link_policy.force_app_only`.
    pub force_app_only: Option<bool>,
    /// Sets `link_policy.uploads`.
    pub uploads: Option<bool>,
    /// Sets `link_policy.edit`.
    pub edit: Option<bool>,
    /// Sets `link_policy.comments`.
    pub comments: Option<bool>,
    /// Sets `link_policy.open_logs`.
    pub open_logs: Option<bool>,
    /// Sets `link_policy.slugs`.
    pub slugs: Option<bool>,
}

/// The patch of the `request_policy` row: one `Option` for each field of
/// [`RequestPolicy`](crate::RequestPolicy).
///
/// Implements planning/architecture.md section 5.10 (row `request_policy`) and section 5.9
/// (`policy_set`).
///
/// Caller: `PolicyPatch`; Seam: S17c1 (the arguments of `policy_set`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RequestPolicyPatch {
    /// Sets `request_policy.allowed`.
    pub allowed: Option<bool>,
    /// Sets `request_policy.max_expiry` (`Some(None)`: no bound).
    pub max_expiry: Option<Option<Span>>,
    /// Sets `request_policy.max_files`.
    pub max_files: Option<u32>,
    /// Sets `request_policy.max_bytes`.
    pub max_bytes: Option<ByteCount>,
}

/// What `policy_set` carries: a patch for the policy of the account or of one group. A field that
/// is `None` is left as it is. For a group the patch is put on what the group inherits, and the
/// result is checked and clamped against it (`check_tightens`, `effective`).
///
/// Implements planning/architecture.md section 5.9 (`policy_set`, `admin_quorum_set`) and section
/// 5.10 (the account policy: one patch field for each row).
///
/// Caller: `apply`, `effective`; Seam: S17c1 (`azlin-teamlog` decodes the arguments of `policy_set`
/// into it and stores one per account and per group).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PolicyPatch {
    /// Sets `credential_ttl`.
    pub credential_ttl: Option<Span>,
    /// Sets `device_approval`.
    pub device_approval: Option<DeviceApproval>,
    /// Sets `max_devices_per_member`.
    pub max_devices_per_member: Option<u8>,
    /// Sets `managed_default`.
    pub managed_default: Option<ManagedDefault>,
    /// Sets `sso_required`.
    pub sso_required: Option<bool>,
    /// Sets `sso_max_age` (`Some(None)`: no bound).
    pub sso_max_age: Option<Option<Span>>,
    /// Sets `enroll_hold`.
    pub enroll_hold: Option<Span>,
    /// Sets `passkey_recovery`.
    pub passkey_recovery: Option<bool>,
    /// Sets `escrow_open_wait`.
    pub escrow_open_wait: Option<Span>,
    /// Sets `audit_level`.
    pub audit_level: Option<AuditLevel>,
    /// Sets `admin_quorum`.
    pub admin_quorum: Option<AdminQuorum>,
    /// The fields of `link_policy` this patch sets.
    pub link_policy: LinkPolicyPatch,
    /// The fields of `request_policy` this patch sets.
    pub request_policy: RequestPolicyPatch,
    /// Sets `browser_devices`.
    pub browser_devices: Option<bool>,
    /// Sets `external_sharing`.
    pub external_sharing: Option<ExternalSharing>,
    /// Sets `updates`.
    pub updates: Option<UpdatePolicy>,
    /// Sets `padding`.
    pub padding: Option<PaddingPolicy>,
}

impl PolicyPatch {
    /// The patch that sets nothing: [`apply`] gives back the policy it was given.
    ///
    /// Implements planning/architecture.md section 5.9 (`policy_set`).
    ///
    /// Caller: `PolicyPatch::is_empty`; Seam: S17c1 (a group whose policy is not set has this
    /// patch), S47a (the stories).
    pub fn none() -> PolicyPatch {
        PolicyPatch::default()
    }

    /// The fields this patch sets, in the order of [`Field::ALL`]: empty for [`PolicyPatch::none`].
    ///
    /// Implements planning/architecture.md section 5.10 (the account policy).
    ///
    /// Caller: `PolicyPatch::is_empty`; Seam: S17c1 (the notice `policy_changed` of 5.13 and the
    /// refusal of a group patch for a field the group may not change).
    pub fn sets(&self) -> Vec<Field> {
        let mut fields = Vec::new();
        macro_rules! collect {
            ($(($variant:ident, $name:literal, $($path:ident).+, $rule:ident)),+ $(,)?) => {
                $(
                    if self.$($path).+.is_some() {
                        fields.push(Field::$variant);
                    }
                )+
            };
        }
        policy_fields!(collect);
        fields
    }

    /// Whether the patch sets no field at all.
    ///
    /// Implements planning/architecture.md section 5.9 (`policy_set`).
    ///
    /// Seam: S17c1 (an empty `policy_set` is no change).
    pub fn is_empty(&self) -> bool {
        self.sets().is_empty()
    }
}

/// The policy `base` with the fields of `patch` set and nothing else changed: the new policy of the
/// account when `base` is the account's, and the policy a group would have before the clamp when
/// `base` is what the group inherits. It does not check anything: whether a group's result may
/// stand is `check_tightens`, and what a group really gets is `effective`.
///
/// Implements planning/architecture.md section 5.9 (`policy_set`) and section 5.10 (the account
/// policy).
///
/// Caller: `effective`; Seam: S17c1 (`azlin-teamlog` applies a `policy_set` entry to the account's
/// policy).
pub fn apply(base: &Policy, patch: &PolicyPatch) -> Policy {
    let mut policy = base.clone();
    macro_rules! set {
        ($(($variant:ident, $name:literal, $($path:ident).+, $rule:ident)),+ $(,)?) => {
            $(
                if let Some(value) = patch.$($path).+ {
                    policy.$($path).+ = value;
                }
            )+
        };
    }
    policy_fields!(set);
    policy
}
