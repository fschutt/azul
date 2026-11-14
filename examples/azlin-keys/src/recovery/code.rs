//! The recovery code: 128 random bits, written down as 26 Crockford base32 characters in five
//! groups (planning/architecture.md section 5.5 "Recovery", section 7.4 "The paper key"; build
//! unit S15, lane W1-S15d).
//!
//! The text is the 128 bits followed by two zero padding bits, five bits per symbol, the alphabet
//! `0123456789ABCDEFGHJKMNPQRSTVWXYZ` (no `I`, `L`, `O`, `U`), shown as `XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX`
//! (groups of 5, 5, 5, 5 and 6). A person who types it back may use any case, spaces or hyphens
//! between the symbols, `O` for `0` and `I` or `L` for `1`; the two padding bits must be zero, so a
//! typo in the last symbol is more often caught than not.
//!
//! The text of a secret is made and read WITHOUT a table indexed by the secret and without a
//! branch on it: a symbol is computed from its value by arithmetic (`symbol`), and a typed symbol
//! is found by comparing it with all 32 symbols in constant time (`decode_symbol`). Every buffer
//! that holds the code or part of it is wiped.
//!
//! `check_value` is a four-byte fingerprint of the code, the same one every Shamir share of it
//! carries as its split id (DRIFT X-DELTA-W1S15D-8): it detects a wrong recombination of shares
//! before an Argon2id derivation is paid for.

use core::fmt;

use subtle::{Choice, ConditionallySelectable, ConstantTimeEq};
use zeroize::{Zeroize, Zeroizing};

use super::error::RecoveryError;
use crate::derive::{derive_public, RECOVERY_CHECK};
use crate::seam::Entropy;
use crate::secret::Secret;

/// The bytes of a recovery code: 128 bits.
///
/// Caller: `RecoveryCode`, `split` (the secret of a Shamir split), the tests.
pub const CODE_LEN: usize = 16;

/// The symbols of a recovery code's text: 26 (130 bits: the code's 128 and two zero padding bits).
///
/// Caller: `RecoveryCode::parse`, `RecoveryCode::to_text`, the quiz.
pub const CODE_SYMBOLS: usize = 26;

/// The lengths of the five groups of the text, in order: 5, 5, 5, 5 and 6 symbols (a 26-symbol code
/// does not divide into equal groups: DRIFT X-DELTA-W1S15D-2).
///
/// Caller: `RecoveryCode::to_text`, the quiz (a group is the unit it asks for).
pub const GROUP_LENGTHS: [usize; 5] = [5, 5, 5, 5, 6];

/// The bytes of a check value.
///
/// Caller: `RecoveryCode::check_value`, `Share::split_id`.
pub const CHECK_LEN: usize = 4;

/// The Crockford symbol of a 5-bit value, by arithmetic: digits for 0 to 9, then the letters A to
/// Z without `I` (skipped after `H`), `L` (after `K`), `O` (after `N`) and `U` (after `T`). No table
/// is indexed by the value and no branch depends on it.
pub(crate) fn symbol(value: u8) -> u8 {
    let v = value & 31;
    let letter = u8::from(v >= 10);
    let skipped = u8::from(v >= 18) + u8::from(v >= 20) + u8::from(v >= 22) + u8::from(v >= 27);
    b'0' + 7 * letter + v + skipped
}

/// The value (0 to 31) of a typed symbol, or `None` for a character that is no symbol: any case,
/// `O` as 0, `I` and `L` as 1. The symbol is compared with all 32 symbols in constant time, so the
/// time says nothing of which one it was.
pub(crate) fn decode_symbol(c: char) -> Option<u8> {
    let Ok(byte) = u8::try_from(c) else {
        return None;
    };
    let upper = byte.to_ascii_uppercase();
    let is_o = upper.ct_eq(&b'O');
    let is_i_or_l = upper.ct_eq(&b'I') | upper.ct_eq(&b'L');
    let mut canonical = u8::conditional_select(&upper, &b'0', is_o);
    canonical = u8::conditional_select(&canonical, &b'1', is_i_or_l);
    let mut value = 0u8;
    let mut found = Choice::from(0u8);
    for candidate in 0u8..32 {
        let hit = canonical.ct_eq(&symbol(candidate));
        value = u8::conditional_select(&value, &candidate, hit);
        found |= hit;
    }
    if bool::from(found) {
        Some(value)
    } else {
        None
    }
}

