//! Recipients: the public side (`Recipient`, the P-6 choice of algorithm) and the secret side
//! (`RecipientSecret`, what opens an envelope) (planning/architecture.md section 5.5; build unit
//! S15, lane W1-S15b).
//!
//! A recipient has an X25519 key and, when it has been generated with one, an ML-KEM-768 key.
//! The `rcpt` of an envelope names the keys it was sealed to (`KeyId::of_bundle`), so a
//! recipient with both keys has TWO ids: the id of `(kx)` for envelopes of algorithm 1 and the
//! id of `(kx, pq)` for envelopes of algorithm 2, and `open` accepts each only for its own
//! algorithm (DRIFT X-DELTA-W1S15A-1).

use core::fmt;

use super::alg_table::Alg;
use super::ctx::Ctx;
use super::error::WrapError;
use super::hybrid::{public_key_of_seed, wrap_hybrid, RecipientHybrid};
use super::wire::Envelope;
use super::x25519::{public_of, wrap_x25519, RecipientX25519};
use crate::ids::{DeviceId, KeyId};
use crate::seam::Entropy;
use crate::secret::Secret;
use crate::sign::SignKeypair;

/// The public keys an envelope can be sealed to: an X25519 key, or an X25519 key with an
/// ML-KEM-768 key.
///
/// Caller: S15c1 (`PublicBundle::recipient`), INT-S15-1.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Recipient {
    /// Only an X25519 key: algorithm 1.
    X25519(RecipientX25519),
    /// An X25519 key and an ML-KEM-768 key: algorithm 2.
    Hybrid(RecipientHybrid),
}

impl Recipient {
    /// The recipient with these public keys: hybrid when there is an ML-KEM key (`Some` of an
    /// empty key is none), X25519 only otherwise (P-6).
    pub fn from_public(kx: [u8; 32], pq: Option<Vec<u8>>) -> Recipient {
        match pq {
            Some(pq) if !pq.is_empty() => Recipient::Hybrid(RecipientHybrid { kx, pq }),
            _ => Recipient::X25519(RecipientX25519 { kx }),
        }
    }

    /// The `rcpt` of an envelope sealed to this recipient.
    pub fn key_id(&self) -> KeyId {
        match self {
            Recipient::X25519(r) => r.key_id(),
            Recipient::Hybrid(r) => r.key_id(),
        }
    }

    /// Wraps `plaintext` to this recipient with the algorithm it supports: `wrap_hybrid`
    /// whenever the recipient has an ML-KEM key, `wrap_x25519` otherwise (P-6). In a build
    /// without the feature `pq` a hybrid recipient is `Unsupported`: there is no silent
    /// downgrade to algorithm 1. A policy that requires post-quantum wraps is the caller's flag,
    /// not the format's.
    ///
    /// Caller: S15c1 (the wraps of member, group and space keys), INT-S15-1.
    pub fn wrap(
        &self,
        ctx: &Ctx,
        plaintext: &[u8],
        signer: &DeviceId,
        key: &SignKeypair,
        ent: &mut dyn Entropy,
    ) -> Result<Envelope, WrapError> {
        match self {
            Recipient::X25519(r) => wrap_x25519(r, ctx, plaintext, signer, key, ent),
            Recipient::Hybrid(r) => wrap_hybrid(r, ctx, plaintext, signer, key, ent),
        }
    }
}

/// What opens an envelope: the recipient's X25519 secret and, when it has one, the seed
/// `(d, z)` of its ML-KEM-768 decapsulation key. Everything secret is a `Secret` (wiped on drop,
/// never printed); the type is not `Clone` and its `Debug` shows nothing.
///
/// The public keys are computed once, when the secret is made. In a build without the feature
/// `pq` the ML-KEM public key cannot be computed, so [`RecipientSecret::pq_public`] is `None` and
/// the recipient opens envelopes of algorithm 1 only.
///
/// A second owner of the secret is made only by building another `RecipientSecret` from the
/// same `Secret`s; the type itself cannot be copied:
///
/// ```compile_fail
/// use azlin_keys::envelope::RecipientSecret;
/// fn needs_clone<T: Clone>() {}
/// needs_clone::<RecipientSecret>();
/// ```
///
/// Caller: S15c1 (`KeyBundle::recipient_secret`), S15d (`open_share`), INT-S15-1.
pub struct RecipientSecret {
    kx: Secret<32>,
    kx_public: [u8; 32],
    pq_seed: Option<Secret<64>>,
    pq_public: Option<Vec<u8>>,
}

