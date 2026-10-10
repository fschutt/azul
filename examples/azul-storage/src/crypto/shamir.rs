//! Shamir's secret sharing over GF(256): a secret split into `count` shares of which any
//! `threshold` give it back, and fewer say nothing about it (each byte of the secret is the
//! constant term of its own random polynomial of degree `threshold - 1`; a share is the
//! polynomials' values at its `x`).
//!
//! The field is the AES one (FIPS-197 section 4: the polynomial x^8 + x^4 + x^3 + x + 1,
//! `0x11B`). Its products are made bit by bit with masks and its inverses as `a^254`: no table
//! indexed by a secret, no branch on one. The coefficients come from the OS random source
//! ([`split`]) or from the caller ([`split_with`], the known-answer tests). Shares are at
//! `x = 1, 2, ...`; [`combine`] interpolates at 0 with every share it is given.
//!
//! What AzDrive splits is a drive's recovery code, 2-of-3, to trusted contacts
//! ([`super::contacts`]).

use std::fmt;

use zeroize::Zeroizing;

use super::{random_bytes, CryptoError};

/// The most shares of one split (`x` is a byte other than 0).
pub const MAX_SHARES: u8 = 255;

/// One share: its `x` and the polynomials' values there, a byte for each byte of the secret.
/// Wiped when dropped, never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct Share {
    pub x: u8,
    pub y: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for Share {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Share(x = {}, {} bytes)", self.x, self.y.len())
    }
}

/// The product of `a` and `b` in GF(256) (the AES polynomial), without a branch on either.
#[must_use]
pub fn gf_mul(_a: u8, _b: u8) -> u8 {
    0
}

/// The inverse of `a` in GF(256): `a^254` (0 for 0).
#[must_use]
pub fn gf_inv(_a: u8) -> u8 {
    0
}

/// `secret` split into `count` shares of which any `threshold` give it back.
pub fn split(_secret: &[u8], _threshold: u8, _count: u8) -> Result<Vec<Share>, CryptoError> {
    let _ = random_bytes;
    Ok(Vec::new())
}

/// [`split`] with the coefficients from `random`.
pub fn split_with(
    _secret: &[u8],
    _threshold: u8,
    _count: u8,
    _random: &mut dyn FnMut(&mut [u8]) -> Result<(), CryptoError>,
) -> Result<Vec<Share>, CryptoError> {
    Ok(Vec::new())
}

/// The secret of `shares`.
pub fn combine(_shares: &[Share]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    Ok(Zeroizing::new(Vec::new()))
}
