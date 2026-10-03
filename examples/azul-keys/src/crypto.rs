//! The vault file: `keys/vaults/<vault-uuid>.azkv` in the data tree, a small JSON envelope.
//!
//! ```text
//! { "format": "azkeys-vault", "version": 1, "id": "<uuid>", "name": "Personal",
//!   "kdf":  { "algorithm": "argon2id", "version": 19, "memory_kib": 65536, "iterations": 3,
//!             "parallelism": 1, "salt": "<base64, 16 bytes>" },
//!   "key":  { "cipher": "xchacha20poly1305", "nonce": "<base64, 24 bytes>", "sealed": "..." },
//!   "check": "<base64, 16 bytes of SHA-256 over the vault key>",
//!   "data": { "cipher": "xchacha20poly1305", "nonce": "<base64, 24 bytes>", "sealed": "..." } }
//! ```
//!
//! Two keys. The VAULT KEY (256 random bits) seals the vault's JSON (`data`). The master
//! password, through Argon2id with the file's salt and cost, gives the PASSWORD KEY that seals
//! the vault key (`key`). So:
//! - unlocking with the password = derive the password key, open `key`, open `data`;
//! - unlocking from the OS keyring (biometrics) = the vault key itself, kept there after a
//!   password unlock (base64), opens `data` directly;
//! - a save seals `data` again with a fresh random nonce and keeps `key` as it is (no Argon2id,
//!   no password needed);
//! - changing the master password seals the same vault key under a new salt (the keyring entry
//!   stays valid).
//!
//! What is authenticated besides the ciphertext (the AEAD's associated data): for `key` the
//! format, the vault id and every KDF parameter with the salt (a lowered cost or a swapped salt
//! does not open); for `data` the format, the id and the name (a renamed vault file does not
//! open). `check` tells a wrong vault key (the keyring's, of a vault re-created since) from
//! damaged data, and commits the data to their key. XChaCha20-Poly1305's 192-bit nonces are random per seal: no counter to keep between
//! two devices that share the bucket.
//!
//! Secrets in memory: [`SecretKey`] wipes itself when dropped and prints as `SecretKey(***)`;
//! passwords are taken as `&str` and the derived key bytes are wiped; opened plaintext comes back
//! as a [`Zeroizing`] buffer. Nothing here logs.

use std::fmt;

use chacha20poly1305::{
    aead::{Aead, Payload},
    Key, KeyInit, XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

/// The `format` of a vault file.
pub const FORMAT: &str = "azkeys-vault";
/// The `version` this build writes and reads.
pub const VERSION: u32 = 1;
/// The vault file's suffix.
pub const SUFFIX: &str = ".azkv";
/// The bytes of a key.
pub const KEY_LEN: usize = 32;
/// The bytes of a salt.
pub const SALT_LEN: usize = 16;
/// The bytes of an XChaCha20-Poly1305 nonce.
pub const NONCE_LEN: usize = 24;
/// The KDF and the cipher, as the file names them.
pub const KDF_ALGORITHM: &str = "argon2id";
pub const CIPHER: &str = "xchacha20poly1305";
/// Argon2 version 1.3 (0x13).
pub const ARGON2_VERSION: u32 = 19;

/// The most memory a vault file may ask Argon2id for (1 GiB): a file asking for more is refused
/// rather than exhausting the machine.
pub const MAX_MEMORY_KIB: u32 = 1024 * 1024;
/// The most passes and lanes a file may ask for.
pub const MAX_ITERATIONS: u32 = 64;
pub const MAX_PARALLELISM: u32 = 16;

/// Why a vault does not open (or cannot be written). Never holds a secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VaultError {
    /// The password key does not open the vault key: a wrong master password (or a file whose
    /// KDF parameters were changed).
    WrongPassword,
    /// The key from the keyring does not open the data (the vault was re-created since).
    WrongKey,
    /// The file is not a vault file, or its data do not authenticate.
    Corrupt(String),
    /// A format, version, KDF or cipher this build does not know, or a cost out of bounds.
    Unsupported(String),
    /// The OS random source failed.
    Random,
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaultError::WrongPassword => f.write_str("the master password is not right"),
            VaultError::WrongKey => {
                f.write_str("the key saved on this device no longer opens the vault")
            }
            VaultError::Corrupt(why) => write!(f, "the vault file is damaged ({why})"),
            VaultError::Unsupported(why) => write!(f, "the vault file cannot be read ({why})"),
            VaultError::Random => f.write_str("the system's random source failed"),
        }
    }
}

