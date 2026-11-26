//! The pairing's arithmetic. The Mac makes an OFFER (its dialing ticket and a one-time 16-byte secret) and shows it
//! as a QR code, or hands it to the phone in a file while the phone is on the cable. The phone dials the ticket and
//! PROVES it knows the secret (an HMAC-SHA256 over both keys: it cannot be replayed by another key); both sides
//! work out the same six-digit CODE (SHA-256 over the secret and both keys) and show it, and the person at the Mac
//! confirms it matches. The secret is spent by the first pairing.
//!
//! The text of an offer is `azrv1:<32 hex secret>:<8 hex check>:<ticket>`; the check catches a pasted text that was
//! cut or mistyped (a QR code has its own error correction). The keys are the transports' authenticated ids, lowercase
//! hex, as `Link` reports them.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::limits::{MAX_TICKET_BYTES, PAIRING_SECRET_BYTES};

/// The first part of an offer's text.
pub const OFFER_PREFIX: &str = "azrv1";

/// What a QR code or the cable file carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingOffer {
    /// How the phone dials the Mac (the iroh endpoint ticket).
    pub ticket: String,
    /// The one-time secret.
    pub secret: [u8; PAIRING_SECRET_BYTES],
}

/// Why a text is no offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferError {
    /// Not an AzReview offer at all.
    NotAnOffer,
    /// An offer of a version this build does not know: update the companion or AzReview.
    WrongVersion,
    /// The secret is not 32 hex characters.
    BadSecret,
    /// The check does not fit the rest: the text was cut or changed.
    BadCheck,
    /// The ticket is empty or has characters a ticket does not have.
    BadTicket,
    /// Longer than any offer.
    TooLong,
}

impl core::fmt::Display for OfferError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            OfferError::NotAnOffer => "this is not an AzReview pairing code",
            OfferError::WrongVersion => "this pairing code is from another version: update the companion or AzReview",
            OfferError::BadSecret => "the pairing code is damaged (its secret)",
            OfferError::BadCheck => "the pairing code is damaged (it was cut or changed): show it again",
            OfferError::BadTicket => "the pairing code is damaged (its address)",
            OfferError::TooLong => "this is too long to be an AzReview pairing code",
        })
    }
}

impl PairingOffer {
    /// The text of the offer (what the QR code encodes).
    #[must_use]
    pub fn to_text(&self) -> String {
        format!("{OFFER_PREFIX}:{}:{}:{}", hex(&self.secret), hex(&offer_check(&self.secret, &self.ticket)), self.ticket)
    }

    /// The offer in `text` (surrounding white space is ignored, so a pasted line works).
    ///
    /// # Errors
    /// Why the text is no offer.
    pub fn parse(text: &str) -> Result<PairingOffer, OfferError> {
        if text.len() > MAX_OFFER_BYTES + 256 {
            return Err(OfferError::TooLong);
        }
        let text = text.trim();
        if text.len() > MAX_OFFER_BYTES {
            return Err(OfferError::TooLong);
        }
        let mut parts = text.splitn(4, ':');
        let prefix = parts.next().unwrap_or("");
        if prefix != OFFER_PREFIX {
            return Err(if prefix.starts_with("azrv") && prefix.len() <= 8 && text.contains(':') {
                OfferError::WrongVersion
            } else {
                OfferError::NotAnOffer
            });
        }
        let (Some(secret), Some(check), Some(ticket)) = (parts.next(), parts.next(), parts.next()) else {
            return Err(OfferError::NotAnOffer);
        };
        let secret = from_hex_fixed::<PAIRING_SECRET_BYTES>(secret).ok_or(OfferError::BadSecret)?;
        let check = from_hex_fixed::<4>(check).ok_or(OfferError::BadCheck)?;
        if ticket.is_empty() || !ticket.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(OfferError::BadTicket);
        }
        if offer_check(&secret, ticket) != check {
            return Err(OfferError::BadCheck);
        }
        Ok(PairingOffer { ticket: ticket.to_string(), secret })
    }
}

/// Where random bytes come from (a seam: the system's in the app, a counter in a test).
pub trait Entropy {
    /// Fills `buf` with random bytes.
    fn fill(&mut self, buf: &mut [u8]);
}

/// The operating system's randomness: `/dev/urandom` where there is one, else the keys the standard library's hasher
/// draws from the system at start. The secret is one-time and lives five minutes; the person at the Mac confirms a
/// code besides.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsEntropy;

impl Entropy for OsEntropy {
    fn fill(&mut self, buf: &mut [u8]) {
        if !fill_from_urandom(buf) {
            fill_from_hasher_keys(buf);
        }
    }
}

/// A deterministic generator for tests and the simulator (xorshift64*).
#[derive(Debug, Clone)]
pub struct SeededEntropy(pub u64);

