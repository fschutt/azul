//! Azlin policy: the account policy of planning/architecture.md section 5.10 as typed data and the
//! pure functions over it (build unit S17, lane W1-S17b): the defaults for each kind of account,
//! "groups inherit and may only tighten" and the quorum that counts distinct people.
//!
//! The team log's permission check (lane S17c), the links (S16c), the requests (S16f), the
//! enrollment holds (S17e) and the spaces (S16b) read the policy through this crate and nowhere
//! else. No function of it reads a clock, the environment, a file or a network: the policy never
//! reads time, and every input is a parameter.
//!
//! | item | what | module |
//! |---|---|---|
//! | `AccountKind`, `MemberKind`, `PolicyEpoch` | the shared enums of `azlin-seams`, re-exported | [`kind`] |
//! | `Policy`, `Field`, the value types, `AdminQuorum`, `RequestPolicy` | every row of the table of 5.10, typed | [`fields`] |
//! | `Policy::defaults` | the table's column for a kind | [`defaults`] |
//! | `PolicyPatch`, `apply` | what `policy_set` carries, and putting it on a policy | [`patch`] |
//! | `Tighter`, `check_tightens`, `clamp`, `Loosened` | "groups may only tighten": the order of each field | [`tighten`] |
//! | `effective`, `next_epoch` | the policy of a group down a tree; the epoch of a change | [`effective`] |
//! | `QuorumClass`, `required_people`, `Signatory`, `distinct_people`, `quorum_met` | the quorum counts distinct people | [`quorum`] |
//! | `LinkPolicy`, `AudienceSet`, `LINK_OPTION_NAMES` | the `link_policy` row and the link options | [`link_policy`] |
//!
//! ```
//! use azlin_policy::{quorum_met, required_people, AccountKind, Policy, QuorumClass, Signatory};
//! use azlin_seams::MemberId;
//!
//! // A business asks two different people for an admin change.
//! let business = Policy::defaults(AccountKind::Business);
//! assert_eq!(required_people(&business, QuorumClass::AdminChange), 2);
//!
//! // Five devices of one person are one person.
//! let alice = MemberId::from_bytes([1; 16]);
//! let five_devices = [Signatory::person(alice); 5];
//! assert!(quorum_met(&business, QuorumClass::AdminChange, &five_devices).is_err());
//!
//! // Two people are enough.
//! let bob = MemberId::from_bytes([2; 16]);
//! let two_people = [Signatory::person(alice), Signatory::person(bob)];
//! assert!(quorum_met(&business, QuorumClass::AdminChange, &two_people).is_ok());
//! ```
//!
//! Implements planning/architecture.md section 5.10 (the account policy), section 7.2 (policies sit
//! on the account or a group), section 5.9 (the ops that need the policy's quorum) and section 12
//! (E1).

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
#![warn(missing_docs)]

/// The `ALL`, `as_str`, `Display` and strict `FromStr` of an enum of words, from one table of
/// variant and word: a variant missing from (or extra in) the table does not compile. The words are
/// lower case with underscores; `FromStr` accepts exactly them and nothing else, and an unknown word
/// is an error, never a default.
macro_rules! words {
    ($name:ident, $what:literal, [$($variant:ident => $word:literal),+ $(,)?]) => {
        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The word of this value: lower-case snake case, the one spelling `FromStr` accepts.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $word),+
                }
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        /// Strict: exactly one of the words, in lower case.
        impl ::core::str::FromStr for $name {
            type Err = ::azlin_seams::UnknownWord;

            fn from_str(text: &str) -> Result<$name, ::azlin_seams::UnknownWord> {
                match text {
                    $($word => Ok($name::$variant),)+
                    _ => Err(::azlin_seams::UnknownWord { vocabulary: $what }),
                }
            }
        }
    };
}