/// Fills `buf` from the OS random source.
pub fn random_bytes(buf: &mut [u8]) -> Result<(), VaultError> {
    getrandom::getrandom(buf).map_err(|_| VaultError::Random)
}

/// A 256-bit key, wiped when dropped, never printed.
pub struct SecretKey([u8; KEY_LEN]);

impl SecretKey {
    /// A new random key.
    pub fn random() -> Result<SecretKey, VaultError> {
        let mut key = SecretKey([0u8; KEY_LEN]);
        random_bytes(&mut key.0)?;
        Ok(key)
    }

    /// The key of exactly [`KEY_LEN`] bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<SecretKey> {
        if bytes.len() != KEY_LEN {
            return None;
        }
        let mut key = SecretKey([0u8; KEY_LEN]);
        key.0.copy_from_slice(bytes);
        Some(key)
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// The key as base64, for the OS keyring (a buffer wiped when dropped).
    #[must_use]
    pub fn to_base64(&self) -> Zeroizing<String> {
        Zeroizing::new(azul_pim::data_uri::encode_base64(&self.0))
    }

    /// The key of the keyring's base64.
    #[must_use]
    pub fn from_base64(text: &str) -> Option<SecretKey> {
        let bytes = Zeroizing::new(azul_pim::data_uri::decode_base64(text)?);
        SecretKey::from_bytes(&bytes)
    }
}

impl Clone for SecretKey {
    fn clone(&self) -> SecretKey {
        SecretKey(self.0)
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretKey(***)")
    }
}

impl PartialEq for SecretKey {
    /// Constant time over the key's bytes.
    fn eq(&self, other: &SecretKey) -> bool {
        self.0
            .iter()
            .zip(other.0.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

/// How the password key is derived: Argon2id with this cost and salt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub algorithm: String,
    pub version: u32,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    /// Base64 of [`SALT_LEN`] random bytes.
    pub salt: String,
}

impl KdfParams {
    /// The cost a new vault gets: 64 MiB, 3 passes, 1 lane (about a quarter second on a laptop;
    /// Bitwarden's Argon2id default memory and passes).
    pub const DEFAULT_MEMORY_KIB: u32 = 64 * 1024;
    pub const DEFAULT_ITERATIONS: u32 = 3;
    pub const DEFAULT_PARALLELISM: u32 = 1;

    /// The default cost with a fresh random salt.
    pub fn fresh() -> Result<KdfParams, VaultError> {
        KdfParams::with_cost(
            KdfParams::DEFAULT_MEMORY_KIB,
            KdfParams::DEFAULT_ITERATIONS,
            KdfParams::DEFAULT_PARALLELISM,
        )
    }

    /// `memory_kib`, `iterations` and `parallelism` with a fresh random salt (the tests use a
    /// tiny cost).
    pub fn with_cost(
        memory_kib: u32,
        iterations: u32,
        parallelism: u32,
    ) -> Result<KdfParams, VaultError> {
        let mut salt = [0u8; SALT_LEN];
        random_bytes(&mut salt)?;
        Ok(KdfParams {
            algorithm: KDF_ALGORITHM.to_string(),
            version: ARGON2_VERSION,
            memory_kib,
            iterations,
            parallelism,
            salt: azul_pim::data_uri::encode_base64(&salt),
        })
    }

    /// The salt's bytes when the parameters are ones this build derives with: Argon2id 1.3, the
    /// cost in bounds (Argon2 needs 8 KiB of memory per lane), a salt of [`SALT_LEN`] bytes.
    fn checked_salt(&self) -> Result<Vec<u8>, VaultError> {
        if self.algorithm != KDF_ALGORITHM {
            return Err(VaultError::Unsupported(format!(
                "the key derivation \"{}\" is not Argon2id",
                self.algorithm
            )));
        }
        if self.version != ARGON2_VERSION {
            return Err(VaultError::Unsupported(format!(
                "Argon2 version {} (only 19, i.e. 1.3)",
                self.version
            )));
        }
        let lanes_ok = (1..=MAX_PARALLELISM).contains(&self.parallelism);
        let passes_ok = (1..=MAX_ITERATIONS).contains(&self.iterations);
        let memory_ok = self.memory_kib <= MAX_MEMORY_KIB
            && u64::from(self.memory_kib) >= 8 * u64::from(self.parallelism.max(1));
        if !(lanes_ok && passes_ok && memory_ok) {
            return Err(VaultError::Unsupported(format!(
                "an Argon2id cost out of bounds ({} KiB, {} passes, {} lanes)",
                self.memory_kib, self.iterations, self.parallelism
            )));
        }
        match azul_pim::data_uri::decode_base64(&self.salt) {
            Some(salt) if salt.len() == SALT_LEN => Ok(salt),
            _ => Err(VaultError::Corrupt(
                "the salt is not 16 bytes of base64".to_string(),
            )),
        }
    }

    /// Whether this build can derive with these parameters (known algorithm, cost in bounds, a
    /// salt of the right length).
    pub fn check(&self) -> Result<(), VaultError> {
        self.checked_salt().map(|_| ())
    }

    /// The password key of `password`.
    pub fn derive(&self, password: &str) -> Result<SecretKey, VaultError> {
        let salt = self.checked_salt()?;
        let params = argon2::Params::new(
            self.memory_kib,
            self.iterations,
            self.parallelism,
            Some(KEY_LEN),
        )
        .map_err(|e| VaultError::Unsupported(format!("Argon2id parameters: {e}")))?;
        let argon =
            argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let mut key = SecretKey([0u8; KEY_LEN]);
        argon
            .hash_password_into(password.as_bytes(), &salt, &mut key.0)
            .map_err(|e| VaultError::Unsupported(format!("Argon2id: {e}")))?;
        Ok(key)
    }

    /// The associated data that binds a sealed vault key to these parameters and the vault id.
    fn key_aad(&self, id: &str) -> Vec<u8> {
        format!(
            "{FORMAT}/{VERSION}/key/{id}/{}/{}/{}/{}/{}/{}",
            self.algorithm,
            self.version,
            self.memory_kib,
            self.iterations,
            self.parallelism,
            self.salt
        )
        .into_bytes()
    }
}

/// The associated data that binds the sealed vault JSON to the vault's id and name.
fn data_aad(id: &str, name: &str) -> Vec<u8> {
    format!("{FORMAT}/{VERSION}/data/{id}/{name}").into_bytes()
}

/// The check value of a vault key: base64 of the first 16 bytes of SHA-256 over a label and the
/// key. Says whether a key is the vault's before the data are tried (a 256-bit random key cannot
/// be found from it), and commits the data to their key.
#[must_use]
pub fn key_check(key: &SecretKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"azkeys-vault/key-check/");
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    azul_pim::data_uri::encode_base64(&digest[..16])
}

/// One sealed part of the file: the cipher, the nonce and ciphertext + tag (base64).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sealed {
    pub cipher: String,
    pub nonce: String,
    pub sealed: String,
}

