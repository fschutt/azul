//! An Azlin drive encrypted on this device (feature `encryption`): azul-storage's
//! `crypto::device` flows for a drive this device holds.
//!
//! - [`Account::setup_encryption`]: the first device turns encryption on - a drive key, the
//!   recovery code for the recovery sheet (shown once, stored nowhere), the recovery wrap and
//!   this device's member wrap in the bucket, the keys in `keyring`.
//! - [`Account::invite_with_key`]: a join code that carries the drive key, sealed to a one-time
//!   key (the code's `key_seal`), when this device holds the key.
//! - [`Account::adopt_join_key`]: after [`Account::join`], the joining device opens the seal,
//!   enrols itself and deletes the one-time wrap.
//! - [`Account::recover_key`]: the recovery code opens the drive key on a device that lost it.
//! - [`Account::open_encrypted`] / [`open_encrypted`]: the drive's files through
//!   azul-storage's `EncryptedDrive`, the index from an `IndexProvider` (the bucket's
//!   encrypted metadata repository).
//!
//! `keyring` is the OS keyring in the apps (azul-storage's `AzulKeyring`) and the state
//! folder's secrets file on the command line ([`crate::secrets::FileSecrets`]). Blocking: call
//! it from an azul `Thread`.

use std::sync::Arc;

pub use azul_storage::encrypted::{open_encrypted, IndexProvider};
use azul_storage::{
    crypto::{
        device,
        keys::{RecoveryCode, RecoveryKdf},
        DriveKey,
    },
    encrypted::EncryptedDrive,
    keyring::KeyringStore,
    Drive, S3Config, S3Drive,
};

use crate::{
    account::{Account, JoinCode},
    error::{fail, CloudResult},
};

impl Account {
    /// The drive's bucket as a plain S3 drive with the current credentials (no refresh: call
    /// [`Account::ensure_fresh`] first) - what the keys live in, below the encryption.
    ///
    /// # Errors
    ///
    /// When the secrets hold no credentials, or the record no usable bucket.
    pub fn bucket_drive(&self) -> CloudResult<S3Drive> {
        let record = self.record();
        let config = S3Config {
            endpoint: record.endpoint.clone(),
            region: record.region.clone(),
            bucket: record.bucket.clone(),
            path_style: record.path_style,
        };
        Ok(S3Drive::new(
            config,
            self.credentials()?,
            (self.transports())(),
        )?)
    }

    /// Turns encryption on for this drive from this device: a new drive key and recovery code,
    /// the recovery wrap (Argon2id with `kdf`) and this device's member wrap into the bucket,
    /// the keys into `keyring`. Returns the recovery code for the recovery sheet: it is stored
    /// nowhere, this is the one moment it can be shown.
    ///
    /// # Errors
    ///
    /// A drive encrypted already, the bucket's or the keyring's refusal.
    pub fn setup_encryption(
        &self,
        keyring: &dyn KeyringStore,
        kdf: RecoveryKdf,
    ) -> CloudResult<RecoveryCode> {
        let bucket = self.bucket_drive()?;
        let (_, code) = device::setup_new_drive(&bucket, keyring, &self.record().id, kdf)?;
        Ok(code)
    }

    /// A join code for another device ([`Account::invite`]); when this device holds the drive
    /// key it rides along, sealed to a one-time key whose secret is the code's `key_seal`.
    ///
    /// # Errors
    ///
    /// The token server's or the bucket's refusal, a keyring that cannot be read.
    pub fn invite_with_key(
        &self,
        member: Option<&str>,
        keyring: &dyn KeyringStore,
    ) -> CloudResult<JoinCode> {
        let mut code = self.invite(member)?;
        let drive = self.record().id.clone();
        if let Some(drive_key) = device::load_drive_key(keyring, &drive)? {
            let bucket = self.bucket_drive()?;
            let invite = device::seal_invite(&bucket, &drive, &drive_key)?;
            code.key_seal = Some(device::invite_text(&invite).to_string());
        }
        Ok(code)
    }

    /// After [`Account::join`] with `code`: the drive key its seal opens, kept in `keyring` with
    /// this device enrolled (its own member wrap); the one-time wrap is deleted. `None` for a
    /// code without a seal (the drive is not encrypted, or the inviting device held no key).
    ///
    /// # Errors
    ///
    /// A seal that was used already or withdrawn (`Denied`), a damaged one, the bucket's or
    /// the keyring's refusal.
    pub fn adopt_join_key(
        &self,
        code: &JoinCode,
        keyring: &dyn KeyringStore,
    ) -> CloudResult<Option<DriveKey>> {
        let Some(seal) = code.key_seal.as_deref() else {
            return Ok(None);
        };
        let Some(invite) = device::invite_from_text(seal) else {
            fail!("the join code's key seal is damaged (not 64 hex digits)");
        };
        let bucket = self.bucket_drive()?;
        Ok(Some(device::adopt_invite(
            &bucket,
            keyring,
            &self.record().id,
            &invite,
        )?))
    }

    /// The drive key the recovery code opens, kept in `keyring` with this device enrolled.
    ///
    /// # Errors
    ///
    /// A wrong code (`Denied`), the bucket's or the keyring's refusal.
    pub fn recover_key(
        &self,
        code: &RecoveryCode,
        keyring: &dyn KeyringStore,
    ) -> CloudResult<DriveKey> {
        let bucket = self.bucket_drive()?;
        Ok(device::recover(&bucket, keyring, &self.record().id, code)?)
    }

    /// The drive's files through the encryption: the bucket with the current credentials, the
    /// drive key from `keyring` (or this device's member wrap), the index from `provider`.
    ///
    /// # Errors
    ///
    /// No key on this device (`Denied`), the provider's or the bucket's refusal.
    pub fn open_encrypted(
        &self,
        keyring: &dyn KeyringStore,
        provider: &dyn IndexProvider,
    ) -> CloudResult<EncryptedDrive<Arc<dyn Drive>>> {
        let bucket: Arc<dyn Drive> = Arc::new(self.bucket_drive()?);
        Ok(open_encrypted(
            bucket,
            keyring,
            &self.record().id,
            provider,
        )?)
    }
}
