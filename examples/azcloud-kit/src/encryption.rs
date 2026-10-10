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
//! - [`Account::rotate_drive_key`]: "I was hacked" - the lockdown, then a new drive key
//!   (azul-storage's `rotation`); [`Account::reencrypt`]: every file into a new object
//!   afterwards (recommended after a compromise).
//! - [`Account::enable_mail_drop`]: incoming mail for the encrypted drive - its drop key
//!   (azul-storage's `crypto::drops`), whose public half the customer's mail Worker seals to
//!   (set there with the customer's own Cloudflare token: [`crate::cloudflare`]).
//!
//! `keyring` is the OS keyring in the apps (azul-storage's `AzulKeyring`) and the state
//! folder's secrets file on the command line ([`crate::secrets::FileSecrets`]). Blocking: call
//! it from an azul `Thread`.

use std::sync::Arc;

pub use azul_storage::encrypted::{open_encrypted, IndexProvider};
use azul_storage::{
    crypto::{
        device,
        drops::{self, DropPublic},
        keys::{RecoveryCode, RecoveryKdf},
        DriveKey,
    },
    encrypted::EncryptedDrive,
    keyring::KeyringStore,
    rotation::{self, ReencryptState, Rotated},
    Drive, DriveError, S3Config, S3Drive,
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
            endpoint: self.s3_endpoint().to_string(),
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

    /// The drive key on this device: `keyring`'s, else the one this device's member wrap in the
    /// bucket holds (then kept in `keyring`). `None` when this device has neither: it needs a
    /// join code from a device that has the key, or the recovery code.
    ///
    /// # Errors
    ///
    /// The bucket's or the keyring's refusal, a damaged wrap.
    pub fn unlock_key(&self, keyring: &dyn KeyringStore) -> CloudResult<Option<DriveKey>> {
        let bucket = self.bucket_drive()?;
        Ok(device::unlock(&bucket, keyring, &self.record().id)?)
    }

    /// `(whether the bucket holds an encrypted drive, whether this device keeps its key)`; the
    /// second needs no network.
    ///
    /// # Errors
    ///
    /// The bucket's or the keyring's refusal.
    pub fn encryption_status(&self, keyring: &dyn KeyringStore) -> CloudResult<(bool, bool)> {
        let kept = self.holds_drive_key(keyring)?;
        let bucket = self.bucket_drive()?;
        Ok((device::is_encrypted(&bucket)? || kept, kept))
    }

    /// Whether this device keeps the drive's key (a look into `keyring`, no network): its
    /// plaintext commands must not write into the drive then.
    ///
    /// # Errors
    ///
    /// The keyring's refusal, a damaged entry.
    pub fn holds_drive_key(&self, keyring: &dyn KeyringStore) -> CloudResult<bool> {
        Ok(device::load_drive_key(keyring, &self.record().id)?.is_some())
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

    /// "I was hacked": the drive's lockdown ([`Account::lockdown`]: every other device, key and
    /// link loses access at once), then a new drive key from this device (azul-storage's
    /// `rotation::rotate`: the index rekeyed and re-wrapped, this device re-enrolled with a new
    /// member key, the other devices' wraps and the invites deleted, a new recovery code, a new
    /// drop key, every share revoked). A rotation that stopped resumes with the same call (the
    /// lockdown is not repeated). The answer holds the recovery code for the sheet and the new
    /// drop key for the mail Worker.
    ///
    /// # Errors
    ///
    /// No drive key here (`Denied`), an index that cannot be rekeyed (`Unsupported`, nothing
    /// changed), the token server's, the bucket's or the keyring's refusal.
    pub fn rotate_drive_key(
        &mut self,
        keyring: &dyn KeyringStore,
        provider: &dyn IndexProvider,
        kdf: RecoveryKdf,
    ) -> CloudResult<Rotated> {
        let resuming = rotation::pending(&self.bucket_drive()?)?.is_some();
        if !resuming {
            self.lockdown()?;
        }
        // The lockdown's new credentials.
        let bucket: Arc<dyn Drive> = Arc::new(self.bucket_drive()?);
        Ok(rotation::rotate(
            bucket,
            keyring,
            &self.record().id,
            provider,
            kdf,
        )?)
    }

    /// "Re-encrypt everything" after a rotation: every file modified before `state.before` into
    /// a new object with a new file key ([`rotation::reencrypt_pass`]); `save` gets the state
    /// after every file, `stop` is asked before every file. `Ok(true)` when every file is done.
    ///
    /// # Errors
    ///
    /// No key here, the provider's or the bucket's refusal.
    pub fn reencrypt(
        &self,
        keyring: &dyn KeyringStore,
        provider: &dyn IndexProvider,
        state: &mut ReencryptState,
        save: &mut dyn FnMut(&ReencryptState) -> Result<(), DriveError>,
        stop: &dyn Fn() -> bool,
    ) -> CloudResult<bool> {
        let drive = self.open_encrypted(keyring, provider)?;
        Ok(rotation::reencrypt_pass(&drive, state, save, stop)?)
    }

    /// Turns incoming mail on for the encrypted drive: its drop key (the bucket's, else a new
    /// one sealed with the drive key). Returns the public half for the customer's mail Worker
    /// ([`crate::cloudflare::Cloudflare::set_worker_secret`] with
    /// [`crate::cloudflare::DROP_KEY_VARIABLE`]).
    ///
    /// # Errors
    ///
    /// No drive key on this device (the drive is not encrypted, or not unlocked here), the
    /// bucket's or the keyring's refusal.
    pub fn enable_mail_drop(&self, keyring: &dyn KeyringStore) -> CloudResult<DropPublic> {
        let Some(drive_key) = self.unlock_key(keyring)? else {
            fail!(
                "incoming mail is sealed to an encrypted drive's key, and this device has none for \
                 drive {} (encrypt it, or unlock it here first)",
                self.record().id
            );
        };
        let bucket = self.bucket_drive()?;
        Ok(drops::enable_drop(&bucket, &drive_key, &self.record().id)?)
    }

    /// The drive's drop public key when incoming mail is on (and this device holds the drive
    /// key); `None` otherwise.
    ///
    /// # Errors
    ///
    /// The bucket's or the keyring's refusal, a damaged key file.
    pub fn mail_drop_key(&self, keyring: &dyn KeyringStore) -> CloudResult<Option<DropPublic>> {
        let Some(drive_key) = device::load_drive_key(keyring, &self.record().id)? else {
            return Ok(None);
        };
        let bucket = self.bucket_drive()?;
        Ok(drops::load_drop_key(&bucket, &drive_key, &self.record().id)?.map(|secret| secret.public()))
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
