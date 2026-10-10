//! Incoming mail for an encrypted drive: the customer's Cloudflare Email Worker
//! (examples/azlin-mail-worker) seals each message to the drive's DROP KEY and puts it into the
//! bucket; a device of the drive opens it, files it into the encrypted mail folders and deletes
//! the drop. The bucket - and the Worker's own storage - never holds a message in the clear.
//!
//! - The DROP KEY ([`DropSecret`]) is an X25519 key pair per drive. Its public half goes into
//!   the customer's Worker configuration (set by the app with the customer's own Cloudflare API
//!   token, never through Azlin); its secret stays in the bucket sealed with the drive key,
//!   `.azlin/keys/_drop.key` ([`DropKeyFile`]): XChaCha20-Poly1305 under the drive key's
//!   derivation [`DROP_KEY_WRAP_CONTEXT`] and a random 192-bit nonce, the associated data
//!   binding the drive, the public key and the drive key's id. The name starts with `_`, which
//!   no member id can, so it never meets a member's `<member>.key`.
//! - A DROP is one AZD1 object at `.azlin/drop/<32 hex digits of 128 random bits>` (no address,
//!   date or subject in its name). The Worker has Web Crypto only (no ChaCha20, no BLAKE3), so
//!   AZD1 is X25519, HKDF-SHA256 and AES-256-GCM:
//!
//! ```text
//!   bytes   0   4  magic "AZD1"
//!           4   1  version 1
//!           5  16  the drop key's id: SHA-256("Azlin AZD1 drop key id" || drop public key)[..16]
//!          21  32  E: a fresh X25519 public key, made for this message only
//!          53  12  the AES-GCM nonce (random; the key is new per message anyway, so it never
//!                  meets another nonce under the same key)
//!          65   *  AES-256-GCM(plaintext), then its 16-byte tag
//!   key       = HKDF-SHA256(ikm = X25519(e, drop public key), salt = E || drop public key,
//!                           info = "Azlin AZD1 drop v1"), 32 bytes; a non-contributory
//!                           (low-order) X25519 result is refused on both sides
//!   aad       = bytes 0..65 || u32 LE length || drive id || u32 LE length || object key
//!   plaintext = {"v":1,"received":<seconds since 1970>,"folder":"Inbox"|"Spam"} "\n" message
//! ```
//!
//! - Anyone with the public key can make a drop: what keeps strangers out is the bucket's write
//!   credential (the Worker's). A drop says nothing about who sent the mail - the message's own
//!   headers do, as for any mail.
//! - [`ingest`] lists the drops, opens each, hands it to the caller (who writes the message into
//!   the encrypted drive under its stable name, so a drop ingested twice lands on the same
//!   name) and deletes it. A drop sealed to another drop key (an old one, from before a key
//!   rotation) or damaged stays and is reported.
//! - The test vector (examples/azlin-mail-worker/test/azd1-vector.json) is checked here, by the
//!   Worker's tests and by scripts/azlin_drop.py.

use std::fmt;

use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit, Nonce,
};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use super::{
    hex_array, keys::associated_data, random_bytes, to_hex, CryptoError, DriveKey, KeyId, KEY_LEN, NONCE_LEN, TAG_LEN,
};
use crate::{ops::list_all, Drive, DriveError};

/// The folder of the drops in the bucket.
pub const DROP_PREFIX: &str = ".azlin/drop/";
/// The drop key's file in the bucket.
pub const DROP_KEY_FILE: &str = ".azlin/keys/_drop.key";
/// The drop key before a key rotation replaced it, kept (sealed with the new drive key) for the
/// drops the Worker sealed to it until it got the new one.
pub const PREVIOUS_DROP_KEY_FILE: &str = ".azlin/keys/_drop-previous.key";
/// The first bytes of a drop.
pub const MAGIC: &[u8; 4] = b"AZD1";
pub const VERSION: u8 = 1;
/// Bytes of a drop key's id.
pub const DROP_KEY_ID_LEN: usize = 16;
/// Bytes of the AES-GCM nonce.
pub const GCM_NONCE_LEN: usize = 12;
/// Bytes of a drop's header (the associated data's first part).
pub const HEADER_LEN: usize = 4 + 1 + DROP_KEY_ID_LEN + KEY_LEN + GCM_NONCE_LEN;
/// The largest drop opened (a Cloudflare Email Worker takes messages up to 25 MiB).
pub const MAX_DROP_LEN: u64 = 64 << 20;
/// The longest header line of a drop's plaintext.
const MAX_HEADER_LINE: usize = 1024;