/// THE TABLE of the leaves of the policy. One row for each leaf: the variant of [`Field`], its name
/// (its path in `Policy`), its path in `Policy` (and so in `PolicyPatch`, whose structures have the
/// same shape), and the rule of `tighten.rs` that orders its values by tightness. The patch
/// (`apply`, `sets`), the tightening check, the clamp and `Field` itself are all written from this
/// table, so the three of them can never disagree about which leaves there are, and a leaf's order
/// of tightness is changed here and nowhere else. (A new leaf also needs its field in `Policy` and
/// `PolicyPatch`, its default and its text for a refusal: the compiler and the per-field tests ask
/// for each.) The callback macro gets the rows as `(Variant, "name", path.to.field, Rule)`.
///
/// If the owner answers the question of drift row X-DELTA-W1S17B-1 (the order of tightness) in
/// another way, the last column is all that changes.
macro_rules! policy_fields {
    ($callback:ident) => {
        $callback! {
            (CredentialTtl, "credential_ttl", credential_ttl, SmallerIsTighter),
            (DeviceApproval, "device_approval", device_approval, HigherRankIsTighter),
            (MaxDevicesPerMember, "max_devices_per_member", max_devices_per_member, SmallerIsTighter),
            (ManagedDefault, "managed_default", managed_default, SameAsParent),
            (SsoRequired, "sso_required", sso_required, OnIsTighter),
            (SsoMaxAge, "sso_max_age", sso_max_age, SmallerLimitIsTighter),
            (EnrollHold, "enroll_hold", enroll_hold, LargerIsTighter),
            (PasskeyRecovery, "passkey_recovery", passkey_recovery, OffIsTighter),
            (EscrowOpenWait, "escrow_open_wait", escrow_open_wait, LargerIsTighter),
            (AuditLevel, "audit_level", audit_level, HigherRankIsTighter),
            (AdminQuorum, "admin_quorum", admin_quorum, LargerIsTighter),
            (LinkAudiences, "link_policy.audiences", link_policy.audiences, SubsetIsTighter),
            (LinkMaxExpiry, "link_policy.max_expiry", link_policy.max_expiry, SmallerLimitIsTighter),
            (LinkPasswordForAnyone, "link_policy.password_for_anyone", link_policy.password_for_anyone, OnIsTighter),
            (LinkForcePreviewOnly, "link_policy.force_preview_only", link_policy.force_preview_only, OnIsTighter),
            (LinkForceWatermark, "link_policy.force_watermark", link_policy.force_watermark, OnIsTighter),
            (LinkForceAppOnly, "link_policy.force_app_only", link_policy.force_app_only, OnIsTighter),
            (LinkUploads, "link_policy.uploads", link_policy.uploads, OffIsTighter),
            (LinkEdit, "link_policy.edit", link_policy.edit, OffIsTighter),
            (LinkComments, "link_policy.comments", link_policy.comments, OffIsTighter),
            (LinkOpenLogs, "link_policy.open_logs", link_policy.open_logs, OffIsTighter),
            (LinkSlugs, "link_policy.slugs", link_policy.slugs, OffIsTighter),
            (RequestAllowed, "request_policy.allowed", request_policy.allowed, OffIsTighter),
            (RequestMaxExpiry, "request_policy.max_expiry", request_policy.max_expiry, SmallerLimitIsTighter),
            (RequestMaxFiles, "request_policy.max_files", request_policy.max_files, SmallerIsTighter),
            (RequestMaxBytes, "request_policy.max_bytes", request_policy.max_bytes, SmallerIsTighter),
            (BrowserDevices, "browser_devices", browser_devices, OffIsTighter),
            (ExternalSharing, "external_sharing", external_sharing, HigherRankIsTighter),
            (Updates, "updates", updates, SameAsParent),
            (Padding, "padding", padding, SameAsParent),
        }
    };
}

pub mod defaults;
pub mod effective;
pub mod fields;
pub mod kind;
pub mod link_policy;
pub mod patch;
pub mod quorum;
pub mod tighten;

pub use effective::{effective, next_epoch, EpochExhausted};
pub use fields::{
    AdminQuorum, AuditLevel, DeviceApproval, ExternalSharing, Field, ManagedDefault, PaddingPolicy,
    Policy, QuorumOutOfRange, RequestPolicy, UpdatePolicy, MAX_ADMIN_QUORUM,
};
pub use kind::{AccountKind, MemberKind, PolicyEpoch};
pub use link_policy::{AudienceClass, AudienceSet, LinkPolicy, LINK_OPTION_NAMES};
pub use patch::{apply, LinkPolicyPatch, PolicyPatch, RequestPolicyPatch};
pub use quorum::{
    distinct_people, quorum_met, required_people, QuorumClass, QuorumShort, Signatory,
};
pub use tighten::{check_tightens, clamp, Loosened, Rank, Tighter};

/// The ids of the mutation guards of this crate (`azlin_seams::mutant!`), each with the test that
/// must fail when the guard is on: the hold that proves the tests can fail runs
/// `AZLIN_MUTANT=<id> cargo test -p azlin-policy --features azlin-seams/mutants` once for each id.
/// A guard is listed here when it is in the source (`tests/hygiene.rs` reads both).
///
/// | id | what the guard turns off | killer |
/// |---|---|---|
/// | `policy_effective_skips_clamp` | the clamp of a child to its parent | `effective_policy_clamps_a_child_looser_than_a_later_tightened_parent` |
/// | `policy_epoch_always_bumps` | "bumps only if the effective policy changed" | `policy_epoch_bumps_only_when_the_effective_policy_changes` |
/// | `policy_quorum_needs_one` | the policy's `admin_quorum` | `quorum_two_for_business_admin_change` |
/// | `policy_quorum_counts_devices` | "distinct people, never one person's devices" | `quorum_counts_distinct_people_never_devices` |
/// | `policy_quorum_counts_apps` | "an app principal never counts" | `quorum_app_principal_never_counts` |
/// | `policy_quorum_off_by_one` | "at least" the needed number of people | `quorum_two_for_business_admin_change` |
///
/// Implements planning/architecture.md section 12 (E1) and section 1.4 rule 1 (the guard reads its
/// switch as a parameter of the process).
///
/// Seam: the mutant hold of lane W1-S17b, S47a (the escalation lane attacks the same rules).
pub const MUTANT_IDS: [&str; 6] = [
    "policy_effective_skips_clamp",
    "policy_epoch_always_bumps",
    "policy_quorum_needs_one",
    "policy_quorum_counts_devices",
    "policy_quorum_counts_apps",
    "policy_quorum_off_by_one",
];
