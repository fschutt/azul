//! Azlin spaces: the 5.4 model of spaces that AzDrive, the apps and sharing build on - the short signed
//! membership chain of each space, the grants it produces, and the rule that an epoch of a space key
//! counts only if a signed chain entry from an authorized device commits to its key id
//! (planning/architecture.md section 5.4 "Spaces", section 5.5 "Keys", section 5.9 "The team log and the
//! space chains", section 5.10 `external_sharing`, section 5.12 "External members", section 5.15 M6
//! "capability grants", section 6.10 S14 "Quota exhaustion through shares"; build unit S16, lane
//! W1-S16b).
//!
//! The node (S9) applies chain entries with it; devices verify chains with it; the key code (S15) asks
//! `epoch_authorized`. This is PERMISSION AND KEY-EPOCH code: who has which role in a space, what a
//! removal rotates, who may charge storage.
//!
//! | what | module | the plan |
//! |---|---|---|
//! | `SpaceKind`, `HistoryMode` | [`kind`] | 5.4 |
//! | `SpaceRole`, `Caps`, `role_caps`, `union`: THE ONLY role table (R14) | [`role`] | 5.4, 7.2 |
//! | `Grantee`, `Grant`, `Payer`, `Source`, `capability_hash` | [`grant`] | 5.4, 5.12, 5.15 M6 |
//! | `SpaceOp`, `RekeyReason`: the five ops of the chain | [`ops`] | 5.9 |
//! | `SpaceState`, `EntryRecord`, `SpaceRow`, the limits | [`state`] | 5.4, 5.9 |
//! | `SpaceEntry`, `SpaceProposal`, `verify_entry`: the entry and its signatures | [`proposal`] | 5.9, 2.5 |
//! | `admit`, `SignerFacts`, `AccountView`, `SpaceRefusal`: the permission check | [`admit`](mod@admit) | 5.9, 5.10, 12 E1 |
//! | `apply`, `Admitted`, `SpaceEffects`, `replay`: the deterministic apply | [`apply`](mod@apply) | 5.9, 5.5, 5.6 |
//! | `epoch_authorized`, `SpaceChains`, `chain_visible_to`: authorized key epochs | [`epochs`] | 5.5, 5.4 |
//! | `access`, `Principal`: what a node checks per connection | [`access`](mod@access) | 2.4, 5.15 M6 |
//! | `charge`, `Charge`: whose quota a write counts against | [`charge`](mod@charge) | 5.4, 6.10 S14 |
//!
//! THE FLOW of one entry, for the node and for every device: `verify_entry` checks the signatures and
//! makes a `SpaceProposal`; `admit` is the permission check (pure: the state, the proposal, the facts about
//! the signer, the account's view and the leader's stamp); `apply` turns the admitted entry,
//! deterministically, into the new state and the [`SpaceEffects`] (registry rows, the wraps that must
//! exist, notices, obligations). `replay` is a device's whole check of a chain. Nothing here does I/O,
//! reads a clock or draws entropy: the stamp, the verifier, the directory and the account's view are
//! parameters. Every collection is ordered, so two replicas that apply the same entries hold equal states
//! and equal `state_hash`es.
//!
//! WHO MAY SIGN WHAT is the table at the top of [`admit`](mod@admit) (the brief's v0, drift row X-DELTA-W1S16B-1):
//! only managers sign grants, rekeys and history switches; an account owner or admin may revoke, and may
//! add a manager to an ORPHANED space only with the admin quorum (never itself alone: the next forward
//! rotation of the key agent would wrap the key to it); the grantee may leave; nobody removes the last
//! manager; an app principal (the key agent) signs a rekey and its own leaving, nothing else.
//!
//! THE EPOCH RULE (section 5.5): `epoch_authorized(state, epoch, key_id)` is true only for a key id that a
//! signed `space_create` or `space_rekey` entry committed for that epoch. A removed member who knows epoch
//! n and plants an epoch n+1 they also know gets `false`: no entry committed it. `SpaceChains` offers the
//! same facts to the key code through `azlin_keys::SpaceChainView`, keeping an unreadable chain (an error)
//! apart from "no commitment" and from "not authorized".
//!
//! Mutation guards (`azlin_seams::mutant!`, the feature `azlin-seams/mutants`) sit at each decision that a
//! privilege escalation or an epoch plant would turn off ([`MUTANT_IDS`]); the mutant hold of this crate
//! turns each one off and watches its named test fail (the table is in `tests/hygiene.rs`).
//!
//! DRIFT (the register is `delta/W1S16B.tsv`, ids `X-DELTA-W1S16B-<n>`): who may grant (-1), the creation
//! rules (-2, -5), the ids the brief typed as raw numbers (-3), the additions to the facts and the account
//! view (-4), uploader and union (-8), several write grants (-9), history joins (-10), apps (-11), group
//! managers (-12), hash domains (-13), the unwiped hasher (-14).
//!
//! ```
//! use azlin_seams::Secret;
//! use azlin_spaces::{capability_hash, role_caps, union, Caps, SpaceRole};
//!
//! // A viewer and an uploader together read and write; an uploader alone cannot read.
//! assert!(!role_caps(SpaceRole::Uploader).can_read());
//! let both = union(role_caps(SpaceRole::Viewer), role_caps(SpaceRole::Uploader));
//! assert!(both.can_read() && both.can_write() && !both.can_manage());
//! assert!(role_caps(SpaceRole::Manager).contains(Caps::MANAGE));
//!
//! // A capability is stored as the hash of its secret, never the secret.
//! let secret = Secret::<32>::from_bytes([7; 32]);
//! assert_eq!(capability_hash(&secret), capability_hash(&secret));
//! ```

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