/// What a drop key's id is the SHA-256 of, before the public key.
const KEY_ID_LABEL: &[u8] = b"Azlin AZD1 drop key id";
/// The HKDF info of a drop's AES key.
const HKDF_INFO: &[u8] = b"Azlin AZD1 drop v1";
/// BLAKE3 `derive_key` context of the key that seals the drop key's secret in the bucket.
pub const DROP_KEY_WRAP_CONTEXT: &str = "Azlin 2026-10-10 drive key: drop key wrapping";
/// The first bytes of the drop key file's associated data.
const DROP_KEY_AAD_LABEL: &[u8] = b"Azlin drop key v1";
const FORMAT: &str = "azlin-drive-key";
const FILE_VERSION: u32 = 1;
const KIND_DROP: &str = "drop";

fn damaged(why: impl Into<String>) -> CryptoError {
    CryptoError::Damaged(why.into())
}

/// A drive's drop key: an X25519 secret. Wiped when dropped, never printed.
pub struct DropSecret(StaticSecret);

impl DropSecret {
    /// A new key pair from the OS random source.
    pub fn generate() -> Result<DropSecret, CryptoError> {
        let mut bytes = Zeroizing::new([0u8; KEY_LEN]);
        random_bytes(&mut bytes[..])?;
        Ok(DropSecret(StaticSecret::from(*bytes)))
    }

    /// The secret of these bytes. Wipe the caller's copy.
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> DropSecret {
        DropSecret(StaticSecret::from(bytes))
    }

    #[must_use]
    pub fn to_bytes(&self) -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new(self.0.to_bytes())
    }

    /// The public half, which the Worker seals to.
    #[must_use]
    pub fn public(&self) -> DropPublic {
        DropPublic(PublicKey::from(&self.0).to_bytes())
    }
}

impl fmt::Debug for DropSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DropSecret(***)")
    }
}

/// A drive's drop public key: what the Worker's configuration holds (64 hex digits).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DropPublic(pub [u8; KEY_LEN]);

impl DropPublic {
    /// The key's id in every drop sealed to it.
    #[must_use]
    pub fn key_id(&self) -> [u8; DROP_KEY_ID_LEN] {
        let digest = Sha256::new()
            .chain_update(KEY_ID_LABEL)
            .chain_update(self.0)
            .finalize();
        let mut id = [0u8; DROP_KEY_ID_LEN];
        id.copy_from_slice(&digest[..DROP_KEY_ID_LEN]);
        id
    }

    #[must_use]
    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }

    #[must_use]
    pub fn from_hex(text: &str) -> Option<DropPublic> {
        hex_array(text.trim()).map(DropPublic)
    }
}

impl fmt::Debug for DropPublic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DropPublic({})", self.to_hex())
    }
}

/// The folder a drop's message goes to (the Worker's spam verdict).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DropFolder {
    Inbox,
    Spam,
}

impl DropFolder {
    /// The drive's mail folder (`mail/<name>/`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            DropFolder::Inbox => "Inbox",
            DropFolder::Spam => "Spam",
        }
    }
}

/// The header line of a drop's plaintext. Its field order is the bytes' order (the test
/// vector): `v`, `received`, `folder`.
#[derive(Serialize, Deserialize)]
struct DropHeader {
    v: u32,
    received: u64,
    folder: DropFolder,
}

/// An opened drop.
pub struct Dropped {
    /// When the Worker received the message, in seconds since 1970 (its name's stamp).
    pub received: u64,
    pub folder: DropFolder,
    /// The RFC 5322 message, exactly as it arrived. Wiped when dropped.
    pub raw: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for Dropped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dropped")
            .field("received", &self.received)
            .field("folder", &self.folder)
            .field("bytes", &self.raw.len())
            .finish()
    }
}

