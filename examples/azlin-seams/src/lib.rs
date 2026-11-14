//! Azlin seams: the vocabulary every lane of the engine builds on (planning/architecture.md
//! section 1.4 rule 1 "I/O is a parameter", section 5.9 "The team log and the space chains",
//! section 2.4 "time is the leader's stamp"; build unit S17, lane W1-S17a).
//!
//! The crate re-exports what `azlin-keys` owns (the typed ids, `Secret`, `Entropy`, `Clock`,
//! `UnixTime`, `Span`, `DeviceDirectory`, the signing rule) so that a lane names one crate, and adds
//! what the lanes of planner P4 need and `azlin-keys` lacks. No lane invents a second clock, random
//! source, size or hash type.
//!
//! | item | what | module |
//! |---|---|---|
//! | `Stamp` | the metadata group's leader stamp: an alias of `UnixTime` (section 2.4) | this file |
//! | `Hash32`, `hash32` | a 32-byte BLAKE3 value, hashed under a domain string; strict hex | [`hash`] |
//! | `ByteCount` | the one size type: checked arithmetic, `kib`/`mib`/`gib` | [`bytes`] |
//! | `SignatureVerifier`, `SigAlg`, `VerifyError`, `Ed25519Verifier`, `FakeScheme` | the verifier seam over ALREADY FRAMED preimages | [`verify`] |
//! | `AccountKind`, `AccountRole`, `MemberKind`, `PolicyEpoch`, `RestoreEpoch` | the shared enums and counters of the registry, policy, team log, spaces and restores | [`vocab`] |
//! | `mutant!` | the mutation guard: false unless the feature `mutants` is on AND `AZLIN_MUTANT` names the id | [`mutants`] |
//! | `sim::Rng`, `sim::minimize` | the seeded generator and the shrinker of every simulation test | [`sim`] |
//!
//! THE ONE SIGNING RULE (ARCH29 9.13 decision 2): the preimage of every signature is
//! `domain || 0x00 || bytes`, built once by `azlin_keys::sign::signing_input` (re-exported here as
//! [`signing_input`]). [`SignatureVerifier::verify`] takes that preimage as it is: the caller frames,
//! the verifier never does, so there is no double framing.
//!
//! Features: `os-rng` (default off) forwards to `azlin-keys` and re-exports `OsEntropy`; `mutants`
//! turns the guard of [`mutant!`] on (the holds of every crate that carries guards run
//! `cargo test -p <crate> --features azlin-seams/mutants`, COMMON-W1R R19). The test doubles of
//! `azlin-keys` (`SeededEntropy`, `ManualClock`, `MapDirectory`) sit behind that crate's feature
//! `test-support`, which this crate's final manifest does not forward: DRIFT X-DELTA-W1S17A-1.
//!
//! No unsafe code, no clock, file, network or thread: a pure library whose I/O is a parameter.
//!
//! ```
//! use azlin_seams::{hash32, ByteCount, Stamp, UnixTime};
//!
//! let digest = hash32("example-domain-v1", &[b"a", b"bc"]);
//! assert_eq!(digest.to_string().len(), 64);
//! assert_ne!(digest, hash32("example-domain-v1", &[b"ab", b"c"])); // the parts are length-framed
//! assert_eq!(ByteCount::kib(4).get(), 4096);
//! let stamp: Stamp = UnixTime::from_secs(5);
//! assert_eq!(stamp.as_secs(), 5);
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

// A `usize` fits a `u64` on every target this engine builds for (16, 32 and 64 bits): the widening
// casts of `bytes` and `hash` rest on it, so a target where it fails does not compile.
const _: () = assert!(usize::BITS <= u64::BITS);

// The macro of `mutants` must be defined before the modules that use it.
#[macro_use]
pub mod mutants;

pub mod bytes;
pub mod hash;
pub mod sim;
pub mod verify;
pub mod vocab;

pub use azlin_keys::sign::signing_input;
#[cfg(feature = "os-rng")]
pub use azlin_keys::OsEntropy;
pub use azlin_keys::{
    AccountId, ChainSeq, Clock, ClockError, DeviceDirectory, DeviceId, Domain, Entropy,
    EntropyError, Epoch, GroupId, IdError, KeyId, MemberId, Secret, SignError, SignKeypair,
    Signature, SpaceId, Span, SystemClockSeam, UnixTime, VerifyAt, VerifyKey,
};

pub use bytes::ByteCount;
pub use hash::{hash32, Hash32};
pub use verify::{Ed25519Verifier, FakeScheme, SigAlg, SignatureVerifier, VerifyError};
pub use vocab::{AccountKind, AccountRole, MemberKind, PolicyEpoch, RestoreEpoch, UnknownWord};

/// The metadata group's leader stamp: the time the leader writes into an entry (planning/architecture.md
/// section 2.4 "Time is the leader's stamp, checked"). It is the same type as `UnixTime` (whole
/// seconds since 1970); the name says which clock a value came from. Every state transition of the
/// engine's pure crates takes a `Stamp` as a parameter and reads no clock of its own.
///
/// Caller: every lane that stamps, expires or compares (S17b policy, S17c team log, S16a heads and
/// logs, S16b space chains, S9a registry rows).
pub type Stamp = UnixTime;
