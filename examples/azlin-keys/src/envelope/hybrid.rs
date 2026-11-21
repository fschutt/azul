//! Algorithm 2: the X25519 + ML-KEM-768 hybrid (planning/architecture.md section 5.5, "2 X25519 +
//! ML-KEM-768 hybrid"; section 5.1, every wrap prefers post-quantum; build unit S15, lane
//! W1-S15b).
//!
//! `wrap_hybrid` does the X25519 half of algorithm 1 and, beside it, an ML-KEM-768 encapsulation
//! to the recipient's encapsulation key; the two shared secrets and every public value feed ONE
//! combiner (`combiner::combine`), which gives the AEAD key. The draws are, in this order: the
//! 32-byte ephemeral X25519 secret, the 32-byte ML-KEM message `m` (`encapsulate_deterministic`:
//! the randomness comes from the seam, so a seeded run replays), then the 24-byte nonce. The
//! `ml-kem` crate calls that interface unsafe because the encapsulation is only secure when `m`
//! is uniformly random: here `m` is a draw from the entropy seam and nothing else (never derived,
//! never reused: check C9).
//!
//! ML-KEM is the crate `ml-kem` behind the feature `pq` (NOT a default feature until the owner
//! has reviewed it: ARCH29 9.13 decision 3). Without the feature `wrap_hybrid` and the open of an
//! algorithm 2 envelope answer `Unsupported` BEFORE any entropy is drawn, and nothing falls back
//! to algorithm 1: a downgrade is the caller's decision, never ours.
//!
//! The recipient's encapsulation key is checked before it is used: 1,184 bytes, and the
//! canonical encoding of a key (FIPS 203 section 7.2: decoding and encoding it again must give
//! the same bytes), because the library reduces a non-canonical key silently (DRIFT
//! X-DELTA-W1S15B-13).

use super::alg_table::Alg;
use super::ctx::Ctx;
use super::error::WrapError;
use super::wire::Envelope;
use crate::ids::{DeviceId, KeyId};
use crate::seam::Entropy;
use crate::sign::SignKeypair;

/// The length of an ML-KEM-768 encapsulation key, in bytes (the `pq_pub` column).
///
/// Caller: `wrap_hybrid`, `RecipientSecret`, S15c1 (`PublicBundle` has a `pq_pub` of this length).
pub const PQ_KEY_LEN: usize = 1_184;

/// A recipient that has an X25519 key and an ML-KEM-768 encapsulation key: what an envelope of
/// algorithm 2 is sealed to. Its `rcpt` is `KeyId::of_bundle(kx, Some(pq))`, so it names both
/// keys (DRIFT X-DELTA-W1S15A-1).
///
/// Caller: INT-S15-1, S15c1 (`PublicBundle::recipient`), S15d (a contact's key).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RecipientHybrid {
    /// The recipient's X25519 public key (the `kx_pub` column).
    pub kx: [u8; 32],
    /// The recipient's ML-KEM-768 encapsulation key, 1,184 bytes (the `pq_pub` column).
    pub pq: Vec<u8>,
}

impl RecipientHybrid {
    /// The `rcpt` of an envelope sealed to this recipient: the id of the bundle `(kx, pq)`.
    ///
    /// A recipient whose `pq` is empty would have the id of an X25519-only bundle (the id of
    /// `Some` of an empty key is the id of `None`), so `wrap_hybrid` refuses it before it uses
    /// the id ([`WrapError::BadRecipientKey`]).
    pub fn key_id(&self) -> KeyId {
        KeyId::of_bundle(&self.kx, Some(&self.pq))
    }
}

/// The ML-KEM-768 calls, with the feature `pq`.
#[cfg(feature = "pq")]
pub(crate) mod kem {
    use ml_kem::kem::Decapsulate;
    use ml_kem::{EncapsulateDeterministic, EncodedSizeUser, KemCore, MlKem768, B32};
    use zeroize::Zeroize;

    use crate::secret::Secret;

