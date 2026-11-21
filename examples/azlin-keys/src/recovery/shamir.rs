//! Shamir's secret sharing over GF(256) for the recovery code: a 16-byte secret split into `n`
//! shares of which any `k` give it back and fewer say nothing about it (planning/architecture.md
//! section 5.5 "Recovery": "its 2-of-3 Shamir shares to contacts"; build unit S15, lane W1-S15d).
//!
//! Each byte of the secret is the constant term of its own random polynomial of degree `k - 1`; a
//! share is the polynomials' values at its index `x` (1 to 255; 0 would be the secret). The field is
//! the AES one (FIPS-197 section 4: the polynomial x^8 + x^4 + x^3 + x + 1, `0x11B`); its products are
//! made bit by bit with masks and its inverses as `a^254`: no table indexed by a secret, no branch
//! on one. The coefficients come from the entropy seam in ONE draw of `16 (k - 1)` bytes, k - 1 for
//! each byte of the secret in turn, the lowest power first: the layout of today's `crypto/shamir.rs`
//! (the known answers of its tests pass through this function).
//!
//! WHAT IS SPLIT is the recovery code, never the member key: the shares recombine to the code and
//! the code opens the algorithm 3 wrap (the chain shares to code to member key). Every share
//! carries the code's four-byte check value as its split id, and `combine` checks the recombination
//! against it (DRIFT X-DELTA-W1S15D-8): a mistyped or changed share is an error, never a wrong
//! code. This is the same algorithm as iso's `azlin-proto/src/shamir.rs` (the release key's
//! shares); the two copies are not merged here (DRIFT X-DELTA-W1S15D-4).

use core::fmt;

use zeroize::Zeroizing;

use super::code::{check_value_of, CHECK_LEN, CODE_LEN};
use super::error::RecoveryError;
use crate::envelope::Params;
use crate::seam::Entropy;
use crate::secret::Secret;

/// The most shares of one split: the index is a byte other than 0.
///
/// Caller: `split`.
pub const MAX_SHARES: u8 = 255;

/// The threshold of the default split: any 2 shares give the code back.
///
/// Caller: INT-S15-1 (the contacts' shares of section 5.5).
pub const DEFAULT_THRESHOLD: u8 = 2;

/// The number of shares of the default split: 3, one for each trusted contact.
///
/// Caller: INT-S15-1.
pub const DEFAULT_SHARES: u8 = 3;

/// One share of a split of a recovery code: its index `x`, the threshold `k` and the count `n` of
/// its split, the polynomials' values `y` there, and the split id (the code's check value). Wiped
/// when dropped, never printed, not `Clone`: a second copy is [`Share::duplicate`].
///
/// DRIFT X-DELTA-W1S15D-7: the brief's `Share` has `x`, `y` and `split_id`; `combine` refuses
/// "fewer than k" and the envelope's `params` are `{x, k, n, split_id}`, so a share carries `k`
/// and `n` too.
///
/// Caller: INT-S15-1 (the setup screen splits, the recovery screen combines), `seal_share_to_contact`,
/// `open_share`, the printed share text.
pub struct Share {
    /// The index of the share, 1 to 255 (never 0).
    pub x: u8,
    /// The threshold: how many shares give the code back (2 to `n`).
    pub k: u8,
    /// The number of shares of the split.
    pub n: u8,
    /// The polynomials' values at `x`, one byte for each byte of the code.
    pub y: Secret<CODE_LEN>,
    /// The split id: the four-byte check value of the code that was split.
    pub split_id: [u8; CHECK_LEN],
}

impl Share {
    /// A second owner of the same share. The only way to copy one: every copy in the program is a
    /// call of this function.
    ///
    /// Caller: INT-S15-1 (a recovery screen that keeps shares while it tries combinations).
    pub fn duplicate(&self) -> Share {
        Share {
            x: self.x,
            k: self.k,
            n: self.n,
            y: self.y.duplicate(),
            split_id: self.split_id,
        }
    }

    /// The envelope `params` of this share (algorithm 5: `{x, k, n, split_id}`).
    pub(crate) fn params(&self) -> Params {
        Params::Shamir {
            x: self.x,
            k: self.k,
            n: self.n,
            split_id: self.split_id,
        }
    }

    /// Whether this share can come from a split: an index from 1 to `n`, a threshold from 2 to `n`.
    pub(crate) fn check(&self) -> Result<(), RecoveryError> {
        if self.x == 0 {
            return Err(RecoveryError::ShareIndex);
        }
        if self.k < 2 || self.k > self.n || self.x > self.n {
            return Err(RecoveryError::BadShare);
        }
        Ok(())
    }
}

