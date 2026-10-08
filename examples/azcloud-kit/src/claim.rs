//! The claim of a paid drive (claim contract v1). A checkout names an X25519 public key the app
//! made for it (`claim_key`); once the payment is approved, the token server answers the drive's
//! sign-up SEALED to that key - for 30 days, to every poll - so the drive reaches the app however
//! late it asks (after "Stop waiting", after a restart), and whoever else learns the checkout id
//! (the payment page's address) learns nothing.
//!
//! The seal, the token server's side ([`ClaimKey::open`] is the app's):
//!
//! - `eph`: a fresh X25519 secret; `shared = X25519(eph, claim_pk)`;
//! - `key = HKDF-SHA256(ikm = shared, salt = eph_pk || claim_pk, info = "azlin-claim-v1")`, 32
//!   bytes;
//! - `ct = ChaCha20-Poly1305(key, nonce = 12 random bytes, plaintext = the sign-up JSON's UTF-8
//!   bytes, aad = the checkout id's UTF-8 bytes)`;
//! - `sealed = base64(eph_pk || nonce || ct)`, the standard alphabet, padded.
//!
//! The plaintext is the sign-up a token server without claims handed out as `signup`: a drive
//! bundle ([`crate::bundle`]). A [`ClaimKey`] is kept, as its base64 text, in the keyring with
//! its checkout until the drive is the app's ([`crate::pending`]); `Debug` never shows it.

use std::fmt;

use base64::{engine::general_purpose::STANDARD, Engine};
use chacha20poly1305::{
    aead::{Aead, Payload},
    ChaCha20Poly1305, KeyInit, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

/// The HKDF info of claim contract v1.
pub const CLAIM_INFO: &[u8] = b"azlin-claim-v1";
/// The bytes of an X25519 key and of the derived key.
const KEY_LEN: usize = 32;
/// The bytes of a ChaCha20-Poly1305 nonce.
const NONCE_LEN: usize = 12;
/// The bytes of its tag.
const TAG_LEN: usize = 16;

/// Why a claim key could not be made or a sealed sign-up not opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimError {
    /// The system's random source failed.
    Random,
    /// Not what it should be: what it is not (a claim key, a sealed sign-up, UTF-8 text).
    Malformed(&'static str),
    /// It does not open with this claim key for this checkout: another key, another checkout,
    /// a changed byte.
    Open,
}

impl fmt::Display for ClaimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClaimError::Random => f.write_str("the system's random source failed"),
            ClaimError::Malformed(what) => write!(f, "not {what}"),
            ClaimError::Open => {
                f.write_str("the sealed sign-up does not open with this checkout's claim key")
            }
        }
    }
}

impl std::error::Error for ClaimError {}

/// The X25519 secret of one checkout: its public half goes to `POST /v1/checkout` as
/// `claim_key`, the secret opens the sealed sign-up. `Debug` shows the public half only.
#[derive(Clone)]
pub struct ClaimKey {
    secret: StaticSecret,
}

impl fmt::Debug for ClaimKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaimKey")
            .field("public", &self.public_base64())
            .field("secret", &"<hidden>")
            .finish()
    }
}

impl ClaimKey {
    /// A new claim key from the OS random source.
    ///
    /// # Errors
    ///
    /// [`ClaimError::Random`] when the random source fails.
    pub fn generate() -> Result<ClaimKey, ClaimError> {
        let mut bytes = Zeroizing::new([0u8; KEY_LEN]);
        getrandom::getrandom(&mut bytes[..]).map_err(|_| ClaimError::Random)?;
        Ok(ClaimKey {
            secret: StaticSecret::from(*bytes),
        })
    }