/// Whether `key` is a drop's object key: [`DROP_PREFIX`] and 32 lowercase hex digits.
#[must_use]
pub fn is_drop_key(key: &str) -> bool {
    key.strip_prefix(DROP_PREFIX).is_some_and(|rest| {
        rest.len() == 32 && rest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// A new drop's object key (128 random bits).
pub fn new_drop_key() -> Result<String, CryptoError> {
    let mut id = [0u8; 16];
    random_bytes(&mut id)?;
    Ok(format!("{DROP_PREFIX}{}", to_hex(&id)))
}

fn length_prefixed(out: &mut Vec<u8>, part: &[u8]) {
    out.extend_from_slice(&(part.len() as u32).to_le_bytes());
    out.extend_from_slice(part);
}

/// A drop's associated data: its header, the drive's id and its object key.
fn drop_aad(header: &[u8], drive: &str, object_key: &str) -> Vec<u8> {
    let mut aad = header.to_vec();
    length_prefixed(&mut aad, drive.as_bytes());
    length_prefixed(&mut aad, object_key.as_bytes());
    aad
}

/// The AES-256-GCM key of a drop: HKDF-SHA256 over the X25519 shared secret.
fn drop_key(
    shared: &x25519_dalek::SharedSecret,
    ephemeral: &[u8; KEY_LEN],
    drop_public: &[u8; KEY_LEN],
) -> Result<Zeroizing<[u8; KEY_LEN]>, CryptoError> {
    if !shared.was_contributory() {
        return Err(damaged("an X25519 key of low order"));
    }
    let mut salt = [0u8; 2 * KEY_LEN];
    salt[..KEY_LEN].copy_from_slice(ephemeral);
    salt[KEY_LEN..].copy_from_slice(drop_public);
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    Hkdf::<Sha256>::new(Some(&salt), shared.as_bytes())
        .expand(HKDF_INFO, &mut key[..])
        .map_err(|_| damaged("HKDF refused the length"))?;
    Ok(key)
}

/// The plaintext a drop seals: the header line, a newline, the message.
fn drop_plaintext(received: u64, folder: DropFolder, raw: &[u8]) -> Zeroizing<Vec<u8>> {
    let line = serde_json::to_vec(&DropHeader {
        v: 1,
        received,
        folder,
    })
    .unwrap_or_default();
    let mut plain = Zeroizing::new(Vec::with_capacity(line.len() + 1 + raw.len()));
    plain.extend_from_slice(&line);
    plain.push(b'\n');
    plain.extend_from_slice(raw);
    plain
}

/// The AZD1 bytes of `raw` for the drive `drive`, to be stored at `object_key`, sealed to
/// `to`: what the Worker does (the Rust double of it, for tests and tools).
pub fn seal_drop(
    to: &DropPublic,
    drive: &str,
    object_key: &str,
    received: u64,
    folder: DropFolder,
    raw: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let mut ephemeral = Zeroizing::new([0u8; KEY_LEN]);
    random_bytes(&mut ephemeral[..])?;
    let mut nonce = [0u8; GCM_NONCE_LEN];
    random_bytes(&mut nonce)?;
    seal_drop_with(&ephemeral, &nonce, to, drive, object_key, received, folder, raw)
}

/// [`seal_drop`] with a given ephemeral secret and nonce: the test vector only.
#[allow(clippy::too_many_arguments)]
pub(crate) fn seal_drop_with(
    ephemeral: &[u8; KEY_LEN],
    nonce: &[u8; GCM_NONCE_LEN],
    to: &DropPublic,
    drive: &str,
    object_key: &str,
    received: u64,
    folder: DropFolder,
    raw: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let ephemeral_secret = StaticSecret::from(*ephemeral);
    let ephemeral_public = PublicKey::from(&ephemeral_secret).to_bytes();
    let shared = ephemeral_secret.diffie_hellman(&PublicKey::from(to.0));
    let key = drop_key(&shared, &ephemeral_public, &to.0)?;
    let mut out = Vec::with_capacity(HEADER_LEN + raw.len() + 80);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&to.key_id());
    out.extend_from_slice(&ephemeral_public);
    out.extend_from_slice(nonce);
    let aad = drop_aad(&out, drive, object_key);
    let plain = drop_plaintext(received, folder, raw);
    let sealed = Aes256Gcm::new_from_slice(&key[..])
        .map_err(|_| damaged("an AES key of the wrong length"))?
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: &plain,
                aad: &aad,
            },
        )
        .map_err(|_| damaged("the cipher refused to seal the drop"))?;
    out.extend_from_slice(&sealed);
    Ok(out)
}

