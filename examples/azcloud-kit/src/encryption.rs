//! An Azlin drive encrypted on this device (feature `encryption`): azul-storage's
//! `crypto::device` flows for a drive this device holds.
//!
//! - [`Account::setup_encryption`]: the first device turns encryption on - a drive key, the
//!   recovery code for the recovery sheet (shown once, stored nowhere), the recovery wrap and
//!   this device's member wrap in the bucket, the keys in `keyring`.
//! - [`Account::invite_with_key`]: a join code that carries the drive key, sealed to a one-time
//!   key (the code's `key_seal`), when this device holds the key.
//! - [`Account::adopt_join_key_reporting`]: after [`Account::join`], the joining device opens the
//!   seal, enrols itself and deletes the one-time wrap - and says when that wrap could not be
//!   deleted, so the code is still live ([`Account::adopt_join_key`] is the same without the word).
//! - [`Account::recover_key`]: the recovery code opens the drive key on a device that lost it.
//! - [`Account::open_encrypted`] / [`open_encrypted`]: the drive's files through
//!   azul-storage's `EncryptedDrive`, the index from an `IndexProvider` (the bucket's
//!   encrypted metadata repository).
//! - [`Account::rotate_drive_key`]: "I was hacked" - the lockdown, then a new drive key
//!   (azul-storage's `rotation`); [`Account::reencrypt`]: every file into a new object
//!   afterwards (recommended after a compromise).
//! - [`Account::serve_git_remote`]: git's remote helper over the drive index in the bucket
//!   (`git clone azlin::drive://<drive id>`, through `azcloud git-remote`).
//! - [`Account::enable_mail_drop`]: incoming mail for the encrypted drive - its drop key
//!   (azul-storage's `crypto::drops`), whose public half the customer's mail Worker seals to
//!   (set there with the customer's own Cloudflare token: [`crate::cloudflare`]).
//!
//! `keyring` is the OS keyring in the apps (azul-storage's `AzulKeyring`) and the state
//! folder's secrets file on the command line ([`crate::secrets::FileSecrets`]). Blocking: call
//! it from an azul `Thread`.

use std::{
    io::{self, BufRead, Write},
    sync::Arc,
};

pub use azul_storage::encrypted::{open_encrypted, IndexProvider, OpenRefusal};
use azul_storage::{
    crypto::{
        device::{self, Adopted},
        drops::{self, DropPublic},
        keys::{RecoveryCode, RecoveryKdf},
        keystate::Op,
        DriveKey, KeyId,
    },
    encrypted::{EncryptedDrive, Recorded},
    keyring::KeyringStore,
    meta::{git, DriveBucket},
    rotation::{self, ReencryptState, Rotated},
    trust::EncState,
    Drive, DriveError, S3Config, S3Drive,
};

use crate::{
    account::{Account, JoinCode},
    error::{fail, CloudResult},
    failover::{Failover, Node},
};

/// Where a new drive's keys go as it is made ([`Account::signup`]).
pub struct NewKeys<'k> {
    /// This device's keyring: the drive key and the device's member key.
    pub keyring: &'k dyn KeyringStore,
    /// The S3 endpoint its bucket is reached at (a configured `--s3-url`); `None`: the drive's
    /// own.
    pub s3_endpoint: Option<&'k str>,
    /// The recovery code's key derivation.
    pub kdf: RecoveryKdf,
}

impl Account {
    /// Signs up for a drive of `tier` at the token server `token_url` (`POST /v1/drives`;
    /// development servers), keeps it in `state`, and encrypts it before anything else can
    /// touch it: a new drive key and recovery code, the recovery wrap and this device's member
    /// wrap into its bucket, the keys into the keyring ([`Account::setup_encryption`]). Every
    /// drive is encrypted as it is made. Returns the account and the recovery code, which is
    /// stored nowhere: this is the one moment it can be shown.
    ///
    /// # Errors
    ///
    /// The token server's refusal, no answer, a file that cannot be written, the bucket's or
    /// the keyring's refusal. A drive made whose keys could not be set up stays in `state`, and
    /// takes no file until it is encrypted.
    pub fn signup(
        state: &crate::state::StateDir,
        token_url: &str,
        transports: crate::drive::TransportFactory,
        tier: &str,
        name: &str,
        keys: NewKeys<'_>,
    ) -> CloudResult<(Account, RecoveryCode)> {
        // Before the drive is made: a keyring that keeps no key would refuse the first key written,
        // after the sign-up, and leave a drive that takes no file (X-DELTA-AZDRIVE-KEYS-25).
        device::check_keeps_keys(keys.keyring, "the new drive's key")?;
        let account = Account::sign_up_at_token_server(state, token_url, transports, tier, name)?
            .with_s3_endpoint(keys.s3_endpoint);
        let code = account.setup_encryption(keys.keyring, keys.kdf)?;
        Ok((account, code))
    }