/// The cipher of `key`.
fn cipher_of(key: &SecretKey) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new(Key::from_slice(key.as_bytes()))
}

/// Seals `plaintext` with `key` and a fresh random nonce, authenticating `aad` with it.
pub fn seal(key: &SecretKey, plaintext: &[u8], aad: &[u8]) -> Result<Sealed, VaultError> {
    let mut nonce = [0u8; NONCE_LEN];
    random_bytes(&mut nonce)?;
    let sealed = cipher_of(key)
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::Corrupt("the cipher refused to seal".to_string()))?;
    Ok(Sealed {
        cipher: CIPHER.to_string(),
        nonce: azul_pim::data_uri::encode_base64(&nonce),
        sealed: azul_pim::data_uri::encode_base64(&sealed),
    })
}

/// Opens what [`seal`] sealed; `Err(Corrupt)` when the key, the nonce, the ciphertext or `aad`
/// is not the sealed one (the callers turn that into `WrongPassword` / `WrongKey`).
pub fn open(
    key: &SecretKey,
    sealed: &Sealed,
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    if sealed.cipher != CIPHER {
        return Err(VaultError::Unsupported(format!(
            "the cipher \"{}\" is not XChaCha20-Poly1305",
            sealed.cipher
        )));
    }
    let nonce = match azul_pim::data_uri::decode_base64(&sealed.nonce) {
        Some(n) if n.len() == NONCE_LEN => n,
        _ => {
            return Err(VaultError::Corrupt(
                "a nonce is not 24 bytes of base64".to_string(),
            ))
        }
    };
    let Some(ciphertext) = azul_pim::data_uri::decode_base64(&sealed.sealed) else {
        return Err(VaultError::Corrupt(
            "a sealed part is not base64".to_string(),
        ));
    };
    cipher_of(key)
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad,
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| VaultError::Corrupt("a sealed part does not authenticate".to_string()))
}

