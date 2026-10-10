//! Trusted contacts (D51): a drive's recovery code split 2-of-3 ([`super::shamir`]) to three
//! people, each share sealed to the X25519 key of that person's AzDrive - or printed, for
//! someone without AzDrive.
//!
//! WHAT IS SPLIT is the recovery code itself (its 16 bytes): two shares give back the code, and
//! with it the code's own way back in - the recovery key it derives signs the lockdown at the
//! token server, which waits 48 hours for the owner's devices to cancel (D42). No method skips
//! the notice, and the token server needs nothing new. A new code (a rotation) makes the old
//! shares useless; they are split again.
//!
//! The pieces, each a text to pass on by any channel (only the sealed ones are private):
//! - a CONTACT KEY `azlin-contact:<64 hex>`: the public half of a key the contact's AzDrive
//!   made for this owner (one per owner: two owners cannot tell they share a contact);
//! - a SEALED SHARE `azlin-share:<hex>`: one share and the owner's words ("Felix, drive
//!   Photos"), sealed to a contact key;
//! - a PRINTED SHARE `S<n>-<the split's id, 8 hex>-<the share as a recovery code's text>`: on
//!   paper, unsealed - one share alone says nothing about the code;
//! - a RECOVERY REQUEST `azlin-recover:<64 hex>`: the public half of a one-time key the
//!   recovering computer made, with its SAFETY NUMBER (12 digits from it) that the contact
//!   compares with the owner by phone or in person before answering - against scams that fake
//!   a friend;
//! - a REPLY `azlin-share-reply:<hex>`: the contact's share sealed to the request's key.
//!
//! The split's id is the first 4 bytes of a BLAKE3 key derivation of the code: every share
//! carries it, and the code two shares give must have it (a mistyped share, or shares of two
//! splits, are refused rather than giving a wrong code).
//!
//! A sealed box (AZC1):
//!
//! ```text
//!   bytes  0   4  magic "AZC1"
//!          4   1  kind: 1 a share for a contact, 2 a reply to a recovery request
//!          5  32  the recipient's X25519 public key
//!         37  32  the ephemeral X25519 public key (made for this box only)
//!         69  24  nonce (random)
//!         93   *  XChaCha20-Poly1305(JSON {"v":1,"share":"<42 hex>","label":"..."}), its tag
//!   key = BLAKE3 derive_key(the kind's context, X25519 shared secret || ephemeral || recipient)
//!   aad = "AZC1" || kind || recipient || ephemeral
//! ```

use std::fmt;

use chacha20poly1305::{
    aead::{Aead, Payload},
    Key, KeyInit, XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

use super::{
    from_hex,
    keys::{MemberPublic, MemberSecret, RecoveryCode, RECOVERY_CODE_LEN},
    random_bytes,
    shamir::{self, Share},
    to_hex, CryptoError, KEY_LEN, NONCE_LEN, TAG_LEN,
};

/// Shares that give the code back, and shares made.
pub const THRESHOLD: u8 = 2;
pub const SHARES: u8 = 3;
/// Bytes of a split's id.
pub const SET_LEN: usize = 4;
/// What the texts start with.
pub const CONTACT_PREFIX: &str = "azlin-contact:";
pub const SHARE_PREFIX: &str = "azlin-share:";
pub const REQUEST_PREFIX: &str = "azlin-recover:";
pub const REPLY_PREFIX: &str = "azlin-share-reply:";

/// BLAKE3 `derive_key` contexts: the split's id, the two kinds of box, the safety number.
const SET_CONTEXT: &str = "Azlin 2026-10-10 trusted contacts: split id of a recovery code";
const SHARE_BOX_CONTEXT: &str = "Azlin 2026-10-10 trusted contacts: share sealed to a contact";
const REPLY_BOX_CONTEXT: &str = "Azlin 2026-10-10 trusted contacts: share sealed to a request";
const SAFETY_CONTEXT: &str = "Azlin 2026-10-10 trusted contacts: safety number of a request";
const MAGIC: &[u8; 4] = b"AZC1";
const KIND_SHARE: u8 = 1;
const KIND_REPLY: u8 = 2;
/// Bytes before the sealed JSON.
const HEADER_LEN: usize = 4 + 1 + KEY_LEN + KEY_LEN + NONCE_LEN;
/// A share's bytes inside a box: its number, the split's id, the share.
const SHARE_BYTES: usize = 1 + SET_LEN + RECOVERY_CODE_LEN;
/// The longest box opened (a share and the owner's words).
const MAX_BOX_LEN: usize = 64 * 1024;

fn damaged(why: impl Into<String>) -> CryptoError {
    CryptoError::Damaged(why.into())
}

/// The id of the split of `code`: the first [`SET_LEN`] bytes of a key derived from it.
#[must_use]
pub fn set_of(code: &RecoveryCode) -> [u8; SET_LEN] {
    let derived = Zeroizing::new(blake3::derive_key(SET_CONTEXT, code.as_bytes()));
    let mut set = [0u8; SET_LEN];
    set.copy_from_slice(&derived[..SET_LEN]);
    set
}

/// One of the three shares of a drive's recovery code. Wiped when dropped, never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct CodeShare {
    index: u8,
    set: [u8; SET_LEN],
    y: [u8; RECOVERY_CODE_LEN],
}

