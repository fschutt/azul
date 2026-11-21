//! Trusted contacts: the TEXT forms a contact key, a sealed share, a printed share, a recovery
//! request and a reply travel in, and the safety number two people compare (planning/architecture.md
//! section 5.5 "Recovery": "its 2-of-3 Shamir shares to contacts"; section 7.5 "Notice and hold":
//! "the contacts' shares"; build unit S15, lane W1-S15d). The forms are today's (`crypto/contacts.rs`),
//! ported:
//!
//! | text | what |
//! |---|---|
//! | `azlin-contact:<64 hex>` | the public half of a key the contact's app made for ONE owner (two owners cannot tell they share a contact); with an ML-KEM-768 key: `azlin-contact:<64 hex>:<2368 hex>` (DRIFT X-DELTA-W1S15D-13) |
//! | `azlin-share:<hex>` | an algorithm 5 envelope: one share and the owner's words, sealed to a contact key |
//! | `S<x>-<k>-<n>-<id>-<text>` | a printed share, unsealed: its index, threshold, count, split id (8 hex digits) and the share as a recovery code's text (DRIFT X-DELTA-W1S15D-7) |
//! | `azlin-recover:<64 hex>` | the public half of the one-time key the recovering computer made |
//! | `azlin-share-reply:<hex>` | a contact's share sealed to the request's key (an algorithm 5 envelope too) |
//!
//! Hex is LOWER case only: an upper case digit, a digit that is none, an odd number of digits and a
//! wrong prefix are refused (a pasted text may carry white space around it and, for the long forms,
//! line breaks between digits; nothing else). The printed share is read as a person types it: any
//! case, hyphens or spaces between the parts, `O` for `0` and `I` or `L` for `1`.
//!
//! THE SAFETY NUMBER (20 decimal digits, about 66 bits) is what two people compare by phone before
//! a contact answers a recovery request, against a substituted key: it is a function of TWO public
//! keys and the same in either order (DRIFT X-DELTA-W1S15D-14).

use zeroize::Zeroizing;

use super::code::{grouped_text, symbols_of, RecoveryCode};
use super::error::RecoveryError;
use super::open::SAFETY_NUMBER_CONTEXT;
use super::shamir::Share;
use crate::envelope::hybrid::PQ_KEY_LEN;
use crate::envelope::{Alg, Envelope, Recipient, RecipientSecret, MAX_ENVELOPE};
use crate::ids::KeyId;
use crate::seam::Entropy;
use crate::secret::Secret;

/// The start of a contact key's text.
///
/// Caller: `ContactKey::to_text`, `ContactKey::from_text`.
pub const CONTACT_PREFIX: &str = "azlin-contact:";

/// The start of a recovery request's text.
///
/// Caller: `RequestKey::to_text`, `RequestKey::from_text`.
pub const REQUEST_PREFIX: &str = "azlin-recover:";

/// The start of a sealed share's text.
///
/// Caller: `share_text`, `share_from_text`.
pub const SHARE_PREFIX: &str = "azlin-share:";

/// The start of a reply's text.
///
/// Caller: `reply_text`, `reply_from_text`.
pub const REPLY_PREFIX: &str = "azlin-share-reply:";

/// The longest text any form is read from, in bytes: a sealed share is at most `MAX_ENVELOPE`
/// bytes of envelope, so 8 KiB of hex, and line breaks may add some.
///
/// Caller: every parser of this module (everything has a limit).
pub const MAX_TEXT_LEN: usize = 16 * 1024;

/// How many digits a safety number has: 20 (about 66 bits, in five groups of four): a person still
/// reads 20 digits to someone on the phone, and grinding a substituted key whose number matches
/// costs one X25519 key and one hash for each try.
///
/// Caller: `safety_number`, `safety_number_text`.
pub const SAFETY_DIGITS: usize = 20;

/// The most bytes of a printed share's text: its parts are about 60 bytes, a person adds spaces.
const MAX_PRINTED_LEN: usize = 256;

/// The lower case hex digit of a nibble (public data: no table).
fn hex_digit(nibble: u8) -> char {
    char::from(b'0' + nibble + 39 * u8::from(nibble > 9))
}

