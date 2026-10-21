//! The idempotency key: a request that grants or spends something carries one, and a repeat
//! with the same key (after a lost answer) gets the first answer instead of a second grant.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::InvalidValue;

/// The header the key travels in, as base64url without padding.
pub const IDEMPOTENCY_HEADER: &str = "x-azlin-idempotency";

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// 16 random bytes naming one logical request across its retries.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct IdempotencyKey([u8; 16]);

impl IdempotencyKey {
    /// A new key from 16 bytes of the OS random source. The bytes must be random: a key made
    /// from anything a caller can repeat (a checkout id, a time) would merge two requests.
    #[must_use]
    pub fn fresh(random: [u8; 16]) -> IdempotencyKey {
        IdempotencyKey(random)
    }

    /// The key of a header's value, or of one a client kept for its retry.
    ///
    /// # Errors
    ///
    /// Not 22 characters of base64url.
    pub fn parse(text: &str) -> Result<IdempotencyKey, InvalidValue> {
        let bad = || InvalidValue::new("idempotency key (22 characters of base64url)", text);
        let text = text.trim();
        if text.len() != 22 {
            return Err(bad());
        }
        let mut bits: u32 = 0;
        let mut held = 0u32;
        let mut out = [0u8; 16];
        let mut n = 0;
        for c in text.bytes() {
            let value = ALPHABET.iter().position(|&a| a == c).ok_or_else(bad)?;
            bits = (bits << 6) | value as u32;
            held += 6;
            if held >= 8 {
                held -= 8;
                // 22 characters carry 132 bits: the 4 left over must be zero.
                out[n] = (bits >> held) as u8;
                n += 1;
                bits &= (1 << held) - 1;
            }
        }
        if n != 16 || bits != 0 {
            return Err(bad());
        }
        Ok(IdempotencyKey(out))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// The header's value: base64url, no padding.
    #[must_use]
    pub fn header_value(&self) -> String {
        let mut out = String::with_capacity(22);
        let mut bits: u32 = 0;
        let mut held = 0u32;
        for byte in self.0 {
            bits = (bits << 8) | u32::from(byte);
            held += 8;
            while held >= 6 {
                held -= 6;
                out.push(ALPHABET[((bits >> held) & 63) as usize] as char);
            }
            bits &= (1 << held) - 1;
        }
        if held > 0 {
            out.push(ALPHABET[((bits << (6 - held)) & 63) as usize] as char);
        }
        out
    }
}

impl fmt::Debug for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IdempotencyKey({})", self.header_value())
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.header_value())
    }
}

impl TryFrom<String> for IdempotencyKey {
    type Error = InvalidValue;
    fn try_from(text: String) -> Result<IdempotencyKey, InvalidValue> {
        IdempotencyKey::parse(&text)
    }
}

impl From<IdempotencyKey> for String {
    fn from(key: IdempotencyKey) -> String {
        key.header_value()
    }
}
