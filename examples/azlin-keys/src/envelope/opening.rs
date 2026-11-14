//! Assembling and opening envelopes: the one pipeline every algorithm goes through (planning/
//! architecture.md section 5.5; build unit S15, lane W1-S15b).
//!
//! `seal_assembled` is the only place an envelope is made from a header and a key: it builds the
//! associated data from the header, seals under it, and signs the whole. `open` is the only place
//! a sealed box or a hybrid envelope is opened, and it checks, IN THIS ORDER: the format
//! (the field rules of the algorithm; the version is the decoder's), the algorithm, that the
//! context is the one the caller expects, that the envelope is for this recipient, the signature
//! of the wrapping device (through the directory), and only then the AEAD. A failure at any step
//! returns its own error and nothing after it runs: a refused context is refused before any
//! signature is checked or any byte decrypted.
//!
//! Algorithms 3 to 5 have their keys derived by lane W1-S15d and algorithms 6 and 7 by S17:
//! `seal_with_key` and `open_with_key` are the same pipeline for a key the caller holds.

use zeroize::Zeroizing;

use super::aead::{open_under, seal_under};
use super::alg_table::Alg;
use super::combiner::{alg1_key, Alg1Input};
use super::ctx::Ctx;
use super::error::{open_error_of, EnvelopeError, OpenError, WrapError};
use super::recipient::RecipientSecret;
use super::verify::verify_signature;
use super::wire::{field, Envelope, Header};
use super::x25519::dh;
use crate::seam::Entropy;
use crate::secret::Secret;
use crate::sign::{DeviceDirectory, SignKeypair, Signature, VerifyAt};

/// Makes the signed envelope of `header`, `key` and `plaintext`: the associated data is the
/// header's, the nonce is drawn from `ent` (the last draw of every construction), and the
/// signature is the wrapping device's over the whole envelope but `sig`. After an error no
/// envelope exists.
pub(crate) fn seal_assembled(
    header: &Header,
    key: &Secret<32>,
    plaintext: &[u8],
    signer_key: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, WrapError> {
    let ad = header.header_ad()?;
    let (nonce, ct) = seal_under(key, &ad, plaintext, ent)?;
    let mut envelope = Envelope {
        alg: header.alg,
        rcpt: header.rcpt,
        epk: header.epk,
        kem_ct: header.kem_ct.clone(),
        nonce,
        ct,
        ctx: header.ctx.clone(),
        signer: header.signer,
        // Replaced by the signature below; `signing_input` does not read it.
        sig: Signature::from_bytes([0u8; 64]),
        params: header.params.clone(),
        crit: Vec::new(),
        unknown: Vec::new(),
    };
    envelope.sign(signer_key)?;
    Ok(envelope)
}

/// Wraps `plaintext` under an AEAD key the caller holds, as an envelope of algorithm 3, 4, 5, 6
/// or 7: the associated data is built from `header` (`params` included), the nonce is drawn from
/// `ent`, and the envelope is signed by `signer_key`. Algorithms 1 and 2 are `WrongAlg`: they
/// have their own constructions.
///
/// The key's derivation (Argon2id, the passkey PRF, a share ...) is the caller's; this function
/// makes it impossible to forget what the key's envelope must bind (the header, the context, the
/// parameters) or to skip the signature.
///
/// Seam: W1-S15d (`wrap_with_code`, `wrap_with_prf`, `seal_share_to_contact`) and S17 (algorithms
/// 6 and 7).
pub fn seal_with_key(
    header: &Header,
    key: &Secret<32>,
    plaintext: &[u8],
    signer_key: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, WrapError> {
    match header.alg {
        Alg::X25519SealedBox | Alg::X25519MlKem768 => Err(WrapError::WrongAlg(header.alg)),
        _ => seal_assembled(header, key, plaintext, signer_key, ent),
    }
}

/// Opens an envelope of algorithm 3, 4, 5, 6 or 7 under an AEAD key the caller derived: checks
/// the format, the algorithm, that `ctx` is `expect` and the signature of the wrapping device,
/// then opens. The `rcpt` is NOT checked (it names nobody here: the caller knows whose key it
/// derived). Algorithms 1 and 2 are `WrongAlg`.
///
/// Seam: W1-S15d (`unwrap_with_code`, `unwrap_with_prf`, `open_share`) and S17.
pub fn open_with_key(
    env: &Envelope,
    key: &Secret<32>,
    expect: &Ctx,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    env.check_rules().map_err(OpenError::Malformed)?;
    if matches!(env.alg, Alg::X25519SealedBox | Alg::X25519MlKem768) {
        return Err(OpenError::WrongAlg(env.alg));
    }
    if env.ctx != *expect {
        return Err(OpenError::CtxMismatch);
    }
    verify_signature(env, dir, at).map_err(open_error_of)?;
    let ad = env.header_ad().map_err(OpenError::Malformed)?;
    open_under(key, &ad, &env.nonce, &env.ct)
}

/// Opens `env` with the recipient secret `me`: the plaintext, in a `Zeroizing` buffer.
///
/// The checks run in this order, and each failure stops the call with its own error:
/// the format (the field rules of the algorithm: [`OpenError::Malformed`]), the algorithm
/// ([`OpenError::Unsupported`]: algorithm 2 in a build without `pq`, algorithms 3 to 7 here at
/// all), that `env.ctx` equals `expect` ([`OpenError::CtxMismatch`]: before the signature, before
/// any decryption), that `env.rcpt` is the id of `me` for the envelope's algorithm
/// ([`OpenError::NotForMe`]), the signature of `env.signer` through `dir` at `at` (the signer
/// errors), and last the key exchange and the AEAD ([`OpenError::NonContributory`],
/// [`OpenError::Auth`]).
///
/// Caller: INT-S15-1 (azul-storage unwraps), S15c1 (member, group, space and chain opens), S15d
/// (a share's inner envelope).
pub fn open(
    env: &Envelope,
    me: &RecipientSecret,
    expect: &Ctx,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    open_checked(env, me, expect, || {
        verify_signature(env, dir, at).map_err(open_error_of)
    })
}

/// The checks of `open`, with the signature check as a parameter so that the tests can leave it
/// out and show that the AEAD alone refuses what the signature also refuses. The production
/// caller passes the real check; there is no other copy of this order.
fn open_checked(
    env: &Envelope,
    me: &RecipientSecret,
    expect: &Ctx,
    check_signature: impl FnOnce() -> Result<(), OpenError>,
) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    env.check_rules().map_err(OpenError::Malformed)?;
    match env.alg {
        Alg::X25519SealedBox => {}
        Alg::X25519MlKem768 if cfg!(feature = "pq") => {}
        other => return Err(OpenError::Unsupported(other)),
    }
    if env.ctx != *expect {
        return Err(OpenError::CtxMismatch);
    }
    match me.id_for(env.alg) {
        Some(id) if id == env.rcpt => {}
        _ => return Err(OpenError::NotForMe),
    }
    check_signature()?;
    decrypt(env, me)
}

/// The key exchange, the key derivation and the AEAD of an envelope of algorithm 1 or 2: the
/// last step of `open`, and the whole of it for a test that wants to see the AEAD alone.
pub(crate) fn decrypt(
    env: &Envelope,
    me: &RecipientSecret,
) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    match env.alg {
        Alg::X25519SealedBox => decrypt_sealed_box(env, me),
        Alg::X25519MlKem768 => decrypt_hybrid(env, me),
        other => Err(OpenError::Unsupported(other)),
    }
}