/// Opens the drop `bytes` stored at `object_key` of the drive `drive`. `WrongKey` for a drop
/// sealed to another drop key; `Damaged` for one that was changed, moved to another name or
/// drive, or cut short; `Unsupported` for another version.
pub fn open_drop(
    bytes: &[u8],
    secret: &DropSecret,
    drive: &str,
    object_key: &str,
) -> Result<Dropped, CryptoError> {
    if bytes.len() < HEADER_LEN + TAG_LEN || &bytes[..4] != MAGIC {
        return Err(damaged("not an AZD1 drop"));
    }
    if bytes[4] != VERSION {
        return Err(CryptoError::Unsupported(format!("AZD1 version {}", bytes[4])));
    }
    let public = secret.public();
    if bytes[5..5 + DROP_KEY_ID_LEN] != public.key_id() {
        return Err(CryptoError::WrongKey);
    }
    let ephemeral: [u8; KEY_LEN] = bytes[21..21 + KEY_LEN]
        .try_into()
        .map_err(|_| damaged("a drop's ephemeral key"))?;
    let nonce = &bytes[21 + KEY_LEN..HEADER_LEN];
    let shared = secret.0.diffie_hellman(&PublicKey::from(ephemeral));
    let key = drop_key(&shared, &ephemeral, &public.0)?;
    let aad = drop_aad(&bytes[..HEADER_LEN], drive, object_key);
    let plain = Aes256Gcm::new_from_slice(&key[..])
        .map_err(|_| damaged("an AES key of the wrong length"))?
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: &bytes[HEADER_LEN..],
                aad: &aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| damaged("the drop does not authenticate (changed, moved or cut short)"))?;
    let newline = plain
        .iter()
        .take(MAX_HEADER_LINE)
        .position(|&b| b == b'\n')
        .ok_or_else(|| damaged("a drop without its header line"))?;
    let header: DropHeader = serde_json::from_slice(&plain[..newline])
        .map_err(|_| damaged("a drop's header line"))?;
    if header.v != 1 {
        return Err(CryptoError::Unsupported(format!(
            "a drop header of version {}",
            header.v
        )));
    }
    Ok(Dropped {
        received: header.received,
        folder: header.folder,
        raw: Zeroizing::new(plain[newline + 1..].to_vec()),
    })
}

// ==== The drop key in the bucket ====

/// The drop key's secret sealed with the drive key: `.azlin/keys/_drop.key`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropKeyFile {
    /// The id of the drive key that sealed it.
    pub drive_key_id: KeyId,
    pub public: DropPublic,
    pub nonce: [u8; NONCE_LEN],
    pub sealed: [u8; KEY_LEN + TAG_LEN],
}

#[derive(Serialize, Deserialize)]
struct DropFileJson {
    format: String,
    version: u32,
    kind: String,
    drive_key_id: String,
    public: String,
    nonce: String,
    sealed: String,
}

fn drop_file_aad(drive: &str, public: &DropPublic, drive_key_id: &KeyId) -> Vec<u8> {
    associated_data(
        DROP_KEY_AAD_LABEL,
        &[drive.as_bytes(), &public.0, &drive_key_id.0],
    )
}