/// The upper case hex digit of a nibble.
fn hex_digit_upper(nibble: u8) -> char {
    char::from(b'0' + nibble + 7 * u8::from(nibble > 9))
}

/// `bytes` as lower case hex.
fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 15));
    }
    out
}

/// The value of a lower case hex digit.
fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

/// The bytes of lower case hex: nothing (`TextLength`), an odd number of digits (`TextLength`), or
/// a character that is no lower case hex digit (`TextDigit`) are refused.
fn hex_bytes(digits: &str) -> Result<Vec<u8>, RecoveryError> {
    let bytes = digits.as_bytes();
    if bytes.is_empty() || !bytes.len().is_multiple_of(2) {
        return Err(RecoveryError::TextLength);
    }
    let (pairs, _) = bytes.as_chunks::<2>();
    let mut out = Vec::with_capacity(pairs.len());
    for [high, low] in pairs {
        match (nibble(*high), nibble(*low)) {
            (Some(high), Some(low)) => out.push((high << 4) | low),
            _ => return Err(RecoveryError::TextDigit),
        }
    }
    Ok(out)
}

/// The part of a pasted text after `prefix`, with the white space inside removed: the text is
/// trimmed, must start with `prefix`, and may not be longer than `MAX_TEXT_LEN`.
fn body_after(text: &str, prefix: &str) -> Result<String, RecoveryError> {
    if text.len() > MAX_TEXT_LEN {
        return Err(RecoveryError::TextTooLong);
    }
    let rest = text
        .trim()
        .strip_prefix(prefix)
        .ok_or(RecoveryError::TextPrefix)?;
    Ok(rest.chars().filter(|c| !c.is_whitespace()).collect())
}

/// A 32-byte key of exactly 64 hex digits.
fn key_bytes(digits: &str) -> Result<[u8; 32], RecoveryError> {
    let bytes = hex_bytes(digits)?;
    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| RecoveryError::TextLength)
}

/// The public half of a contact's key for ONE owner: an X25519 key and, when the contact's app has
/// one, an ML-KEM-768 encapsulation key. A share sealed to it is an algorithm 5 envelope over
/// algorithm 1 (X25519) or algorithm 2 (the hybrid): the hybrid only when the key has an ML-KEM
/// half and this build has the feature `pq` (otherwise `seal_share_to_contact` answers
/// `Unsupported`, never a silent downgrade).
///
/// Caller: INT-S15-1 (the owner's setup screen reads a contact's text; the contact's app shows it),
/// `seal_share_to_contact`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ContactKey {
    kx: [u8; 32],
    pq: Option<Vec<u8>>,
}

impl ContactKey {
    /// The key of these public keys; the ML-KEM key, when there is one, is exactly 1,184 bytes
    /// (`TextLength` otherwise; an empty key is not "none").
    ///
    /// Caller: INT-S15-1 (a contact key read from a QR code or from the registry).
    pub fn new(kx: [u8; 32], pq: Option<Vec<u8>>) -> Result<ContactKey, RecoveryError> {
        if let Some(pq) = &pq {
            if pq.len() != PQ_KEY_LEN {
                return Err(RecoveryError::TextLength);
            }
        }
        Ok(ContactKey { kx, pq })
    }

    /// The public half of the contact's own secret (what the contact's app shows the owner).
    ///
    /// Caller: INT-S15-1 (the contact's app: `ContactKey::of_secret(&new_contact_secret(..)?)`).
    pub fn of_secret(me: &RecipientSecret) -> ContactKey {
        ContactKey {
            kx: *me.kx_public(),
            pq: me.pq_public().map(<[u8]>::to_vec),
        }
    }

    /// The X25519 public key.
    pub fn kx(&self) -> &[u8; 32] {
        &self.kx
    }

    /// The ML-KEM-768 encapsulation key, when the contact has one.
    pub fn pq(&self) -> Option<&[u8]> {
        self.pq.as_deref()
    }

    /// The id a share sealed to this key names in its `rcpt`: the id of the bundle `(kx)` or
    /// `(kx, pq)`, as for every recipient (`KeyId::of_bundle`).
    pub fn key_id(&self) -> KeyId {
        KeyId::of_bundle(&self.kx, self.pq.as_deref())
    }

