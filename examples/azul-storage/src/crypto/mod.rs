//! Client-side encryption (feature `encryption`): what an encrypted drive keeps in its bucket
//! is ciphertext under random names; every key that opens it lives on the user's devices.
//!
//! The keys, from the top:
//!
//! - The DRIVE KEY ([`DriveKey`], 256 random bits) belongs to one drive. It never leaves a
//!   device in the clear: the bucket holds it sealed to each member's X25519 key
//!   (`.azlin/keys/<member>.key`) and to the drive's recovery code (Argon2id,
//!   `.azlin/keys/recovery.key`); see [`keys`].
//! - Every FILE VERSION gets its own FILE KEY ([`FileKey`], 256 random bits; a write is a new
//!   object with a new key, a key never seals two objects). The drive key wraps it
//!   ([`WrappedKey`]); the wrap rides in the object's header and in the drive's index.
//! - A SHARE KEY ([`ShareKey`]) wraps the file keys of what is shared and never the drive key:
//!   whoever holds it opens those files and nothing else.
//!
//! The objects ([`azl1`]): a 4 KiB header, the file in 1 MiB segments - each compressed when
//! that saves 5 % ([`codec`]), each sealed with XChaCha20-Poly1305 in the STREAM construction -
//! and an encrypted trailer of the segment lengths. An object's bucket key is a random 128-bit
//! id, `data/<2 hex digits>/<32 hex digits>` ([`ObjectId`]): no name, path or folder of the
//! drive ever reaches the bucket.
//!
//! No primitive is made here; each comes from its crate: XChaCha20-Poly1305 and aead's STREAM
//! (`chacha20poly1305`), X25519 (`x25519-dalek`), BLAKE3's hash, keyed hash and key derivation
//! (`blake3`), Argon2id (`argon2`), the OS random source (`getrandom`). Every key derived from
//! another one goes through BLAKE3's `derive_key` with a context string of its own (the
//! `*_CONTEXT` constants): one key, one purpose. Secrets wipe themselves when dropped
//! (`zeroize`) and print as `***`; nothing here logs.

pub mod azl1;
pub mod codec;
pub mod device;
pub mod keys;

use std::fmt;

use chacha20poly1305::{
    aead::{Aead, Payload},
    Key, KeyInit, XChaCha20Poly1305, XNonce,
};
use zeroize::Zeroize;
/// The wiped-when-dropped buffer the secrets here come back in (a recovery code's text, an
/// invite's secret): zeroize's, for apps without a dependency of their own on it.
pub use zeroize::Zeroizing;

use crate::DriveError;

/// Bytes of every key here (256 bits).
pub const KEY_LEN: usize = 32;
/// Bytes of an XChaCha20-Poly1305 nonce: 192 bits, so random nonces never meet in practice.
pub const NONCE_LEN: usize = 24;
/// Bytes of a Poly1305 tag.
pub const TAG_LEN: usize = 16;
/// Bytes of an object id (128 random bits).
pub const OBJECT_ID_LEN: usize = 16;
/// Bytes of a key id.
pub const KEY_ID_LEN: usize = 16;

/// The folder of the data objects in the bucket.
pub const DATA_PREFIX: &str = "data/";

/// BLAKE3 `derive_key` contexts of the drive key: its public id, the key that wraps file keys.
const DRIVE_KEY_ID_CONTEXT: &str = "Azlin 2026-10-08 drive key: key id";
const DRIVE_KEY_WRAP_CONTEXT: &str = "Azlin 2026-10-08 drive key: file key wrapping";
/// The same for a share key (other contexts: the same bytes as a drive key and as a share key
/// give unrelated ids and wrapping keys).
const SHARE_KEY_ID_CONTEXT: &str = "Azlin 2026-10-08 share key: key id";
const SHARE_KEY_WRAP_CONTEXT: &str = "Azlin 2026-10-08 share key: file key wrapping";
/// The first bytes of a wrapped file key's associated data.
const FILE_KEY_AAD_LABEL: &[u8] = b"AZL1 wrapped file key v1";

