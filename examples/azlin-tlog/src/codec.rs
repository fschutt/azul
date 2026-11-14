//! The small text codecs every module shares: canonical standard base64, strict decimal numbers, lower-case
//! hex and the 32-byte hash in base64 (C2SP `signed-note` "Conventions": "Encoders MUST generate canonical
//! base64 according to RFC 4648, Section 3.5, and decoders MUST reject non-canonical encodings"; the
//! decimal rule of `tlog-checkpoint`: "no extra leading zeros. Zero is encoded as `0`").
//!
//! Crate-private: nothing here is part of the API, and every function is a pure parse that returns a
//! value or `None`/an error, never a default.

use azlin_merkle::Hash;
use base64::{engine::general_purpose::STANDARD, Engine};

/// Standard base64 with `=` padding.
pub(crate) fn b64_encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Standard base64 with `=` padding, canonical only: a missing or extra padding, a non-zero trailing bit, a
/// character outside the alphabet and a space or newline inside are all refused (the `base64` crate's
/// `STANDARD` engine decodes canonically: trailing bits must be zero and the padding must be exact).
pub(crate) fn b64_decode(text: &str) -> Option<Vec<u8>> {
    STANDARD.decode(text).ok()
}

/// A 32-byte hash in canonical base64 (44 characters).
pub(crate) fn b64_hash(text: &str) -> Option<Hash> {
    let bytes = b64_decode(text)?;
    let array: [u8; 32] = bytes.try_into().ok()?;
    Some(Hash(array))
}

/// Why a decimal number (a tree size, a leaf index, an `old` size) is refused. Never a string.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NumberError {
    /// Empty, or a character that is not an ASCII digit (no sign, no space).
    NotDecimal,
    /// A leading zero in a number longer than one digit.
    LeadingZero,
    /// More than `u64::MAX`.
    Overflow,
}

impl std::fmt::Display for NumberError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            NumberError::NotDecimal => "the number is not an ASCII decimal",
            NumberError::LeadingZero => "the number has a leading zero",
            NumberError::Overflow => "the number does not fit in 64 bits",
        })
    }
}

impl std::error::Error for NumberError {}

/// An ASCII decimal without leading zeros (zero is `0`) that fits in a `u64`.
pub(crate) fn parse_decimal(text: &str) -> Result<u64, NumberError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(NumberError::NotDecimal);
    }
    if text.len() > 1 && text.starts_with('0') {
        return Err(NumberError::LeadingZero);
    }
    text.parse::<u64>().map_err(|_| NumberError::Overflow)
}

/// Lower-case hex of `bytes`.
pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 0x0F));
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => char::from(b'0' + nibble),
        _ => char::from(b'a' + (nibble - 10)),
    }
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Exactly `N` bytes in lower-case hex (`2 * N` digits; upper case and any other character are refused).
pub(crate) fn hex_decode<const N: usize>(text: &str) -> Option<[u8; N]> {
    let bytes = text.as_bytes();
    if bytes.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (slot, pair) in out.iter_mut().zip(bytes.chunks_exact(2)) {
        let (high, low) = match pair {
            [high, low] => (hex_value(*high)?, hex_value(*low)?),
            _ => return None,
        };
        *slot = (high << 4) | low;
    }
    Some(out)
}
