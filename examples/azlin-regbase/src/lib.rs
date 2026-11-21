//! Azlin registry vocabulary: the ids, times, bounded value types, shared enums, trust anchors,
//! signing rule and key proofs that the registry (azlin-registry, in azul-apps) and its clients
//! (azlin-creds, azlin-session, the services) share (planning/architecture.md section 5.3
//! "Registry: accounts, members, devices", section 2.4 "The metadata group and the session
//! plane"; build unit S9, lane W1-S9a).
//!
//! It is the vocabulary only: no I/O, no clock, no randomness of its own (an entropy source is a
//! parameter), no SQL, no Raft. It builds for `wasm32-unknown-unknown` and stays small, because the
//! services link it through azlin-creds.
//!
//! | module | what |
//! |---|---|
//! | [`ids`] | `FamilyId`, `CredentialNonce`, `TokenHash`, `VoterId`, `MetaGroupId`, `MetaGroupEpoch`, `LogIndex`, `KeyEpoch`, `Generation`; the ids of azlin-keys re-exported |
//! | [`time`] | `LeaderTime` (the leader's stamp), `LeaderClock`, `DayStart` ("Times are day starts", D37) |
//! | [`bytes`] | `SealedBlob`, `SignedBytes`, `SmallText`, `Features`, `RegionSet`, `UsedHashes`, `ClientVersion`: bounded, refusing, never truncating |
//! | [`enums`] | `AccountKind`, `AccountRole`, `MemberKind`, `PolicyEpoch` (from azlin-seams), `EntitlementState`, `RefusalWord` |
//! | [`anchors`] | `KeyWindow`, `AnchorSet`: the keys a verifier trusts, replicated state |
//! | [`sign`] | `signing_input` (azlin-keys' rule, re-exported), `PublicKey32`, `Signature64`, the three proof domains |
//! | [`proof`] | `Challenge`, `KeyProof`, `ProofPurpose`, `proof_message`, `verify_key_proof`: "device key or token" |
//! | [`error`] | the typed refusals |
//!
//! One vocabulary (COMMON-W1R R3): the ids of accounts, members, devices, spaces and user groups
//! are azlin-keys', the four shared enums and `PolicyEpoch` are azlin-seams', `PublicKey32` and
//! `Signature64` are aliases of azlin-keys' key and signature, times and spans are azlin-keys'
//! `UnixTime` and `Span`, sizes are azlin-seams' `ByteCount`. The status codes of the protocol are
//! azlin-contract's; the registry crate imports them.

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

pub mod anchors;
pub mod bytes;
pub mod enums;
pub mod error;
pub mod ids;
pub mod proof;
pub mod sign;
pub mod time;

pub use anchors::{AnchorSet, KeyWindow, ANCHOR_WINDOWS_MAX};
pub use bytes::{
    ClientVersion, Features, RegionId, RegionSet, SealedBlob, SignedBytes, SmallText, UsedHashes,
    FEATURES_MAX, REGIONS_MAX, SEALED_BLOB_MAX, SIGNED_BYTES_MAX, SMALL_TEXT_MAX, USED_HASHES_KEPT,
};
pub use enums::{
    AccountKind, AccountRole, EntitlementState, MemberKind, PolicyEpoch, RefusalWord, UnknownWord,
};
pub use error::{BoundError, IdParseError, ProofError, VersionError};
pub use ids::{
    AccountId, CredentialNonce, DeviceId, FamilyId, Generation, GroupId, KeyEpoch, LogIndex,
    MemberId, MetaGroupEpoch, MetaGroupId, SpaceId, TokenHash, UserGroupId, VoterId,
};
pub use proof::{
    proof_message, sign_key_proof, verify_key_proof, Challenge, KeyProof, ProofPurpose,
};
pub use sign::{signing_input, Domain, PublicKey32, Signature64};
pub use time::{DayStart, LeaderClock, LeaderTime};