impl DropKeyFile {
    /// `secret` sealed with `drive_key` for the drive `drive`.
    pub fn seal(
        drive_key: &DriveKey,
        drive: &str,
        secret: &DropSecret,
    ) -> Result<DropKeyFile, CryptoError> {
        let public = secret.public();
        let drive_key_id = drive_key.id();
        let mut nonce = [0u8; NONCE_LEN];
        random_bytes(&mut nonce)?;
        let wrapping_key = drive_key.derive(DROP_KEY_WRAP_CONTEXT);
        let sealed = XChaCha20Poly1305::new(Key::from_slice(&wrapping_key[..]))
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &secret.to_bytes()[..],
                    aad: &drop_file_aad(drive, &public, &drive_key_id),
                },
            )
            .map_err(|_| damaged("the cipher refused to seal the drop key"))?;
        let sealed: [u8; KEY_LEN + TAG_LEN] = sealed
            .as_slice()
            .try_into()
            .map_err(|_| damaged("a sealed key of the wrong length"))?;
        Ok(DropKeyFile {
            drive_key_id,
            public,
            nonce,
            sealed,
        })
    }

    /// The drop key's secret. `WrongKey` for another drive key (one from before a rotation) or
    /// another drive; `Damaged` when it opens to a key of another public half.
    pub fn open(&self, drive_key: &DriveKey, drive: &str) -> Result<DropSecret, CryptoError> {
        if drive_key.id() != self.drive_key_id {
            return Err(CryptoError::WrongKey);
        }
        let wrapping_key = drive_key.derive(DROP_KEY_WRAP_CONTEXT);
        let plain = XChaCha20Poly1305::new(Key::from_slice(&wrapping_key[..]))
            .decrypt(
                XNonce::from_slice(&self.nonce),
                Payload {
                    msg: &self.sealed,
                    aad: &drop_file_aad(drive, &self.public, &self.drive_key_id),
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| CryptoError::WrongKey)?;
        let bytes: [u8; KEY_LEN] = plain
            .as_slice()
            .try_into()
            .map_err(|_| damaged("a sealed drop key is not 32 bytes"))?;
        let secret = DropSecret::from_bytes(bytes);
        if secret.public() != self.public {
            return Err(damaged("the sealed drop key is not the one its file names"));
        }
        Ok(secret)
    }

    /// The key file's bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let file = DropFileJson {
            format: FORMAT.to_string(),
            version: FILE_VERSION,
            kind: KIND_DROP.to_string(),
            drive_key_id: self.drive_key_id.to_hex(),
            public: self.public.to_hex(),
            nonce: to_hex(&self.nonce),
            sealed: to_hex(&self.sealed),
        };
        let mut bytes = serde_json::to_vec_pretty(&file).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// The key file of these bytes.
    pub fn parse(bytes: &[u8]) -> Result<DropKeyFile, CryptoError> {
        let file: DropFileJson =
            serde_json::from_slice(bytes).map_err(|_| damaged("not a drop key file"))?;
        if file.format != FORMAT || file.kind != KIND_DROP {
            return Err(damaged("not a drop key file"));
        }
        if file.version != FILE_VERSION {
            return Err(CryptoError::Unsupported(format!(
                "drop key file version {}",
                file.version
            )));
        }
        let field = |what: &str| damaged(format!("the drop key file's {what}"));
        Ok(DropKeyFile {
            drive_key_id: KeyId::from_hex(&file.drive_key_id).ok_or_else(|| field("key id"))?,
            public: DropPublic::from_hex(&file.public).ok_or_else(|| field("public key"))?,
            nonce: hex_array(&file.nonce).ok_or_else(|| field("nonce"))?,
            sealed: hex_array(&file.sealed).ok_or_else(|| field("sealed key"))?,
        })
    }
}

/// The drive's drop key, opened with its drive key; `None` when the bucket holds none, or one
/// sealed with another drive key (from before a rotation: [`enable_drop`] replaces it).
pub fn load_drop_key(
    bucket: &dyn Drive,
    drive_key: &DriveKey,
    drive: &str,
) -> Result<Option<DropSecret>, DriveError> {
    load_drop_key_at(bucket, DROP_KEY_FILE, drive_key, drive)
}

/// The drop key a rotation replaced (see [`PREVIOUS_DROP_KEY_FILE`]); `None` when there is none.
pub fn load_previous_drop_key(
    bucket: &dyn Drive,
    drive_key: &DriveKey,
    drive: &str,
) -> Result<Option<DropSecret>, DriveError> {
    load_drop_key_at(bucket, PREVIOUS_DROP_KEY_FILE, drive_key, drive)
}