/// Why something cannot be sealed or opened. Never holds a secret or plaintext.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CryptoError {
    /// The key given is not the one that sealed it: another drive's key, another member's
    /// key, a wrong recovery code, a share key that does not cover the file.
    WrongKey,
    /// The file key opened the wrap but is not the key the object commits to (its header's
    /// key commitment): the object is not the one the key belongs to.
    KeyMismatch,
    /// The bytes are damaged or were changed: a part does not authenticate, the object is
    /// cut short, its segments were reordered, its lengths do not add up.
    Damaged(String),
    /// A version, codec, segment size or cost this build does not read.
    Unsupported(String),
    /// The OS random source failed.
    Random,
    /// Reading or writing the bytes failed.
    Io(String),
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::WrongKey => f.write_str("the key does not open it"),
            CryptoError::KeyMismatch => {
                f.write_str("the key is not the one it was encrypted with (key commitment)")
            }
            CryptoError::Damaged(why) => write!(f, "it is damaged or was changed ({why})"),
            CryptoError::Unsupported(why) => {
                write!(f, "this version cannot read it ({why})")
            }
            CryptoError::Random => f.write_str("the system's random source failed"),
            CryptoError::Io(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for CryptoError {}

impl CryptoError {
    /// The error as a drive's error about `key`: a wrong key is a refusal, a damaged object
    /// [`DriveError::Corrupt`], everything else what it is.
    #[must_use]
    pub fn for_key(self, key: &str) -> DriveError {
        match self {
            CryptoError::WrongKey => DriveError::Denied {
                message: format!("this drive's key does not open \"{key}\""),
            },
            CryptoError::KeyMismatch => DriveError::Corrupt {
                key: key.to_string(),
                reason: String::from("it is not the object its key belongs to (key commitment)"),
            },
            CryptoError::Damaged(why) => DriveError::Corrupt {
                key: key.to_string(),
                reason: why,
            },
            CryptoError::Unsupported(why) => DriveError::Unsupported(format!("\"{key}\": {why}")),
            CryptoError::Random => {
                DriveError::Io(String::from("the system's random source failed"))
            }
            CryptoError::Io(why) => DriveError::Io(format!("{key}: {why}")),
        }
    }
}

impl From<std::io::Error> for CryptoError {
    /// A `CryptoError` that travelled through `io::Read` / `io::Write` comes back as itself.
    fn from(e: std::io::Error) -> Self {
        if let Some(inner) = e
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<CryptoError>())
        {
            return inner.clone();
        }
        CryptoError::Io(e.to_string())
    }
}

impl From<CryptoError> for std::io::Error {
    fn from(e: CryptoError) -> Self {
        let kind = match e {
            CryptoError::Io(_) => std::io::ErrorKind::Other,
            _ => std::io::ErrorKind::InvalidData,
        };
        std::io::Error::new(kind, e)
    }
}

/// Fills `buf` from the OS random source.
pub fn random_bytes(buf: &mut [u8]) -> Result<(), CryptoError> {
    getrandom::getrandom(buf).map_err(|_| CryptoError::Random)
}

/// Whether two secrets are equal, in constant time: blake3's `Hash` compares with
/// `constant_time_eq`, which never stops at the first differing byte.
fn same_secret(a: &[u8; KEY_LEN], b: &[u8; KEY_LEN]) -> bool {
    blake3::Hash::from_bytes(*a) == *b
}