    type EncapsulationKey = <MlKem768 as KemCore>::EncapsulationKey;

    /// The encapsulation key of these bytes, or `None` when they are not exactly the canonical
    /// encoding of a 1,184-byte key (FIPS 203 section 7.2: the type check and the modulus check).
    fn parse_encapsulation_key(pq: &[u8]) -> Option<EncapsulationKey> {
        let encoded = match <&ml_kem::Encoded<EncapsulationKey>>::try_from(pq) {
            Ok(encoded) => encoded,
            Err(_) => return None,
        };
        let key = EncapsulationKey::from_bytes(encoded);
        if key.as_bytes().as_slice() == pq {
            Some(key)
        } else {
            None
        }
    }

    /// Whether `pq` is a valid encapsulation key.
    pub(crate) fn is_valid_encapsulation_key(pq: &[u8]) -> bool {
        parse_encapsulation_key(pq).is_some()
    }

    /// The ML-KEM-768 ciphertext and the shared secret of encapsulating the message `m` to `pq`;
    /// `None` when `pq` is not a valid key or the KEM refuses. The message is wiped.
    pub(crate) fn encapsulate(pq: &[u8], m: &[u8; 32]) -> Option<(Vec<u8>, Secret<32>)> {
        let key = parse_encapsulation_key(pq)?;
        let mut message = B32::from(*m);
        let outcome = key.encapsulate_deterministic(&message);
        message.as_mut_slice().zeroize();
        let (ciphertext, mut shared) = match outcome {
            Ok(pair) => pair,
            Err(_) => return None,
        };
        let secret = Secret::<32>::from_slice(shared.as_slice());
        shared.as_mut_slice().zeroize();
        secret.map(|secret| (ciphertext.as_slice().to_vec(), secret))
    }

    /// The two 32-byte halves `(d, z)` of the 64-byte seed of a decapsulation key.
    fn split_seed(seed: &Secret<64>) -> Option<(B32, B32)> {
        let (d, rest) = seed.expose().split_first_chunk::<32>()?;
        let (z, _) = rest.split_first_chunk::<32>()?;
        Some((B32::from(*d), B32::from(*z)))
    }

    /// The encapsulation key (public) that the seed `(d, z)` generates, 1,184 bytes.
    pub(crate) fn public_key(seed: &Secret<64>) -> Option<Vec<u8>> {
        let (mut d, mut z) = split_seed(seed)?;
        let (_decapsulation_key, encapsulation_key) = MlKem768::generate_deterministic(&d, &z);
        d.as_mut_slice().zeroize();
        z.as_mut_slice().zeroize();
        Some(encapsulation_key.as_bytes().as_slice().to_vec())
    }

    /// The ML-KEM shared secret of the ciphertext `kem_ct` under the key of `seed`. ML-KEM
    /// rejects implicitly: a corrupted ciphertext gives a pseudorandom secret, not an error, so
    /// the AEAD that follows fails. `None` only for a ciphertext of the wrong length.
    pub(crate) fn decapsulate(seed: &Secret<64>, kem_ct: &[u8]) -> Option<Secret<32>> {
        let (mut d, mut z) = split_seed(seed)?;
        let (decapsulation_key, _encapsulation_key) = MlKem768::generate_deterministic(&d, &z);
        d.as_mut_slice().zeroize();
        z.as_mut_slice().zeroize();
        let ciphertext = match <&ml_kem::Ciphertext<MlKem768>>::try_from(kem_ct) {
            Ok(ciphertext) => ciphertext,
            Err(_) => return None,
        };
        let mut shared = match decapsulation_key.decapsulate(ciphertext) {
            Ok(shared) => shared,
            Err(_) => return None,
        };
        let secret = Secret::<32>::from_slice(shared.as_slice());
        shared.as_mut_slice().zeroize();
        secret
    }
}

