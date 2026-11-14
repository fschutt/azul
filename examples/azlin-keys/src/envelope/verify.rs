//! The KEY-LESS functions of envelope v2: what a node that stores wraps it can never open, and
//! any other verifier, can check (planning/architecture.md section 5.3, `reg_wrap`: "The server
//! stores them and never opens them. All wraps live here, not in the bucket: they are small,
//! Raft-replicated, and the server can enforce who writes them"; section 5.5, "Envelopes carry
//! the signer"; build unit S15, lane W1-S15b).
//!
//! [`check_shape`] takes bytes and nothing else: the size limit, strict deterministic CBOR, the
//! field rules of the algorithm, the grammar of `ctx`. [`verify_signature`] takes an envelope and
//! a device directory (which holds PUBLIC keys): the signature of the wrapping device over the
//! envelope. Neither takes a secret, a recipient key or a plaintext. The registry (S9) calls both on every
//! `reg_wrap` write and stores the bytes untouched (assumption A-14 of the brief).

use super::alg_table::Alg;
use super::ctx::Ctx;
use super::error::EnvelopeError;
use super::wire::Envelope;
use crate::error::SignError;
use crate::ids::{DeviceId, KeyId};
use crate::sign::{self, DeviceDirectory, VerifyAt, ENVELOPE_V2};

/// The largest envelope the registry stores: 4 KiB (P-5 of the brief; a hybrid wrap is its
/// 1,088-byte KEM ciphertext plus about 300 bytes of header and a 100-byte plaintext). A wrap is
/// small by design; a bigger one is refused before it is parsed.
///
/// Caller: `check_shape`, INT-S15-2 (the registry's limit for a `reg_wrap` row).
pub const MAX_ENVELOPE: usize = 4096;

/// What a node learns from an envelope without opening it: the fields a `reg_wrap` row must agree
/// with.
///
/// Caller: INT-S15-2 (`s3d/meta` compares them with the row), S15c2 (`validate_row`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvelopeMeta {
    /// The algorithm.
    pub alg: Alg,
    /// The id of the recipient's public keys.
    pub rcpt: KeyId,
    /// What is wrapped.
    pub ctx: Ctx,
    /// The wrapping device.
    pub signer: DeviceId,
    /// The size of the envelope in bytes (DRIFT X-DELTA-W1S15B-10: a raw size in a public type,
    /// as the brief names it; azlin-keys has no byte-count type).
    pub size: usize,
}

/// Checks that `bytes` are exactly one well-formed envelope of v2 that the registry may store:
/// at most [`MAX_ENVELOPE`] bytes (checked first), strict deterministic CBOR, `v` 2, a known
/// algorithm, every field of the right type and length, the per-algorithm field rules (`epk` and
/// `kem_ct` present or absent as the algorithm says, `params` of its kind), a `ctx` in the
/// grammar, unknown keys ignored unless `crit` lists them. It does NOT check the signature
/// ([`verify_signature`] does) and it cannot open anything: it takes no key.
///
/// Algorithms 3 to 7 pass as shapes; their cryptography is not checked here, by anybody.
///
/// Caller: INT-S15-2 (the registry on every `reg_wrap` write), S15c2 (`validate_row`), the fuzz
/// target `envelope_decode`.
pub fn check_shape(bytes: &[u8]) -> Result<EnvelopeMeta, EnvelopeError> {
    if bytes.len() > MAX_ENVELOPE {
        return Err(EnvelopeError::TooLarge);
    }
    let envelope = Envelope::from_cbor(bytes)?;
    Ok(EnvelopeMeta {
        alg: envelope.alg,
        rcpt: envelope.rcpt,
        ctx: envelope.ctx,
        signer: envelope.signer,
        size: bytes.len(),
    })
}

/// Verifies the signature of the wrapping device: the directory names the key of `env.signer` at
/// `at`, and the signature must verify (strictly: a signature that is not canonical, or a signer
/// key of small order, is refused) over `azlin-envelope-v2 0x00` and the canonical bytes of the
/// envelope without `sig`. The signature covers the ciphertext, so the signer vouches for it; a
/// `ct` swapped in from another envelope with an identical header does not verify.
///
/// Every refusal has its own name: an unknown signer, a signer not yet valid, a revoked signer
/// and a directory that cannot answer are four different errors, and none is turned into a
/// default key; a signature that does not verify is [`EnvelopeError::BadSignature`].
///
/// Caller: INT-S15-2 (the registry on every `reg_wrap` write: only an authorized device may
/// write), S15c2 (`validate_row`, `authorize_epoch`), `open` (this crate).
pub fn verify_signature(
    env: &Envelope,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<(), EnvelopeError> {
    env.check_rules()?;
    let message = env.signing_input()?;
    sign::verify_with(dir, &env.signer, at, &ENVELOPE_V2, &message, &env.sig).map_err(|e| match e {
        SignError::Bad => EnvelopeError::BadSignature,
        SignError::UnknownSigner => EnvelopeError::UnknownSigner,
        SignError::NotYetActive => EnvelopeError::SignerNotYetActive,
        SignError::Revoked => EnvelopeError::SignerRevoked,
        SignError::DirectoryUnavailable => EnvelopeError::DirectoryUnavailable,
    })
}