/// Prints the index and the split, never a byte of the share.
impl fmt::Debug for Share {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Share(x = {}, {} of {}, split ", self.x, self.k, self.n)?;
        for byte in &self.split_id {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(", y ***)")
    }
}

/// The product of `a` and `b` in GF(256) (the AES polynomial), without a branch and without a
/// table indexed by either.
///
/// Caller: `split`, `combine`, `gf_inv`; the test against the FIPS-197 table.
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

/// The inverse of `a` in GF(256): `a^254` (0 for 0), square and multiply over the public exponent.
///
/// Caller: `combine` (the Lagrange weights), the test against the FIPS-197 table.
pub fn gf_inv(a: u8) -> u8 {
    let mut result = 1u8;
    let mut power = a;
    for bit in 0..8u32 {
        if (254u32 >> bit) & 1 == 1 {
            result = gf_mul(result, power);
        }
        power = gf_mul(power, power);
    }
    result
}

/// Splits `secret` (a recovery code's 16 bytes) into `n` shares of which any `k` give it back.
/// `2 <= k <= n <= 255`, else `BadSplit`. The coefficients are ONE draw of `16 (k - 1)` bytes from
/// `ent`, the lowest power first for each byte of the secret in turn; on an entropy failure no
/// share exists. Shares are at `x = 1, 2, ... n`.
///
/// Caller: INT-S15-1 (account setup: `split(code.as_secret(), DEFAULT_THRESHOLD, DEFAULT_SHARES,
/// ent)`; a new split whenever the contacts change).
pub fn split(
    secret: &Secret<CODE_LEN>,
    k: u8,
    n: u8,
    ent: &mut dyn Entropy,
) -> Result<Vec<Share>, RecoveryError> {
    if k < 2 || k > n {
        return Err(RecoveryError::BadSplit);
    }
    let degree = usize::from(k - 1);
    let mut coefficients = Zeroizing::new(vec![0u8; CODE_LEN * degree]);
    ent.fill(coefficients.as_mut_slice())?;
    let split_id = check_value_of(secret);
    let mut shares = Vec::with_capacity(usize::from(n));
    for x in 1..=n {
        let mut y = Secret::<CODE_LEN>::from_bytes([0u8; CODE_LEN]);
        for ((out, constant), own) in y
            .expose_mut()
            .iter_mut()
            .zip(secret.expose().iter())
            .zip(coefficients.chunks_exact(degree))
        {
            // Horner's rule from the highest power down to the first, then one more factor x.
            let tail = own.iter().rev().fold(0u8, |acc, &c| gf_mul(acc, x) ^ c);
            *out = gf_mul(tail, x) ^ *constant;
        }
        shares.push(Share {
            x,
            k,
            n,
            y,
            split_id,
        });
    }
    Ok(shares)
}

/// The secret of `shares` by Lagrange's interpolation at 0 over every share given. The shares must
/// be well formed, of one split and at distinct indices: the caller (`combine`) checks.
pub(crate) fn interpolate(shares: &[Share]) -> Secret<CODE_LEN> {
    let mut secret = Secret::<CODE_LEN>::from_bytes([0u8; CODE_LEN]);
    for share in shares {
        // The Lagrange basis polynomial of this share at 0: the product of x_j / (x_i + x_j).
        let (numerator, denominator) = shares
            .iter()
            .filter(|other| other.x != share.x)
            .fold((1u8, 1u8), |(n, d), other| {
                (gf_mul(n, other.x), gf_mul(d, share.x ^ other.x))
            });
        let weight = gf_mul(numerator, gf_inv(denominator));
        for (out, y) in secret.expose_mut().iter_mut().zip(share.y.expose().iter()) {
            *out ^= gf_mul(*y, weight);
        }
    }
    secret
}

/// The secret of `shares` (the shares of ONE split, at least its threshold of them): Lagrange's
/// interpolation at 0, checked against the split id.
///
/// The checks run in this order, each its own error: no share at all (`NotEnough`); a share that
/// cannot come from a split (`ShareIndex` for index 0, `BadShare`); shares of different splits
/// (`MixedSplits`: another split id, threshold or count); the same index twice (`DuplicateShare`);
/// fewer shares than the threshold (`NotEnough`: never a wrong secret); and last, that the
/// recombination has the check value the shares carry (`CheckMismatch`: a mistyped or changed share
/// is detected here, before an Argon2id derivation is paid for). All the shares given are
/// interpolated: when a recombination fails and more shares than the threshold were given, try
/// other subsets.
///
/// Caller: INT-S15-1 (the recovery screen: two contacts' answers or printed shares give the code).
pub fn combine(shares: &[Share]) -> Result<Secret<CODE_LEN>, RecoveryError> {
    let Some(first) = shares.first() else {
        return Err(RecoveryError::NotEnough);
    };
    for share in shares {
        share.check()?;
    }
    if shares
        .iter()
        .any(|share| share.split_id != first.split_id || share.k != first.k || share.n != first.n)
    {
        return Err(RecoveryError::MixedSplits);
    }
    let mut seen = [false; 256];
    for share in shares {
        let slot = seen
            .get_mut(usize::from(share.x))
            .ok_or(RecoveryError::BadShare)?;
        if *slot {
            return Err(RecoveryError::DuplicateShare);
        }
        *slot = true;
    }
    if shares.len() < usize::from(first.k) {
        return Err(RecoveryError::NotEnough);
    }
    let secret = interpolate(shares);
    if check_value_of(&secret) != first.split_id {
        return Err(RecoveryError::CheckMismatch);
    }
    Ok(secret)
}