    /// The recipient an envelope can be sealed to (`Recipient::from_public`).
    pub fn recipient(&self) -> Recipient {
        Recipient::from_public(self.kx, self.pq.clone())
    }

    /// `azlin-contact:<64 hex>`, or `azlin-contact:<64 hex>:<2368 hex>` with an ML-KEM key.
    pub fn to_text(&self) -> String {
        let mut text = format!("{CONTACT_PREFIX}{}", hex_lower(&self.kx));
        if let Some(pq) = &self.pq {
            text.push(':');
            text.push_str(&hex_lower(pq));
        }
        text
    }

    /// The contact key of a text (`to_text`): trimmed, lower case hex, the exact lengths.
    pub fn from_text(text: &str) -> Result<ContactKey, RecoveryError> {
        let body = body_after(text, CONTACT_PREFIX)?;
        let mut parts = body.split(':');
        let kx = key_bytes(parts.next().ok_or(RecoveryError::TextLength)?)?;
        let pq = match parts.next() {
            None => None,
            Some(digits) => {
                let bytes = hex_bytes(digits)?;
                if bytes.len() != PQ_KEY_LEN {
                    return Err(RecoveryError::TextLength);
                }
                Some(bytes)
            }
        };
        if parts.next().is_some() {
            return Err(RecoveryError::TextLength);
        }
        ContactKey::new(kx, pq)
    }
}

/// A fresh secret for ONE contact relationship (or one recovery request): an X25519 secret and,
/// with `with_pq`, an ML-KEM seed (a build without the feature `pq` has no ML-KEM public key: the
/// key is then X25519 only). Every owner gets its own, so two owners cannot tell they share a
/// contact. The draws are the 32-byte X25519 secret, then the 64-byte ML-KEM seed.
///
/// Caller: INT-S15-1 (the contact's app, once for each owner; the recovering computer, once for a
/// request).
pub fn new_contact_secret(
    ent: &mut dyn Entropy,
    with_pq: bool,
) -> Result<RecipientSecret, RecoveryError> {
    let kx = Secret::<32>::generate(ent)?;
    let seed = if with_pq {
        Some(Secret::<64>::generate(ent)?)
    } else {
        None
    };
    Ok(RecipientSecret::new(&kx, seed.as_ref()))
}

/// The public half of the one-time key a recovering computer made: an X25519 key. A contact answers
/// it by sealing a share to it (a reply).
///
/// Caller: INT-S15-1 (the recovery screen shows `to_text` and its safety number; the contact's app
/// reads it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestKey {
    kx: [u8; 32],
}

impl RequestKey {
    /// The request key of this X25519 public key.
    pub fn new(kx: [u8; 32]) -> RequestKey {
        RequestKey { kx }
    }

    /// The X25519 public key.
    pub fn kx(&self) -> &[u8; 32] {
        &self.kx
    }

    /// The key as a recipient of a reply.
    pub fn as_contact(&self) -> ContactKey {
        ContactKey {
            kx: self.kx,
            pq: None,
        }
    }

    /// `azlin-recover:<64 hex>`.
    pub fn to_text(&self) -> String {
        format!("{REQUEST_PREFIX}{}", hex_lower(&self.kx))
    }

    /// The request key of a text (`to_text`).
    pub fn from_text(text: &str) -> Result<RequestKey, RecoveryError> {
        let body = body_after(text, REQUEST_PREFIX)?;
        Ok(RequestKey::new(key_bytes(&body)?))
    }
}

/// The text of an envelope under `prefix`: the hex of its canonical CBOR. Only an algorithm 5
/// envelope (a sealed share) has a text.
fn envelope_text(prefix: &str, env: &Envelope) -> Result<String, RecoveryError> {
    if env.alg != Alg::Shamir {
        return Err(RecoveryError::WrongAlg(env.alg));
    }
    let bytes = env.to_cbor().map_err(|_| RecoveryError::NotAnEnvelope)?;
    Ok(format!("{prefix}{}", hex_lower(&bytes)))
}

