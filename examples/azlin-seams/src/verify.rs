//! The signature verifier seam: `SignatureVerifier`, the strict Ed25519 verifier, and a test
//! double (planning/architecture.md section 2.5 "Signed structures, each with its own domain
//! string", section 5.9 "every entry is signed by a device whose role allows it", section 12 E4;
//! build unit S17, lane W1-S17a).
//!
//! THE PREIMAGE IS ALREADY FRAMED (COMMON-W1R R4, ARCH29 9.13 decision 2): the `preimage` every
//! method takes is `azlin_keys::sign::signing_input(domain, bytes)`, that is `domain || 0x00 ||
//! bytes`, built by the CALLER. The verifier checks the signature over those bytes exactly as they
//! are and never frames, so a structure is never framed twice and no verifier can be talked into
//! another domain.
//!
//! - [`Ed25519Verifier`] is `azlin_keys::sign::verify_preimage` (strict verification: canonical `S`,
//!   no small-order `R` or key) with the failure told apart: a key that is no usable curve point
//!   is [`VerifyError::BadKey`], a signature of the wrong length is [`VerifyError::Malformed`],
//!   and a signature that does not verify is [`VerifyError::BadSignature`]. Whether it verifies
//!   is decided by `azlin-keys` alone.
//! - A signature algorithm this crate cannot check is `Unsupported(alg)`, never a pass: ML-DSA
//!   (`MlDsa65`, no vetted crate in either lock) and ECDSA P-256 (`EcdsaP256`, WebAuthn is S17d)
//!   fail closed. A chain that requires one of them fails closed too (DRIFT 3 of the brief).
//! - [`FakeScheme`] is a keyed-hash "signature" for tests: it verifies only what
//!   `FakeScheme::sign` made for the same key and preimage. It is NOT a signature scheme and
//!   anybody can forge it: it never appears in a production composition.

use azlin_keys::sign::verify_preimage;
use azlin_keys::{KeyId, Signature, VerifyKey};

/// The signature algorithms the engine's formats name.
///
/// Implements planning/architecture.md section 5.9 (`sig` Ed25519, `pq_sig` ML-DSA, `webauthn`
/// passkey assertions, which are ECDSA P-256 in practice).
///
/// Caller: every `SignatureVerifier` call (S17c entries, S16a records, S17d assertions, azlin-chain
/// restore records).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SigAlg {
    /// Ed25519 (RFC 8032), checked with `verify_strict`.
    Ed25519,
    /// ML-DSA-65: no vetted crate is in either lockfile, so it is `Unsupported` here.
    MlDsa65,
    /// ECDSA over P-256: the passkey assertions of S17d, so it is `Unsupported` here.
    EcdsaP256,
}

/// Why a signature is not accepted. One small `Copy` enum, no data that could carry a secret.
///
/// Implements planning/architecture.md section 5.9 ("a bad signature" is a reason the group gives).
///
/// Caller: every `SignatureVerifier` implementation and its callers (azlin-chain `HeadVerifier`,
/// S17c `Refusal::BadSignature`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum VerifyError {
    /// The signature does not verify: a wrong message, key, domain or signature, or a
    /// non-canonical signature.
    #[error("the signature does not verify")]
    BadSignature,
    /// The public key is unusable: of the wrong length, no curve point, or of small order.
    #[error("the public key is unusable")]
    BadKey,
    /// The signature has the wrong shape (for Ed25519: not exactly 64 bytes).
    #[error("the signature has the wrong shape")]
    Malformed,
    /// The algorithm cannot be checked here: a refusal, never a pass.
    #[error("the signature algorithm {0:?} is not supported here")]
    Unsupported(SigAlg),
}

/// The seam through which the pure crates check a signature: algorithm, public key, the framed
/// preimage and the signature bytes. See the module for the framing rule.
///
/// Implements planning/architecture.md section 2.5 and section 5.9.
///
/// Seam: S17c (`verify_signatures`), S16a (`AppendLog::append`), S16f (`WriterProof`), S45c, the
/// restore and head verifiers of azlin-chain; production passes [`Ed25519Verifier`], tests pass
/// [`FakeScheme`] or the real one.
pub trait SignatureVerifier {
    /// `Ok(())` when `sig` is a valid signature of `preimage` under `public_key` with `alg`;
    /// otherwise why not. `preimage` is the framed bytes (`signing_input`), checked as they are.
    fn verify(
        &self,
        alg: SigAlg,
        public_key: &[u8],
        preimage: &[u8],
        sig: &[u8],
    ) -> Result<(), VerifyError>;
}

/// The strict Ed25519 verifier: a thin wrapper over `azlin_keys::sign::verify_preimage`.
///
/// Implements planning/architecture.md section 2.5 and section 5.9 (every entry is signed by a
/// device whose role allows it).
///
/// Caller: the production compositions of S17c, S16a and azlin-chain's verifiers (through the
/// glue units CHAIN-GLUE and TEAMLOG-APPLY).
#[derive(Clone, Copy, Debug, Default)]
pub struct Ed25519Verifier;

