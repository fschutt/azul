//! The drive key in the bucket: sealed to each member's X25519 key
//! (`.azlin/keys/<member>.key`) and to the drive's recovery code (`.azlin/keys/recovery.key`).
//! The bucket never holds the drive key in the clear; a share never holds it at all (a
//! [`super::ShareKey`] wraps file keys only).
//!
//! A MEMBER is a device or a person with an X25519 key pair ([`MemberSecret`]; the secret
//! stays in that member's OS keyring). The drive key is sealed to the member's public key the
//! way a sealed box does it, from the crates' primitives:
//!
//! - a fresh X25519 key pair for this wrap only (the "ephemeral" key, its public half stored
//!   with the wrap), and the shared secret with the member's public key; a non-contributory
//!   (low-order) result is refused;
//! - the wrapping key is BLAKE3 `derive_key` (context [`MEMBER_WRAP_CONTEXT`]) over the shared
//!   secret, the ephemeral public key and the member's public key;
//! - the drive key is sealed with XChaCha20-Poly1305 under it, with a random nonce; the
//!   associated data binds the drive, the member and the drive key's id.
//!
//! The RECOVERY CODE ([`RecoveryCode`]) is 128 random bits, written as 26 Crockford base32
//! characters. Argon2id (256 MiB, 3 passes by default, its cost and salt stored with the
//! wrap) turns it into a root key; BLAKE3 `derive_key` turns that into the wrapping key (other
//! keys - the lockdown signing key - can come from the same root under other contexts later).
//!
//! The key files are small JSON documents (format `azlin-drive-key`, version 1), their binary
//! fields in hex. They name the member by an opaque id ([`MemberPublic::id`]), never by a
//! person's name: the bucket shows only how many members a drive has.

use std::{collections::BTreeMap, fmt};

use chacha20poly1305::{
    aead::{Aead, Payload},
    Key, KeyInit, XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

use super::{hex_array, random_bytes, to_hex, CryptoError, DriveKey, KeyId, KEY_LEN, NONCE_LEN, TAG_LEN};
use crate::{Drive, DriveError};

/// The folder of the key files in the bucket.
pub const KEYS_PREFIX: &str = ".azlin/keys/";
/// The recovery code's key file.
pub const RECOVERY_KEY_FILE: &str = ".azlin/keys/recovery.key";
/// The start of an invite's member id (`invite-<id>`): a one-time wrap for a join code, not a
/// member.
pub const INVITE_PREFIX: &str = "invite-";
/// Bytes of a recovery code (128 random bits).
pub const RECOVERY_CODE_LEN: usize = 16;
/// Bytes of an Argon2id salt.
pub const SALT_LEN: usize = 16;

/// BLAKE3 `derive_key` context of a member wrap's wrapping key.
pub const MEMBER_WRAP_CONTEXT: &str = "Azlin 2026-10-08 member wrap: X25519 to drive key wrapping";
/// BLAKE3 `derive_key` context of a member's id.
const MEMBER_ID_CONTEXT: &str = "Azlin 2026-10-08 member id";
/// BLAKE3 `derive_key` context of the recovery wrap's wrapping key (from the Argon2id root).
pub const RECOVERY_WRAP_CONTEXT: &str = "Azlin 2026-10-08 recovery code: drive key wrapping";
/// The first bytes of the associated data of a member wrap and of the recovery wrap.
const MEMBER_AAD_LABEL: &[u8] = b"Azlin member drive key v1";
const RECOVERY_AAD_LABEL: &[u8] = b"Azlin recovery drive key v1";

/// The key file format.
const FORMAT: &str = "azlin-drive-key";
const FILE_VERSION: u32 = 1;
const KIND_MEMBER: &str = "member";
const KIND_RECOVERY: &str = "recovery";
const ARGON2ID: &str = "argon2id";
/// Argon2 version 1.3 (0x13).
const ARGON2_VERSION: u32 = 19;
/// The longest member id.
const MEMBER_ID_MAX: usize = 64;

/// Crockford's base32 alphabet (no I, L, O, U).
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// Base32 characters of a recovery code (130 bits: the code's 128 and two zero bits).
const RECOVERY_CODE_CHARS: usize = 26;

fn damaged(why: impl Into<String>) -> CryptoError {
    CryptoError::Damaged(why.into())
}

/// A member id fit for a file name in the bucket: 1 to 64 of `a-z`, `0-9` and `-`, and not
/// `recovery` (the recovery code's file).
fn check_member(member: &str) -> Result<(), CryptoError> {
    let fits = (1..=MEMBER_ID_MAX).contains(&member.len())
        && member
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && member != "recovery";
    if fits {
        Ok(())
    } else {
        Err(CryptoError::Unsupported(format!(
            "the member id \"{member}\" (1 to 64 of a-z, 0-9 and -, not \"recovery\")"
        )))
    }
}

/// `.azlin/keys/<member>.key`: the member's copy of the drive key in the bucket.
pub fn member_key_file(member: &str) -> Result<String, CryptoError> {
    check_member(member)?;
    Ok(format!("{KEYS_PREFIX}{member}.key"))
}

/// `label`, then each part with its length in front (two different lists never give the same
/// bytes).
pub(crate) fn associated_data(label: &[u8], parts: &[&[u8]]) -> Vec<u8> {
    let mut out = label.to_vec();
    for part in parts {
        out.extend_from_slice(&(part.len() as u32).to_le_bytes());
        out.extend_from_slice(part);
    }
    out
}

/// The drive key sealed under `wrapping_key` with a fresh random nonce.
fn seal_drive_key(
    wrapping_key: &[u8; KEY_LEN],
    drive_key: &DriveKey,
    aad: &[u8],
) -> Result<([u8; NONCE_LEN], [u8; KEY_LEN + TAG_LEN]), CryptoError> {
    let mut nonce = [0u8; NONCE_LEN];
    random_bytes(&mut nonce)?;
    let sealed = XChaCha20Poly1305::new(Key::from_slice(wrapping_key))
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: drive_key.as_bytes(),
                aad,
            },
        )
        .map_err(|_| damaged("the cipher refused to seal the drive key"))?;
    let sealed: [u8; KEY_LEN + TAG_LEN] = sealed
        .as_slice()
        .try_into()
        .map_err(|_| damaged("a sealed key of the wrong length"))?;
    Ok((nonce, sealed))
}

