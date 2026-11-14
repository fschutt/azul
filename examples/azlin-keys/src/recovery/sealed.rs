//! Algorithm 5: a Shamir share sealed to a trusted contact (planning/architecture.md section 5.5
//! "Recovery": "its 2-of-3 Shamir shares to contacts", "Envelope v2": `alg` 5 "Shamir share"; build
//! unit S15, lane W1-S15d).
//!
//! The envelope is an algorithm 5 envelope sealed like algorithm 1 (an ephemeral X25519 key to the
//! contact's X25519 key) or, when the contact key has an ML-KEM half, like algorithm 2 (the hybrid,
//! feature `pq`): the key schedule is the SAME one, `combiner::alg1_key` and `combiner::combine`
//! (DRIFT X-DELTA-W1S15D-12), so there is one construction for each key agreement; the algorithm
//! number inside the associated data tells the boxes apart. Its `params` are the share's
//! `{x, k, n, split_id}`, its `ctx` is `recovery/<member>/share` (the owner), its `rcpt` is the
//! contact key's id, its `epk` (and `kem_ct`) the ephemeral key. The plaintext is the 16 bytes of
//! the share followed by the owner's words (UTF-8: "Felix, drive Photos"), so the words are
//! authenticated and only the contact reads them.
//!
//! A share never wraps the member key: it is a share of the recovery CODE, and the recombined code
//! opens the algorithm 3 wrap (the chain shares to code to member key).
//!
//! Draws, in this order: the 32-byte ephemeral X25519 secret, for the hybrid the 32-byte ML-KEM
//! message, and last the 24-byte nonce.
//!
//! The contact verifies the signature of the wrapping device through a directory it holds: how the
//! contact learns the owner's signing key is the integration's (INT-S15-1; DRIFT
//! X-DELTA-W1S15D-12).

use zeroize::Zeroizing;

use super::code::CODE_LEN;
use super::contacts::ContactKey;
use super::error::RecoveryError;
use super::open::{malformed, open_recovery, require_method};
use super::shamir::Share;
use crate::envelope::combiner::{alg1_key, Alg1Input};
use crate::envelope::wire::field;
use crate::envelope::x25519::{dh, public_of};
use crate::envelope::{
    seal_with_key, Alg, Ctx, Envelope, EnvelopeError, Header, OpenError, Params, ParamsError,
    RecipientSecret, RecoveryMethod, WrapError,
};
use crate::ids::{DeviceId, KeyId};
use crate::seam::Entropy;
use crate::secret::Secret;
use crate::sign::{DeviceDirectory, SignKeypair, VerifyAt};

/// The longest owner's words a share carries, in bytes (UTF-8): 256.
///
/// Caller: `seal_share_to_contact`, `open_share`.
pub const MAX_OWNER_WORDS: usize = 256;

/// A share a contact opened, with the owner's words.
///
/// Caller: INT-S15-1 (the contact's app shows the words: "this share is Felix's, drive Photos").
#[derive(Debug)]
pub struct OpenedShare {
    /// The share (index, threshold, count, split id, bytes).
    pub share: Share,
    /// The owner's words, as the owner wrote them when sealing.
    pub owner_words: String,
}

/// The owner's words must be at most `MAX_OWNER_WORDS` bytes without a control character (a
/// contact's screen shows them).
fn check_words(words: &str) -> Result<(), RecoveryError> {
    if words.len() > MAX_OWNER_WORDS || words.chars().any(char::is_control) {
        return Err(RecoveryError::BadWords);
    }
    Ok(())
}

/// The plaintext of a sealed share: the 16 share bytes, then the owner's words.
fn plaintext_of(share: &Share, words: &str) -> Zeroizing<Vec<u8>> {
    let mut plaintext = Zeroizing::new(Vec::with_capacity(CODE_LEN + words.len()));
    plaintext.extend_from_slice(share.y.expose());
    plaintext.extend_from_slice(words.as_bytes());
    plaintext
}

