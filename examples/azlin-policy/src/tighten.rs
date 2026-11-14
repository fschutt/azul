//! "Groups inherit and may only tighten" (planning/architecture.md section 5.10, section 7.2:
//! "subgroups inherit and may tighten, never loosen"). The plan gives no order of tightness per
//! field, so this file is the ONE place that says which direction is tighter for each field (the
//! proposed order of the brief of lane W1-S17b; drift register X-DELTA-W1S17B-1: if the owner
//! answers differently, only the rule column of `policy_fields!` in lib.rs changes):
//!
//! | rule | the tighter value | fields |
//! |---|---|---|
//! | [`SmallerIsTighter`] | the smaller | `credential_ttl`, `max_devices_per_member`, `request_policy.max_files`, `request_policy.max_bytes` |
//! | [`LargerIsTighter`] | the larger | `enroll_hold`, `escrow_open_wait`, `admin_quorum` |
//! | [`SmallerLimitIsTighter`] | a limit, and the smaller limit (no limit is the loosest) | `sso_max_age`, `link_policy.max_expiry`, `request_policy.max_expiry` |
//! | [`OnIsTighter`] | `true` | `sso_required`, `link_policy.password_for_anyone`, `link_policy.force_preview_only`, `link_policy.force_watermark`, `link_policy.force_app_only` |
//! | [`OffIsTighter`] | `false` | `passkey_recovery`, `browser_devices`, `request_policy.allowed`, `link_policy.uploads`, `.edit`, `.comments`, `.open_logs`, `.slugs` |
//! | [`HigherRankIsTighter`] | the higher [`Rank`] | `device_approval` (`SelfOnly` over `SelfOrQuorum`), `audit_level` (none, admin, access_summary, access_full), `external_sharing` (`SameWorkspaceOnly` over `Allowed`) |
//! | [`SubsetIsTighter`] | the subset | `link_policy.audiences` |
//! | [`SameAsParent`] | none: a group may not change it | `managed_default`, `updates`, `padding` |
//!
//! A rule gives two functions of the trait [`Tighter`]: whether a child's value is at least as tight
//! as its parent's (the check at `policy_set` time, [`check_tightens`]), and the tighter of the two
//! (the clamp that `effective` applies down the tree, [`clamp`]). Both read the same rule, so a
//! value that the check accepts is never changed by the clamp, and a value that it refuses is.
//!
//! Implements planning/architecture.md section 5.10 ("groups inherit and may only tighten") and
//! section 7.2 ("Policies (5.10) sit on the account or a group; subgroups inherit and may tighten,
//! never loosen").

use azlin_seams::{ByteCount, Span};

use crate::fields::{
    AdminQuorum, AuditLevel, DeviceApproval, ExternalSharing, Field, ManagedDefault, PaddingPolicy,
    Policy, UpdatePolicy,
};
use crate::link_policy::AudienceSet;

/// How the values of one type of field are ordered by tightness, for one direction of the order. The
/// implementors are the rule types of this file; `policy_fields!` names one for each field.
///
/// Implements planning/architecture.md section 5.10 ("groups inherit and may only tighten").
///
/// Caller: `check_tightens`, `clamp`; Seam: S47a, S47b (the escalation models read the rules).
pub trait Tighter<T> {
    /// Whether `child` is at least as tight as `parent` (equal is tight enough).
    fn at_least_as_tight(parent: &T, child: &T) -> bool;

    /// The tighter of the two values: never looser than either of them (for a rule with a partial
    /// order the greatest value that is no looser than both). For [`SameAsParent`] it is the
    /// parent's value.
    fn tighter(parent: &T, child: &T) -> T;
}

/// Smaller is tighter.
///
/// Implements planning/architecture.md section 5.10 (`credential_ttl`, `max_devices_per_member`,
/// `request_policy` maxima).
///
/// Caller: `policy_fields!` (lib.rs).
#[derive(Clone, Copy, Debug)]
pub struct SmallerIsTighter;

impl<T: Ord + Clone> Tighter<T> for SmallerIsTighter {
    fn at_least_as_tight(parent: &T, child: &T) -> bool {
        child <= parent
    }

    fn tighter(parent: &T, child: &T) -> T {
        if child < parent {
            child.clone()
        } else {
            parent.clone()
        }
    }
}

/// Larger is tighter.
///
/// Implements planning/architecture.md section 5.10 (`enroll_hold`, `escrow_open_wait`,
/// `admin_quorum`).
///
/// Caller: `policy_fields!` (lib.rs).
#[derive(Clone, Copy, Debug)]
pub struct LargerIsTighter;

impl<T: Ord + Clone> Tighter<T> for LargerIsTighter {
    fn at_least_as_tight(parent: &T, child: &T) -> bool {
        child >= parent
    }

    fn tighter(parent: &T, child: &T) -> T {
        if child > parent {
            child.clone()
        } else {
            parent.clone()
        }
    }
}