/// The drive key a wrap holds: `WrongKey` when `wrapping_key` does not open it, `Damaged` when
/// it opens to a key with another id than the wrap names.
fn open_drive_key(
    wrapping_key: &[u8; KEY_LEN],
    nonce: &[u8; NONCE_LEN],
    sealed: &[u8; KEY_LEN + TAG_LEN],
    aad: &[u8],
    expected: &KeyId,
) -> Result<DriveKey, CryptoError> {
    let plain = XChaCha20Poly1305::new(Key::from_slice(wrapping_key))
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: sealed,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::WrongKey)?;
    let drive_key =
        DriveKey::from_slice(&plain).ok_or_else(|| damaged("a sealed drive key is not 32 bytes"))?;
    if drive_key.id() != *expected {
        return Err(damaged("the sealed drive key is not the one its file names"));
    }
    Ok(drive_key)
}

/// A member's X25519 secret key: in that member's OS keyring, never in the bucket. Wiped when
/// dropped, never printed.
pub struct MemberSecret(StaticSecret);

impl MemberSecret {
    /// A new key pair from the OS random source.
    pub fn generate() -> Result<MemberSecret, CryptoError> {
        let mut bytes = Zeroizing::new([0u8; KEY_LEN]);
        random_bytes(&mut bytes[..])?;
        Ok(MemberSecret(StaticSecret::from(*bytes)))
    }

    /// The secret of these bytes (from the keyring). Wipe the caller's copy.
    #[must_use]
    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> MemberSecret {
        MemberSecret(StaticSecret::from(bytes))
    }

    /// The secret's bytes, for the keyring.
    #[must_use]
    pub fn to_bytes(&self) -> Zeroizing<[u8; KEY_LEN]> {
        Zeroizing::new(self.0.to_bytes())
    }

    /// The public half, which the drive key is sealed to.
    #[must_use]
    pub fn public(&self) -> MemberPublic {
        MemberPublic(PublicKey::from(&self.0).to_bytes())
    }
}

impl fmt::Debug for MemberSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MemberSecret(***)")
    }
}