/// A 256-bit secret key type: wiped when dropped, printed as `Name(***)`, compared in
/// constant time.
macro_rules! secret_key {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        pub struct $name([u8; KEY_LEN]);

        impl $name {
            /// A new key from the OS random source.
            pub fn generate() -> Result<$name, CryptoError> {
                let mut key = $name([0u8; KEY_LEN]);
                random_bytes(&mut key.0)?;
                Ok(key)
            }

            /// The key of these bytes (from the OS keyring, or opened from a wrap). Wipe the
            /// caller's copy.
            #[must_use]
            pub fn from_bytes(bytes: [u8; KEY_LEN]) -> $name {
                $name(bytes)
            }

            /// The key of exactly [`KEY_LEN`] bytes.
            #[must_use]
            pub fn from_slice(bytes: &[u8]) -> Option<$name> {
                let bytes: [u8; KEY_LEN] = bytes.try_into().ok()?;
                Some($name(bytes))
            }

            /// The key's bytes: for the OS keyring only, never for a file or a log.
            #[must_use]
            pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
                &self.0
            }
        }

        impl Clone for $name {
            fn clone(&self) -> $name {
                $name(self.0)
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                self.0.zeroize();
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($name), "(***)"))
            }
        }

        impl PartialEq for $name {
            /// Constant time over the key's bytes.
            fn eq(&self, other: &$name) -> bool {
                same_secret(&self.0, &other.0)
            }
        }

        impl Eq for $name {}
    };
}

secret_key! {
    /// The key of one file version (one AZL1 object): 256 random bits the writer makes and
    /// uses for that object only. Its subkeys and its commitment are derived in [`azl1`].
    FileKey
}

secret_key! {
    /// The key of a drive. It wraps every file key ([`DriveKey::wrap_file_key`]) and is itself
    /// kept in the bucket only sealed: to each member's X25519 key and to the recovery code
    /// ([`keys`]). Rotating it re-wraps the file keys in the index; the objects stay.
    DriveKey
}

secret_key! {
    /// The key of a share: it wraps only the file keys of what is shared, never the drive key,
    /// so it opens those files and nothing else.
    ShareKey
}

/// The public id of a drive key or a share key: 16 bytes of a BLAKE3 key derivation from it.
/// It says which key sealed something (an object's header, a member's copy of the drive key)
/// and nothing about the key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyId(pub [u8; KEY_ID_LEN]);

impl KeyId {
    /// The id of `key` under `context` (the key kind's id context).
    fn derive(context: &str, key: &[u8; KEY_LEN]) -> KeyId {
        let full = Zeroizing::new(blake3::derive_key(context, key));
        let mut id = [0u8; KEY_ID_LEN];
        id.copy_from_slice(&full[..KEY_ID_LEN]);
        KeyId(id)
    }

    /// Lowercase hex, 32 digits.
    #[must_use]
    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }

    /// The id of 32 hex digits.
    #[must_use]
    pub fn from_hex(text: &str) -> Option<KeyId> {
        hex_array(text).map(KeyId)
    }
}

impl fmt::Debug for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeyId({})", self.to_hex())
    }
}

impl fmt::Display for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// The random 128-bit id of an AZL1 object. Its key in the bucket is
/// `data/<first 2 hex digits>/<32 hex digits>` ([`ObjectId::bucket_key`]): the two-digit
/// fan-out spreads the objects over 256 prefixes, and nothing in it comes from the file.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(pub [u8; OBJECT_ID_LEN]);

impl ObjectId {
    /// A new id from the OS random source.
    pub fn generate() -> Result<ObjectId, CryptoError> {
        let mut id = [0u8; OBJECT_ID_LEN];
        random_bytes(&mut id)?;
        Ok(ObjectId(id))
    }

    /// Lowercase hex, 32 digits.
    #[must_use]
    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }

    /// The id of 32 hex digits.
    #[must_use]
    pub fn from_hex(text: &str) -> Option<ObjectId> {
        hex_array(text).map(ObjectId)
    }

    /// `data/ab/ab12...`: the object's key in the bucket.
    #[must_use]
    pub fn bucket_key(&self) -> String {
        let hex = self.to_hex();
        format!("{DATA_PREFIX}{}/{hex}", &hex[..2])
    }

    /// The id of a bucket key [`ObjectId::bucket_key`] made (exactly that form).
    #[must_use]
    pub fn from_bucket_key(key: &str) -> Option<ObjectId> {
        let (_, hex) = key.strip_prefix(DATA_PREFIX)?.split_once('/')?;
        let id = ObjectId::from_hex(hex)?;
        (id.bucket_key() == key).then_some(id)
    }
}