/// A vault file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    pub kdf: KdfParams,
    pub key: Sealed,
    /// [`key_check`] of the vault key: tells a wrong key from damaged data.
    pub check: String,
    pub data: Sealed,
}

impl Envelope {
    /// A new vault file for `plaintext` (the vault's JSON): a fresh vault key, sealed with the
    /// key `password` derives under `kdf`. Hands the vault key back for the next saves.
    pub fn create(
        id: &str,
        name: &str,
        password: &str,
        kdf: KdfParams,
        plaintext: &[u8],
    ) -> Result<(Envelope, SecretKey), VaultError> {
        let vault_key = SecretKey::random()?;
        let password_key = kdf.derive(password)?;
        let key = seal(&password_key, vault_key.as_bytes(), &kdf.key_aad(id))?;
        let data = seal(&vault_key, plaintext, &data_aad(id, name))?;
        let envelope = Envelope {
            format: FORMAT.to_string(),
            version: VERSION,
            id: id.to_string(),
            name: name.to_string(),
            kdf,
            key,
            check: key_check(&vault_key),
            data,
        };
        Ok((envelope, vault_key))
    }

    /// The file's header and sealed parts, without opening anything (the unlock screen lists
    /// vaults by name). Refuses another format or version.
    pub fn parse(bytes: &[u8]) -> Result<Envelope, VaultError> {
        let envelope: Envelope = serde_json::from_slice(bytes)
            .map_err(|e| VaultError::Corrupt(format!("not a vault file: line {}", e.line())))?;
        if envelope.format != FORMAT {
            return Err(VaultError::Unsupported(format!(
                "the format \"{}\" is not an AzKeys vault",
                envelope.format
            )));
        }
        if envelope.version != VERSION {
            return Err(VaultError::Unsupported(format!(
                "vault version {} (this AzKeys reads version {VERSION}; a newer AzKeys wrote it)",
                envelope.version
            )));
        }
        Ok(envelope)
    }

    /// The file's bytes (pretty JSON: the header is readable, the rest is sealed).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// The vault key, opened with the master password.
    pub fn unwrap_key(&self, password: &str) -> Result<SecretKey, VaultError> {
        let password_key = self.kdf.derive(password)?;
        let bytes = match open(&password_key, &self.key, &self.kdf.key_aad(&self.id)) {
            Ok(bytes) => bytes,
            Err(VaultError::Corrupt(_)) => return Err(VaultError::WrongPassword),
            Err(e) => return Err(e),
        };
        let key = SecretKey::from_bytes(&bytes)
            .ok_or_else(|| VaultError::Corrupt("the vault key is not 32 bytes".to_string()))?;
        if key_check(&key) != self.check {
            return Err(VaultError::Corrupt(
                "the vault key does not match its check value".to_string(),
            ));
        }
        Ok(key)
    }