/// The algorithm 5 envelope of a text under `prefix`: lower case hex of at most `MAX_ENVELOPE`
/// bytes that are exactly one envelope (`NotAnEnvelope` otherwise) of algorithm 5 (`WrongAlg`).
fn envelope_from_text(prefix: &str, text: &str) -> Result<Envelope, RecoveryError> {
    let body = body_after(text, prefix)?;
    if body.len() > 2 * MAX_ENVELOPE {
        return Err(RecoveryError::TextTooLong);
    }
    let bytes = hex_bytes(&body)?;
    let env = Envelope::from_cbor(&bytes).map_err(|_| RecoveryError::NotAnEnvelope)?;
    if env.alg != Alg::Shamir {
        return Err(RecoveryError::WrongAlg(env.alg));
    }
    Ok(env)
}

/// `azlin-share:<hex>`: a sealed share (an algorithm 5 envelope) as text.
///
/// Caller: INT-S15-1 (the owner hands a contact its share).
pub fn share_text(env: &Envelope) -> Result<String, RecoveryError> {
    envelope_text(SHARE_PREFIX, env)
}

/// The sealed share of a text (`share_text`).
///
/// Caller: INT-S15-1 (the contact's app reads what the owner sent).
pub fn share_from_text(text: &str) -> Result<Envelope, RecoveryError> {
    envelope_from_text(SHARE_PREFIX, text)
}

/// `azlin-share-reply:<hex>`: a contact's answer to a recovery request (a share sealed to the
/// request's key: an algorithm 5 envelope) as text.
///
/// Caller: INT-S15-1 (the contact's app answers a request).
pub fn reply_text(env: &Envelope) -> Result<String, RecoveryError> {
    envelope_text(REPLY_PREFIX, env)
}

/// The reply of a text (`reply_text`).
///
/// Caller: INT-S15-1 (the recovering computer reads a contact's answer).
pub fn reply_from_text(text: &str) -> Result<Envelope, RecoveryError> {
    envelope_from_text(REPLY_PREFIX, text)
}

/// The printed share: `S<x>-<k>-<n>-<split id as 8 upper case hex digits>-<the share as a recovery
/// code's text>` (for example `S2-2-3-C08FFFEB-XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX`), in a buffer that is
/// wiped when dropped. One share alone says nothing about the code.
///
/// Caller: INT-S15-1 (the paper for a contact without the app).
pub fn printed_share_text(share: &Share) -> Zeroizing<String> {
    let mut prefix = format!("S{}-{}-{}-", share.x, share.k, share.n);
    for byte in &share.split_id {
        prefix.push(hex_digit_upper(byte >> 4));
        prefix.push(hex_digit_upper(byte & 15));
    }
    prefix.push('-');
    let symbols = symbols_of(&share.y);
    let grouped = grouped_text(&symbols);
    let mut text = Zeroizing::new(String::with_capacity(prefix.len() + grouped.len()));
    text.push_str(&prefix);
    text.push_str(&grouped);
    text
}

/// A decimal number of 1 to 3 digits that fits a byte: no digits is `TextLength`, a character that
/// is no digit `TextDigit`, a value above 255 `BadShare`.
fn small_number(token: &str) -> Result<u8, RecoveryError> {
    if token.is_empty() || token.len() > 3 {
        return Err(RecoveryError::TextLength);
    }
    if !token.bytes().all(|b| b.is_ascii_digit()) {
        return Err(RecoveryError::TextDigit);
    }
    let value: u16 = token.parse().map_err(|_| RecoveryError::TextDigit)?;
    u8::try_from(value).map_err(|_| RecoveryError::BadShare)
}

/// The split id of a printed share: exactly 8 hex digits in either case, read as a person types
/// them (`O` for `0`, `I` and `L` for `1`).
fn split_id_of(token: &str) -> Result<[u8; 4], RecoveryError> {
    if token.len() != 8 {
        return Err(RecoveryError::TextLength);
    }
    let mut out = [0u8; 4];
    let mut digits = token.bytes().map(|b| match b.to_ascii_lowercase() {
        b'o' => b'0',
        b'i' | b'l' => b'1',
        other => other,
    });
    for slot in &mut out {
        let high = digits
            .next()
            .and_then(nibble)
            .ok_or(RecoveryError::TextDigit)?;
        let low = digits
            .next()
            .and_then(nibble)
            .ok_or(RecoveryError::TextDigit)?;
        *slot = (high << 4) | low;
    }
    Ok(out)
}

