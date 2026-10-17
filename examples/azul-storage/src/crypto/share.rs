//! A share's MANIFEST (AZS1): what a link to files of an encrypted drive opens. The manifest
//! lists the shared files - their names relative to the share, their objects, sizes, hashes and
//! file keys wrapped by the SHARE KEY ([`ShareKey`], never the drive key) - and is itself sealed
//! with a key derived from the share key. Whoever holds the share key (the link's fragment)
//! reads these files and nothing else; the bucket holds the manifest as ciphertext.
//!
//! ```text
//!   bytes  0   4  magic "AZS1"
//!          4   1  version 1
//!          5  24  nonce (random)
//!         29   *  XChaCha20-Poly1305(the manifest's JSON), then its 16-byte tag
//!   key = BLAKE3 derive_key(SHARE_MANIFEST_CONTEXT, share key)
//!   aad = "AZS1" || version || the share's id (the share key's id, 16 bytes)
//!   JSON {"v":1,"created":<secs>,"expires":<secs>|null,"title":<text>|null,
//!         "entries":[{"name":"<relative path>","object":"<32 hex>","stored_size":<n>,
//!                     "size":<n>,"blake3":"<64 hex>","wrapped_key":"<176 hex>",
//!                     "url":"<a presigned GET of the object>"|absent}]}
//! ```
//!
//! The share's id is the share key's public id ([`ShareKey::id`], the `key_id` of every wrapped
//! key in the manifest); the manifest lives at `.azlin/shares/<id hex>.azs`
//! ([`manifest_key`]). The share key travels in a link's fragment as 64 hex digits
//! ([`share_key_text`]): browsers never send the fragment to a server.

use chacha20poly1305::{
    aead::{Aead, Payload},
    Key, KeyInit, XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{
    from_hex, hex_array, random_bytes, to_hex, CryptoError, KeyId, ObjectId, ShareKey, WrappedKey,
    KEY_LEN, NONCE_LEN, TAG_LEN,
};

/// The folder of the share manifests in the bucket.
pub const SHARES_PREFIX: &str = ".azlin/shares/";
pub const MAGIC: &[u8; 4] = b"AZS1";
pub const VERSION: u8 = 1;
/// Bytes before the sealed JSON.
pub const HEADER_LEN: usize = 4 + 1 + NONCE_LEN;
/// BLAKE3 `derive_key` context of the key that seals a share's manifest.
pub const SHARE_MANIFEST_CONTEXT: &str = "Azlin 2026-10-10 share key: manifest sealing";
/// The largest manifest opened (a share of some ten thousand files).
pub const MAX_MANIFEST_LEN: usize = 16 << 20;

fn damaged(why: impl Into<String>) -> CryptoError {
    CryptoError::Damaged(why.into())
}

/// One shared file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareEntry {
    /// Its name in the share: a path relative to the share (`report.pdf`,
    /// `Photos/2026/a.jpg`).
    pub name: String,
    pub object: ObjectId,
    /// The object's bytes in the bucket.
    pub stored_size: u64,
    /// The plaintext's bytes.
    pub size: u64,
    /// BLAKE3 of the plaintext.
    pub blake3: [u8; 32],
    /// The file key, wrapped by the share key.
    pub wrapped_key: WrappedKey,
    /// Where the object downloads without credentials (a presigned GET), when the share was
    /// made with presigned links; `None` when a server hands the links out.
    pub url: Option<String>,
}

/// What a share opens (see the module documentation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareManifest {
    /// When it was made, in seconds since 1970.
    pub created: u64,
    /// When its links stop working (presigned links: at most seven days after `created`).
    pub expires: Option<u64>,
    /// What the share is called on its page (the sharer's words; inside the ciphertext).
    pub title: Option<String>,
    pub entries: Vec<ShareEntry>,
}