fn load_drop_key_at(
    bucket: &dyn Drive,
    at: &str,
    drive_key: &DriveKey,
    drive: &str,
) -> Result<Option<DropSecret>, DriveError> {
    let bytes = match bucket.get(at) {
        Ok(bytes) => bytes,
        Err(DriveError::NotFound { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    let file = DropKeyFile::parse(&bytes).map_err(|e| e.for_key(at))?;
    match file.open(drive_key, drive) {
        Ok(secret) => Ok(Some(secret)),
        Err(CryptoError::WrongKey) => Ok(None),
        Err(e) => Err(e.for_key(at)),
    }
}

/// A key rotation's step: the drive's drop key (opened with `old`, if there is one) is kept as
/// the previous one sealed with `new`, and a new drop key takes its place. Returns the new
/// public half (for the Worker) when the drive had incoming mail on.
pub fn rotate_drop_key(
    bucket: &dyn Drive,
    old: &DriveKey,
    new: &DriveKey,
    drive: &str,
) -> Result<Option<DropPublic>, DriveError> {
    if let Some(current) = load_drop_key(bucket, new, drive)? {
        // Done already (a resumed rotation).
        return Ok(Some(current.public()));
    }
    let Some(previous) = load_drop_key(bucket, old, drive)? else {
        return Ok(None);
    };
    let file = DropKeyFile::seal(new, drive, &previous).map_err(|e| e.for_key(PREVIOUS_DROP_KEY_FILE))?;
    bucket.put(PREVIOUS_DROP_KEY_FILE, &file.to_bytes())?;
    replace_drop_key(bucket, new, drive).map(Some)
}

/// Forgets the previous drop key (the Worker seals to the new one, the old drops are in).
pub fn forget_previous_drop_key(bucket: &dyn Drive) -> Result<(), DriveError> {
    match bucket.delete(PREVIOUS_DROP_KEY_FILE) {
        Ok(()) | Err(DriveError::NotFound { .. }) => Ok(()),
        Err(e) => Err(e),
    }
}

/// Turns incoming mail on for an encrypted drive: its drop key (the bucket's, else a new one,
/// sealed with `drive_key` into the bucket). Returns the public half for the Worker's
/// configuration. A new key replaces one that `drive_key` cannot open.
pub fn enable_drop(
    bucket: &dyn Drive,
    drive_key: &DriveKey,
    drive: &str,
) -> Result<DropPublic, DriveError> {
    if let Some(secret) = load_drop_key(bucket, drive_key, drive)? {
        return Ok(secret.public());
    }
    replace_drop_key(bucket, drive_key, drive)
}

/// A new drop key for the drive, sealed with `drive_key` into the bucket in the old one's place
/// (a key rotation; the Worker must get the new public half). Drops sealed to the old key no
/// longer open: [`ingest`] them first.
pub fn replace_drop_key(
    bucket: &dyn Drive,
    drive_key: &DriveKey,
    drive: &str,
) -> Result<DropPublic, DriveError> {
    let secret = DropSecret::generate().map_err(|e| e.for_key(DROP_KEY_FILE))?;
    let file = DropKeyFile::seal(drive_key, drive, &secret).map_err(|e| e.for_key(DROP_KEY_FILE))?;
    bucket.put(DROP_KEY_FILE, &file.to_bytes())?;
    Ok(secret.public())
}

/// Turns incoming mail off: the drop key leaves the bucket (drops still there stay unopened).
pub fn disable_drop(bucket: &dyn Drive) -> Result<(), DriveError> {
    bucket.delete(DROP_KEY_FILE)
}

/// What [`ingest`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IngestReport {
    /// Drops handed to the caller and deleted.
    pub delivered: u64,
    /// Drops left in the bucket, with why (sealed to another drop key, damaged, too big).
    pub left: Vec<(String, String)>,
}

/// Opens every drop in `bucket` (the drive's plain bucket) with `secret`, hands each to
/// `deliver` (which files the message into the encrypted drive) and deletes it once `deliver`
/// returned. An error of `deliver` or of the bucket stops the run: the drops not yet deleted
/// are ingested again the next time - `deliver` must be idempotent (the mail's stable names
/// are). Drops that do not open stay and are reported.
pub fn ingest(
    bucket: &dyn Drive,
    secret: &DropSecret,
    drive: &str,
    deliver: &mut dyn FnMut(&Dropped) -> Result<(), DriveError>,
) -> Result<IngestReport, DriveError> {
    let mut report = IngestReport::default();
    for object in list_all(bucket, DROP_PREFIX)? {
        if !is_drop_key(&object.key) {
            continue;
        }
        if object.size > MAX_DROP_LEN {
            report
                .left
                .push((object.key, String::from("bigger than a message can be")));
            continue;
        }
        let bytes = match bucket.get(&object.key) {
            Ok(bytes) => bytes,
            Err(DriveError::NotFound { .. }) => continue, // another device took it
            Err(e) => return Err(e),
        };
        let dropped = match open_drop(&bytes, secret, drive, &object.key) {
            Ok(dropped) => dropped,
            Err(e) => {
                report.left.push((object.key, e.to_string()));
                continue;
            }
        };
        deliver(&dropped)?;
        match bucket.delete(&object.key) {
            Ok(()) | Err(DriveError::NotFound { .. }) => {}
            Err(e) => return Err(e),
        }
        report.delivered += 1;
    }
    Ok(report)
}
