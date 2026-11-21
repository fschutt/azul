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
pub fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut product = 0u8;
    for _ in 0..8 {
        product ^= a & 0u8.wrapping_sub(b & 1);
        let carry = 0u8.wrapping_sub(a >> 7);
        a = (a << 1) ^ (0x1B & carry);
        b >>= 1;
    }
    product
}

/// The inverse of `a` in GF(256): `a^254` (0 for 0).
#[must_use]
pub fn gf_inv(a: u8) -> u8 {
    // 254 = 0b1111_1110: square and multiply over a public exponent.
    let mut result = 1u8;
    let mut power = a;
    for bit in 0..8 {
        if (254u32 >> bit) & 1 == 1 {
            result = gf_mul(result, power);
        }
        power = gf_mul(power, power);
    }
    result
}

fn check_split(threshold: u8, count: u8) -> Result<(), CryptoError> {
    if threshold < 2 || threshold > count {
        return Err(CryptoError::Unsupported(format!(
            "a split of {threshold} of {count} shares (2 to {MAX_SHARES}, the threshold at most \
             the count)"
        )));
    }
    Ok(())
}

/// `secret` split into `count` shares of which any `threshold` give it back, the coefficients
/// from the OS random source.
pub fn split(secret: &[u8], threshold: u8, count: u8) -> Result<Vec<Share>, CryptoError> {
    split_with(secret, threshold, count, &mut |buf: &mut [u8]| {
        random_bytes(buf)
    })
}

/// [`split`] with the coefficients from `random`: it fills one buffer, `threshold - 1`
/// coefficients for each byte of the secret in turn (the lowest power first).
pub fn split_with(
    secret: &[u8],
    threshold: u8,
    count: u8,
    random: &mut dyn FnMut(&mut [u8]) -> Result<(), CryptoError>,
) -> Result<Vec<Share>, CryptoError> {
    check_split(threshold, count)?;
    let degree = usize::from(threshold - 1);
    let mut coefficients = Zeroizing::new(vec![0u8; secret.len() * degree]);
    random(&mut coefficients[..])?;
    let shares = (1..=count)
        .map(|x| {
            let y = secret
                .iter()
                .enumerate()
                .map(|(i, &constant)| {
                    // Horner's rule from the highest power down to the constant term.
                    let own = &coefficients[i * degree..(i + 1) * degree];
                    let tail = own.iter().rev().fold(0u8, |acc, &c| gf_mul(acc, x) ^ c);
                    gf_mul(tail, x) ^ constant
                })
                .collect::<Vec<u8>>();
            Share {
                x,
                y: Zeroizing::new(y),
            }
        })
        .collect();
    Ok(shares)
}

/// The secret of `shares` (at least the split's threshold of them, of one split): Lagrange's
/// interpolation at 0 over every share given. `Damaged` for no share, a share at 0, two at the
/// same `x` or shares of different lengths. Fewer shares than the threshold give some other
/// bytes: whoever combines checks the result (AzDrive: against the split's id).
pub fn combine(shares: &[Share]) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    let Some(first) = shares.first() else {
        return Err(CryptoError::Damaged(String::from("no share")));
    };
    let len = first.y.len();
    for (i, share) in shares.iter().enumerate() {
        if share.x == 0 {
            return Err(CryptoError::Damaged(String::from("a share at x = 0")));
        }
        if share.y.len() != len {
            return Err(CryptoError::Damaged(String::from(
                "shares of different lengths",
            )));
        }
        if shares[..i].iter().any(|other| other.x == share.x) {
            return Err(CryptoError::Damaged(String::from("the same share twice")));
        }
    }
    // Each share's Lagrange basis polynomial at 0: the product of x_j / (x_i + x_j).
    let weights: Vec<u8> = shares
        .iter()
        .map(|share| {
            let (numerator, denominator) = shares
                .iter()
                .filter(|other| other.x != share.x)
                .fold((1u8, 1u8), |(n, d), other| {
                    (gf_mul(n, other.x), gf_mul(d, share.x ^ other.x))
                });
            gf_mul(numerator, gf_inv(denominator))
        })
        .collect();
    let mut secret = Zeroizing::new(vec![0u8; len]);
    for (share, &weight) in shares.iter().zip(&weights) {
        for (out, &y) in secret.iter_mut().zip(share.y.iter()) {
            *out ^= gf_mul(y, weight);
        }
    }
    Ok(secret)
}