/// The ML-KEM-768 public key of the seed `(d, z)`, or `None` in a build without the feature `pq`
/// (a recipient has then only its X25519 key).
///
/// Caller: `RecipientSecret::new`.
#[cfg(feature = "pq")]
pub(crate) fn public_key_of_seed(seed: &crate::secret::Secret<64>) -> Option<Vec<u8>> {
    kem::public_key(seed)
}

/// The ML-KEM-768 public key of the seed `(d, z)`, or `None` in a build without the feature `pq`
/// (a recipient has then only its X25519 key).
///
/// Caller: `RecipientSecret::new`.
#[cfg(not(feature = "pq"))]
pub(crate) fn public_key_of_seed(_seed: &crate::secret::Secret<64>) -> Option<Vec<u8>> {
    None
}

/// Wraps `plaintext` to the recipient `r` as an envelope of algorithm 2 for the context `ctx`,
/// signed by the device `signer` with `key`: X25519 and ML-KEM-768 together, combined by
/// `combiner::combine`.
///
/// The draws are the 32-byte ephemeral key, the 32-byte ML-KEM message and the 24-byte nonce.
/// Without the feature `pq` this is [`WrapError::Unsupported`], before any draw. A recipient
/// without a valid `pq` key ([`WrapError::BadRecipientKey`]), an entropy failure, a recipient
/// key of low order and an oversize plaintext each return an error and NO envelope.
///
/// Caller: INT-S15-1 (azul-storage wraps), S15c1 (member, group and space wraps for recipients
/// with an ML-KEM key: P-6, the default whenever the recipient has one), S15d.
#[cfg(feature = "pq")]
pub fn wrap_hybrid(
    r: &RecipientHybrid,
    ctx: &Ctx,
    plaintext: &[u8],
    signer: &DeviceId,
    key: &SignKeypair,
    ent: &mut dyn Entropy,
) -> Result<Envelope, WrapError> {
    use zeroize::Zeroizing;

    use super::combiner::{combine, Transcript};
    use super::opening::seal_assembled;
    use super::wire::{Header, Params, MAX_PLAINTEXT};
    use super::x25519::{dh, public_of};
    use crate::secret::Secret;

    if plaintext.len() > MAX_PLAINTEXT {
        return Err(WrapError::PlaintextTooLarge);
    }
    if !kem::is_valid_encapsulation_key(&r.pq) {
        return Err(WrapError::BadRecipientKey);
    }
    let ephemeral = Secret::<32>::generate(ent)?;
    let mut message = Zeroizing::new([0u8; 32]);
    ent.fill(message.as_mut_slice())?;
    let epk = public_of(&ephemeral);
    let ss_x25519 = dh(&ephemeral, &r.kx).ok_or(WrapError::NonContributory)?;
    let (kem_ct, ss_mlkem) = kem::encapsulate(&r.pq, &message).ok_or(WrapError::Kem)?;
    let aead_key = combine(
        &ss_mlkem,
        &ss_x25519,
        &Transcript {
            kem_ct: &kem_ct,
            epk: &epk,
            rcpt_kx: &r.kx,
            pq: &r.pq,
        },
    );
    let header = Header {
        alg: Alg::X25519MlKem768,
        rcpt: r.key_id(),
        epk: Some(epk),
        kem_ct: Some(kem_ct),
        params: Params::None,
        ctx: ctx.clone(),
        signer: *signer,
    };
    seal_assembled(&header, &aead_key, plaintext, key, ent)
}

/// Without the feature `pq` there is no ML-KEM: algorithm 2 is `Unsupported`, before any entropy
/// is drawn, and nothing falls back to algorithm 1.
///
/// Caller: INT-S15-1, S15c1, S15d.
#[cfg(not(feature = "pq"))]
pub fn wrap_hybrid(
    _r: &RecipientHybrid,
    _ctx: &Ctx,
    _plaintext: &[u8],
    _signer: &DeviceId,
    _key: &SignKeypair,
    _ent: &mut dyn Entropy,
) -> Result<Envelope, WrapError> {
    Err(WrapError::Unsupported(Alg::X25519MlKem768))
}
