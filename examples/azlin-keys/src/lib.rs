//! Azlin keys: the typed ids, the secret type, the entropy and clock seams, the registry of key
//! derivation contexts, domain-separated signatures and the device-directory seam that every
//! lane of the engine builds on, and (in the modules the later lanes fill) the envelopes, the
//! key hierarchy, the recovery paths and the hardware binding
//! (planning/architecture.md section 5.5 "Keys", section 5.3, section 1.4 rule 1; build unit S15,
//! lanes W1-S15a to W1-S15e).
//!
//! This crate is the first lane's vocabulary plus one pre-declared stub per later module:
//!
//! | module | what | lane |
//! |---|---|---|
//! | [`ids`] | `AccountId`, `MemberId`, `DeviceId`, `SpaceId`, `GroupId`, `KeyId`, `Epoch`, `ChainSeq` | S15a |
//! | [`secret`] | `Secret<N>`: no copy, no print, wiped on drop | S15a |
//! | [`seam`] | `Entropy`, `Clock`, `UnixTime`, `Span`, `SpaceChainView`, `Commitment`; the test doubles | S15a |
//! | [`mod@derive`] | the registry of every BLAKE3 `derive_key` context, `derive`, `derive_public` | S15a |
//! | [`sign`] | `signing_input`, `sign`, `verify`, `Domain`, `DeviceDirectory`, `VerifyAt`, `MapDirectory` | S15a |
//! | [`error`] | the typed errors of the modules above | S15a |
//! | `envelope` | envelope v2: the wrap format, algorithms 1 and 2, the signer, the key-less shape and signature checks | S15b |
//! | `member`, `group`, `space` | the key hierarchy: member and device key bundles, group keys, space keys per epoch (`K_seal`, `K_chunk`), the epoch chain, the history modes | S15c1 |
//! | `epoch`, `reg_wrap`, `rekey`, `recovery`, `binding` | stubs: `//! owned by lane W1-S15x` | S15c2 to S15e |
//!
//! Features: `os-rng` (default: `OsEntropy` over `getrandom`), `pq` (ML-KEM-768, envelope
//! algorithm 2; NOT default until the owner has reviewed the crate: ARCH29 9.13 decision 3),
//! `binding` (the hardware binding, S15e) and `test-support` (`SeededEntropy`, `FailingEntropy`,
//! `ManualClock`, `MapDirectory`, `MapChainView`: test doubles, never in a production build).
//! Every dependent uses this crate with `default-features = false`.
//!
//! No unsafe code, no clock, file, network or thread, no `serde` (the formats are CBOR through
//! `azlin-cbor`): a pure library whose I/O is a parameter. It builds for
//! `wasm32-unknown-unknown` without default features.
//!
//! ```
//! use azlin_keys::sign::{self, POINTER_V2};
//! use azlin_keys::{Secret, SignKeypair};
//!
//! let key = SignKeypair::from_seed(&Secret::from_bytes([7u8; 32]));
//! let sig = sign::sign(&POINTER_V2, b"message", &key);
//! assert!(sign::verify(&POINTER_V2, b"message", &key.verify_key(), &sig).is_ok());
//! assert!(sign::verify(&sign::ENVELOPE_V2, b"message", &key.verify_key(), &sig).is_err());
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

pub mod derive;
pub mod error;
pub mod ids;
pub mod seam;
pub mod secret;
pub mod sign;

// The modules of the later lanes: each is a stub file that its lane replaces (R2: nobody edits
// this file after the skeleton). `binding` exists only under its feature.
pub mod envelope;
pub mod epoch;
pub mod group;
pub mod member;
pub mod recovery;
pub mod reg_wrap;
pub mod rekey;
pub mod space;

#[cfg(feature = "binding")]
pub mod binding;

pub use error::{
    ChainError, ClockError, DirectoryError, EntropyError, IdError, KeysError, SignError,
};
pub use ids::{AccountId, ChainSeq, DeviceId, Epoch, GroupId, KeyId, MemberId, SpaceId};
pub use seam::{
    Clock, CommitKind, Commitment, Entropy, SpaceChainView, Span, SystemClockSeam, UnixTime,
};
pub use secret::Secret;
pub use sign::{DeviceDirectory, Domain, SignKeypair, Signature, VerifyAt, VerifyKey};

#[cfg(feature = "os-rng")]
pub use seam::OsEntropy;

#[cfg(any(test, feature = "test-support"))]
pub use seam::{FailingEntropy, ManualClock, MapChainView, SeededEntropy};
#[cfg(any(test, feature = "test-support"))]
pub use sign::{DeviceState, DirectoryEntry, MapDirectory};