impl Drop for CodeShare {
    fn drop(&mut self) {
        self.y.zeroize();
    }
}

impl fmt::Debug for CodeShare {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CodeShare({} of {SHARES}, split {}, ***)",
            self.index,
            self.set_hex()
        )
    }
}

impl CodeShare {
    /// The three shares of `code`, any two of which give it back.
    pub fn split(code: &RecoveryCode) -> Result<Vec<CodeShare>, CryptoError> {
        let set = set_of(code);
        shamir::split(code.as_bytes(), THRESHOLD, SHARES)?
            .into_iter()
            .map(|share| {
                let mut y = [0u8; RECOVERY_CODE_LEN];
                if share.y.len() != RECOVERY_CODE_LEN {
                    return Err(damaged("a share of the wrong length"));
                }
                y.copy_from_slice(&share.y);
                Ok(CodeShare {
                    index: share.x,
                    set,
                    y,
                })
            })
            .collect()
    }

    /// The recovery code of two or three shares of one split. `Unsupported` for fewer than two;
    /// `Damaged` for shares of different splits, the same share twice, or shares that do not
    /// give back a code of their split (one mistyped, or of two splits of the code).
    pub fn combine(shares: &[CodeShare]) -> Result<RecoveryCode, CryptoError> {
        if shares.len() < usize::from(THRESHOLD) {
            return Err(CryptoError::Unsupported(format!(
                "{} shares: {THRESHOLD} of the {SHARES} give the code back",
                shares.len()
            )));
        }
        let set = shares[0].set;
        if shares.iter().any(|share| share.set != set) {
            return Err(damaged(
                "shares of different splits (one is of an older code)",
            ));
        }
        let points: Vec<Share> = shares
            .iter()
            .map(|share| Share {
                x: share.index,
                y: Zeroizing::new(share.y.to_vec()),
            })
            .collect();
        let bytes = shamir::combine(&points)?;
        if bytes.len() != RECOVERY_CODE_LEN {
            return Err(damaged("a share of the wrong length"));
        }
        let mut raw = [0u8; RECOVERY_CODE_LEN];
        raw.copy_from_slice(&bytes);
        let code = RecoveryCode::from_bytes(raw);
        raw.zeroize();
        if set_of(&code) != set {
            return Err(damaged(
                "the shares do not give back a code of their split: one is mistyped",
            ));
        }
        Ok(code)
    }

    /// Which share it is (1 to 3).
    #[must_use]
    pub fn index(&self) -> u8 {
        self.index
    }

    /// The split's id, 8 hex digits (upper case, as printed).
    #[must_use]
    pub fn set_hex(&self) -> String {
        to_hex(&self.set).to_ascii_uppercase()
    }

