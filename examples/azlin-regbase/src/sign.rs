//! The signing rule and the key types of the registry (planning/architecture.md section 2.5
//! "Signed structures, each with its own domain string"; COMMON-W1R R3c and R4; build unit S9,
//! lane W1-S9a).
//!
//! THE ONE SIGNING RULE: the preimage of every signature is `domain || 0x00 || bytes`, built once
//! by `azlin_keys::sign::signing_input` and re-exported here as [`signing_input`]. The draft of
//! this crate (`azlin-sig-v1 || len(domain) || domain || canonical`) is VOID (conflict C-01); this
//! module holds no framing of its own.
//!
//! `PublicKey32` and `Signature64` keep their names but are aliases of azlin-keys' public-data
//! types (`VerifyKey`, `Signature`): both are `Copy`, ordered and hashable (so they are map keys)
//! and have `from_bytes` / `as_bytes`.
//!
//! The three domains of the key proofs of [`crate::proof`] are registered here for uniqueness; they
//! join azlin-contract's `signed.rs` as `Proposed` rows through S5b (conflict C-12).

pub use azlin_keys::sign::{signing_input, Domain};

/// An Ed25519 public key (the `sign_pub` columns): 32 bytes of public data. It is
/// `azlin_keys::VerifyKey`.
///
/// Caller: every key column of azlin-registry's rows, `KeyWindow`, `verify_key_proof`.
pub type PublicKey32 = azlin_keys::VerifyKey;

/// An Ed25519 signature: 64 bytes of public data. It is `azlin_keys::Signature`.
///
/// Caller: `KeyProof`, azlin-creds' certificates and entitlements (S9e).
pub type Signature64 = azlin_keys::Signature;

/// The domain of a device's key proof for a refresh (planning/architecture.md section 2.4
/// "refresh: device key or token"; X-D-07).
///
/// Caller: `ProofPurpose::Refresh`.
pub const REFRESH_V1: Domain = Domain::new("azlin-refresh-v1");

/// The domain of a NEW device's key proof when it claims its first token
/// (`device_enroll_finish`).
///
/// Caller: `ProofPurpose::EnrollFinish`.
pub const ENROLL_FINISH_V1: Domain = Domain::new("azlin-enroll-finish-v1");

/// The domain of an app principal's key proof for a refresh (app principals never rotate a token:
/// planning/architecture.md section 5.3).
///
/// Caller: `ProofPurpose::AppRefresh`.
pub const APP_REFRESH_V1: Domain = Domain::new("azlin-app-refresh-v1");

/// The signature domains this crate defines: the three key proofs. It defines no other.
///
/// Caller: the tests, INT-WIRE (the comparison with azlin-contract's `SIGNED_DOMAINS`).
pub const PROOF_DOMAINS: &[Domain] = &[REFRESH_V1, ENROLL_FINISH_V1, APP_REFRESH_V1];