    /// The claim key whose secret [`Self::to_base64`] wrote.
    ///
    /// # Errors
    ///
    /// [`ClaimError::Malformed`] for anything but the padded base64 of 32 bytes.
    pub fn from_base64(text: &str) -> Result<ClaimKey, ClaimError> {
        let bytes = Zeroizing::new(
            STANDARD
                .decode(text.trim())
                .map_err(|_| ClaimError::Malformed("a claim key"))?,
        );
        let secret: [u8; KEY_LEN] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| ClaimError::Malformed("a claim key"))?;
        Ok(ClaimKey {
            secret: StaticSecret::from(secret),
        })
    }

    /// The secret, as the keyring keeps it (standard padded base64). A secret: never print it.
    #[must_use]
    pub fn to_base64(&self) -> String {
        STANDARD.encode(self.secret.as_bytes())
    }

    /// The public key, as the checkout names it (`claim_key`: standard padded base64).
    #[must_use]
    pub fn public_base64(&self) -> String {
        STANDARD.encode(PublicKey::from(&self.secret).as_bytes())
    }

    /// The sign-up `sealed` holds, sealed for `checkout_id` (its JSON text).
    ///
    /// # Errors
    ///
    /// [`ClaimError::Malformed`] for what is no sealed sign-up (not base64, too short, a
    /// plaintext that is no UTF-8 text); [`ClaimError::Open`] when it was sealed to another key,
    /// for another checkout, or a byte of it changed.
    pub fn open(&self, checkout_id: &str, sealed: &str) -> Result<String, ClaimError> {
        let bytes = STANDARD
            .decode(sealed.trim())
            .map_err(|_| ClaimError::Malformed("a sealed sign-up (standard base64)"))?;
        if bytes.len() < KEY_LEN + NONCE_LEN + TAG_LEN {
            return Err(ClaimError::Malformed("a sealed sign-up (it is too short)"));
        }
        let (eph, rest) = bytes.split_at(KEY_LEN);
        let (nonce, body) = rest.split_at(NONCE_LEN);
        let eph: [u8; KEY_LEN] = eph
            .try_into()
            .map_err(|_| ClaimError::Malformed("a sealed sign-up"))?;
        let shared = self.secret.diffie_hellman(&PublicKey::from(eph));
        // An ephemeral key of a small order gives a shared secret everyone knows.
        if !shared.was_contributory() {
            return Err(ClaimError::Open);
        }
        let key = sealing_key(
            shared.as_bytes(),
            &eph,
            PublicKey::from(&self.secret).as_bytes(),
        )?;
        let plain = Zeroizing::new(
            ChaCha20Poly1305::new_from_slice(&key[..])
                .map_err(|_| ClaimError::Open)?
                .decrypt(
                    Nonce::from_slice(nonce),
                    Payload {
                        msg: body,
                        aad: checkout_id.as_bytes(),
                    },
                )
                .map_err(|_| ClaimError::Open)?,
        );
        std::str::from_utf8(&plain)
            .map(str::to_string)
            .map_err(|_| ClaimError::Malformed("UTF-8 text"))
    }
}

/// HKDF-SHA256 of the shared secret with `eph_pk || claim_pk` as its salt: the sealing key.
fn sealing_key(
    shared: &[u8; KEY_LEN],
    eph_pk: &[u8; KEY_LEN],
    claim_pk: &[u8; KEY_LEN],
) -> Result<Zeroizing<[u8; KEY_LEN]>, ClaimError> {
    let mut salt = [0u8; 2 * KEY_LEN];
    salt[..KEY_LEN].copy_from_slice(eph_pk);
    salt[KEY_LEN..].copy_from_slice(claim_pk);
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    Hkdf::<Sha256>::new(Some(&salt[..]), shared)
        .expand(CLAIM_INFO, &mut key[..])
        .map_err(|_| ClaimError::Open)?;
    Ok(key)
}

/// The token server's half, for the tests' round trips: `plaintext` sealed to the claim key
/// `claim_key` (the base64 a checkout names) for `checkout_id`, with a fresh ephemeral key and
/// nonce.
#[cfg(test)]
pub(crate) fn seal(claim_key: &str, checkout_id: &str, plaintext: &[u8]) -> String {
    let mut eph = [0u8; KEY_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut eph).expect("the OS random source");
    getrandom::getrandom(&mut nonce).expect("the OS random source");
    seal_with(claim_key, checkout_id, plaintext, eph, nonce)
}

/// [`seal`] with the ephemeral secret `eph` and the nonce `nonce` (a fixed vector).
#[cfg(test)]
pub(crate) fn seal_with(
    claim_key: &str,
    checkout_id: &str,
    plaintext: &[u8],
    eph: [u8; KEY_LEN],
    nonce: [u8; NONCE_LEN],
) -> String {
    let claim_pk: [u8; KEY_LEN] = STANDARD
        .decode(claim_key)
        .expect("a claim key is base64")
        .try_into()
        .expect("a claim key is 32 bytes");
    let eph = StaticSecret::from(eph);
    let eph_pk = PublicKey::from(&eph);
    let shared = eph.diffie_hellman(&PublicKey::from(claim_pk));
    let key = sealing_key(shared.as_bytes(), eph_pk.as_bytes(), &claim_pk).expect("32 bytes");
    let sealed = ChaCha20Poly1305::new_from_slice(&key[..])
        .expect("a 32-byte key")
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: checkout_id.as_bytes(),
            },
        )
        .expect("ChaCha20-Poly1305 seals any plaintext");
    let mut out = Vec::with_capacity(KEY_LEN + NONCE_LEN + sealed.len());
    out.extend_from_slice(eph_pk.as_bytes());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    STANDARD.encode(out)
}
