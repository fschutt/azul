//! [`Secret`]: key material of a fixed size, held so that it never reaches a log, an error or a
//! second owner by accident (planning/architecture.md section 5.5 and section 1.4 rule 1; QUALITY
//! rule 8; build unit S15, lane W1-S15a).
//!
//! - No `Clone`, no `Copy`, no `Default`, no `PartialEq`, no `Display`, no serialization (the crate
//!   has no serde): a second copy is an explicit [`Secret::duplicate`], greppable, and two secrets
//!   are compared with [`Secret::ct_eq`] (constant time, `subtle`).
//! - `Debug` prints `Secret<32>(***)`, never a byte, so a derived `Debug` of a struct that holds
//!   one is safe to log.
//! - The bytes are wiped when the value is dropped (`zeroize`), and [`Zeroize::zeroize`] wipes
//!   them at once.
//! - [`Secret::expose`] is the one way to read the bytes, so every use of a secret is a grep away.
//!
//! The iso workspace has its own `azlin_proto::secret::Secret<T>` over `String` and `Vec<u8>`; the
//! azul crates cannot depend on it, so this is the fixed-size twin (drift row X-DELTA-W1S15A-3).
//!
//! ```
//! use azlin_keys::Secret;
//!
//! let a = Secret::<32>::from_bytes([7; 32]);
//! let b = a.duplicate(); // the explicit, greppable copy
//! assert!(a.ct_eq(&b));
//! assert_eq!(format!("{a:?}"), "Secret<32>(***)");
//! ```
//!
//! There is no `Clone`, `Copy`, `Default`, `PartialEq` or `Display`; each of these does not
//! compile:
//!
//! ```compile_fail
//! use azlin_keys::Secret;
//! let a = Secret::<32>::from_bytes([7; 32]);
//! let _b = a.clone();
//! ```
//!
//! ```compile_fail
//! use azlin_keys::Secret;
//! let a = Secret::<32>::from_bytes([7; 32]);
//! let _b = a;
//! let _c = a; // moved: it is not `Copy`
//! ```
//!
//! ```compile_fail
//! use azlin_keys::Secret;
//! let a = Secret::<32>::from_bytes([7; 32]);
//! let b = Secret::<32>::from_bytes([7; 32]);
//! assert!(a == b);
//! ```
//!
//! ```compile_fail
//! use azlin_keys::Secret;
//! let a = Secret::<32>::from_bytes([7; 32]);
//! let _text = format!("{a}");
//! ```
//!
//! ```compile_fail
//! use azlin_keys::Secret;
//! let _a = Secret::<32>::default();
//! ```

use core::fmt;

use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::EntropyError;
use crate::seam::Entropy;

/// `N` bytes of key material, wiped on drop. See the module documentation for what the type does
/// not implement and why.
///
/// Caller: every key, seed, shared secret and derived key of this crate and its dependents
/// (S15b-S15e, S14b-S14e, S9, S16, S17).
pub struct Secret<const N: usize>([u8; N]);

impl<const N: usize> Secret<N> {
    /// A secret holding these bytes. The caller's own array is not wiped by this call: prefer
    /// [`Secret::generate`], [`Secret::from_slice`] or [`Secret::expose_mut`] where the bytes are
    /// made, so no stray copy exists.
    ///
    /// Caller: key loading (INT-S15-1) and the lanes that derive or decode key bytes.
    pub fn from_bytes(bytes: [u8; N]) -> Secret<N> {
        Secret(bytes)
    }

    /// A secret from a slice of exactly `N` bytes; `None` for any other length.
    ///
    /// Caller: S15c1 (`SpaceKey::from_plaintext`) and S15d (a recombined code): key bytes that
    /// arrive as a decrypted buffer.
    pub fn from_slice(bytes: &[u8]) -> Option<Secret<N>> {
        match <[u8; N]>::try_from(bytes) {
            Ok(array) => Some(Secret(array)),
            Err(_) => None,
        }
    }

    /// `N` fresh random bytes from the entropy seam, written straight into the secret. On an
    /// entropy failure no secret exists and the error is returned: there is no secret of zeroes.
    ///
    /// Caller: every key generator (S15c1 `KeyBundle::generate`, `GroupKey::generate`,
    /// `SpaceKey::generate`; `SignKeypair::generate`).
    pub fn generate(ent: &mut dyn Entropy) -> Result<Secret<N>, EntropyError> {
        let mut secret = Secret([0u8; N]);
        ent.fill(&mut secret.0)?;
        Ok(secret)
    }

    /// The bytes, for the one place that needs them (a KDF, a cipher, a key file). Never pass them
    /// to a log, an error or a document.
    ///
    /// Caller: the crypto of every lane (S15b AEAD and ECDH, S15d KDFs, S14b chunk keys).
    pub fn expose(&self) -> &[u8; N] {
        &self.0
    }

    /// The bytes, to be written in place (the output of a KDF, a fill from an entropy source), so
    /// that no unwiped copy of the key is made on the way.
    ///
    /// Caller: S15d (Argon2id writes its output here) and the lanes that fill a key in place.
    pub fn expose_mut(&mut self) -> &mut [u8; N] {
        &mut self.0
    }

    /// Whether the two secrets are equal, in constant time: the time an unequal comparison takes
    /// says nothing of where the bytes differ.
    ///
    /// Caller: S15d (`RecoveryCode` checks), S15e (binding), S15c1 (key identity tests).
    pub fn ct_eq(&self, other: &Secret<N>) -> bool {
        bool::from(self.0.as_slice().ct_eq(other.0.as_slice()))
    }

    /// A second owner of the same bytes. The only way to copy a secret: every copy in the
    /// program is a call of this function.
    ///
    /// Caller: S15c1 (`KeyBundle::signer` hands a seed to a `SignKeypair`), S15b (a recipient
    /// secret derived from a bundle).
    pub fn duplicate(&self) -> Secret<N> {
        Secret(self.0)
    }
}

impl<const N: usize> Drop for Secret<N> {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<const N: usize> Zeroize for Secret<N> {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl<const N: usize> ZeroizeOnDrop for Secret<N> {}

/// Prints `Secret<N>(***)`: the size, never a byte.
impl<const N: usize> fmt::Debug for Secret<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret<{N}>(***)")
    }
}