    /// The drive's bucket as a plain S3 drive with the current credentials (no refresh: call
    /// [`Account::ensure_fresh`] first) - what the keys live in, below the encryption. Its
    /// requests go through the same failover as the drive's files: the nodes of the last
    /// refresh when the block endpoint does not answer, and their addresses when its name does
    /// not resolve, so a key listing works wherever the files do.
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
        let failover = Arc::new(Failover::new(&config.endpoint));
        failover.set_nodes(Node::list(&record.nodes));
        failover.add_alternatives(record.failover.clone());
        Ok(S3Drive::new(
            config,
            self.credentials()?,
            (self.transports())(),
        )?
        .with_router(failover))
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
        let (drive_key, code) = device::setup_new_drive(&bucket, keyring, &self.record().id, kdf)?;
        // The code below is shown once and stored nowhere: a note that could not be written must
        // not take it away. (collapse-ok: the next open of the drive writes the note again.)
        let _ = self.note_encrypted(&drive_key.id().to_hex());
        Ok(code)
    }

    /// A join code for another device ([`Account::invite`]); when this device holds the drive
    /// key it rides along, sealed to a one-time key whose secret is the code's `key_seal`, and
    /// the code names the key's id (`key_id`), so the joining device takes no other.
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
            // A key change that is not over would hand out the key it replaces, and delete the invite.
            let _permit = device::wraps_permit(Op::Invite, keyring, &drive)?;
            let bucket = self.bucket_drive()?;
            let invite = device::seal_invite(&bucket, &drive, &drive_key)?;
            code.key_seal = Some(device::invite_text(&invite).to_string());
            code.key_id = Some(drive_key.id().to_hex());
        }
        Ok(code)
    }

    /// BEFORE [`Account::join`] with `code`: whether the key the code carries (`key_seal`) can be
    /// kept in `keyring`. A join spends the code's one-time token; a keyring that keeps no key (a
    /// plain file the user did not allow, AZ-K7) refuses the key AFTER that, and the retry with the
    /// word allowed would present a token that was rotated already (X-DELTA-AZDRIVE-KEYS-25). A
    /// code without a seal asks nothing of the keyring.
    ///
    /// # Errors
    ///
    /// The keyring's reason, naming the plain file and what the user can say.
    pub fn check_join_key_can_be_kept(
        code: &JoinCode,
        keyring: &dyn KeyringStore,
    ) -> CloudResult<()> {
        if code.key_seal.is_none() {
            return Ok(());
        }
        // The key permit's Recover cell, asked on the keyring alone before the code's token is spent: a
        // keyring that keeps no key says so, and a device whose own setup or key change is not over takes
        // no key over it (X-KEYSTATE-5).
        let _permit = device::recover_permit(keyring, &code.drive_id)?;
        Ok(())
    }

    /// After [`Account::join`] with `code`: the drive key its seal opens, kept in `keyring` with
    /// this device enrolled (its own member wrap); the one-time wrap is deleted. A seal of
    /// another key than the code's `key_id` is refused. `None` for a code without a seal (the
    /// drive is not encrypted, or the inviting device held no key). A wrap that could not be
    /// deleted is not said here: the apps ask [`Account::adopt_join_key_reporting`].
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
        Ok(self
            .adopt_join_key_reporting(code, keyring)?
            .map(|adopted| adopted.key))
    }

    /// [`Account::adopt_join_key`] that says whether the one-time invite is still live afterwards
    /// ([`Adopted::invite_left`]): the device has joined either way, and the caller tells its
    /// user that the code still opens the drive key.
    ///
    /// # Errors
    ///
    /// As [`Account::adopt_join_key`], the failure to delete the invite excepted.
    pub fn adopt_join_key_reporting(
        &self,
        code: &JoinCode,
        keyring: &dyn KeyringStore,
    ) -> CloudResult<Option<Adopted>> {
        let Some(seal) = code.key_seal.as_deref() else {
            return Ok(None);
        };
        let Some(invite) = device::invite_from_text(seal) else {
            fail!("the join code's key seal is damaged (not 64 hex digits)");
        };
        let key_id = match code.key_id.as_deref() {
            Some(text) => match KeyId::from_hex(text.trim()) {
                Some(id) => Some(id),
                None => fail!("the join code's key id is damaged (not 32 hex digits)"),
            },
            None => None,
        };
        let bucket = self.bucket_drive()?;
        let adopted = device::adopt_invite_reporting(
            &bucket,
            keyring,
            &self.record().id,
            &invite,
            key_id.as_ref(),
        )?;
        // collapse-ok: the next open of the drive writes the note again.
        let _ = self.note_encrypted(&adopted.key.id().to_hex());
        Ok(Some(adopted))
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

    /// git's remote helper over this drive's index (`git clone azlin::drive://<drive id>`:
    /// azul-storage's `git-remote-azlin` hands the URL to `azcloud git-remote`, which calls
    /// this). It serves git's commands from `input` to `output` over the drive's metadata
    /// repository in its bucket (azul-storage's `meta::git::serve_bucket`). The S3 requests
    /// are signed with the account's credentials (no refresh: call [`Account::ensure_fresh`]
    /// first) and sent through its transports, and the repository opens with the drive key
    /// `keyring` keeps. `index_pack` takes each pack into git's repository.
    ///
    /// # Errors
    ///
    /// No drive key here, the bucket's refusal, a broken protocol.
    pub fn serve_git_remote(
        &self,
        keyring: &dyn KeyringStore,
        input: impl BufRead,
        output: impl Write,
        index_pack: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> CloudResult<()> {
        let Some(drive_key) = self.unlock_key(keyring)? else {
            fail!(
                "this device has no key for drive {}: a join code from a device that has it, or \
                 the recovery code",
                self.record().id
            );
        };
        let bucket = DriveBucket::new(self.bucket_drive()?);
        git::serve_bucket(bucket, drive_key, input, output, index_pack)?;
        Ok(())
    }

    /// What this device knows of the drive's encryption, the way AzDrive decides it: the keyring
    /// and the account file's record first, the bucket's listing of the key files as an addition
    /// to them and never a replacement ([`azul_storage::encrypted::judge`]). A drive this device
    /// holds no key for and recorded nothing of, in a bucket that lists no key files, is not
    /// encrypted as far as it can tell - which is also what a bucket that hides them looks like:
    /// no command writes into it ([`Account::open_encrypted_only`]). Needs the network only when
    /// the device keeps no key.
    ///
    /// # Errors
    ///
    /// The keyring's (a declined prompt, none on the system) or the bucket's refusal.
    pub fn encryption_state(&self, keyring: &dyn KeyringStore) -> CloudResult<EncState> {
        let bucket = self.bucket_drive()?;
        Ok(azul_storage::encrypted::judge(
            &bucket,
            keyring,
            &self.record().id,
            self.recorded()?,
        )?)
    }

    /// What the account file records of the drive's encryption NOW (another run may have written
    /// it since this account was opened), as AzDrive's `AutoEncrypted` takes a record.
    ///
    /// # Errors
    ///
    /// The account file cannot be read: an unreadable record is no record of "not encrypted".
    pub fn recorded(&self) -> CloudResult<Recorded> {
        let file = crate::account::AccountFile::load(&self.state().account_file())?;
        let record = file
            .get(Some(&self.record().id))
            .unwrap_or_else(|| self.record());
        // A key id that cannot be read is a damaged record, not "no id" (X-DELTA-AZDRIVE-KEYS-21).
        Ok(Recorded::from_entry(
            record.encrypted,
            record.key_id.as_deref(),
        ))
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
        let drive_key = device::recover(&bucket, keyring, &self.record().id, code)?;
        // collapse-ok: the next open of the drive writes the note again.
        let _ = self.note_encrypted(&drive_key.id().to_hex());
        Ok(drive_key)
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
        // The key permit asks BEFORE the lockdown locks the other devices out: a keyring that keeps no
        // key would refuse the first key of the rotation (X-DELTA-AZDRIVE-KEYS-25), a rotation that
        // cannot be finished (no key here, an index that opens with neither key this device holds, a
        // keyring or a bucket that cannot be read) would leave the drive locked with its key not
        // replaced. A rotation of THIS device that stopped has had its lockdown - the permit says so -
        // and a journal anyone planted in the bucket is no such rotation: it must not skip it.
        let rotation = rotation::ask(&self.bucket_drive()?, keyring, &self.record().id)?;
        if rotation.lockdown().is_some() {
            self.lockdown()?;
        }
        // The lockdown's new credentials.
        let bucket: Arc<dyn Drive> = Arc::new(self.bucket_drive()?);
        Ok(rotation::rotate(
            rotation.permit(),
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
        // The key permit asks: a key change that is not over, or a keyring that cannot be read, refuses
        // it (the files would be written under the key the change replaces).
        let permit = rotation::ask_reencrypt(keyring, &self.record().id, &drive)?;
        Ok(rotation::reencrypt_pass(
            &permit, &drive, state, save, stop,
        )?)
    }

    /// Turns incoming mail on for the encrypted drive: its drop key (the bucket's, else a new
    /// one sealed with the drive key). Returns the public half for the customer's mail Worker
    /// ([`crate::cloudflare::Cloudflare::set_worker_secret`] with
    /// [`crate::cloudflare::DROP_KEY_VARIABLE`]).
    ///
    /// The key permit's `DropKey` cell asks first (X-DELTA-KEYS3-2): a key change that is not over, or a drop
    /// key file sealed under another drive key than this device holds - its key was replaced - refuses, and
    /// nothing is written. A device that still held the key before a rotation used to write a new drop key
    /// over the one the new key sealed, and the mail sealed to it opened with nothing.
    ///
    /// # Errors
    ///
    /// No drive key on this device (the drive is not encrypted, or not unlocked here), the key permit's
    /// refusal (`Denied`, `KeyRefused`), the bucket's or the keyring's refusal.
    pub fn enable_mail_drop(&self, keyring: &dyn KeyringStore) -> CloudResult<DropPublic> {
        let Some(drive_key) = self.unlock_key(keyring)? else {
            fail!(
                "incoming mail is sealed to an encrypted drive's key, and this device has none for \
                 drive {} (encrypt it, or unlock it here first)",
                self.record().id
            );
        };
        let drive = self.record().id.clone();
        let bucket = self.bucket_drive()?;
        let permit = device::drop_key_permit(&bucket, keyring, &drive, &drive_key)?;
        Ok(drops::enable_drop(&permit, &bucket, &drive_key, &drive)?)
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

    /// The drive's files through the encryption for a program that has NO plaintext path (the
    /// command line): the drive key from `keyring` (or this device's member wrap), the index from
    /// `provider`. The decision is AzDrive's - this device's keyring and the account file's
    /// record first, the bucket's listing as an addition and never a replacement - and a drive
    /// that is not encrypted (new, older, or a bucket that hides its key files, which cannot be
    /// told apart) is refused ([`OpenRefusal::NotEncrypted`]) instead of being read or written as
    /// its bucket lists. Opened, the drive is written down as encrypted in the account file with
    /// the id of its key.
    ///
    /// # Errors
    ///
    /// An [`OpenRefusal`].
    pub fn open_encrypted_only(
        &self,
        keyring: &dyn KeyringStore,
        provider: &dyn IndexProvider,
    ) -> Result<EncryptedDrive<Arc<dyn Drive>>, OpenRefusal> {
        let refused = |e: crate::error::CloudError| {
            OpenRefusal::Refused(DriveError::InvalidConfig(e.to_string()))
        };
        let bucket: Arc<dyn Drive> = Arc::new(self.bucket_drive().map_err(refused)?);
        let opened = azul_storage::encrypted::open_encrypted_only(
            bucket,
            keyring,
            &self.record().id,
            provider,
            self.recorded().map_err(refused)?,
        )?;
        // collapse-ok: a note that cannot be written is written at the next open; the drive
        // opened.
        let _ = self.note_encrypted(&opened.drive_key_id().to_hex());
        Ok(opened)
    }
}