/// The canonical symbol of a typed character (`symbol(value)`), or the byte 0 for a character that
/// is no symbol (which no code holds). The quiz compares typed answers with it.
///
/// Caller: the quiz (answers are normalised like `parse` reads them).
pub(crate) fn normalise_char(c: char) -> u8 {
    match decode_symbol(c) {
        Some(value) => symbol(value),
        None => 0,
    }
}

/// The 26 canonical symbols of `secret` as ASCII bytes (no hyphens): the first 25 are the top 125
/// bits, the 26th the last three bits followed by the two zero padding bits.
///
/// Caller: `RecoveryCode::to_text`, the quiz, the printed share text.
pub(crate) fn symbols_of(secret: &Secret<CODE_LEN>) -> Zeroizing<[u8; CODE_SYMBOLS]> {
    let mut number = u128::from_be_bytes(*secret.expose());
    let mut out = Zeroizing::new([0u8; CODE_SYMBOLS]);
    for (index, slot) in out.iter_mut().enumerate() {
        let value = if index < CODE_SYMBOLS - 1 {
            // Symbol `index` is bits 127 - 5 index down to 123 - 5 index of the code.
            let shift = 123 - 5 * index;
            ((number >> shift) & 31) as u8
        } else {
            ((number & 7) << 2) as u8
        };
        *slot = symbol(value);
    }
    number.zeroize();
    out
}

/// The slice of one group (1 to 5, as the quiz numbers them) of 26 symbols.
///
/// Caller: the quiz, `RecoveryCode::to_text`.
pub(crate) fn group_of(symbols: &[u8], position: u8) -> Option<&[u8]> {
    let index = usize::from(position).checked_sub(1)?;
    let length = *GROUP_LENGTHS.get(index)?;
    let start: usize = GROUP_LENGTHS.iter().take(index).sum();
    symbols.get(start..start + length)
}

/// The five groups of `symbols` joined by hyphens, in a buffer that is wiped when dropped.
pub(crate) fn grouped_text(symbols: &[u8; CODE_SYMBOLS]) -> Zeroizing<String> {
    let mut text = Zeroizing::new(String::with_capacity(CODE_SYMBOLS + GROUP_LENGTHS.len()));
    for position in 1..=5u8 {
        if position > 1 {
            text.push('-');
        }
        if let Some(group) = group_of(symbols.as_slice(), position) {
            for byte in group {
                text.push(char::from(*byte));
            }
        }
    }
    text
}

/// The four-byte check value of 16 secret bytes: the first four bytes of BLAKE3 `derive_key` under
/// the context `azlin 2026-11 recovery code check v1` (the one-way commitment that
/// `derive_public` is for). The ONE definition: `RecoveryCode::check_value` and the split id of
/// every share are this function.
///
/// Caller: `RecoveryCode::check_value`, `split`, `combine`.
pub(crate) fn check_value_of(secret: &Secret<CODE_LEN>) -> [u8; CHECK_LEN] {
    let derived = Zeroizing::new(derive_public(&RECOVERY_CHECK, secret.expose()));
    let mut out = [0u8; CHECK_LEN];
    for (slot, byte) in out.iter_mut().zip(derived.iter()) {
        *slot = *byte;
    }
    out
}

/// A recovery code: 128 random bits, wiped when dropped, never printed, never copied by accident
/// (no `Clone`, no `Copy`, no `Default`, no `PartialEq`: compare with [`RecoveryCode::ct_eq`]).
/// Shown once as [`RecoveryCode::to_text`] (the paper key, planning/architecture.md section 7.4) and
/// typed back with [`RecoveryCode::parse`].
///
/// Caller: INT-S15-1 (the setup screen, the paper key PDF, the recovery screen), `wrap_with_code`,
/// `unwrap_with_code`, `split`, the quiz.
pub struct RecoveryCode(Secret<CODE_LEN>);