/// A field that is a limit or no limit (`None`): any limit is tighter than none, and a smaller
/// limit is tighter than a larger one.
///
/// Implements planning/architecture.md section 5.10 (`sso_max_age_s`, the maximum expiries).
///
/// Caller: `policy_fields!` (lib.rs).
#[derive(Clone, Copy, Debug)]
pub struct SmallerLimitIsTighter;

impl<T: Ord + Clone> Tighter<Option<T>> for SmallerLimitIsTighter {
    fn at_least_as_tight(parent: &Option<T>, child: &Option<T>) -> bool {
        match (parent, child) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(parent), Some(child)) => child <= parent,
        }
    }

    fn tighter(parent: &Option<T>, child: &Option<T>) -> Option<T> {
        match (parent, child) {
            (None, child) => child.clone(),
            (Some(parent), None) => Some(parent.clone()),
            (Some(parent), Some(child)) => Some(if child < parent {
                child.clone()
            } else {
                parent.clone()
            }),
        }
    }
}

/// A restriction: `true` is tighter than `false` (the policy forces or requires something).
///
/// Implements planning/architecture.md section 5.10 (`sso_required`, the `link_policy` forces and
/// the password for `anyone`).
///
/// Caller: `policy_fields!` (lib.rs).
#[derive(Clone, Copy, Debug)]
pub struct OnIsTighter;

impl Tighter<bool> for OnIsTighter {
    fn at_least_as_tight(parent: &bool, child: &bool) -> bool {
        *child || !*parent
    }

    fn tighter(parent: &bool, child: &bool) -> bool {
        *parent || *child
    }
}

/// A permission: `false` is tighter than `true` (the policy allows something or not).
///
/// Implements planning/architecture.md section 5.10 (`passkey_recovery`, `browser_devices`, the
/// allow-flags of `link_policy`, `request_policy` allowed).
///
/// Caller: `policy_fields!` (lib.rs).
#[derive(Clone, Copy, Debug)]
pub struct OffIsTighter;

impl Tighter<bool> for OffIsTighter {
    fn at_least_as_tight(parent: &bool, child: &bool) -> bool {
        !*child || *parent
    }

    fn tighter(parent: &bool, child: &bool) -> bool {
        *parent && *child
    }
}

/// A value with a closed order of its own: the higher [`Rank`] is tighter.
///
/// Implements planning/architecture.md section 5.10 (`device_approval`, `audit_level`,
/// `external_sharing`).
///
/// Caller: `policy_fields!` (lib.rs).
#[derive(Clone, Copy, Debug)]
pub struct HigherRankIsTighter;

impl<T: Rank + Clone> Tighter<T> for HigherRankIsTighter {
    fn at_least_as_tight(parent: &T, child: &T) -> bool {
        child.tightness() >= parent.tightness()
    }

    fn tighter(parent: &T, child: &T) -> T {
        if child.tightness() > parent.tightness() {
            child.clone()
        } else {
            parent.clone()
        }
    }
}

/// A set of allowed things: a subset is tighter, and the tighter of two sets is their intersection.
///
/// Implements planning/architecture.md section 5.10 (`link_policy`: "the audiences allowed").
///
/// Caller: `policy_fields!` (lib.rs).
#[derive(Clone, Copy, Debug)]
pub struct SubsetIsTighter;

impl Tighter<AudienceSet> for SubsetIsTighter {
    fn at_least_as_tight(parent: &AudienceSet, child: &AudienceSet) -> bool {
        child.is_subset_of(*parent)
    }

    fn tighter(parent: &AudienceSet, child: &AudienceSet) -> AudienceSet {
        parent.intersection(*child)
    }
}

/// A field of the account only: a group may not change it, so the only child value that is tight
/// enough is the parent's own, and the clamp gives the parent's value.
///
/// Implements planning/architecture.md section 5.10 (`managed_default`, `updates`; section 5.15
/// `padding`: account-level settings).
///
/// Caller: `policy_fields!` (lib.rs).
#[derive(Clone, Copy, Debug)]
pub struct SameAsParent;

impl<T: PartialEq + Clone> Tighter<T> for SameAsParent {
    fn at_least_as_tight(parent: &T, child: &T) -> bool {
        parent == child
    }

    fn tighter(parent: &T, _child: &T) -> T {
        parent.clone()
    }
}

/// The tightness of a value of a type with a closed order of its own: a higher number is tighter. The
/// numbers are the order of the plan's words, loosest first.
///
/// Implements planning/architecture.md section 5.10 (`device_approval`, `audit_level`,
/// `external_sharing`).
///
/// Caller: [`HigherRankIsTighter`].
pub trait Rank {
    /// The tightness of this value: 0 is the loosest.
    fn tightness(&self) -> u8;
}

impl Rank for DeviceApproval {
    fn tightness(&self) -> u8 {
        match self {
            DeviceApproval::SelfOrQuorum => 0,
            DeviceApproval::SelfOnly => 1,
        }
    }
}

