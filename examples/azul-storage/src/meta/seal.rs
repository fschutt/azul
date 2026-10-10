//! What seals the metadata repository's objects: the drive key, behind the
//! [`Sealer`] trait.
//!
//! The drive key (`crate::crypto::DriveKey`, feature `encryption`) implements
//! [`Sealer`]: XChaCha20-Poly1305 under a BLAKE3 subkey, a random 24-byte
//! nonce per object, BLAKE3 keyed hashes for names (see the `drive_key`
//! module below). [`TestSealer`] is the tests' stand-in without the feature: a
//! real cipher and MAC built from HMAC-SHA256, so the tests see ciphertext that
//! hides every name, but a test double, never a drive's key.

use std::{
    fmt,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Sealing or opening failed: the wrong key, a changed byte, another
/// object's context, or a cipher error. The reason never contains key
/// material or plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealError {
    pub reason: String,
}

impl SealError {
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        SealError {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for SealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for SealError {}

/// Encrypts and authenticates the metadata repository's objects with the
/// drive key. What the repository needs from the drive's key hierarchy:
///
/// - [`Sealer::seal`] / [`Sealer::open`]: an AEAD. `context` is authenticated, not
///   encrypted, and not stored: the opener passes the same bytes. The repository
///   passes the object's key in the bucket (plus a label and, for pack chunks, the
///   chunk's position), so an object moved to another key, or a chunk moved within a
///   pack, does not open. The drive key's implementation: XChaCha20-Poly1305 under a
///   BLAKE3 subkey of the drive key, a fresh random 24-byte nonce per call, stored in
///   front of the ciphertext, and the key's id, so an object sealed before a key
///   rotation still opens.
/// - [`Sealer::name_hash`]: names in the bucket (pack names) that say nothing about
///   the content: a keyed hash (BLAKE3 keyed with another subkey of the drive key).
///   The same data gives the same name, on every device holding the key.
pub trait Sealer: Send + Sync {
    /// `plaintext` encrypted and authenticated together with `context`.
    fn seal(&self, context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError>;
    /// The plaintext of what [`Sealer::seal`] made with the same `context`; an
    /// error when anything differs.
    fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, SealError>;
    /// A 32-byte name for `data` that reveals nothing about it without the key.
    fn name_hash(&self, data: &[u8]) -> [u8; 32];
}

impl<S: Sealer + ?Sized> Sealer for Arc<S> {
    fn seal(&self, context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        (**self).seal(context, plaintext)
    }
    fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, SealError> {
        (**self).open(context, sealed)
    }
    fn name_hash(&self, data: &[u8]) -> [u8; 32] {
        (**self).name_hash(data)
    }
}

impl<S: Sealer + ?Sized> Sealer for &S {
    fn seal(&self, context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        (**self).seal(context, plaintext)
    }
    fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, SealError> {
        (**self).open(context, sealed)
    }
    fn name_hash(&self, data: &[u8]) -> [u8; 32] {
        (**self).name_hash(data)
    }
}

impl<S: Sealer + ?Sized> Sealer for Box<S> {
    fn seal(&self, context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        (**self).seal(context, plaintext)
    }
    fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, SealError> {
        (**self).open(context, sealed)
    }
    fn name_hash(&self, data: &[u8]) -> [u8; 32] {
        (**self).name_hash(data)
    }
}

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA256 of the parts, one after the other.
fn hmac_parts(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("HMAC takes a key of any length");
    for part in parts {
        mac.update(part);
    }
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

/// The test double of the drive key's [`Sealer`]: NOT a production cipher.
///
/// Construction (encrypt-then-MAC, every key separated by HMAC labels):
/// - subkeys: `enc = HMAC(key, "azul-storage meta TestSealer v1: encryption")`,
///   `mac = HMAC(key, "... v1: authentication")`, `names = HMAC(key, "... v1: names")`;
/// - nonce: 16 bytes, `HMAC(names, "nonce" || salt || counter)` truncated, where the salt
///   is random per sealer and the counter counts its calls: never repeats under one
///   sealer, and two sealers with the same key differ by their salts;
/// - ciphertext: plaintext XOR the keystream `HMAC(enc, nonce || block index)` (32-byte
///   blocks, the index as u64 little-endian);
/// - tag: `HMAC(mac, len(context) as u64 LE || context || nonce || ciphertext)`, all 32
///   bytes, compared in constant time;
/// - the sealed object: `"TS1" || nonce || ciphertext || tag`.
pub struct TestSealer {
    enc: [u8; 32],
    mac: [u8; 32],
    names: [u8; 32],
    salt: u64,
    counter: AtomicU64,
}

const TEST_MAGIC: &[u8; 3] = b"TS1";
const TEST_NONCE: usize = 16;
const TEST_TAG: usize = 32;

impl TestSealer {
    /// A sealer under `key` (the drive key's stand-in).
    #[must_use]
    pub fn new(key: [u8; 32]) -> Self {
        TestSealer {
            enc: hmac_parts(&key, &[b"azul-storage meta TestSealer v1: encryption"]),
            mac: hmac_parts(&key, &[b"azul-storage meta TestSealer v1: authentication"]),
            names: hmac_parts(&key, &[b"azul-storage meta TestSealer v1: names"]),
            salt: crate::ids::random_seed(),
            counter: AtomicU64::new(0),
        }
    }

    fn keystream_xor(&self, nonce: &[u8], data: &mut [u8]) {
        for (index, block) in data.chunks_mut(32).enumerate() {
            let stream = hmac_parts(&self.enc, &[nonce, &(index as u64).to_le_bytes()]);
            for (byte, key) in block.iter_mut().zip(stream.iter()) {
                *byte ^= key;
            }
        }
    }

    fn tag_mac(&self, context: &[u8], nonce: &[u8], ciphertext: &[u8]) -> HmacSha256 {
        let mut mac =
            <HmacSha256 as Mac>::new_from_slice(&self.mac).expect("HMAC takes a key of any length");
        mac.update(&(context.len() as u64).to_le_bytes());
        mac.update(context);
        mac.update(nonce);
        mac.update(ciphertext);
        mac
    }
}

impl Sealer for TestSealer {
    fn seal(&self, context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        let nonce_full = hmac_parts(
            &self.names,
            &[b"nonce", &self.salt.to_le_bytes(), &count.to_le_bytes()],
        );
        let nonce = &nonce_full[..TEST_NONCE];
        let mut out = Vec::with_capacity(TEST_MAGIC.len() + TEST_NONCE + plaintext.len() + TEST_TAG);
        out.extend_from_slice(TEST_MAGIC);
        out.extend_from_slice(nonce);
        let start = out.len();
        out.extend_from_slice(plaintext);
        self.keystream_xor(nonce, &mut out[start..]);
        let tag = self.tag_mac(context, nonce, &out[start..]).finalize().into_bytes();
        out.extend_from_slice(&tag);
        Ok(out)
    }

    fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, SealError> {
        let head = TEST_MAGIC.len() + TEST_NONCE;
        if sealed.len() < head + TEST_TAG || &sealed[..TEST_MAGIC.len()] != TEST_MAGIC {
            return Err(SealError::new("not a sealed object"));
        }
        let nonce = &sealed[TEST_MAGIC.len()..head];
        let ciphertext = &sealed[head..sealed.len() - TEST_TAG];
        let tag = &sealed[sealed.len() - TEST_TAG..];
        self.tag_mac(context, nonce, ciphertext)
            .verify_slice(tag)
            .map_err(|_| SealError::new("the object does not open with this key and context"))?;
        let mut plaintext = ciphertext.to_vec();
        self.keystream_xor(nonce, &mut plaintext);
        Ok(plaintext)
    }

    fn name_hash(&self, data: &[u8]) -> [u8; 32] {
        hmac_parts(&self.names, &[b"name", data])
    }
}

/// The drive key seals the metadata repository (feature `encryption`).
///
/// - **Keys:** `seal = BLAKE3 derive_key("Azlin 2026-10-10 drive key: metadata repository
///   sealing", drive key)` and `names = BLAKE3 derive_key("Azlin 2026-10-10 drive key: metadata
///   repository names", drive key)`; the drive key itself seals and hashes nothing.
/// - **Nonce:** 24 bytes from the OS random source for every [`Sealer::seal`] (192 bits:
///   random nonces never meet in practice), stored in the object.
/// - **Associated data:** `"AZM1 sealed metadata v1" || key id || context`: the object's bucket
///   key (the context) and the drive key's id are bound to the ciphertext.
/// - **Object:** `"AZM1" | key id (16) | nonce (24) | ciphertext | tag (16)`. An object sealed
///   under another drive key (one from before a rotation) is refused by its key id before
///   anything is decrypted.
/// - **Names:** `BLAKE3 keyed_hash(names, data)`.
#[cfg(feature = "encryption")]
mod drive_key {
    use chacha20poly1305::{
        aead::{Aead, Payload},
        Key, KeyInit, XChaCha20Poly1305, XNonce,
    };

    use super::{SealError, Sealer};
    use crate::crypto::{random_bytes, DriveKey, KEY_ID_LEN, NONCE_LEN, TAG_LEN};

    const SEAL_CONTEXT: &str = "Azlin 2026-10-10 drive key: metadata repository sealing";
    const NAME_CONTEXT: &str = "Azlin 2026-10-10 drive key: metadata repository names";
    const MAGIC: &[u8; 4] = b"AZM1";
    const AAD_LABEL: &[u8] = b"AZM1 sealed metadata v1";
    /// Magic, key id, nonce.
    const HEAD: usize = 4 + KEY_ID_LEN + NONCE_LEN;

    fn aad(key_id: &[u8], context: &[u8]) -> Vec<u8> {
        let mut aad = Vec::with_capacity(AAD_LABEL.len() + key_id.len() + context.len());
        aad.extend_from_slice(AAD_LABEL);
        aad.extend_from_slice(key_id);
        aad.extend_from_slice(context);
        aad
    }

    impl Sealer for DriveKey {
        fn seal(&self, context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
            let mut nonce = [0u8; NONCE_LEN];
            random_bytes(&mut nonce)
                .map_err(|_| SealError::new("the system gave no random bytes"))?;
            let id = self.id();
            let key = self.derive(SEAL_CONTEXT);
            let sealed = XChaCha20Poly1305::new(Key::from_slice(&key[..]))
                .encrypt(
                    XNonce::from_slice(&nonce),
                    Payload {
                        msg: plaintext,
                        aad: &aad(&id.0, context),
                    },
                )
                .map_err(|_| SealError::new("the cipher refused to seal"))?;
            let mut out = Vec::with_capacity(HEAD + sealed.len());
            out.extend_from_slice(MAGIC);
            out.extend_from_slice(&id.0);
            out.extend_from_slice(&nonce);
            out.extend_from_slice(&sealed);
            Ok(out)
        }

        fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, SealError> {
            if sealed.len() < HEAD + TAG_LEN || &sealed[..4] != MAGIC {
                return Err(SealError::new("not a sealed metadata object"));
            }
            let id = self.id();
            if sealed[4..4 + KEY_ID_LEN] != id.0 {
                return Err(SealError::new("sealed under another drive key"));
            }
            let key = self.derive(SEAL_CONTEXT);
            XChaCha20Poly1305::new(Key::from_slice(&key[..]))
                .decrypt(
                    XNonce::from_slice(&sealed[4 + KEY_ID_LEN..HEAD]),
                    Payload {
                        msg: &sealed[HEAD..],
                        aad: &aad(&id.0, context),
                    },
                )
                .map_err(|_| SealError::new("the object does not authenticate in this place"))
        }

        fn name_hash(&self, data: &[u8]) -> [u8; 32] {
            *blake3::keyed_hash(&self.derive(NAME_CONTEXT), data).as_bytes()
        }
    }
}