/// A member's X25519 public key.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemberPublic(pub [u8; KEY_LEN]);

impl MemberPublic {
    /// The member's id in the bucket: 32 hex digits of a BLAKE3 derivation from the public
    /// key (says nothing of the person or the device).
    #[must_use]
    pub fn id(&self) -> String {
        to_hex(&blake3::derive_key(MEMBER_ID_CONTEXT, &self.0)[..16])
    }

    #[must_use]
    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }

    #[must_use]
    pub fn from_hex(text: &str) -> Option<MemberPublic> {
        hex_array(text).map(MemberPublic)
    }
}

impl fmt::Debug for MemberPublic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MemberPublic({})", self.to_hex())
    }
}

/// The wrapping key of a member wrap: BLAKE3 `derive_key` over the shared secret, the
/// ephemeral public key and the member's public key.
fn member_wrapping_key(
    shared: &[u8; KEY_LEN],
    ephemeral: &[u8; KEY_LEN],
    member: &[u8; KEY_LEN],
) -> Zeroizing<[u8; KEY_LEN]> {
    let mut hasher = blake3::Hasher::new_derive_key(MEMBER_WRAP_CONTEXT);
    hasher.update(shared);
    hasher.update(ephemeral);
    hasher.update(member);
    Zeroizing::new(*hasher.finalize().as_bytes())
}

fn member_aad(drive: &str, member: &str, drive_key_id: &KeyId) -> Vec<u8> {
    associated_data(
        MEMBER_AAD_LABEL,
        &[drive.as_bytes(), member.as_bytes(), &drive_key_id.0],
    )
}

/// The drive key sealed to one member's X25519 key: `.azlin/keys/<member>.key`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberWrap {
    /// The member's id ([`MemberPublic::id`] or another opaque id).
    pub member: String,
    /// The id of the drive key inside.
    pub drive_key_id: KeyId,
    /// The public half of the X25519 key made for this wrap.
    pub ephemeral: [u8; KEY_LEN],
    pub nonce: [u8; NONCE_LEN],
    pub sealed: [u8; KEY_LEN + TAG_LEN],
}

#[derive(Serialize, Deserialize)]
struct MemberFile {
    format: String,
    version: u32,
    kind: String,
    member: String,
    drive_key_id: String,
    ephemeral: String,
    nonce: String,
    sealed: String,
}

impl MemberWrap {
    /// `drive_key` sealed to `to`, the key of `member`, for the drive `drive` (its id: the
    /// Azlin drive id, or the bucket's name for a bucket of the user's own). The wrap opens
    /// only for that member and that drive.
    pub fn seal(
        drive_key: &DriveKey,
        drive: &str,
        member: &str,
        to: &MemberPublic,
    ) -> Result<MemberWrap, CryptoError> {
        check_member(member)?;
        let mut ephemeral = Zeroizing::new([0u8; KEY_LEN]);
        random_bytes(&mut ephemeral[..])?;
        let ephemeral_secret = StaticSecret::from(*ephemeral);
        let ephemeral_public = PublicKey::from(&ephemeral_secret);
        let shared = ephemeral_secret.diffie_hellman(&PublicKey::from(to.0));
        if !shared.was_contributory() {
            return Err(damaged("a member key of low order"));
        }
        let wrapping_key =
            member_wrapping_key(shared.as_bytes(), ephemeral_public.as_bytes(), &to.0);
        let drive_key_id = drive_key.id();
        let (nonce, sealed) = seal_drive_key(
            &wrapping_key,
            drive_key,
            &member_aad(drive, member, &drive_key_id),
        )?;
        Ok(MemberWrap {
            member: member.to_string(),
            drive_key_id,
            ephemeral: ephemeral_public.to_bytes(),
            nonce,
            sealed,
        })
    }

    /// The drive key, opened with the member's secret key. `WrongKey` for another member's
    /// secret or another drive.
    pub fn open(&self, drive: &str, secret: &MemberSecret) -> Result<DriveKey, CryptoError> {
        let shared = secret.0.diffie_hellman(&PublicKey::from(self.ephemeral));
        if !shared.was_contributory() {
            return Err(damaged("the wrap's ephemeral key is of low order"));
        }
        let me = PublicKey::from(&secret.0);
        let wrapping_key = member_wrapping_key(shared.as_bytes(), &self.ephemeral, me.as_bytes());
        open_drive_key(
            &wrapping_key,
            &self.nonce,
            &self.sealed,
            &member_aad(drive, &self.member, &self.drive_key_id),
            &self.drive_key_id,
        )
    }