// A `usize` fits a `u64` on every target this engine builds for: the widening casts of the counts rest
// on it, so a target where it fails does not compile (the same assertion as azlin-seams).
const _: () = assert!(usize::BITS <= u64::BITS);

/// The `ALL`, `as_str`, `Display` and strict `FromStr` of an enum of words, from one table of variant and
/// word: a variant missing from (or extra in) the table does not compile. The words are lower case with
/// underscores; `FromStr` accepts exactly them and nothing else, and an unknown word is an error, never a
/// default.
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

pub mod access;
pub mod admit;
pub mod apply;
pub mod charge;
pub mod epochs;
pub mod grant;
pub mod kind;
pub mod ops;
pub mod proposal;
pub mod role;
pub mod state;

pub use access::{access, Principal};
pub use admit::{admit, AccountView, SignerFacts, SpaceRefusal};
pub use apply::{
    apply, replay, Admitted, ChainWrapNeed, Obligation, ReplayRefusal, ReplayStep, SpaceDelta,
    SpaceEffects, SpaceNotice, StopServing, WrapNeed,
};
pub use charge::{charge, Charge, ChargeRefusal};
pub use epochs::{chain_visible_to, epoch_authorized, EpochChainView, EpochRecord, SpaceChains};
pub use grant::{
    capability_hash, Grant, GrantState, Grantee, Payer, ShareId, Source, CAPABILITY_HASH_DOMAIN,
};
pub use kind::{HistoryMode, SpaceKind};
pub use ops::{RekeyReason, SpaceOp, SpaceOpName};
pub use proposal::{
    entry_hash, verify_entry, Cosig, SpaceEntry, SpaceProposal, COSIGS_MAX, ENTRY_HASH_DOMAIN,
    PREIMAGE_MAX, SIG_MAX_LEN, SPACE_CHAIN_DOMAIN,
};
pub use role::{role_caps, union, Caps, SpaceRole};
pub use state::{
    Actor, Authority, EntryRecord, SpaceRow, SpaceState, MAX_CAPABILITIES, MAX_ENTRIES, MAX_GRANTS,
    STATE_HASH_DOMAIN,
};

