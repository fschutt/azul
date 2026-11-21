//! Azlin team log: the account's authority as a signed, hash-chained log, its permission check and
//! its deterministic apply (planning/architecture.md section 5.9 "The team log and the space chains",
//! section 7.2 "The model: RBAC on a group hierarchy", section 7.3, section 7.5, section 12 E1; build
//! unit S17, lanes W1-S17c1 and W1-S17c2).
//!
//! The metadata group (S9) and every device (which "runs its own check", section 12 E1) call the same
//! three functions, in this order, for every entry:
//!
//! 1. `verify_signatures(state, entry, verifier, trusted, decoder)` makes a `Proposal`: every signature
//!    of the entry (the signer's and the cosigners') holds over the entry's preimage, the copy is
//!    intact, and the typed fields `admit` reads are the fields of the signed bytes (the `EntryDecoder`
//!    seam decodes the preimage: an old signed entry cannot be given the position of a new one).
//! 2. `admit(state, proposal, now)` is THE PERMISSION CHECK: whether the entry may enter the account's
//!    log, as the authority table says (roles, scopes, group inheritance, delegations, quorum of
//!    distinct people). It is pure: it never changes the state. `now` is the leader's stamp
//!    (section 2.4), never a clock of the device.
//! 3. `apply(state, admitted)` turns an admitted entry, deterministically, into the registry's rows
//!    ([`Effects::registry`]) and the notices of section 5.9 ([`Effects::notices`]); `Admitted` is a
//!    witness only `admit` can make. `replay` is a device's whole check of a run of entries.
//!
//! | what | module | the plan |
//! |---|---|---|
//! | the ops with their arguments, the wire words, the app name | [`ops`] | 5.9 |
//! | roles and the order of privilege | [`roles`] | 7.2 |
//! | members: state, invitation, app principal | [`members`] | 5.3, 5.11 |
//! | devices: keys, state, the enrollment hold | [`devices`] | 5.3, 7.5 |
//! | the group tree and the subtree queries a scope is made of | [`groups`] | 7.2 |
//! | `TeamState` and its plain `StateView`; the limits | [`state`] | 5.9, 5.3 |
//! | the canonical `state_hash` and the entry hash | [`hash`] | 5.9 |
//! | `Effects`, `RegistryDelta`, `Notice` | [`effects`] | 5.9, 5.3 |
//! | `apply` and the `Admitted` witness | [`apply`](mod@apply) | 5.9 |
//! | the entry as it arrives, `Proposal`, `TrustedKeys` | [`proposal`] | 5.9 |
//! | the seam that binds the typed entry to its signed bytes: `EntryDecoder`, `FakeDecoder` | [`decode`] | 5.9, 2.5 |
//! | `verify_signatures` | [`verify`] | 5.9 |
//! | the authority table v0 as data | [`authority`] | 5.9, 7.2, 7.3, 7.5 |
//! | `admit` and `Refusal` | [`admit`](mod@admit) | 5.9, 12 E1 |
//! | delegations (the identity service) | [`delegate`] | 5.9, 7.3 |
//! | invitations | [`invite`] | 5.11 |
//! | sealed fields | [`sealed`] | 5.9, 5.17 |
//! | `replay` | [`replay`](mod@replay) | 5.9, 12 E1 |
//!
//! Nothing here does I/O, reads a clock or draws entropy by itself: the stamp, the verifier and the
//! entropy are parameters. Every collection is ordered, so two replicas that apply the same entries hold
//! equal states and equal state hashes. The crate depends on the azul crates `azlin-seams`,
//! `azlin-chain` and `azlin-policy` only (and BLAKE3, XChaCha20-Poly1305, `zeroize`, `thiserror`);
//! it builds for `wasm32` without a change, because the clients check the log too.
//!
//! The authority table is the owner's to ratify (ARCH29 9.13, X-D-54): until then lanes S17c and S47a
//! implement table v0 and every place where the plan is silent is a row of the drift register
//! (`notes/audits/closeout-2026-11/delta/W1S17C.tsv`).

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

pub mod apply;
pub mod devices;
pub mod effects;
pub mod groups;
pub mod hash;
pub mod members;
pub mod ops;
pub mod roles;
pub mod state;

// The check (lane W1-S17c2): declared here with the rest, so that no file is an orphan.
pub mod admit;
pub mod authority;
pub mod decode;
pub mod delegate;
pub mod invite;
pub mod proposal;
pub mod replay;
pub mod sealed;
pub mod verify;

pub use admit::{admit, Invalid, PolicyReason, Refusal, MUTANT_IDS};
pub use apply::{apply, Admitted};
pub use decode::{canonical_of, DecodedEntry, EntryDecoder, FakeDecoder, NotAnEntry};
pub use delegate::{delegable_ops, is_delegable};
pub use devices::{Device, DeviceState, EnrolledBy, PublicKeys};
pub use effects::{
    subject_of, Audience, Effects, Notice, RegistryDelta, RevocationReason, RevocationSubject,
    Subject,
};
pub use groups::{Group, Groups};
pub use hash::{entry_hash, ENTRY_HASH_DOMAIN, STATE_HASH_DOMAIN};
pub use invite::{invite_hash, INVITE_HASH_DOMAIN};
pub use members::{AppSpec, Invite, Member, MemberState};
pub use ops::{
    AppName, BadAppName, EntitlementState, EntitlementVersion, EscrowReason, InviteSecret, Op,
    OpName, PolicyScope, UnknownOp, APP_BODY_MAX, APP_NAME_MAX_LEN, SEALED_MAX,
};
pub use proposal::{
    AssertionVerdict, Proposal, RawEntry, Signer, TrustedKeys, ASSERTIONS_MAX, COSIGS_MAX,
    PREIMAGE_MAX, SIG_MAX_LEN, TEAM_LOG_DOMAIN,
};
pub use replay::replay;
pub use roles::{MemberKind, Role};
pub use sealed::{open_field, seal_field, SealError, SealKey};
pub use state::{
    Delegation, EntitlementFacts, PendingOpen, StateView, TeamState, MAX_DELEGATIONS, MAX_DEVICES,
    MAX_ESCROW_HOLDERS, MAX_GROUPS, MAX_GROUPS_PER_MEMBER, MAX_GROUP_DEPTH, MAX_MEMBERS,
    MAX_PENDING_ESCROW_OPENS,
};
pub use verify::verify_signatures;

#[cfg(test)]
mod tests {
    mod kit;
    mod signed;

    mod apply_tests;
    mod hash_tests;
    mod ops_tests;
    mod state_tests;

    mod admit_tests;
    mod delegate_tests;
    mod determinism_tests;
    mod escalation_tests;
    mod invite_tests;
    mod matrix_tests;
    mod replay_tests;
    mod sealed_tests;
}