    /// Its bytes inside a box: the number, the split's id, the share.
    fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(SHARE_BYTES));
        bytes.push(self.index);
        bytes.extend_from_slice(&self.set);
        bytes.extend_from_slice(&self.y);
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<CodeShare, CryptoError> {
        if bytes.len() != SHARE_BYTES || !(1..=SHARES).contains(&bytes[0]) {
            return Err(damaged("not a share of a recovery code"));
        }
        let mut set = [0u8; SET_LEN];
        set.copy_from_slice(&bytes[1..1 + SET_LEN]);
        let mut y = [0u8; RECOVERY_CODE_LEN];
        y.copy_from_slice(&bytes[1 + SET_LEN..]);
        Ok(CodeShare {
            index: bytes[0],
            set,
            y,
        })
    }

    /// The printed share: `S<n>-<split id>-<the share as a recovery code's text>`.
    #[must_use]
    pub fn to_text(&self) -> Zeroizing<String> {
        let as_code = RecoveryCode::from_bytes(self.y);
        Zeroizing::new(format!(
            "S{}-{}-{}",
            self.index,
            self.set_hex(),
            as_code.to_text().as_str()
        ))
    }

    /// A printed share as a person types it back: any case, dashes or spaces anywhere, `O` for
    /// 0 and `I` / `L` for 1.
    #[must_use]
    pub fn parse(text: &str) -> Option<CodeShare> {
        let compact: Zeroizing<String> = Zeroizing::new(
            text.chars()
                .filter(|c| *c != '-' && !c.is_whitespace())
                .map(|c| c.to_ascii_uppercase())
                .collect(),
        );
        let rest = compact.strip_prefix('S')?;
        let index = u8::try_from(rest.chars().next()?.to_digit(10)?).ok()?;
        if !(1..=SHARES).contains(&index) {
            return None;
        }
        let hex: String = rest
            .get(1..1 + 2 * SET_LEN)?
            .chars()
            .map(|c| match c {
                'O' => '0',
                'I' | 'L' => '1',
                other => other,
            })
            .collect();
        let set: [u8; SET_LEN] = from_hex(&hex)?.try_into().ok()?;
        let y = RecoveryCode::parse(rest.get(1 + 2 * SET_LEN..)?)?;
        Some(CodeShare {
            index,
            set,
            y: *y.as_bytes(),
        })
    }
}

/// What a box holds.
#[derive(Serialize, Deserialize)]
struct BoxContent {
    v: u32,
    share: String,
    #[serde(default)]
    label: String,
}

fn box_key(
    kind: u8,
    shared: &[u8; KEY_LEN],
    ephemeral: &[u8; KEY_LEN],
    recipient: &[u8; KEY_LEN],
) -> Zeroizing<[u8; KEY_LEN]> {
    let context = if kind == KIND_SHARE {
        SHARE_BOX_CONTEXT
    } else {
        REPLY_BOX_CONTEXT
    };
    let mut hasher = blake3::Hasher::new_derive_key(context);
    hasher.update(shared);
    hasher.update(ephemeral);
    hasher.update(recipient);
    Zeroizing::new(*hasher.finalize().as_bytes())
}

fn box_aad(kind: u8, recipient: &[u8; KEY_LEN], ephemeral: &[u8; KEY_LEN]) -> Vec<u8> {
    let mut aad = MAGIC.to_vec();
    aad.push(kind);
    aad.extend_from_slice(recipient);
    aad.extend_from_slice(ephemeral);
    aad
}