impl RecoveryCode {
    /// A new code: 16 bytes from the entropy seam. On an entropy failure there is no code.
    ///
    /// Caller: INT-S15-1 (account setup, and whenever the recovery code changes).
    pub fn generate(ent: &mut dyn Entropy) -> Result<RecoveryCode, RecoveryError> {
        Ok(RecoveryCode(Secret::<CODE_LEN>::generate(ent)?))
    }

    /// The code of these 16 bytes (a recombination of Shamir shares).
    ///
    /// Caller: INT-S15-1 (the recovery screen after `combine`).
    pub fn from_secret(secret: Secret<CODE_LEN>) -> RecoveryCode {
        RecoveryCode(secret)
    }

    /// The 16 bytes of the code, as the secret `split` takes.
    ///
    /// Caller: INT-S15-1 (`split(code.as_secret(), 2, 3, ent)` for the contacts' shares).
    pub fn as_secret(&self) -> &Secret<CODE_LEN> {
        &self.0
    }

    /// Whether the two codes are equal, in constant time.
    ///
    /// Caller: the tests, INT-S15-1.
    pub fn ct_eq(&self, other: &RecoveryCode) -> bool {
        self.0.ct_eq(&other.0)
    }

    /// The four-byte fingerprint that every share of this code carries as its split id, and that a
    /// recombination is checked against: BLAKE3 `derive_key` under the context
    /// `azlin 2026-11 recovery code check v1`, first four bytes (DRIFT X-DELTA-W1S15D-8: the brief's
    /// "keyed-BLAKE3 fingerprint").
    ///
    /// Caller: `split` (the split id), `combine`, INT-S15-1.
    pub fn check_value(&self) -> [u8; CHECK_LEN] {
        check_value_of(&self.0)
    }

    /// The text: 26 Crockford base32 characters in five groups of 5, 5, 5, 5 and 6, joined by
    /// hyphens, in a buffer that is wiped when dropped.
    ///
    /// Caller: INT-S15-1 (the paper key PDF's text, the setup screen).
    pub fn to_text(&self) -> Zeroizing<String> {
        grouped_text(&symbols_of(&self.0))
    }

    /// The code of a text as a person types it back: any case, spaces and hyphens anywhere between
    /// the symbols, `O` for `0`, `I` and `L` for `1`. Exactly 26 symbols whose last two bits are
    /// zero; `U` and every other character is refused.
    ///
    /// Caller: INT-S15-1 (the recovery screen, the quiz), the printed share text.
    pub fn parse(text: &str) -> Result<RecoveryCode, RecoveryError> {
        let mut code = Secret::<CODE_LEN>::from_bytes([0u8; CODE_LEN]);
        let mut buffer = Zeroizing::new(0u32);
        let mut bits = 0u32;
        let mut symbols = 0usize;
        let mut filled = 0usize;
        for c in text.chars() {
            if c == '-' || c.is_whitespace() {
                continue;
            }
            symbols = symbols.saturating_add(1);
            if symbols > CODE_SYMBOLS {
                return Err(RecoveryError::CodeLength);
            }
            let value = decode_symbol(c).ok_or(RecoveryError::CodeCharacter)?;
            *buffer = ((*buffer << 5) | u32::from(value)) & 0xFFFF;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                let byte = ((*buffer >> bits) & 0xFF) as u8;
                let slot = code
                    .expose_mut()
                    .get_mut(filled)
                    .ok_or(RecoveryError::CodeLength)?;
                *slot = byte;
                filled += 1;
            }
        }
        if symbols != CODE_SYMBOLS || filled != CODE_LEN || bits != 2 {
            return Err(RecoveryError::CodeLength);
        }
        if *buffer & 0b11 != 0 {
            return Err(RecoveryError::CodePadding);
        }
        Ok(RecoveryCode(code))
    }
}

/// Prints `RecoveryCode(***)`: never a symbol of the code.
impl fmt::Debug for RecoveryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryCode(***)")
    }
}