/// The ids of the mutation guards of this crate (`azlin_seams::mutant!`), each at one decision that a
/// privilege escalation, a plant of a key epoch or a quota bypass would turn off. The hold that proves
/// the tests can fail runs `AZLIN_MUTANT=<id> cargo test -p azlin-spaces --features azlin-seams/mutants`
/// once for each id; `tests/hygiene.rs` keeps this list, the guards in `src/` and the table of killers
/// (id to the test that must fail) one set.
///
/// Implements planning/architecture.md section 12 (E1) and section 1.4 rule 1 (the guard reads its switch
/// as a parameter of the process).
///
/// Seam: the mutant hold of lane W1-S16b, S47a (the escalation lane attacks the same rules).
pub const MUTANT_IDS: [&str; 70] = [
    "spaces_access_archived_writable",
    "spaces_access_external_ignores_account",
    "spaces_access_groups_ignored",
    "spaces_access_ignores_revoked",
    "spaces_account_view_unchecked",
    "spaces_app_any_op",
    "spaces_archived_unchecked",
    "spaces_audit_space_unchecked",
    "spaces_capability_any_secret_accepted",
    "spaces_capability_manager_allowed",
    "spaces_capability_paid_by_grantee",
    "spaces_chain_unchecked",
    "spaces_charge_archived_unchecked",
    "spaces_charge_cap_off_by_one",
    "spaces_charge_cap_unchecked",
    "spaces_charge_payer_swapped",
    "spaces_charge_picks_least_room",
    "spaces_cosig_unchecked",
    "spaces_cosigner_role_unchecked",
    "spaces_creation_role_unchecked",
    "spaces_device_active_unchecked",
    "spaces_epoch_any_key",
    "spaces_epoch_current_only",
    "spaces_epoch_planted_authorized",
    "spaces_external_confused_with_member",
    "spaces_external_internal_unchecked",
    "spaces_external_policy_unchecked",
    "spaces_facts_device_unchecked",
    "spaces_first_manager_unchecked",
    "spaces_forward_grant_no_rekey_due",
    "spaces_forward_style_grant_in_full",
    "spaces_grant_needs_no_manage",
    "spaces_grant_wrap_missing",
    "spaces_grantee_member_unchecked",
    "spaces_grants_limit_unchecked",
    "spaces_group_unchecked",
    "spaces_hash_unchecked",
    "spaces_history_needs_no_manage",
    "spaces_last_manager_check_off",
    "spaces_leave_any_grantee",
    "spaces_member_active_unchecked",
    "spaces_override_any_role",
    "spaces_override_for_external",
    "spaces_override_not_orphaned",
    "spaces_override_quorum_skipped",
    "spaces_private_any_account",
    "spaces_rekey_admin_override",
    "spaces_rekey_chain_in_forward",
    "spaces_rekey_epoch_unchecked",
    "spaces_rekey_keeps_due",
    "spaces_rekey_key_reuse_allowed",
    "spaces_rekey_needs_no_manage",
    "spaces_rekey_wraps_revoked",
    "spaces_revoke_needs_no_authority",
    "spaces_revoke_no_rekey_due",
    "spaces_revoke_no_stop_serving",
    "spaces_root_anywhere",
    "spaces_second_create_allowed",
    "spaces_signature_unchecked",
    "spaces_signer_window_off_by_one",
    "spaces_signer_window_open_ended",
    "spaces_standing_ignores_groups",
    "spaces_standing_uses_revoked",
    "spaces_union_is_intersection",
    "spaces_uploader_reads",
    "spaces_view_future_position_answered",
    "spaces_view_unreadable_as_none",
    "spaces_view_unreadable_as_unauthorized",
    "spaces_viewer_writes",
    "spaces_visible_uses_current_mode",
];