    /// The key file's bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let file = MemberFile {
            format: FORMAT.to_string(),
            version: FILE_VERSION,
            kind: KIND_MEMBER.to_string(),
            member: self.member.clone(),
            drive_key_id: self.drive_key_id.to_hex(),
            ephemeral: to_hex(&self.ephemeral),
            nonce: to_hex(&self.nonce),
            sealed: to_hex(&self.sealed),
        };
        let mut bytes = serde_json::to_vec_pretty(&file).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// The wrap of a key file.
    pub fn parse(bytes: &[u8]) -> Result<MemberWrap, CryptoError> {
        let file: MemberFile =
            serde_json::from_slice(bytes).map_err(|_| damaged("not a member key file"))?;
        check_file(&file.format, file.version, &file.kind, KIND_MEMBER)?;
        check_member(&file.member)?;
        let field = |what: &str| damaged(format!("the member key file's {what}"));
        Ok(MemberWrap {
            drive_key_id: KeyId::from_hex(&file.drive_key_id).ok_or_else(|| field("key id"))?,
            ephemeral: hex_array(&file.ephemeral).ok_or_else(|| field("ephemeral key"))?,
            nonce: hex_array(&file.nonce).ok_or_else(|| field("nonce"))?,
            sealed: hex_array(&file.sealed).ok_or_else(|| field("sealed key"))?,
            member: file.member,
        })
    }
}

/// A key file of this format and version, of the kind expected.
fn check_file(format: &str, version: u32, kind: &str, expected: &str) -> Result<(), CryptoError> {
    if format != FORMAT {
        return Err(damaged(format!("\"{format}\" is not a drive key file")));
    }
    if version != FILE_VERSION {
        return Err(CryptoError::Unsupported(format!(
            "drive key file version {version}"
        )));
    }
    if kind != expected {
        return Err(damaged(format!("a {kind} key file where a {expected} one belongs")));
    }
    Ok(())
}

/// The value of one Crockford base32 character (either case; `O` reads as 0, `I` and `L` as
/// 1, as Crockford's decoding does).
fn crockford_value(c: char) -> Option<u32> {
    match c.to_ascii_uppercase() {
        'O' => Some(0),
        'I' | 'L' => Some(1),
        c => CROCKFORD
            .iter()
            .position(|&a| char::from(a) == c)
            .map(|p| p as u32),
    }
}

/// A drive's recovery code: 128 random bits. Wiped when dropped, never printed; shown to the
/// user once as [`RecoveryCode::to_text`] (the recovery sheet), typed back with
/// [`RecoveryCode::parse`].
pub struct RecoveryCode([u8; RECOVERY_CODE_LEN]);

impl RecoveryCode {
    /// A new code from the OS random source.
    pub fn generate() -> Result<RecoveryCode, CryptoError> {
        let mut code = RecoveryCode([0u8; RECOVERY_CODE_LEN]);
        random_bytes(&mut code.0)?;
        Ok(code)
    }

    #[must_use]
    pub fn from_bytes(bytes: [u8; RECOVERY_CODE_LEN]) -> RecoveryCode {
        RecoveryCode(bytes)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; RECOVERY_CODE_LEN] {
        &self.0
    }