#[derive(Serialize, Deserialize)]
struct EntryJson {
    name: String,
    object: String,
    stored_size: u64,
    size: u64,
    blake3: String,
    wrapped_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ManifestJson {
    v: u32,
    created: u64,
    expires: Option<u64>,
    title: Option<String>,
    entries: Vec<EntryJson>,
}

/// `.azlin/shares/<id hex>.azs`.
#[must_use]
pub fn manifest_key(share: &KeyId) -> String {
    format!("{SHARES_PREFIX}{}.azs", share.to_hex())
}

/// The share id of a manifest's bucket key; `None` for any other key.
#[must_use]
pub fn share_of_manifest_key(key: &str) -> Option<KeyId> {
    KeyId::from_hex(key.strip_prefix(SHARES_PREFIX)?.strip_suffix(".azs")?)
}

/// The share key as a link's fragment carries it: 64 hex digits. A secret.
#[must_use]
pub fn share_key_text(key: &ShareKey) -> Zeroizing<String> {
    Zeroizing::new(to_hex(key.as_bytes()))
}

/// The share key of [`share_key_text`]'s text.
#[must_use]
pub fn share_key_from_text(text: &str) -> Option<ShareKey> {
    hex_array::<KEY_LEN>(text.trim())
        .map(Zeroizing::new)
        .map(|bytes| ShareKey::from_bytes(*bytes))
}

fn manifest_aad(share: &KeyId) -> Vec<u8> {
    let mut aad = MAGIC.to_vec();
    aad.push(VERSION);
    aad.extend_from_slice(&share.0);
    aad
}

impl ShareManifest {
    /// The AZS1 bytes of the manifest, sealed for `key`'s share.
    pub fn seal(&self, key: &ShareKey) -> Result<Vec<u8>, CryptoError> {
        let json = ManifestJson {
            v: 1,
            created: self.created,
            expires: self.expires,
            title: self.title.clone(),
            entries: self
                .entries
                .iter()
                .map(|e| EntryJson {
                    name: e.name.clone(),
                    object: e.object.to_hex(),
                    stored_size: e.stored_size,
                    size: e.size,
                    blake3: to_hex(&e.blake3),
                    wrapped_key: e.wrapped_key.to_hex(),
                    url: e.url.clone(),
                })
                .collect(),
        };
        let plain = Zeroizing::new(serde_json::to_vec(&json).unwrap_or_default());
        let mut nonce = [0u8; NONCE_LEN];
        random_bytes(&mut nonce)?;
        let sealing = key.derive(SHARE_MANIFEST_CONTEXT);
        let sealed = XChaCha20Poly1305::new(Key::from_slice(&sealing[..]))
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plain,
                    aad: &manifest_aad(&key.id()),
                },
            )
            .map_err(|_| damaged("the cipher refused to seal the manifest"))?;
        let mut out = Vec::with_capacity(HEADER_LEN + sealed.len());
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&sealed);
        Ok(out)
    }

    /// The manifest of AZS1 `bytes`, opened with the share key. `WrongKey` for another share's
    /// key, `Damaged` for changed bytes, `Unsupported` for another version.
    pub fn open(bytes: &[u8], key: &ShareKey) -> Result<ShareManifest, CryptoError> {
        if bytes.len() < HEADER_LEN + TAG_LEN || &bytes[..4] != MAGIC {
            return Err(damaged("not a share manifest"));
        }
        if bytes[4] != VERSION {
            return Err(CryptoError::Unsupported(format!(
                "share manifest version {}",
                bytes[4]
            )));
        }
        if bytes.len() > MAX_MANIFEST_LEN {
            return Err(damaged("a share manifest larger than any share"));
        }
        let sealing = key.derive(SHARE_MANIFEST_CONTEXT);
        let plain = XChaCha20Poly1305::new(Key::from_slice(&sealing[..]))
            .decrypt(
                XNonce::from_slice(&bytes[5..HEADER_LEN]),
                Payload {
                    msg: &bytes[HEADER_LEN..],
                    aad: &manifest_aad(&key.id()),
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| CryptoError::WrongKey)?;
        let json: ManifestJson =
            serde_json::from_slice(&plain).map_err(|_| damaged("a share manifest's contents"))?;
        if json.v != 1 {
            return Err(CryptoError::Unsupported(format!(
                "share manifest contents version {}",
                json.v
            )));
        }
        let field = |what: &str| damaged(format!("a share entry's {what}"));
        let entries = json
            .entries
            .into_iter()
            .map(|e| {
                Ok(ShareEntry {
                    object: ObjectId::from_hex(&e.object).ok_or_else(|| field("object"))?,
                    blake3: hex_array(&e.blake3).ok_or_else(|| field("hash"))?,
                    wrapped_key: from_hex(&e.wrapped_key)
                        .as_deref()
                        .and_then(WrappedKey::from_bytes)
                        .ok_or_else(|| field("wrapped key"))?,
                    name: e.name,
                    stored_size: e.stored_size,
                    size: e.size,
                    url: e.url,
                })
            })
            .collect::<Result<Vec<_>, CryptoError>>()?;
        Ok(ShareManifest {
            created: json.created,
            expires: json.expires,
            title: json.title,
            entries,
        })
    }
}
