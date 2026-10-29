//! The idempotency key: a request that grants or spends something carries one, and a repeat
//! with the same key (after a lost answer) gets the first answer instead of a second grant.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::InvalidValue;

/// The header the key travels in, as base64url without padding.
pub const IDEMPOTENCY_HEADER: &str = "x-azlin-idempotency";

/// The error code of a repeated request whose first answer was used already (409): the grant
/// reached the caller after all, so no second copy of it is handed out.
pub const ALREADY_ANSWERED: &str = "already_answered";

/// The error code of a grant an app sent without its key (400): an app names every grant, so
/// a retry after a lost answer can never grant twice.
pub const IDEMPOTENCY_REQUIRED: &str = "idempotency_required";

/// The error code of a key sent again with another request than the one it named (422): a key
/// names one request, so nothing was made for the second. Asking again does not change it; an
/// app that sends it reused a key, which is its bug.
pub const IDEMPOTENCY_KEY_REUSED: &str = "idempotency_key_reused";

/// What kind of request a key names. A key counts within its scope and for its caller only, so
/// one key sent to two routes is two requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdempotentScope {
    /// "Lock down from this device", or by the recovery key.
    Lockdown,
    /// A picked-up drive's own token family, by the sign-up's claim ticket.
    Claim,
    /// A period token redeemed for a month.
    Redeem,
    /// A voucher redeemed.
    Voucher,
    /// A paid checkout's period tokens signed.
    Issue,
    /// A token family for another device.
    AddMember,
    /// A device removed.
    RemoveMember,
    /// A recovery key set, added or removed.
    RecoveryKey,
    /// A restore queued.
    Restore,
    /// A checkout's provider session given up.
    Abandon,
    /// A checkout made: its order, answered again as it was the first time.
    Checkout,
}

impl IdempotentScope {
    /// Every scope, in the order of its fixture.
    pub const ALL: [IdempotentScope; 11] = [
        IdempotentScope::Lockdown,
        IdempotentScope::Claim,
        IdempotentScope::Redeem,
        IdempotentScope::Voucher,
        IdempotentScope::Issue,
        IdempotentScope::AddMember,
        IdempotentScope::RemoveMember,
        IdempotentScope::RecoveryKey,
        IdempotentScope::Restore,
        IdempotentScope::Abandon,
        IdempotentScope::Checkout,
    ];

    /// Its name on the wire and in the services' tables: `add_member`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            IdempotentScope::Lockdown => "lockdown",
            IdempotentScope::Claim => "claim",
            IdempotentScope::Redeem => "redeem",
            IdempotentScope::Voucher => "voucher",
            IdempotentScope::Issue => "issue",
            IdempotentScope::AddMember => "add_member",
            IdempotentScope::RemoveMember => "remove_member",
            IdempotentScope::RecoveryKey => "recovery_key",
            IdempotentScope::Restore => "restore",
            IdempotentScope::Abandon => "abandon",
            IdempotentScope::Checkout => "checkout",
        }
    }
}

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