/// Seals `share` to the contact key `contact` with the owner's words as an algorithm 5 envelope for
/// the context `ctx` (`recovery/<member>/share`), signed by the device `signer` with `sk`.
///
/// Over algorithm 1 when the contact key is X25519 only; over algorithm 2 when it has an ML-KEM
/// half (a build without the feature `pq` answers `Unsupported`: no silent downgrade). A share
/// that cannot come from a split, words that are too long or hold a control character, a context
/// that is not the share's, an entropy failure and a key of low order each return an error and NO
/// envelope.
///
/// Caller: INT-S15-1 (account setup: one call for each contact; a contact's answer to a recovery
/// request: the same call with the request key as the contact).
pub fn seal_share_to_contact(
    share: &Share,
    contact: &ContactKey,
    owner_words: &str,
    ctx: &Ctx,
    signer: &DeviceId,
    sk: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, RecoveryError> {
    require_method(ctx, RecoveryMethod::Share)?;
    check_words(owner_words)?;
    share.check()?;
    let plaintext = plaintext_of(share, owner_words);
    match contact.pq() {
        None => seal_over_x25519(share, contact.kx(), &plaintext, ctx, signer, sk, ent),
        Some(pq) => seal_over_hybrid(share, contact.kx(), pq, &plaintext, ctx, signer, sk, ent),
    }
}

/// Algorithm 5 over algorithm 1: an ephemeral key to the contact's X25519 key.
fn seal_over_x25519(
    share: &Share,
    kx: &[u8; 32],
    plaintext: &[u8],
    ctx: &Ctx,
    signer: &DeviceId,
    sk: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, RecoveryError> {
    let ephemeral = Secret::<32>::generate(ent)?;
    let epk = public_of(&ephemeral);
    let shared = dh(&ephemeral, kx).ok_or(RecoveryError::Wrap(WrapError::NonContributory))?;
    let key = alg1_key(&Alg1Input {
        shared: &shared,
        epk: &epk,
        rcpt_kx: kx,
    });
    let header = Header {
        alg: Alg::Shamir,
        rcpt: KeyId::of_bundle(kx, None),
        epk: Some(epk),
        kem_ct: None,
        params: share.params(),
        ctx: ctx.clone(),
        signer: *signer,
    };
    seal_with_key(&header, &key, plaintext, sk, ent).map_err(RecoveryError::from)
}

/// Algorithm 5 over algorithm 2: the X25519 agreement and an ML-KEM-768 encapsulation, one
/// combiner.
#[cfg(feature = "pq")]
#[allow(clippy::too_many_arguments)]
fn seal_over_hybrid(
    share: &Share,
    kx: &[u8; 32],
    pq: &[u8],
    plaintext: &[u8],
    ctx: &Ctx,
    signer: &DeviceId,
    sk: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, RecoveryError> {
    use crate::envelope::combiner::{combine, Transcript};
    use crate::envelope::hybrid::kem;

    if !kem::is_valid_encapsulation_key(pq) {
        return Err(RecoveryError::Wrap(WrapError::BadRecipientKey));
    }
    let ephemeral = Secret::<32>::generate(ent)?;
    let mut message = Zeroizing::new([0u8; 32]);
    ent.fill(message.as_mut_slice())?;
    let epk = public_of(&ephemeral);
    let ss_x25519 = dh(&ephemeral, kx).ok_or(RecoveryError::Wrap(WrapError::NonContributory))?;
    let (kem_ct, ss_mlkem) =
        kem::encapsulate(pq, &message).ok_or(RecoveryError::Wrap(WrapError::Kem))?;
    let key = combine(
        &ss_mlkem,
        &ss_x25519,
        &Transcript {
            kem_ct: &kem_ct,
            epk: &epk,
            rcpt_kx: kx,
            pq,
        },
    );
    let header = Header {
        alg: Alg::Shamir,
        rcpt: KeyId::of_bundle(kx, Some(pq)),
        epk: Some(epk),
        kem_ct: Some(kem_ct),
        params: share.params(),
        ctx: ctx.clone(),
        signer: *signer,
    };
    seal_with_key(&header, &key, plaintext, sk, ent).map_err(RecoveryError::from)
}

/// Without the feature `pq` there is no ML-KEM: a contact key with an ML-KEM half is `Unsupported`,
/// before any entropy is drawn, and nothing falls back to algorithm 1.
#[cfg(not(feature = "pq"))]
#[allow(clippy::too_many_arguments)]
fn seal_over_hybrid(
    _share: &Share,
    _kx: &[u8; 32],
    _pq: &[u8],
    _plaintext: &[u8],
    _ctx: &Ctx,
    _signer: &DeviceId,
    _sk: &SignKeypair,
    _ent: &mut dyn Entropy,
) -> Result<Envelope, RecoveryError> {
    Err(RecoveryError::Wrap(WrapError::Unsupported(
        Alg::X25519MlKem768,
    )))
}

/// The checks of a sealed share that cost nothing: it must have its ephemeral key, a hybrid one
/// needs this build to have `pq`, and the `rcpt` must be the id of this recipient for the kind of
/// box it is.
fn admit_share(env: &Envelope, me: &RecipientSecret) -> Result<(), RecoveryError> {
    if env.epk.is_none() {
        return Err(malformed(EnvelopeError::FieldRequired(field::EPK)));
    }
    let id = if env.kem_ct.is_some() {
        if !cfg!(feature = "pq") {
            return Err(RecoveryError::Unsupported(Alg::X25519MlKem768));
        }
        me.hybrid_id()
    } else {
        Some(me.x25519_id())
    };
    match id {
        Some(id) if id == env.rcpt => Ok(()),
        _ => Err(RecoveryError::Open(OpenError::NotForMe)),
    }
}

/// The AEAD key of a sealed share, for the recipient `me`: the key agreement with the ephemeral key
/// and the key schedule of algorithm 1 or 2.
fn share_key(env: &Envelope, me: &RecipientSecret) -> Result<Secret<32>, RecoveryError> {
    let epk = env
        .epk
        .ok_or_else(|| malformed(EnvelopeError::FieldRequired(field::EPK)))?;
    let shared = dh(me.kx(), &epk).ok_or(RecoveryError::Open(OpenError::NonContributory))?;
    match env.kem_ct.as_deref() {
        None => Ok(alg1_key(&Alg1Input {
            shared: &shared,
            epk: &epk,
            rcpt_kx: me.kx_public(),
        })),
        Some(kem_ct) => hybrid_key(me, kem_ct, &epk, &shared),
    }
}

/// The hybrid key of a sealed share: ML-KEM decapsulation and the combiner.
#[cfg(feature = "pq")]
fn hybrid_key(
    me: &RecipientSecret,
    kem_ct: &[u8],
    epk: &[u8; 32],
    shared: &Secret<32>,
) -> Result<Secret<32>, RecoveryError> {
    use crate::envelope::combiner::{combine, Transcript};
    use crate::envelope::hybrid::kem;

    let (Some(seed), Some(pq)) = (me.pq_seed(), me.pq_public()) else {
        return Err(RecoveryError::Open(OpenError::NotForMe));
    };
    let ss_mlkem = kem::decapsulate(seed, kem_ct).ok_or(RecoveryError::Auth)?;
    Ok(combine(
        &ss_mlkem,
        shared,
        &Transcript {
            kem_ct,
            epk,
            rcpt_kx: me.kx_public(),
            pq,
        },
    ))
}

/// Without the feature `pq` a hybrid sealed share is not opened.
#[cfg(not(feature = "pq"))]
fn hybrid_key(
    _me: &RecipientSecret,
    _kem_ct: &[u8],
    _epk: &[u8; 32],
    _shared: &Secret<32>,
) -> Result<Secret<32>, RecoveryError> {
    Err(RecoveryError::Unsupported(Alg::X25519MlKem768))
}

/// The share and the words of a sealed share's plaintext and its `params`.
fn share_of(env: &Envelope, plaintext: &[u8]) -> Result<OpenedShare, RecoveryError> {
    let Params::Shamir { x, k, n, split_id } = &env.params else {
        return Err(malformed(EnvelopeError::BadParams(ParamsError::WrongKind)));
    };
    let Some((y_bytes, words)) = plaintext.split_first_chunk::<CODE_LEN>() else {
        return Err(RecoveryError::BadShare);
    };
    if words.len() > MAX_OWNER_WORDS {
        return Err(RecoveryError::BadWords);
    }
    let owner_words = core::str::from_utf8(words)
        .map_err(|_| RecoveryError::BadWords)?
        .to_owned();
    check_words(&owner_words)?;
    let share = Share {
        x: *x,
        k: *k,
        n: *n,
        y: Secret::from_bytes(*y_bytes),
        split_id: *split_id,
    };
    share.check()?;
    Ok(OpenedShare { share, owner_words })
}

/// Opens an algorithm 5 envelope sealed to the recipient `me` (the contact's secret for this
/// owner): the share and the owner's words.
///
/// The checks run in the order of `open_recovery`: the format, the algorithm (`WrongAlg`), that the
/// context is `expect` (`recovery/<member>/share`: `Open(CtxMismatch)`, `WrongCtx`), that the
/// envelope has its ephemeral key and is for this recipient (`Open(NotForMe)`), the signature of
/// the wrapping device through `dir` at `at`, and last the key agreement and the cipher (`Auth`
/// for a changed header, parameter, words or ciphertext). The share's index, threshold and count
/// must be those of a share (`BadShare`).
///
/// Caller: INT-S15-1 (the contact's app opens what the owner sent; the recovering computer opens a
/// contact's reply with the request key).
pub fn open_share(
    env: &Envelope,
    me: &RecipientSecret,
    expect: &Ctx,
    dir: &dyn DeviceDirectory,
    at: VerifyAt,
) -> Result<OpenedShare, RecoveryError> {
    let plaintext = open_recovery(
        env,
        RecoveryMethod::Share,
        expect,
        dir,
        at,
        |env| admit_share(env, me),
        |env| share_key(env, me),
    )?;
    share_of(env, &plaintext)
}