    /// The vault's JSON, opened with the vault key. `WrongKey` when `key` is not this vault's
    /// (its check value differs: the keyring holds the key of a vault re-created since),
    /// `Corrupt` when it is but the data do not authenticate.
    pub fn open_data(&self, key: &SecretKey) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        if key_check(key) != self.check {
            return Err(VaultError::WrongKey);
        }
        open(key, &self.data, &data_aad(&self.id, &self.name))
    }

    /// The file after a save: `plaintext` sealed with the vault key under a fresh nonce, the name
    /// `name`; the sealed vault key as it was.
    pub fn reseal(
        &self,
        key: &SecretKey,
        name: &str,
        plaintext: &[u8],
    ) -> Result<Envelope, VaultError> {
        let data = seal(key, plaintext, &data_aad(&self.id, name))?;
        Ok(Envelope {
            name: name.to_string(),
            data,
            ..self.clone()
        })
    }

    /// The file after a new master password: the same vault key sealed under `kdf` (a fresh salt)
    /// and `new_password`; the data as they were.
    pub fn rewrap(
        &self,
        key: &SecretKey,
        new_password: &str,
        kdf: KdfParams,
    ) -> Result<Envelope, VaultError> {
        let password_key = kdf.derive(new_password)?;
        let sealed_key = seal(&password_key, key.as_bytes(), &kdf.key_aad(&self.id))?;
        Ok(Envelope {
            kdf,
            key: sealed_key,
            ..self.clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny Argon2id cost: the tests run in debug builds.
    fn cheap() -> KdfParams {
        KdfParams::with_cost(64, 1, 1).expect("random salt")
    }

    const PLAIN: &[u8] = br#"{"items":[{"title":"CodeHost","password":"q7#Rt!vW2m"}]}"#;
    const ID: &str = "5d0f8d4e-6b8e-4c1e-9a43-1f4c2b7e9a10";

    fn vault(password: &str) -> (Envelope, SecretKey) {
        Envelope::create(ID, "Personal", password, cheap(), PLAIN).expect("sealed")
    }

    #[test]
    fn a_vault_sealed_with_a_password_opens_with_the_same_password_after_a_round_trip() {
        let (envelope, key) = vault("correct horse battery staple");
        let bytes = envelope.to_bytes();
        let read = Envelope::parse(&bytes).expect("a vault file");
        assert_eq!(read, envelope);
        assert_eq!(read.name, "Personal");
        assert_eq!(read.id, ID);
        let unwrapped = read
            .unwrap_key("correct horse battery staple")
            .expect("the right password");
        assert_eq!(unwrapped, key);
        assert_eq!(read.open_data(&unwrapped).expect("opens").as_slice(), PLAIN);
    }

    #[test]
    fn a_wrong_password_is_told_apart_from_a_damaged_file() {
        let (envelope, _) = vault("right");
        assert_eq!(envelope.unwrap_key("wrong"), Err(VaultError::WrongPassword));
        assert_eq!(envelope.unwrap_key(""), Err(VaultError::WrongPassword));
        assert_eq!(envelope.unwrap_key("Right"), Err(VaultError::WrongPassword));
        let mut damaged = envelope.clone();
        damaged.data.sealed = flip_last_char(&damaged.data.sealed);
        let key = damaged.unwrap_key("right").expect("the key part is intact");
        assert!(matches!(
            damaged.open_data(&key),
            Err(VaultError::Corrupt(_))
        ));
    }

    #[test]
    fn the_key_kept_in_the_keyring_opens_the_data_without_the_password() {
        let (envelope, key) = vault("pw");
        let stored = key.to_base64();
        let back = SecretKey::from_base64(&stored).expect("base64 of 32 bytes");
        assert_eq!(envelope.open_data(&back).expect("opens").as_slice(), PLAIN);
        let other = SecretKey::random().expect("random");
        assert_eq!(envelope.open_data(&other), Err(VaultError::WrongKey));
        assert!(
            SecretKey::from_base64("c2hvcnQ=").is_none(),
            "5 bytes are no key"
        );
        assert!(SecretKey::from_base64("not base64 at all!").is_none());
    }

    #[test]
    fn a_renamed_vault_or_a_lowered_cost_does_not_open() {
        let (envelope, key) = vault("pw");
        let mut renamed = envelope.clone();
        renamed.name = "Team".to_string();
        assert!(
            renamed.open_data(&key).is_err(),
            "the name is authenticated"
        );
        let mut cheaper = envelope.clone();
        cheaper.kdf.memory_kib = 32;
        assert!(
            cheaper.unwrap_key("pw").is_err(),
            "the cost is authenticated"
        );
        let mut moved = envelope.clone();
        moved.id = "another-id".to_string();
        assert!(moved.unwrap_key("pw").is_err(), "the id is authenticated");
    }

    #[test]
    fn every_save_seals_with_a_fresh_nonce_and_keeps_the_sealed_key() {
        let (envelope, key) = vault("pw");
        let next: &[u8] = br#"{"items":[]}"#;
        let a = envelope.reseal(&key, "Personal", next).expect("sealed");
        let b = envelope.reseal(&key, "Personal", next).expect("sealed");
        assert_ne!(a.data.nonce, b.data.nonce);
        assert_ne!(a.data.sealed, b.data.sealed);
        assert_eq!(a.key, envelope.key, "a save needs no password");
        assert_eq!(a.open_data(&key).expect("opens").as_slice(), next);
        let renamed = envelope.reseal(&key, "Private", next).expect("sealed");
        assert_eq!(renamed.name, "Private");
        assert_eq!(renamed.open_data(&key).expect("opens").as_slice(), next);
    }

    #[test]
    fn a_new_master_password_keeps_the_vault_key() {
        let (envelope, key) = vault("old");
        let changed = envelope.rewrap(&key, "new", cheap()).expect("rewrapped");
        assert_ne!(changed.kdf.salt, envelope.kdf.salt, "a fresh salt");
        assert_eq!(changed.unwrap_key("old"), Err(VaultError::WrongPassword));
        assert_eq!(changed.unwrap_key("new").expect("the new password"), key);
        assert_eq!(changed.data, envelope.data);
    }

    #[test]
    fn the_file_holds_neither_the_password_nor_the_plaintext() {
        let (envelope, key) = vault("hunter2-master");
        let text = String::from_utf8(envelope.to_bytes()).expect("JSON");
        assert!(!text.contains("hunter2-master"));
        assert!(!text.contains("q7#Rt!vW2m"));
        assert!(!text.contains("CodeHost"));
        assert!(!text.contains(key.to_base64().as_str()));
        assert!(text.contains("\"format\": \"azkeys-vault\""), "{text}");
        assert!(text.contains("\"name\": \"Personal\""));
    }

    #[test]
    fn a_key_never_prints() {
        let key = SecretKey::from_bytes(&[7u8; KEY_LEN]).expect("32 bytes");
        assert_eq!(format!("{key:?}"), "SecretKey(***)");
        assert!(SecretKey::from_bytes(&[1u8; 31]).is_none());
    }

    #[test]
    fn the_same_password_and_salt_derive_the_same_key_and_another_salt_another() {
        let kdf = cheap();
        assert_eq!(
            kdf.derive("pw").expect("derives"),
            kdf.derive("pw").expect("derives")
        );
        assert_ne!(
            kdf.derive("pw").expect("derives"),
            kdf.derive("pW").expect("derives")
        );
        let other = cheap();
        assert_ne!(kdf.salt, other.salt);
        assert_ne!(
            kdf.derive("pw").expect("derives"),
            other.derive("pw").expect("derives")
        );
    }

    #[test]
    fn another_format_version_or_an_absurd_cost_is_refused() {
        let (envelope, _) = vault("pw");
        let mut other = envelope.clone();
        other.format = "keepass".to_string();
        assert!(matches!(
            Envelope::parse(&other.to_bytes()),
            Err(VaultError::Unsupported(_))
        ));
        let mut newer = envelope.clone();
        newer.version = 2;
        assert!(matches!(
            Envelope::parse(&newer.to_bytes()),
            Err(VaultError::Unsupported(_))
        ));
        let mut greedy = envelope.clone();
        greedy.kdf.memory_kib = MAX_MEMORY_KIB + 1;
        assert!(matches!(
            greedy.unwrap_key("pw"),
            Err(VaultError::Unsupported(_))
        ));
        let mut scrypt = envelope.clone();
        scrypt.kdf.algorithm = "scrypt".to_string();
        assert!(matches!(
            scrypt.unwrap_key("pw"),
            Err(VaultError::Unsupported(_))
        ));
        assert!(matches!(
            Envelope::parse(b"PK\x03\x04 a zip"),
            Err(VaultError::Corrupt(_))
        ));
    }

    #[test]
    fn the_default_cost_is_argon2id_64_mib_3_passes() {
        let kdf = KdfParams::fresh().expect("random salt");
        assert_eq!(kdf.algorithm, KDF_ALGORITHM);
        assert_eq!(kdf.version, ARGON2_VERSION);
        assert_eq!(
            (kdf.memory_kib, kdf.iterations, kdf.parallelism),
            (65536, 3, 1)
        );
        assert_eq!(
            azul_pim::data_uri::decode_base64(&kdf.salt).map(|s| s.len()),
            Some(SALT_LEN)
        );
        assert!(kdf.check().is_ok());
    }

    /// The text with its last base64 character changed (still base64, other bytes).
    fn flip_last_char(text: &str) -> String {
        let mut chars: Vec<char> = text.trim_end_matches('=').chars().collect();
        let pad = text.len() - chars.len();
        let last = chars.pop().expect("not empty");
        // Change a bit the padding does not hide: the first base64 character of the last group.
        chars.push(if last == 'A' { 'B' } else { 'A' });
        let mut out: String = chars.into_iter().collect();
        out.push_str(&"=".repeat(pad));
        // Flip an earlier character too, so a change confined to padding bits cannot slip by.
        let mut bytes = out.into_bytes();
        bytes[0] = if bytes[0] == b'A' { b'B' } else { b'A' };
        String::from_utf8(bytes).expect("ascii")
    }
}