/// The share of a printed text (`printed_share_text`) as a person types it back: any case, hyphens
/// or white space between the parts, `O` for `0` and `I` or `L` for `1`. The index, threshold and
/// count must be those of a share (`ShareIndex`, `BadShare`); the share's text must be a recovery
/// code's (`CodeLength`, `CodeCharacter`, `CodePadding`).
///
/// Caller: INT-S15-1 (the recovery screen reads a contact's paper).
pub fn printed_share_from_text(text: &str) -> Result<Share, RecoveryError> {
    if text.len() > MAX_PRINTED_LEN {
        return Err(RecoveryError::TextTooLong);
    }
    let mut tokens = text
        .split(|c: char| c == '-' || c.is_whitespace())
        .filter(|token| !token.is_empty());
    let first = tokens.next().ok_or(RecoveryError::TextLength)?;
    let x_digits = first
        .strip_prefix('S')
        .or_else(|| first.strip_prefix('s'))
        .ok_or(RecoveryError::TextPrefix)?;
    let x = small_number(x_digits)?;
    let k = small_number(tokens.next().ok_or(RecoveryError::TextLength)?)?;
    let n = small_number(tokens.next().ok_or(RecoveryError::TextLength)?)?;
    let split_id = split_id_of(tokens.next().ok_or(RecoveryError::TextLength)?)?;
    let mut rest = Zeroizing::new(String::with_capacity(MAX_PRINTED_LEN));
    for token in tokens {
        rest.push_str(token);
    }
    let code = RecoveryCode::parse(&rest)?;
    let share = Share {
        x,
        k,
        n,
        y: code.as_secret().duplicate(),
        split_id,
    };
    share.check()?;
    Ok(share)
}

/// The safety number of two public keys: 20 decimal digits (each byte of the result is one digit,
/// 0 to 9). BLAKE3 `derive_key` under the context `azlin 2026-11 contact safety number v1` over the
/// two keys in byte order (the smaller first), the first 16 bytes as a little-endian number, modulo
/// 10^20 (the bias of the remainder is under one in 10^18). SYMMETRIC: the owner and the contact
/// compute the same number from the same two keys in either argument order; a flipped bit of either
/// key changes it. `request_pub` is the one-time key of the recovering computer, `owner_pub` the
/// owner's key the contact knows.
///
/// Caller: INT-S15-1 (the recovery screen and the contact's app show it; two people compare it by
/// phone before the contact answers).
pub fn safety_number(request_pub: &[u8; 32], owner_pub: &[u8; 32]) -> [u8; SAFETY_DIGITS] {
    let (low, high) = if request_pub <= owner_pub {
        (request_pub, owner_pub)
    } else {
        (owner_pub, request_pub)
    };
    let mut hasher = blake3::Hasher::new_derive_key(SAFETY_NUMBER_CONTEXT);
    hasher.update(low);
    hasher.update(high);
    let hash = hasher.finalize();
    let mut head = [0u8; 16];
    for (slot, byte) in head.iter_mut().zip(hash.as_bytes().iter()) {
        *slot = *byte;
    }
    let mut value = u128::from_le_bytes(head) % 10u128.pow(20);
    let mut digits = [0u8; SAFETY_DIGITS];
    for slot in digits.iter_mut().rev() {
        *slot = (value % 10) as u8;
        value /= 10;
    }
    digits
}

/// The safety number as people read it: five groups of four digits, `0429 1355 7120 3344 7765`.
///
/// Caller: INT-S15-1 (the screens).
pub fn safety_number_text(digits: &[u8; SAFETY_DIGITS]) -> String {
    let mut text = String::with_capacity(SAFETY_DIGITS + 4);
    for (index, digit) in digits.iter().enumerate() {
        if index > 0 && index.is_multiple_of(4) {
            text.push(' ');
        }
        text.push(char::from(b'0' + digit % 10));
    }
    text
}
