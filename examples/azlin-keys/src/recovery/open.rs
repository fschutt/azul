//! The one pipeline every recovery wrap is opened through, and the helpers the three wraps share
//! (planning/architecture.md section 5.5 "Envelope v2 (every wrap)"; build unit S15, lane
//! W1-S15d).
//!
//! `open_recovery` checks, IN THIS ORDER, and each failure stops the call with its own error: the
//! format of the envelope (the field rules of its algorithm), the algorithm, that the context the
//! caller expects is the recovery context of this method and that the envelope carries exactly it,
//! the cheap checks of the wrap itself (`admit`: the Argon2id cost against the floor and the limits,
//! the recipient of a share), the signature of the wrapping device through the directory, and only
//! then the key derivation (`derive`: Argon2id, the passkey PRF, the key agreement) and the cipher.
//! So a hostile server that lowers or raises a cost, a replay of another member's wrap, a forged
//! envelope or a wrap for another recipient costs the device a few comparisons and one signature
//! check, never an Argon2id derivation.
//!
//! `slot_rcpt` is what the `rcpt` field of an algorithm 3 or 4 envelope holds: those wraps have no
//! recipient key, so the field names the slot the wrap fills (DRIFT X-DELTA-W1S15D-9).

use zeroize::{Zeroize, Zeroizing};

use super::error::RecoveryError;
use crate::envelope::error::open_error_of;
use crate::envelope::{
    open_under, verify_signature, Alg, Ctx, CtxKind, Envelope, EnvelopeError, OpenError,
    RecoveryMethod,
};
use crate::ids::KeyId;
use crate::secret::Secret;
use crate::sign::{DeviceDirectory, VerifyAt};

/// The BLAKE3 `derive_key` context of the slot id an algorithm 3 or 4 wrap names in its `rcpt`.
/// An entry that the registry of `derive.rs` does not hold (DRIFT X-DELTA-W1S15D-10: reported to
/// the owner of that table); the test `c14_the_local_contexts_are_unique_and_none_is_registered`
/// keeps it disjoint from every registered context.
///
/// Caller: `slot_rcpt` (this module).
pub const SLOT_ID_CONTEXT: &str = "azlin 2026-11 recovery slot id v1";

/// The BLAKE3 `derive_key` context of the safety number of two public keys (DRIFT
/// X-DELTA-W1S15D-10, like [`SLOT_ID_CONTEXT`]).
///
/// Caller: `contacts::safety_number`.
pub const SAFETY_NUMBER_CONTEXT: &str = "azlin 2026-11 contact safety number v1";

/// The contexts this module derives under that are not in the registry of `derive.rs`, as
/// `(name, text)` pairs (the form of `derive::CONTEXTS`' entries).
///
/// Caller: the test `c14_the_local_contexts_are_unique_and_none_is_registered`, INT-WIRE (the
/// owner of `derive.rs` may adopt them).
pub const LOCAL_CONTEXTS: &[(&str, &str)] = &[
    ("SLOT_ID", SLOT_ID_CONTEXT),
    ("SAFETY_NUMBER", SAFETY_NUMBER_CONTEXT),
];

/// The algorithm of a recovery method: the code is algorithm 3, the passkey algorithm 4, a share
/// algorithm 5.
pub(crate) fn alg_of(method: RecoveryMethod) -> Alg {
    match method {
        RecoveryMethod::Code => Alg::Argon2id,
        RecoveryMethod::Prf => Alg::Prf,
        RecoveryMethod::Share => Alg::Shamir,
    }
}

/// `ctx` must be `recovery/<member>/<method>` of exactly this method (DRIFT X-DELTA-W1S15D-15: the
/// brief's functions take any `Ctx`).
pub(crate) fn require_method(ctx: &Ctx, method: RecoveryMethod) -> Result<(), RecoveryError> {
    if ctx.kind() == CtxKind::Recovery && ctx.method() == Some(method) {
        Ok(())
    } else {
        Err(RecoveryError::WrongCtx)
    }
}

/// The `rcpt` of an algorithm 3 or 4 wrap: BLAKE3 `derive_key` under [`SLOT_ID_CONTEXT`] over the
/// algorithm number, the context's text, a zero byte and `extra` (empty for the code, the
/// credential's hash for a passkey). Public data only; it names the slot, not a key.
pub(crate) fn slot_rcpt(alg: Alg, ctx: &Ctx, extra: &[u8]) -> KeyId {
    let text = ctx.as_str().as_bytes();
    let mut material = Vec::with_capacity(text.len() + extra.len() + 2);
    material.push(alg.number());
    material.extend_from_slice(text);
    material.push(0);
    material.extend_from_slice(extra);
    KeyId::from_bytes(blake3::derive_key(SLOT_ID_CONTEXT, &material))
}

/// BLAKE3 `derive_key` under `context` over the concatenation of `parts`: the hasher (it has seen
/// secret material) and the hash are wiped, and the key lives on only in the `Secret` returned.
pub(crate) fn kdf(context: &str, parts: &[&[u8]]) -> Secret<32> {
    let mut hasher = blake3::Hasher::new_derive_key(context);
    for part in parts {
        hasher.update(part);
    }
    let mut hash = hasher.finalize();
    let key = Secret::from_bytes(*hash.as_bytes());
    hasher.zeroize();
    hash.zeroize();
    key
}

/// An envelope that breaks the field rules of its algorithm: only a hand-built one has this.
pub(crate) fn malformed(e: EnvelopeError) -> RecoveryError {
    RecoveryError::Open(OpenError::Malformed(e))
}

/// The one pipeline: see the module documentation for the order of the checks.
///
/// `admit` sees the envelope after the context matched and before the signature is checked: the
/// checks that cost nothing (and need no directory). `derive` is the expensive step and runs last.
/// Neither may be reordered: the tests count the calls of `derive` for every refusal.
pub(crate) fn open_recovery(
    env: &Envelope,
    method: RecoveryMethod,
    expect: &Ctx,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
    admit: impl FnOnce(&Envelope) -> Result<(), RecoveryError>,
    derive: impl FnOnce(&Envelope) -> Result<Secret<32>, RecoveryError>,
) -> Result<Zeroizing<Vec<u8>>, RecoveryError> {
    env.check_rules().map_err(malformed)?;
    let alg = alg_of(method);
    if env.alg != alg {
        return Err(match env.alg {
            Alg::Attested | Alg::Threshold => RecoveryError::Unsupported(env.alg),
            other => RecoveryError::WrongAlg(other),
        });
    }
    require_method(expect, method)?;
    if env.ctx != *expect {
        return Err(RecoveryError::Open(OpenError::CtxMismatch));
    }
    admit(env)?;
    verify_signature(env, dir, at).map_err(|e| RecoveryError::from(open_error_of(e)))?;
    let key = derive(env)?;
    let ad = env.header_ad().map_err(malformed)?;
    open_under(&key, &ad, &env.nonce, &env.ct).map_err(RecoveryError::from)
}