/// Algorithm 1: the shared secret with the ephemeral key, the derived key, the AEAD.
fn decrypt_sealed_box(
    env: &Envelope,
    me: &RecipientSecret,
) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    let epk = env
        .epk
        .ok_or(OpenError::Malformed(EnvelopeError::FieldRequired(
            field::EPK,
        )))?;
    let shared = dh(me.kx(), &epk).ok_or(OpenError::NonContributory)?;
    let key = alg1_key(&Alg1Input {
        shared: &shared,
        epk: &epk,
        rcpt_kx: me.kx_public(),
    });
    let ad = env.header_ad().map_err(OpenError::Malformed)?;
    open_under(&key, &ad, &env.nonce, &env.ct)
}

/// Algorithm 2: both shared secrets, the combiner, the AEAD. ML-KEM rejects a corrupted
/// ciphertext implicitly, so the failure is the AEAD's, one word.
#[cfg(feature = "pq")]
fn decrypt_hybrid(env: &Envelope, me: &RecipientSecret) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    use super::combiner::{combine, Transcript};
    use super::hybrid::kem;

    let epk = env
        .epk
        .ok_or(OpenError::Malformed(EnvelopeError::FieldRequired(
            field::EPK,
        )))?;
    let kem_ct =
        env.kem_ct
            .as_deref()
            .ok_or(OpenError::Malformed(EnvelopeError::FieldRequired(
                field::KEM_CT,
            )))?;
    let (Some(seed), Some(pq)) = (me.pq_seed(), me.pq_public()) else {
        return Err(OpenError::NotForMe);
    };
    let ss_x25519 = dh(me.kx(), &epk).ok_or(OpenError::NonContributory)?;
    let ss_mlkem = kem::decapsulate(seed, kem_ct).ok_or(OpenError::Auth)?;
    let key = combine(
        &ss_mlkem,
        &ss_x25519,
        &Transcript {
            kem_ct,
            epk: &epk,
            rcpt_kx: me.kx_public(),
            pq,
        },
    );
    let ad = env.header_ad().map_err(OpenError::Malformed)?;
    open_under(&key, &ad, &env.nonce, &env.ct)
}

/// Without the feature `pq` an envelope of algorithm 2 is not opened here.
#[cfg(not(feature = "pq"))]
fn decrypt_hybrid(_env: &Envelope, _me: &RecipientSecret) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    Err(OpenError::Unsupported(Alg::X25519MlKem768))
}

/// `open` without the signature check: every other check and the AEAD, for the tests that show
/// the AEAD refuses a changed header field even when no signature stands in the way.
#[cfg(test)]
pub(crate) fn open_without_signature_for_test(
    env: &Envelope,
    me: &RecipientSecret,
    expect: &Ctx,
) -> Result<Zeroizing<Vec<u8>>, OpenError> {
    open_checked(env, me, expect, || Ok(()))
}