    /// 26 Crockford base32 characters in groups of five (the last of six):
    /// `XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX`. The last character carries the code's last three bits
    /// and two zero bits.
    #[must_use]
    pub fn to_text(&self) -> Zeroizing<String> {
        let mut symbols = Zeroizing::new(String::with_capacity(RECOVERY_CODE_CHARS));
        let mut buffer: u32 = 0;
        let mut bits: u32 = 0;
        for &byte in &self.0 {
            buffer = ((buffer << 8) | u32::from(byte)) & 0xFFFF;
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                symbols.push(char::from(CROCKFORD[((buffer >> bits) & 31) as usize]));
            }
        }
        if bits > 0 {
            symbols.push(char::from(CROCKFORD[((buffer << (5 - bits)) & 31) as usize]));
        }
        buffer.zeroize();
        let mut text = Zeroizing::new(String::with_capacity(RECOVERY_CODE_CHARS + 4));
        for (i, c) in symbols.chars().enumerate() {
            if i > 0 && i % 5 == 0 && i < RECOVERY_CODE_CHARS - 1 {
                text.push('-');
            }
            text.push(c);
        }
        text
    }

    /// The code of its text, as a person types it back: any case, with or without dashes and
    /// spaces, `O` for 0 and `I` / `L` for 1. `None` unless it is exactly 26 characters whose
    /// last two bits are zero.
    #[must_use]
    pub fn parse(text: &str) -> Option<RecoveryCode> {
        let mut code = RecoveryCode([0u8; RECOVERY_CODE_LEN]);
        let mut filled = 0;
        let mut buffer: u32 = 0;
        let mut bits: u32 = 0;
        let mut symbols = 0;
        for c in text.chars() {
            if c == '-' || c.is_whitespace() {
                continue;
            }
            let value = crockford_value(c)?;
            symbols += 1;
            if symbols > RECOVERY_CODE_CHARS {
                return None;
            }
            buffer = ((buffer << 5) | value) & 0xFFFF;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                code.0[filled] = (buffer >> bits) as u8;
                filled += 1;
            }
        }
        let canonical = symbols == RECOVERY_CODE_CHARS
            && filled == RECOVERY_CODE_LEN
            && bits == 2
            && buffer & 0b11 == 0;
        buffer.zeroize();
        canonical.then_some(code)
    }
}

impl Clone for RecoveryCode {
    fn clone(&self) -> RecoveryCode {
        RecoveryCode(self.0)
    }
}

impl Drop for RecoveryCode {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for RecoveryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RecoveryCode(***)")
    }
}

/// How the recovery code becomes a key: Argon2id (version 1.3) with this cost and salt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryKdf {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub salt: [u8; SALT_LEN],
}

impl RecoveryKdf {
    /// The cost of a new recovery wrap: 256 MiB, 3 passes, 1 lane (a second or so, once per
    /// recovery; the code itself has 128 bits, the cost is the margin).
    pub const DEFAULT_MEMORY_KIB: u32 = 256 * 1024;
    pub const DEFAULT_ITERATIONS: u32 = 3;
    pub const DEFAULT_PARALLELISM: u32 = 1;
    /// The most a key file may ask for (a file asking for more is refused rather than
    /// exhausting the machine).
    pub const MAX_MEMORY_KIB: u32 = 4 * 1024 * 1024;
    pub const MAX_ITERATIONS: u32 = 64;
    pub const MAX_PARALLELISM: u32 = 16;

    /// The default cost with a fresh random salt.
    pub fn fresh() -> Result<RecoveryKdf, CryptoError> {
        RecoveryKdf::with_cost(
            RecoveryKdf::DEFAULT_MEMORY_KIB,
            RecoveryKdf::DEFAULT_ITERATIONS,
            RecoveryKdf::DEFAULT_PARALLELISM,
        )
    }

    /// This cost with a fresh random salt (the tests use a tiny one).
    pub fn with_cost(
        memory_kib: u32,
        iterations: u32,
        parallelism: u32,
    ) -> Result<RecoveryKdf, CryptoError> {
        let mut salt = [0u8; SALT_LEN];
        random_bytes(&mut salt)?;
        let kdf = RecoveryKdf {
            memory_kib,
            iterations,
            parallelism,
            salt,
        };
        kdf.check()?;
        Ok(kdf)
    }

    /// A cost this build derives with: in bounds, and the 8 KiB per lane Argon2 needs.
    fn check(&self) -> Result<(), CryptoError> {
        let lanes = (1..=RecoveryKdf::MAX_PARALLELISM).contains(&self.parallelism);
        let passes = (1..=RecoveryKdf::MAX_ITERATIONS).contains(&self.iterations);
        let memory = self.memory_kib <= RecoveryKdf::MAX_MEMORY_KIB
            && u64::from(self.memory_kib) >= 8 * u64::from(self.parallelism.max(1));
        if lanes && passes && memory {
            Ok(())
        } else {
            Err(CryptoError::Unsupported(format!(
                "an Argon2id cost out of bounds ({} KiB, {} passes, {} lanes)",
                self.memory_kib, self.iterations, self.parallelism
            )))
        }
    }