impl RecipientSecret {
    /// The recipient secret of an X25519 secret and, optionally, a 64-byte ML-KEM-768 seed
    /// (`d` then `z`, FIPS 203's `d || z`). The caller keeps its own secrets; this makes copies.
    pub fn new(kx: &Secret<32>, pq_seed: Option<&Secret<64>>) -> RecipientSecret {
        let kx = kx.duplicate();
        let kx_public = public_of(&kx);
        let pq_seed = pq_seed.map(|seed| seed.duplicate());
        let pq_public = pq_seed.as_ref().and_then(public_key_of_seed);
        RecipientSecret {
            kx,
            kx_public,
            pq_seed,
            pq_public,
        }
    }

    /// The X25519 public key (the `kx_pub` column).
    pub fn kx_public(&self) -> &[u8; 32] {
        &self.kx_public
    }

    /// The ML-KEM-768 encapsulation key (the `pq_pub` column), 1,184 bytes; `None` when the
    /// recipient has no ML-KEM seed or the build has no feature `pq`.
    pub fn pq_public(&self) -> Option<&[u8]> {
        self.pq_public.as_deref()
    }

    /// The `rcpt` of envelopes of algorithm 1 to this recipient: the id of `(kx)`.
    pub fn x25519_id(&self) -> KeyId {
        KeyId::of_bundle(&self.kx_public, None)
    }

    /// The `rcpt` of envelopes of algorithm 2 to this recipient: the id of `(kx, pq)`; `None`
    /// when the recipient has no ML-KEM key.
    pub fn hybrid_id(&self) -> Option<KeyId> {
        self.pq_public
            .as_deref()
            .map(|pq| KeyId::of_bundle(&self.kx_public, Some(pq)))
    }

    /// The public side for algorithm 1.
    pub fn recipient_x25519(&self) -> RecipientX25519 {
        RecipientX25519 { kx: self.kx_public }
    }

    /// The public side for algorithm 2; `None` when the recipient has no ML-KEM key.
    pub fn recipient_hybrid(&self) -> Option<RecipientHybrid> {
        self.pq_public.as_ref().map(|pq| RecipientHybrid {
            kx: self.kx_public,
            pq: pq.clone(),
        })
    }

    /// The public side the P-6 default wraps to: hybrid when there is an ML-KEM key.
    pub fn recipient(&self) -> Recipient {
        Recipient::from_public(self.kx_public, self.pq_public.clone())
    }

    /// The `rcpt` this recipient has for an envelope of `alg`; `None` for an algorithm this
    /// recipient does not open.
    pub(crate) fn id_for(&self, alg: Alg) -> Option<KeyId> {
        match alg {
            Alg::X25519SealedBox => Some(self.x25519_id()),
            Alg::X25519MlKem768 => self.hybrid_id(),
            _ => None,
        }
    }

    /// The X25519 secret.
    pub(crate) fn kx(&self) -> &Secret<32> {
        &self.kx
    }

    /// The ML-KEM seed, when the recipient has one (only the hybrid open reads it).
    #[cfg(feature = "pq")]
    pub(crate) fn pq_seed(&self) -> Option<&Secret<64>> {
        self.pq_seed.as_ref()
    }

    /// Whether the recipient was given an ML-KEM seed, whether or not this build (without the
    /// feature `pq`) can use it: a recipient with a seed that cannot compute its public key is
    /// not the same as a recipient that never had one.
    pub fn has_ml_kem_seed(&self) -> bool {
        self.pq_seed.is_some()
    }
}

/// Prints `RecipientSecret(***)`: no key, no seed, not even a public key.
impl fmt::Debug for RecipientSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecipientSecret(***)")
    }
}