/// `share` and `label` sealed to `to` as a box of `kind`.
fn seal(
    kind: u8,
    share: &CodeShare,
    label: &str,
    to: &MemberPublic,
) -> Result<Vec<u8>, CryptoError> {
    let content = BoxContent {
        v: 1,
        share: to_hex(&share.to_bytes()),
        label: label.to_string(),
    };
    let mut plain = Zeroizing::new(
        serde_json::to_vec(&content).map_err(|_| damaged("the share was not written"))?,
    );
    let mut hex = content.share;
    hex.zeroize();
    let mut ephemeral = Zeroizing::new([0u8; KEY_LEN]);
    random_bytes(&mut ephemeral[..])?;
    let ephemeral_secret = StaticSecret::from(*ephemeral);
    let ephemeral_public = PublicKey::from(&ephemeral_secret);
    let shared = ephemeral_secret.diffie_hellman(&PublicKey::from(to.0));
    if !shared.was_contributory() {
        return Err(damaged("a key of low order"));
    }
    let key = box_key(kind, shared.as_bytes(), ephemeral_public.as_bytes(), &to.0);
    let mut nonce = [0u8; NONCE_LEN];
    random_bytes(&mut nonce)?;
    let aad = box_aad(kind, &to.0, ephemeral_public.as_bytes());
    let sealed = XChaCha20Poly1305::new(Key::from_slice(&key[..]))
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plain[..],
                aad: &aad,
            },
        )
        .map_err(|_| damaged("the cipher refused to seal the share"))?;
    plain.zeroize();
    let mut out = Vec::with_capacity(HEADER_LEN + sealed.len());
    out.extend_from_slice(MAGIC);
    out.push(kind);
    out.extend_from_slice(&to.0);
    out.extend_from_slice(ephemeral_public.as_bytes());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// The recipient a box of `kind` names, from its bytes.
fn recipient_of(kind: u8, bytes: &[u8]) -> Result<[u8; KEY_LEN], CryptoError> {
    if bytes.len() < HEADER_LEN + TAG_LEN || bytes.len() > MAX_BOX_LEN || &bytes[..4] != MAGIC {
        return Err(damaged("not a sealed share"));
    }
    if bytes[4] != kind {
        return Err(damaged(if kind == KIND_SHARE {
            "a reply to a recovery request where a share for a contact belongs"
        } else {
            "a share for a contact where a reply to a recovery request belongs"
        }));
    }
    let mut recipient = [0u8; KEY_LEN];
    recipient.copy_from_slice(&bytes[5..5 + KEY_LEN]);
    Ok(recipient)
}