    /// The root key of `code`: Argon2id over the code's 16 bytes and the salt.
    fn root(&self, code: &RecoveryCode) -> Result<Zeroizing<[u8; KEY_LEN]>, CryptoError> {
        self.check()?;
        let params = argon2::Params::new(
            self.memory_kib,
            self.iterations,
            self.parallelism,
            Some(KEY_LEN),
        )
        .map_err(|e| CryptoError::Unsupported(format!("Argon2id parameters: {e}")))?;
        let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let mut root = Zeroizing::new([0u8; KEY_LEN]);
        argon
            .hash_password_into(&code.0, &self.salt, &mut root[..])
            .map_err(|e| CryptoError::Unsupported(format!("Argon2id: {e}")))?;
        Ok(root)
    }
}

fn recovery_aad(drive: &str, kdf: &RecoveryKdf, drive_key_id: &KeyId) -> Vec<u8> {
    associated_data(
        RECOVERY_AAD_LABEL,
        &[
            drive.as_bytes(),
            &kdf.memory_kib.to_le_bytes(),
            &kdf.iterations.to_le_bytes(),
            &kdf.parallelism.to_le_bytes(),
            &kdf.salt,
            &drive_key_id.0,
        ],
    )
}

/// The drive key sealed to the recovery code: `.azlin/keys/recovery.key`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryWrap {
    pub drive_key_id: KeyId,
    pub kdf: RecoveryKdf,
    pub nonce: [u8; NONCE_LEN],
    pub sealed: [u8; KEY_LEN + TAG_LEN],
}

#[derive(Serialize, Deserialize)]
struct KdfFile {
    algorithm: String,
    version: u32,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
}

#[derive(Serialize, Deserialize)]
struct RecoveryFile {
    format: String,
    version: u32,
    kind: String,
    drive_key_id: String,
    kdf: KdfFile,
    nonce: String,
    sealed: String,
}

impl RecoveryWrap {
    /// `drive_key` sealed to `code` for the drive `drive`, the code's key derived with `kdf`
    /// (its cost and salt are bound into the wrap: a lowered cost does not open it).
    pub fn seal(
        drive_key: &DriveKey,
        drive: &str,
        code: &RecoveryCode,
        kdf: RecoveryKdf,
    ) -> Result<RecoveryWrap, CryptoError> {
        let root = kdf.root(code)?;
        let wrapping_key = Zeroizing::new(blake3::derive_key(RECOVERY_WRAP_CONTEXT, &root[..]));
        let drive_key_id = drive_key.id();
        let (nonce, sealed) = seal_drive_key(
            &wrapping_key,
            drive_key,
            &recovery_aad(drive, &kdf, &drive_key_id),
        )?;
        Ok(RecoveryWrap {
            drive_key_id,
            kdf,
            nonce,
            sealed,
        })
    }

    /// The drive key, opened with the recovery code. `WrongKey` for a wrong code (or another
    /// drive).
    pub fn open(&self, drive: &str, code: &RecoveryCode) -> Result<DriveKey, CryptoError> {
        let root = self.kdf.root(code)?;
        let wrapping_key = Zeroizing::new(blake3::derive_key(RECOVERY_WRAP_CONTEXT, &root[..]));
        open_drive_key(
            &wrapping_key,
            &self.nonce,
            &self.sealed,
            &recovery_aad(drive, &self.kdf, &self.drive_key_id),
            &self.drive_key_id,
        )
    }

