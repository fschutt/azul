//! The seam of the key holder (planning/architecture.md section 2.19: "Plaintext only in memory ... On
//! disk only sealed"; section 2.4: "The voters hold the identity key and the credential key, both
//! sealed to each voter through the Raft log"; section 1.4 rule 1: I/O is a parameter).
//!
//! A [`Signer`] is a key that can sign: a voter's memory, the payments machine, AzCtl's Keychain behind
//! a Touch ID prompt. The production holders are NOT here (S9f's `KeyHolder` and S9-int hold the keys in
//! locked memory; AzCtl's `KeySigner` is S45a's): this crate takes a `&dyn Signer` and never sees a
//! seed. The signer FRAMES: it takes the domain and the message and signs `domain || 0x00 || message`
//! through azlin-keys' `signing_input`, so a holder can show the domain on a prompt, and the caller can
//! never hand it a pre-framed input (no double framing; COMMON-W1R R4, drift row P-S9e-2). The domain is
//! a `&Domain`, a constant, never a string (the coordinator's note 0C).
//!
//! [`SeedSigner`] holds a seed in memory, for the tests and for the hosts that really hold it there (a
//! voter's credential key). It never prints it, and wipes it on drop.

use core::fmt;

use azlin_keys::sign::{sign, verify_preimage};
use azlin_keys::{Entropy, EntropyError, Secret, SignKeypair};
use azlin_regbase::{signing_input, Domain, PublicKey32, Signature64};

/// Why a key's holder gave no signature. The holder is a person at a prompt, a locked keychain or a
/// machine that may be down: its refusal is its own answer and never a default.
///
/// Caller: `Signer::sign`; the issuers and `certify` pass it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SignError {
    /// The holder refused: a prompt was declined, or the key may not sign this.
    #[error("the key's holder refused to sign")]
    Refused,
    /// The holder cannot be reached or the key is not loaded.
    #[error("the key's holder is unavailable")]
    Unavailable,
    /// The domain is not well formed (1 to 64 printable ASCII bytes, no NUL): signing under it would
    /// break the rule that two domains never give equal inputs.
    #[error("the signature domain is not well formed")]
    BadDomain,
}

/// A key that can sign: its public half and a signature over `domain || 0x00 || message`.
///
/// Implements planning/architecture.md section 2.4 and section 2.19 (who holds the keys).
///
/// Seam: S9f (`KeyHolder` opens the sealed keys of a voter and wraps them in a `SeedSigner`), S45a
/// (`KeySigner` for AzCtl's payments and operator keys), S13a (signs entitlements through
/// `sign_entitlement` at N24); `certify`, `sign_entitlement` and the issuers call it.
pub trait Signer {
    /// The public key of this signer.
    fn public(&self) -> PublicKey32;

    /// The signature of `message` under `domain`: Ed25519 over `signing_input(domain, message)`, or
    /// why the holder gave none.
    fn sign(&self, domain: &Domain, message: &[u8]) -> Result<Signature64, SignError>;
}

/// A signer that holds its seed in memory: the key pair of a 32-byte seed (RFC 8032), wiped on drop,
/// never printed.
///
/// Seam: S9f (a voter's credential key in locked memory) and every test of the crate and of the lanes
/// that build on it.
pub struct SeedSigner {
    key: SignKeypair,
}

impl SeedSigner {
    /// The signer of this seed. The caller's `Secret` stays the caller's.
    pub fn from_seed(seed: &Secret<32>) -> SeedSigner {
        SeedSigner {
            key: SignKeypair::from_seed(seed),
        }
    }

    /// A signer with a fresh key: the seed comes from the entropy seam; on a failure there is no key.
    ///
    /// Caller: S9f's rotation (the leader makes the next credential key).
    pub fn generate(ent: &mut dyn Entropy) -> Result<SeedSigner, EntropyError> {
        Ok(SeedSigner {
            key: SignKeypair::generate(ent)?,
        })
    }
}

impl Signer for SeedSigner {
    fn public(&self) -> PublicKey32 {
        self.key.verify_key()
    }

    fn sign(&self, domain: &Domain, message: &[u8]) -> Result<Signature64, SignError> {
        if !domain.is_wellformed() {
            return Err(SignError::BadDomain);
        }
        Ok(sign(domain, message, &self.key))
    }
}

/// Prints `SeedSigner(<redacted>)`: not the seed, not the public key.
impl fmt::Debug for SeedSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SeedSigner(<redacted>)")
    }
}

/// Why a checked signature was not made (crate-internal: each caller maps it to its own error).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SignFailure {
    /// The holder gave no signature.
    Sign(SignError),
    /// The holder's signature does not verify under its own public key.
    Fault,
}

/// A signature of `payload` under `domain` that has been CHECKED against the signer's own public key: a
/// holder that returns a signature which does not verify (a failing module, a key that is not the one
/// it names) is caught here and never signs anything that leaves (planning/architecture.md section 1.4
/// rule 11: invariants are asserted in production; one verification at issue time).
pub(crate) fn sign_checked(
    signer: &dyn Signer,
    domain: &Domain,
    payload: &[u8],
) -> Result<Signature64, SignFailure> {
    let sig = signer.sign(domain, payload).map_err(SignFailure::Sign)?;
    let preimage = signing_input(domain, payload);
    if !azlin_seams::mutant!("creds_signer_unchecked") {
        verify_preimage(&preimage, &signer.public(), &sig).map_err(|_| SignFailure::Fault)?;
    }
    Ok(sig)
}