/// The share and the label in a box of `kind`, opened with `secret`. `WrongKey` for a box
/// sealed to another key, or changed.
fn open(kind: u8, bytes: &[u8], secret: &MemberSecret) -> Result<(CodeShare, String), CryptoError> {
    let recipient = recipient_of(kind, bytes)?;
    if secret.public().0 != recipient {
        return Err(CryptoError::WrongKey);
    }
    let mut ephemeral = [0u8; KEY_LEN];
    ephemeral.copy_from_slice(&bytes[5 + KEY_LEN..5 + 2 * KEY_LEN]);
    let own = StaticSecret::from(*secret.to_bytes());
    let shared = own.diffie_hellman(&PublicKey::from(ephemeral));
    if !shared.was_contributory() {
        return Err(damaged("the box's key is of low order"));
    }
    let key = box_key(kind, shared.as_bytes(), &ephemeral, &recipient);
    let plain = XChaCha20Poly1305::new(Key::from_slice(&key[..]))
        .decrypt(
            XNonce::from_slice(&bytes[5 + 2 * KEY_LEN..HEADER_LEN]),
            Payload {
                msg: &bytes[HEADER_LEN..],
                aad: &box_aad(kind, &recipient, &ephemeral),
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::WrongKey)?;
    let content: BoxContent =
        serde_json::from_slice(&plain).map_err(|_| damaged("not a share inside"))?;
    if content.v != 1 {
        return Err(CryptoError::Unsupported(format!(
            "a share box version {}",
            content.v
        )));
    }
    let mut hex = content.share;
    let bytes = from_hex(&hex).map(Zeroizing::new);
    hex.zeroize();
    let share = CodeShare::from_bytes(&bytes.ok_or_else(|| damaged("not a share inside"))?)?;
    Ok((share, content.label))
}

/// The bytes of a text `prefix` + hex.
fn text_bytes(text: &str, prefix: &str) -> Option<Vec<u8>> {
    let body = text.trim().strip_prefix(prefix)?;
    let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    from_hex(&compact)
}

/// A contact key as its text: `azlin-contact:<64 hex>`.
#[must_use]
pub fn contact_text(key: &MemberPublic) -> String {
    format!("{CONTACT_PREFIX}{}", key.to_hex())
}

/// The contact key of a text ([`contact_text`]).
#[must_use]
pub fn contact_from_text(text: &str) -> Option<MemberPublic> {
    let bytes: [u8; KEY_LEN] = text_bytes(text, CONTACT_PREFIX)?.try_into().ok()?;
    Some(MemberPublic(bytes))
}

/// `share` sealed to the contact key `to` with the owner's words `label`:
/// `azlin-share:<hex>`.
pub fn seal_share(
    share: &CodeShare,
    label: &str,
    to: &MemberPublic,
) -> Result<String, CryptoError> {
    Ok(format!(
        "{SHARE_PREFIX}{}",
        to_hex(&seal(KIND_SHARE, share, label, to)?)
    ))
}

/// The contact key a sealed share is for (the keyring entry that opens it).
#[must_use]
pub fn share_recipient(text: &str) -> Option<MemberPublic> {
    recipient_of(KIND_SHARE, &text_bytes(text, SHARE_PREFIX)?)
        .ok()
        .map(MemberPublic)
}

/// A sealed share opened with the contact's key: the share and the owner's words.
pub fn open_share(text: &str, secret: &MemberSecret) -> Result<(CodeShare, String), CryptoError> {
    let bytes = text_bytes(text, SHARE_PREFIX).ok_or_else(|| damaged("not a sealed share"))?;
    open(KIND_SHARE, &bytes, secret)
}

/// A recovery request as its text: `azlin-recover:<64 hex>`.
#[must_use]
pub fn request_text(key: &MemberPublic) -> String {
    format!("{REQUEST_PREFIX}{}", key.to_hex())
}

/// The request key of a text ([`request_text`]).
#[must_use]
pub fn request_from_text(text: &str) -> Option<MemberPublic> {
    let bytes: [u8; KEY_LEN] = text_bytes(text, REQUEST_PREFIX)?.try_into().ok()?;
    Some(MemberPublic(bytes))
}

/// The safety number of a request: 12 digits in four groups (`042 913 557 120`) that the owner
/// and the contact compare before the contact answers.
#[must_use]
pub fn safety_number(request: &MemberPublic) -> String {
    let derived = blake3::derive_key(SAFETY_CONTEXT, &request.0);
    let mut first = [0u8; 8];
    first.copy_from_slice(&derived[..8]);
    let digits = format!("{:012}", u64::from_le_bytes(first) % 1_000_000_000_000);
    let groups: Vec<&str> = (0..4).map(|i| &digits[3 * i..3 * i + 3]).collect();
    groups.join(" ")
}

/// A contact's answer to a request: `share` sealed to the request key `to`,
/// `azlin-share-reply:<hex>`.
pub fn seal_reply(share: &CodeShare, to: &MemberPublic) -> Result<String, CryptoError> {
    Ok(format!(
        "{REPLY_PREFIX}{}",
        to_hex(&seal(KIND_REPLY, share, "", to)?)
    ))
}

/// A reply opened with the request's key.
pub fn open_reply(text: &str, request: &MemberSecret) -> Result<CodeShare, CryptoError> {
    let bytes = text_bytes(text, REPLY_PREFIX).ok_or_else(|| damaged("not a reply"))?;
    open(KIND_REPLY, &bytes, request).map(|(share, _)| share)
}

/// A share as the recovering computer gets it: a reply (opened with the request's key) or a
/// printed share's text.
pub fn read_share(text: &str, request: &MemberSecret) -> Result<CodeShare, CryptoError> {
    if text.trim().starts_with(REPLY_PREFIX) {
        return open_reply(text, request);
    }
    CodeShare::parse(text)
        .ok_or_else(|| damaged("neither a reply nor a printed share (S1-..., S2-... or S3-...)"))
}