    /// The key file's bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let file = RecoveryFile {
            format: FORMAT.to_string(),
            version: FILE_VERSION,
            kind: KIND_RECOVERY.to_string(),
            drive_key_id: self.drive_key_id.to_hex(),
            kdf: KdfFile {
                algorithm: ARGON2ID.to_string(),
                version: ARGON2_VERSION,
                memory_kib: self.kdf.memory_kib,
                iterations: self.kdf.iterations,
                parallelism: self.kdf.parallelism,
                salt: to_hex(&self.kdf.salt),
            },
            nonce: to_hex(&self.nonce),
            sealed: to_hex(&self.sealed),
        };
        let mut bytes = serde_json::to_vec_pretty(&file).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// The wrap of a key file; its cost must be one this build derives with.
    pub fn parse(bytes: &[u8]) -> Result<RecoveryWrap, CryptoError> {
        let file: RecoveryFile =
            serde_json::from_slice(bytes).map_err(|_| damaged("not a recovery key file"))?;
        check_file(&file.format, file.version, &file.kind, KIND_RECOVERY)?;
        if file.kdf.algorithm != ARGON2ID || file.kdf.version != ARGON2_VERSION {
            return Err(CryptoError::Unsupported(format!(
                "the key derivation {} version {}",
                file.kdf.algorithm, file.kdf.version
            )));
        }
        let field = |what: &str| damaged(format!("the recovery key file's {what}"));
        let kdf = RecoveryKdf {
            memory_kib: file.kdf.memory_kib,
            iterations: file.kdf.iterations,
            parallelism: file.kdf.parallelism,
            salt: hex_array(&file.kdf.salt).ok_or_else(|| field("salt"))?,
        };
        kdf.check()?;
        Ok(RecoveryWrap {
            drive_key_id: KeyId::from_hex(&file.drive_key_id).ok_or_else(|| field("key id"))?,
            kdf,
            nonce: hex_array(&file.nonce).ok_or_else(|| field("nonce"))?,
            sealed: hex_array(&file.sealed).ok_or_else(|| field("sealed key"))?,
        })
    }
}

/// Puts `wrap` at `.azlin/keys/<member>.key` in `bucket` (the bucket itself, below any
/// encrypted drive).
pub fn store_member_wrap(bucket: &dyn Drive, wrap: &MemberWrap) -> Result<(), DriveError> {
    let key = member_key_file(&wrap.member).map_err(|e| e.for_key(&wrap.member))?;
    bucket.put(&key, &wrap.to_bytes())
}

/// The wrap of `member` from `bucket`.
pub fn load_member_wrap(bucket: &dyn Drive, member: &str) -> Result<MemberWrap, DriveError> {
    let key = member_key_file(member).map_err(|e| e.for_key(member))?;
    let bytes = bucket.get(&key)?;
    MemberWrap::parse(&bytes).map_err(|e| e.for_key(&key))
}

/// Every member's key file in `bucket` (`.azlin/keys/<member>.key`), its bytes by member id:
/// not the recovery wrap, not a journal (`_rotation.key`), not an invite's one-time wrap. A
/// file that holds no member wrap is left out. This is the record of a drive whose index has
/// no policy yet (`crate::meta::policy`).
pub fn load_member_wraps(bucket: &dyn Drive) -> Result<BTreeMap<String, Vec<u8>>, DriveError> {
    let mut wraps = BTreeMap::new();
    for object in crate::ops::list_all(bucket, KEYS_PREFIX)? {
        let Some(member) = object
            .key
            .strip_prefix(KEYS_PREFIX)
            .and_then(|rest| rest.strip_suffix(".key"))
        else {
            continue;
        };
        if check_member(member).is_err() || member.starts_with(INVITE_PREFIX) {
            continue;
        }
        let bytes = match bucket.get(&object.key) {
            Ok(bytes) => bytes,
            Err(DriveError::NotFound { .. }) => continue,
            Err(e) => return Err(e),
        };
        if MemberWrap::parse(&bytes).is_ok_and(|wrap| wrap.member == member) {
            wraps.insert(member.to_string(), bytes);
        }
    }
    Ok(wraps)
}

/// Puts `wrap` at `.azlin/keys/recovery.key` in `bucket`.
pub fn store_recovery_wrap(bucket: &dyn Drive, wrap: &RecoveryWrap) -> Result<(), DriveError> {
    bucket.put(RECOVERY_KEY_FILE, &wrap.to_bytes())
}

/// The recovery wrap from `bucket`.
pub fn load_recovery_wrap(bucket: &dyn Drive) -> Result<RecoveryWrap, DriveError> {
    let bytes = bucket.get(RECOVERY_KEY_FILE)?;
    RecoveryWrap::parse(&bytes).map_err(|e| e.for_key(RECOVERY_KEY_FILE))
}
