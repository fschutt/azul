//! Algorithm 4: the member key under the PRF output of a passkey (planning/architecture.md section
//! 5.5 "Recovery": "the passkey PRF (D51) wraps the member key", "the passkey wrap is on by default
//! for personal and family accounts"; section 6.11; build unit S15, lane W1-S15d).
//!
//! The platform's WebAuthn PRF extension (CTAP2 `hmac-secret`) answers a 32-byte output for a salt
//! the app chooses; the call is the app's (INT-S15-1: assumption A-22), this module only wraps
//! and unwraps. The wrap's key is BLAKE3 `derive_key` under the context
//! `azlin 2026-11 passkey prf wrap v1` over the PRF output and the salt; the salt and the hash of
//! the credential's id are the envelope's `params` (`{1: salt, 2: cred_id_hash}`), so a recovering
//! device reads them (`prf_inputs`), asks the platform for the PRF of THAT salt with the credential
//! whose id hashes to `cred_id_hash`, and unwraps. A new salt per wrap gives each wrap its own PRF
//! output: a leaked output opens one wrap, not the next.
//!
//! The brief's `wrap_with_prf` has no salt parameter; the salt is an input here because the PRF
//! output is a function of it and the app must have evaluated the PRF on it BEFORE the wrap (DRIFT
//! X-DELTA-W1S15D-11, the brief's own item W1S15D-3).
//!
//! The hold of 48 hours and its notices are the server's (planning/architecture.md section 7.5,
//! assumption A-21): this module never reads a clock.

use zeroize::Zeroizing;

use super::error::RecoveryError;
use super::open::{kdf, malformed, open_recovery, require_method, slot_rcpt};
use crate::derive::PRF_WRAP;
use crate::envelope::{
    seal_with_key, Alg, Ctx, Envelope, EnvelopeError, Header, Params, ParamsError, RecoveryMethod,
};
use crate::ids::DeviceId;
use crate::seam::Entropy;
use crate::secret::Secret;
use crate::sign::{DeviceDirectory, SignKeypair, VerifyAt};

/// The bytes of the PRF salt and of the credential id's hash: 32.
///
/// Caller: `new_prf_salt`, `PrfInputs`.
pub const PRF_BYTES: usize = 32;

/// What a recovering device feeds the platform: the salt the PRF is evaluated on, and the hash of
/// the id of the credential that evaluates it (BLAKE3 of the credential id: the app matches the
/// hash against the credentials the platform offers).
///
/// Caller: INT-S15-1 (the recovery screen reads them from the wrap before it asks the platform).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrfInputs {
    /// The PRF salt of the wrap.
    pub salt: [u8; PRF_BYTES],
    /// The hash of the credential id.
    pub cred_id_hash: [u8; PRF_BYTES],
}

/// A fresh PRF salt: 32 bytes from the entropy seam (the app evaluates the passkey's PRF on it,
/// then wraps with it).
///
/// Caller: INT-S15-1 (passkey registration).
pub fn new_prf_salt(ent: &mut dyn Entropy) -> Result<[u8; PRF_BYTES], RecoveryError> {
    let mut salt = [0u8; PRF_BYTES];
    ent.fill(&mut salt)?;
    Ok(salt)
}

/// The AEAD key of an algorithm 4 wrap: BLAKE3 `derive_key` under the context
/// `azlin 2026-11 passkey prf wrap v1` over `prf_output || salt` (P-2 of the brief).
fn prf_key(prf_output: &Secret<32>, salt: &[u8; PRF_BYTES]) -> Secret<32> {
    kdf(
        PRF_WRAP.as_str(),
        &[prf_output.expose().as_slice(), salt.as_slice()],
    )
}

/// Wraps `member_key` under the passkey's PRF output as an algorithm 4 envelope for the context
/// `ctx` (`recovery/<member>/prf`), signed by the device `signer` with `sk`. `salt` is the PRF
/// input the output was evaluated on, `cred_id_hash` the hash of the credential's id; both are
/// stored in the envelope's `params`. The `rcpt` names the credential's slot.
///
/// The only draw is the 24-byte nonce. An entropy failure and a context that is not the
/// passkey's return an error and NO envelope.
///
/// Caller: INT-S15-1 (passkey registration; on by default for personal and family accounts).
#[allow(clippy::too_many_arguments)]
pub fn wrap_with_prf<const N: usize>(
    member_key: &Secret<N>,
    prf_output: &Secret<32>,
    salt: &[u8; PRF_BYTES],
    cred_id_hash: &[u8; PRF_BYTES],
    ctx: &Ctx,
    signer: &DeviceId,
    sk: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, RecoveryError> {
    require_method(ctx, RecoveryMethod::Prf)?;
    let key = prf_key(prf_output, salt);
    let header = Header {
        alg: Alg::Prf,
        rcpt: slot_rcpt(Alg::Prf, ctx, cred_id_hash),
        epk: None,
        kem_ct: None,
        params: Params::Prf {
            salt: *salt,
            cred_id_hash: *cred_id_hash,
        },
        ctx: ctx.clone(),
        signer: *signer,
    };
    seal_with_key(&header, &key, member_key.expose(), sk, ent).map_err(RecoveryError::from)
}

/// The PRF salt and the credential hash of an algorithm 4 envelope: what the app asks the
/// platform for. `WrongAlg` for another algorithm.
///
/// Caller: INT-S15-1 (the recovery screen, before the platform call).
pub fn prf_inputs(env: &Envelope) -> Result<PrfInputs, RecoveryError> {
    env.check_rules().map_err(malformed)?;
    if env.alg != Alg::Prf {
        return Err(match env.alg {
            Alg::Attested | Alg::Threshold => RecoveryError::Unsupported(env.alg),
            other => RecoveryError::WrongAlg(other),
        });
    }
    match &env.params {
        Params::Prf { salt, cred_id_hash } => Ok(PrfInputs {
            salt: *salt,
            cred_id_hash: *cred_id_hash,
        }),
        _ => Err(malformed(EnvelopeError::BadParams(ParamsError::WrongKind))),
    }
}

/// Opens an algorithm 4 envelope with the PRF output the platform gave for [`prf_inputs`]: the
/// plaintext (the member key), in a buffer that is wiped when dropped.
///
/// The checks run in the order of `open_recovery`: the format, the algorithm (`WrongAlg`; 6 and 7
/// are `Unsupported`), the context (`Open(CtxMismatch)`, `WrongCtx`), the signature of the wrapping
/// device through `dir` at `at`, and last the key derivation and the cipher (`Auth` for a wrong
/// output or a changed header, salt, credential hash or ciphertext).
///
/// `at` is where the wrapping device's key is asked for. A recovery passes the point of the
/// WRITE of the wrap (the position or time of the registry's row), not `VerifyAt::Now`: the
/// device that wrapped the key may have been revoked since, and a lost laptop is exactly when a
/// recovery is needed.
///
/// Caller: INT-S15-1 (the recovery screen: a new phone signed in to the same Apple or Google
/// account, after the 48 h hold).
pub fn unwrap_with_prf(
    env: &Envelope,
    prf_output: &Secret<32>,
    expect: &Ctx,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<Zeroizing<Vec<u8>>, RecoveryError> {
    open_recovery(
        env,
        RecoveryMethod::Prf,
        expect,
        dir,
        at,
        |_| Ok(()),
        |env| match &env.params {
            Params::Prf { salt, .. } => Ok(prf_key(prf_output, salt)),
            _ => Err(malformed(EnvelopeError::BadParams(ParamsError::WrongKind))),
        },
    )
}