impl fmt::Debug for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ObjectId({})", self.to_hex())
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// A file key sealed under a drive key or a share key: the id of the key that sealed it, a
/// random nonce, and the sealed key with its tag. It opens only for the object it was made
/// for (the object id is in its associated data).
#[derive(Clone, PartialEq, Eq)]
pub struct WrappedKey {
    pub key_id: KeyId,
    pub nonce: [u8; NONCE_LEN],
    pub sealed: [u8; KEY_LEN + TAG_LEN],
}

impl WrappedKey {
    /// Bytes of [`WrappedKey::to_bytes`]: key id, nonce, sealed key and tag.
    pub const LEN: usize = KEY_ID_LEN + NONCE_LEN + KEY_LEN + TAG_LEN;

    /// The key id, the nonce, the sealed key and its tag, in this order.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; WrappedKey::LEN] {
        let mut out = [0u8; WrappedKey::LEN];
        out[..KEY_ID_LEN].copy_from_slice(&self.key_id.0);
        out[KEY_ID_LEN..KEY_ID_LEN + NONCE_LEN].copy_from_slice(&self.nonce);
        out[KEY_ID_LEN + NONCE_LEN..].copy_from_slice(&self.sealed);
        out
    }

    /// The wrap of exactly [`WrappedKey::LEN`] bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<WrappedKey> {
        if bytes.len() != WrappedKey::LEN {
            return None;
        }
        let key_id: [u8; KEY_ID_LEN] = bytes[..KEY_ID_LEN].try_into().ok()?;
        let nonce: [u8; NONCE_LEN] = bytes[KEY_ID_LEN..KEY_ID_LEN + NONCE_LEN].try_into().ok()?;
        let sealed: [u8; KEY_LEN + TAG_LEN] = bytes[KEY_ID_LEN + NONCE_LEN..].try_into().ok()?;
        Some(WrappedKey {
            key_id: KeyId(key_id),
            nonce,
            sealed,
        })
    }

    /// [`WrappedKey::to_bytes`] as lowercase hex (for text formats).
    #[must_use]
    pub fn to_hex(&self) -> String {
        to_hex(&self.to_bytes())
    }

    /// The wrap of [`WrappedKey::to_hex`]'s text.
    #[must_use]
    pub fn from_hex(text: &str) -> Option<WrappedKey> {
        WrappedKey::from_bytes(&from_hex(text)?)
    }
}

impl fmt::Debug for WrappedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WrappedKey")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

/// The associated data of a wrapped file key: a label, the object id and the id of the key
/// that wraps it. A wrap moved to another object, or relabelled with another key's id, does
/// not open.
fn file_key_aad(object: &ObjectId, key_id: &KeyId) -> Vec<u8> {
    let mut aad = Vec::with_capacity(FILE_KEY_AAD_LABEL.len() + OBJECT_ID_LEN + KEY_ID_LEN);
    aad.extend_from_slice(FILE_KEY_AAD_LABEL);
    aad.extend_from_slice(&object.0);
    aad.extend_from_slice(&key_id.0);
    aad
}

/// `file_key` sealed with XChaCha20-Poly1305 under `wrapping_key` (a key derived for wrapping
/// only) and a fresh random 192-bit nonce.
fn wrap_with(
    wrapping_key: &[u8; KEY_LEN],
    key_id: KeyId,
    file_key: &FileKey,
    object: &ObjectId,
) -> Result<WrappedKey, CryptoError> {
    let mut nonce = [0u8; NONCE_LEN];
    random_bytes(&mut nonce)?;
    let aad = file_key_aad(object, &key_id);
    let sealed = XChaCha20Poly1305::new(Key::from_slice(wrapping_key))
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: file_key.as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|_| CryptoError::Damaged(String::from("the cipher refused to seal a key")))?;
    let sealed: [u8; KEY_LEN + TAG_LEN] = sealed
        .as_slice()
        .try_into()
        .map_err(|_| CryptoError::Damaged(String::from("a sealed key of the wrong length")))?;
    Ok(WrappedKey {
        key_id,
        nonce,
        sealed,
    })
}