impl Entropy for SeededEntropy {
    fn fill(&mut self, buf: &mut [u8]) {
        // xorshift64*; a zero seed would stay zero, so it starts from a fixed odd number instead
        let mut state = if self.0 == 0 { 0x9E37_79B9_7F4A_7C15 } else { self.0 };
        for chunk in buf.chunks_mut(8) {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            let word = state.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes();
            chunk.copy_from_slice(&word[..chunk.len()]);
        }
        self.0 = state;
    }
}

/// A new one-time secret.
#[must_use]
pub fn new_secret(entropy: &mut dyn Entropy) -> [u8; PAIRING_SECRET_BYTES] {
    let mut secret = [0u8; PAIRING_SECRET_BYTES];
    entropy.fill(&mut secret);
    secret
}

/// The phone's proof that it holds the offer's secret: HMAC-SHA256 keyed with the secret over the domain name and
/// both keys, so it is good for this Mac and this phone only.
#[must_use]
pub fn proof(secret: &[u8; PAIRING_SECRET_BYTES], host_key: &str, phone_key: &str) -> [u8; 32] {
    let tag = proof_mac(secret, host_key, phone_key).finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&tag);
    out
}

/// The HMAC of the proof, keyed with the secret, before it is finished.
fn proof_mac(secret: &[u8; PAIRING_SECRET_BYTES], host_key: &str, phone_key: &str) -> Hmac<Sha256> {
    // an HMAC takes a key of any length: this cannot fail
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret).unwrap_or_else(|_| unreachable!("an HMAC key has any length"));
    mac.update(b"azreview-pair-v1");
    mac.update(&[0]);
    mac.update(host_key.as_bytes());
    mac.update(&[0]);
    mac.update(phone_key.as_bytes());
    mac
}

/// Whether `claimed` is the proof of `secret` for these keys (compared in constant time).
#[must_use]
pub fn verify_proof(secret: &[u8; PAIRING_SECRET_BYTES], host_key: &str, phone_key: &str, claimed: &[u8; 32]) -> bool {
    // `verify_slice` compares in constant time
    proof_mac(secret, host_key, phone_key).verify_slice(claimed).is_ok()
}

/// The six-digit number both screens show (0 to 999 999).
#[must_use]
pub fn confirmation_code(secret: &[u8; PAIRING_SECRET_BYTES], host_key: &str, phone_key: &str) -> u32 {
    let mut hash = Sha256::new();
    hash.update(b"azreview-code-v1");
    hash.update([0u8]);
    hash.update(secret);
    hash.update(host_key.as_bytes());
    hash.update([0u8]);
    hash.update(phone_key.as_bytes());
    let digest = hash.finalize();
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 1_000_000
}

/// A code as people read it: `482 913`.
#[must_use]
pub fn format_code(code: u32) -> String {
    let code = code % 1_000_000;
    format!("{:03} {:03}", code / 1000, code % 1000)
}

/// The most an offer's text can be: the prefix, the secret, the check, the separators and the longest ticket.
const MAX_OFFER_BYTES: usize = OFFER_PREFIX.len() + 1 + PAIRING_SECRET_BYTES * 2 + 1 + 8 + 1 + MAX_TICKET_BYTES;

/// The four bytes that tell a cut or mistyped offer from a whole one.
fn offer_check(secret: &[u8; PAIRING_SECRET_BYTES], ticket: &str) -> [u8; 4] {
    let mut hash = Sha256::new();
    hash.update(b"azreview-offer-v1");
    hash.update([0u8]);
    hash.update(secret);
    hash.update(ticket.as_bytes());
    let digest = hash.finalize();
    [digest[0], digest[1], digest[2], digest[3]]
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `N` bytes from exactly `2 * N` hex characters.
fn from_hex_fixed<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 || !text.is_ascii() {
        return None;
    }
    let mut out = [0u8; N];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Reads `/dev/urandom` (where there is one): whether it filled the buffer.
fn fill_from_urandom(buf: &mut [u8]) -> bool {
    use std::io::Read;
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(buf)).is_ok()
}

/// The standard library seeds each hasher's keys from the operating system: hashing a counter with such a hasher
/// gives bytes nobody can predict without the keys.
fn fill_from_hasher_keys(buf: &mut [u8]) {
    use std::hash::{BuildHasher, Hasher};
    let state = std::collections::hash_map::RandomState::new();
    for (counter, chunk) in buf.chunks_mut(8).enumerate() {
        let mut hasher = state.build_hasher();
        hasher.write_u64(counter as u64);
        let word = hasher.finish().to_le_bytes();
        chunk.copy_from_slice(&word[..chunk.len()]);
    }
}

#[cfg(test)]
#[path = "pairing_test.rs"]
mod pairing_test;
