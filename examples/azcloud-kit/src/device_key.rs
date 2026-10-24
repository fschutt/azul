//! This device's own key for an Azlin drive.
//!
//! An Ed25519 key made on this computer and kept in its keyring. It exists for one moment: if
//! the drive is ever locked (unpaid past its read-only time, its files moved to offline
//! storage), the token server gives it back only to its owner, and it keeps no name or address
//! to know the owner by. This key's signature over the server's statement shows that this
//! computer is one of the drive's own devices ([`crate::TokenServer::archive_prove`]) - for a
//! restore and nothing else: the token server keeps device keys apart from the recovery keys,
//! so a device key never signs a recovery lockdown or a change of the recovery keys.
//!
//! The key is registered once, while the drive is open, by a device that holds the drive key:
//! when it joined or was paired, or unlocked the drive ([`ensure_registered`]). The keyring keeps
//! the key and the id the server gave it.

use std::fmt;

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    error::CloudResult,
    shared::SharedKeyring,
    token::{TokenError, TokenServer},
};

/// The keyring entry of `drive_id`'s device key, beside the drive's session.
#[must_use]
pub fn entry_key(drive_id: &str) -> String {
    format!(
        "{}/device-key",
        azul_storage::config::keyring_key(drive_id)
    )
}

/// What the keyring keeps: the key's seed and the id the token server gave it.
#[derive(Serialize, Deserialize)]
struct Stored {
    seed: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key_id: Option<String>,
}

/// A device's key for a drive. `Debug` shows only its id.
pub struct DeviceKey {
    signing: SigningKey,
    key_id: Option<String>,
}

impl fmt::Debug for DeviceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceKey")
            .field("key", &"<hidden>")
            .field("key_id", &self.key_id)
            .finish()
    }
}

impl DeviceKey {
    /// A new key from the system's random source.
    ///
    /// # Errors
    ///
    /// When the system gives no random bytes.
    pub fn generate() -> Result<DeviceKey, String> {
        let mut seed = Zeroizing::new([0_u8; 32]);
        getrandom::getrandom(&mut seed[..]).map_err(|e| format!("no random source: {e}"))?;
        Ok(DeviceKey::from_seed(&seed))
    }

    /// The key of `seed` (tests, and the keyring's entry read back).
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> DeviceKey {
        DeviceKey {
            signing: SigningKey::from_bytes(seed),
            key_id: None,
        }
    }

    /// The public half as the token server takes it: 32 bytes, standard base64.
    #[must_use]
    pub fn public_base64(&self) -> String {
        STANDARD.encode(self.signing.verifying_key().to_bytes())
    }

    /// The Ed25519 signature of `message`, standard base64.
    #[must_use]
    pub fn sign_base64(&self, message: &[u8]) -> String {
        STANDARD.encode(self.signing.sign(message).to_bytes())
    }

    /// The id the token server gave it (`dk_...`); `None` before it is registered.
    #[must_use]
    pub fn key_id(&self) -> Option<&str> {
        self.key_id.as_deref()
    }

    /// Its registration for `drive_id`: the public half, signed by the key itself over
    /// [`azlin_contract::device_key_statement`].
    #[must_use]
    pub fn registration(&self, drive_id: &str) -> azlin_contract::DeviceKeyRegistration {
        let public_key = self.public_base64();
        let statement = azlin_contract::device_key_statement(drive_id, &public_key);
        azlin_contract::DeviceKeyRegistration {
            signature: self.sign_base64(statement.as_bytes()),
            public_key,
        }
    }

    fn to_text(&self) -> Zeroizing<String> {
        let stored = Stored {
            seed: STANDARD.encode(self.signing.to_bytes()),
            key_id: self.key_id.clone(),
        };
        Zeroizing::new(serde_json::to_string(&stored).unwrap_or_default())
    }

    fn from_text(text: &str) -> Option<DeviceKey> {
        let stored: Stored = serde_json::from_str(text.trim()).ok()?;
        let bytes = Zeroizing::new(STANDARD.decode(stored.seed.as_bytes()).ok()?);
        let seed: [u8; 32] = bytes.as_slice().try_into().ok()?;
        let mut key = DeviceKey::from_seed(&seed);
        key.key_id = stored.key_id.filter(|id| !id.trim().is_empty());
        Some(key)
    }

    /// The device key of `drive_id` this keyring keeps; `None` when it keeps none (or one it
    /// cannot read: a new one is made then, and registered as well).
    ///
    /// # Errors
    ///
    /// When the keyring cannot be read.
    pub fn load(shared: &SharedKeyring, drive_id: &str) -> CloudResult<Option<DeviceKey>> {
        Ok(shared
            .get(&entry_key(drive_id))?
            .and_then(|text| DeviceKey::from_text(&text)))
    }

    fn save(&self, shared: &SharedKeyring, drive_id: &str) -> CloudResult<()> {
        shared.set(&entry_key(drive_id), &self.to_text())
    }
}

/// Makes sure this device's key of the open drive `drive_id` is registered, with the drive token
/// `token` the caller holds under the drive's lock: the key the keyring keeps (a new one is made
/// and kept first), sent once; its id is kept with it. The key's id; one registered before
/// answers its id at once.
///
/// # Errors
///
/// No random source or no keyring (said as [`TokenError::Config`]); the token server's refusal
/// (a locked drive takes no key: 403 `drive_archived`).
pub fn register_with(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    drive_id: &str,
    token: &str,
) -> Result<String, TokenError> {
    let config = |e: crate::error::CloudError| TokenError::Config(e.to_string());
    let kept = DeviceKey::load(shared, drive_id).map_err(config)?;
    let mut key = match kept {
        Some(key) => key,
        None => {
            let key = DeviceKey::generate().map_err(TokenError::Config)?;
            // kept before it is sent: a key the server holds is never one this device lost
            key.save(shared, drive_id).map_err(config)?;
            key
        }
    };
    if let Some(id) = key.key_id() {
        return Ok(id.to_string());
    }
    let answer = server.register_device_key(drive_id, token, &key.registration(drive_id))?;
    key.key_id = Some(answer.key_id.clone());
    // an id not kept is asked again next time: the server answers the same id
    let _ = key.save(shared, drive_id);
    Ok(answer.key_id)
}

/// [`register_with`] under the drive's lock with its newest drive token
/// ([`SharedKeyring::with_drive_token`]): what AzDrive runs once a device holds the drive key.
///
/// # Errors
///
/// No session of the drive, its lock, or [`register_with`]'s.
pub fn ensure_registered(
    server: &TokenServer<'_>,
    shared: &SharedKeyring,
    drive_id: &str,
) -> Result<String, TokenError> {
    shared
        .with_drive_token(drive_id, |token| register_with(server, shared, drive_id, token))
        .map_err(|e| TokenError::Config(e.to_string()))?
}