/// The file key `wrapped` holds for `object`, opened with `wrapping_key`. `WrongKey` when
/// another key sealed it; `Damaged` when it was changed or belongs to another object.
fn unwrap_with(
    wrapping_key: &[u8; KEY_LEN],
    key_id: KeyId,
    wrapped: &WrappedKey,
    object: &ObjectId,
) -> Result<FileKey, CryptoError> {
    if wrapped.key_id != key_id {
        return Err(CryptoError::WrongKey);
    }
    let aad = file_key_aad(object, &key_id);
    let plain = XChaCha20Poly1305::new(Key::from_slice(wrapping_key))
        .decrypt(
            XNonce::from_slice(&wrapped.nonce),
            Payload {
                msg: &wrapped.sealed,
                aad: &aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| {
            CryptoError::Damaged(String::from(
                "the wrapped file key does not authenticate for this object",
            ))
        })?;
    FileKey::from_slice(&plain)
        .ok_or_else(|| CryptoError::Damaged(String::from("a wrapped file key is not 32 bytes")))
}

impl DriveKey {
    /// This key's public id.
    #[must_use]
    pub fn id(&self) -> KeyId {
        KeyId::derive(DRIVE_KEY_ID_CONTEXT, &self.0)
    }

    /// A key derived from this one for `context` (BLAKE3 `derive_key`, the drive key as the
    /// key material): every use of the drive key goes through a key of its own.
    #[must_use]
    pub(crate) fn derive(&self, context: &str) -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new(blake3::derive_key(context, &self.0))
    }

    /// `file_key` wrapped for `object` with a fresh random nonce: what the object's header
    /// and the drive's index keep.
    pub fn wrap_file_key(
        &self,
        file_key: &FileKey,
        object: &ObjectId,
    ) -> Result<WrappedKey, CryptoError> {
        wrap_with(
            &self.derive(DRIVE_KEY_WRAP_CONTEXT),
            self.id(),
            file_key,
            object,
        )
    }

    /// The file key of `object` that `wrapped` holds. `WrongKey` when another key wrapped
    /// it, `Damaged` when it was changed or belongs to another object.
    pub fn unwrap_file_key(
        &self,
        wrapped: &WrappedKey,
        object: &ObjectId,
    ) -> Result<FileKey, CryptoError> {
        unwrap_with(
            &self.derive(DRIVE_KEY_WRAP_CONTEXT),
            self.id(),
            wrapped,
            object,
        )
    }
}

impl ShareKey {
    /// This key's public id.
    #[must_use]
    pub fn id(&self) -> KeyId {
        KeyId::derive(SHARE_KEY_ID_CONTEXT, &self.0)
    }

    /// `file_key` wrapped for `object` under this share key (the drive key stays out of the
    /// share).
    pub fn wrap_file_key(
        &self,
        file_key: &FileKey,
        object: &ObjectId,
    ) -> Result<WrappedKey, CryptoError> {
        let wrapping_key = Zeroizing::new(blake3::derive_key(SHARE_KEY_WRAP_CONTEXT, &self.0));
        wrap_with(&wrapping_key, self.id(), file_key, object)
    }

    /// The file key of `object` that `wrapped` holds: `WrongKey` for a file this share does
    /// not cover (its key was wrapped by another key).
    pub fn unwrap_file_key(
        &self,
        wrapped: &WrappedKey,
        object: &ObjectId,
    ) -> Result<FileKey, CryptoError> {
        let wrapping_key = Zeroizing::new(blake3::derive_key(SHARE_KEY_WRAP_CONTEXT, &self.0));
        unwrap_with(&wrapping_key, self.id(), wrapped, object)
    }
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// Lowercase hex.
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 0x0f)]));
    }
    out
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// The bytes of hex text (either case); `None` for anything else.
pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    if text.len() % 2 != 0 {
        return None;
    }
    text.chunks(2)
        .map(|pair| Some((hex_digit(pair[0])? << 4) | hex_digit(pair[1])?))
        .collect()
}

/// Exactly `N` bytes of hex text.
pub(crate) fn hex_array<const N: usize>(text: &str) -> Option<[u8; N]> {
    from_hex(text)?.try_into().ok()
}