impl Rank for AuditLevel {
    fn tightness(&self) -> u8 {
        match self {
            AuditLevel::None => 0,
            AuditLevel::Admin => 1,
            AuditLevel::AccessSummary => 2,
            AuditLevel::AccessFull => 3,
        }
    }
}

impl Rank for ExternalSharing {
    fn tightness(&self) -> u8 {
        match self {
            ExternalSharing::Allowed => 0,
            ExternalSharing::SameWorkspaceOnly => 1,
        }
    }
}

/// The text of a value in a refusal and in a log: a duration as `43200 s`, a size as `1024 B`, a flag
/// as `on` or `off`, a missing limit as `none`, a word as its word.
trait Show {
    fn show(&self) -> String;
}

impl Show for Span {
    fn show(&self) -> String {
        format!("{} s", self.as_secs())
    }
}

impl Show for ByteCount {
    fn show(&self) -> String {
        self.to_string()
    }
}

impl Show for u8 {
    fn show(&self) -> String {
        self.to_string()
    }
}

impl Show for u32 {
    fn show(&self) -> String {
        self.to_string()
    }
}

impl Show for bool {
    fn show(&self) -> String {
        String::from(if *self { "on" } else { "off" })
    }
}

impl<T: Show> Show for Option<T> {
    fn show(&self) -> String {
        match self {
            None => String::from("none"),
            Some(value) => value.show(),
        }
    }
}

impl Show for AdminQuorum {
    fn show(&self) -> String {
        self.get().to_string()
    }
}

impl Show for AudienceSet {
    fn show(&self) -> String {
        self.to_string()
    }
}

/// `show` of a type that has strict words: the word.
macro_rules! show_word {
    ($($name:ident),+) => {
        $(
            impl Show for $name {
                fn show(&self) -> String {
                    String::from(self.as_str())
                }
            }
        )+
    };
}

show_word!(
    DeviceApproval,
    ManagedDefault,
    AuditLevel,
    ExternalSharing,
    UpdatePolicy,
    PaddingPolicy
);

/// One field of a child policy that is not at least as tight as the parent's: the field and the
/// two values as text (`43200 s`, `on`, `anyone,members`).
///
/// Implements planning/architecture.md section 5.10 ("groups inherit and may only tighten").
///
/// Caller: `check_tightens`; Seam: S17c1 (`Refusal::PolicyForbids` of a `policy_set` entry).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{field}: the child's {child} is not at least as tight as the parent's {parent}")]
pub struct Loosened {
    /// The field.
    pub field: Field,
    /// The parent's value, as text.
    pub parent: String,
    /// The child's value, as text.
    pub child: String,
}

/// Whether `child` is at least as tight as `parent` in every field; if not, every field that is
/// not, in the order of [`Field::ALL`], with the two values. This is the check at `policy_set` time
/// for a group: the parent is what the group inherits (`effective` down to the parent) and the
/// child is that policy with the group's patch applied (`apply`). An equal child is accepted; a
/// field of the account only (`managed_default`, `updates`, `padding`) is accepted only when equal.
///
/// Implements planning/architecture.md section 5.10 ("groups inherit and may only tighten") and
/// section 5.9 (`policy_set`).
///
/// Seam: S17c1 (`azlin-teamlog` refuses a group's `policy_set` that loosens, with the list).
pub fn check_tightens(parent: &Policy, child: &Policy) -> Result<(), Vec<Loosened>> {
    let mut loosened = Vec::new();
    macro_rules! check {
        ($(($variant:ident, $name:literal, $($path:ident).+, $rule:ident)),+ $(,)?) => {
            $(
                if !<$rule as Tighter<_>>::at_least_as_tight(&parent.$($path).+, &child.$($path).+) {
                    loosened.push(Loosened {
                        field: Field::$variant,
                        parent: parent.$($path).+.show(),
                        child: child.$($path).+.show(),
                    });
                }
            )+
        };
    }
    policy_fields!(check);
    if loosened.is_empty() {
        Ok(())
    } else {
        Err(loosened)
    }
}

/// The tighter of `parent` and `child` in every field: the policy a group really has when its own
/// policy is `child` and what it inherits is `parent`. Where the child is tighter the child's value
/// stands, where it is looser the parent's value does, and a field of the account only is always
/// the parent's.
///
/// Implements planning/architecture.md section 5.10 ("groups inherit and may only tighten").
///
/// Caller: `effective`.
pub fn clamp(parent: &Policy, child: &Policy) -> Policy {
    // Starts from the parent: a field that no row of the table covers could not become looser.
    let mut policy = parent.clone();
    macro_rules! tighten {
        ($(($variant:ident, $name:literal, $($path:ident).+, $rule:ident)),+ $(,)?) => {
            $(
                policy.$($path).+ = <$rule as Tighter<_>>::tighter(&parent.$($path).+, &child.$($path).+);
            )+
        };
    }
    policy_fields!(tighten);
    policy
}