impl Ed25519Verifier {
    /// Why a signature that `verify_preimage` refused was refused: an unusable key is `BadKey`
    /// (decided by the curve library's own parsing and its small-order test, only to NAME the
    /// refusal), anything else is `BadSignature`.
    fn refusal(key: &[u8; 32]) -> VerifyError {
        match ed25519_dalek::VerifyingKey::from_bytes(key) {
            Ok(parsed) if !parsed.is_weak() => VerifyError::BadSignature,
            _ => VerifyError::BadKey,
        }
    }
}

impl SignatureVerifier for Ed25519Verifier {
    fn verify(
        &self,
        alg: SigAlg,
        public_key: &[u8],
        preimage: &[u8],
        sig: &[u8],
    ) -> Result<(), VerifyError> {
        if alg != SigAlg::Ed25519 && !mutant!("seams_verifier_any_alg") {
            return Err(VerifyError::Unsupported(alg));
        }
        let key = <[u8; 32]>::try_from(public_key).map_err(|_| VerifyError::BadKey)?;
        let sig = <[u8; 64]>::try_from(sig).map_err(|_| VerifyError::Malformed)?;
        match verify_preimage(
            preimage,
            &VerifyKey::from_bytes(key),
            &Signature::from_bytes(sig),
        ) {
            Ok(()) => Ok(()),
            Err(_) => Err(Ed25519Verifier::refusal(&key)),
        }
    }
}

/// The context of the fake scheme's keyed hash: a string no production hash uses.
const FAKE_CONTEXT: &str = "azlin-seams fake scheme v1 (tests only, forgeable by anyone)";

/// The length of a fake signature.
const FAKE_SIG_LEN: usize = 32;

/// A keyed hash of `(key, preimage)`, length-framed.
fn fake_tag(key: &[u8], preimage: &[u8]) -> blake3::Hash {
    let mut hasher = blake3::Hasher::new_derive_key(FAKE_CONTEXT);
    if !mutant!("seams_fake_scheme_ignores_key") {
        hasher.update(&(key.len() as u64).to_le_bytes());
        hasher.update(key);
    }
    hasher.update(&(preimage.len() as u64).to_le_bytes());
    hasher.update(preimage);
    hasher.finalize()
}

/// A test double of the signature scheme: `FakeScheme::sign(key_id, preimage)` makes a 32-byte tag
/// and the verifier accepts exactly the tags it made, for the same key and preimage. The "public
/// key" of a fake key is the bytes of its `KeyId` ([`FakeScheme::public_key`]). It poses as
/// `SigAlg::Ed25519` and answers `Unsupported` for the other algorithms, like the real verifier.
///
/// It is NOT a signature scheme: the "signing key" is the public key, so anybody can forge a
/// signature. It exists so that the tests of the chain, the sync guard and the team log can sign
/// and tamper without real keys, and it must never be wired into a production composition.
///
/// Implements planning/architecture.md section 5.9 (signed heads and entries, tested without
/// key material) and section 12 E4 (restore announcements are tested with a fake map key and a
/// fake group key).
///
/// Caller: the tests of azlin-chain, S17c, S16a, S47a and S47c (a test double: no production
/// caller).
#[derive(Clone, Copy, Debug, Default)]
pub struct FakeScheme;

impl FakeScheme {
    /// The length of a fake signature, in bytes.
    pub const SIG_LEN: usize = FAKE_SIG_LEN;

    /// The fake "public key" of a key id: the id's own bytes.
    pub fn public_key(key_id: &KeyId) -> [u8; 32] {
        *key_id.as_bytes()
    }

    /// The fake signature of `preimage` by the key `key_id`.
    pub fn sign(key_id: &KeyId, preimage: &[u8]) -> Vec<u8> {
        fake_tag(key_id.as_bytes(), preimage).as_bytes().to_vec()
    }
}

impl SignatureVerifier for FakeScheme {
    fn verify(
        &self,
        alg: SigAlg,
        public_key: &[u8],
        preimage: &[u8],
        sig: &[u8],
    ) -> Result<(), VerifyError> {
        if alg != SigAlg::Ed25519 {
            return Err(VerifyError::Unsupported(alg));
        }
        if public_key.len() != KeyId::LEN {
            return Err(VerifyError::BadKey);
        }
        let tag = <[u8; FAKE_SIG_LEN]>::try_from(sig).map_err(|_| VerifyError::Malformed)?;
        // `blake3::Hash` compares in constant time.
        if fake_tag(public_key, preimage) == blake3::Hash::from(tag) {
            Ok(())
        } else {
            Err(VerifyError::BadSignature)
        }
    }
}
