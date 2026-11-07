//! Where a drive's period tokens are kept: on this computer ([`LocalVault`]) and in the drive
//! itself ([`DriveVault`], an object encrypted with the drive key, which every device holding
//! the key reads). A paid month must not be lost with the computer that bought it, nor wait on a
//! computer that is not used: whichever device of the drive looks first redeems it and issues
//! the next instalment.
//!
//! What a vault holds ([`VaultState`]) only grows by joins: tokens added, nonces spent, grants
//! and their schedules, issue requests kept and done. Two devices that write at once lose
//! nothing (a conditional write that lost reads again and joins once more), and a token one of
//! them spent never comes back from the other's copy. A token redeemed twice is still bought
//! once: the token server answers the drive's repeat with the same month.
//!
//! [`DriveTokens`] is a drive's view over its vaults: it reads them all and joins what one holds
//! into the others (the first look after an update moves this computer's tokens into the
//! drive), and writes every change to each. It is made from the drive with its key
//! ([`crate::period::PeriodTokenStore::for_drive`]); a drive whose key this device does not
//! hold (a plain one) keeps its tokens on this computer only
//! ([`crate::period::PeriodTokenStore::on_this_device`]).

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use serde::{Deserialize, Serialize};

#[cfg(feature = "encryption")]
use crate::state::write_atomic;
use crate::{
    error::{CloudError, CloudResult, Context},
    lock::LockDir,
    period::{IssueRequest, PeriodToken},
    state::{create_private_dir, write_json},
};

/// The format of a vault's JSON.
const VAULT_FORMAT: u32 = 2;

/// One checkout's period tokens as a grant the drive draws on: the months it pays for, its issue
/// key (a secret: `Debug` shows none), and where its instalments stand.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultGrant {
    pub checkout_id: String,
    /// The tier its tokens are for (`100GB`).
    pub tier: String,
    pub months: u32,
    /// base64url of 32 bytes, from the checkout's sealed sign-up.
    pub issue_key: String,
    /// How many months the token server has issued (as it last said).
    #[serde(default)]
    pub issued: u32,
    /// From when the next month may be asked for (seconds since 1970); `None`: ask.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_issue_at: Option<u64>,
    /// Why the token server issues no more of it (a final refusal), once it said so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    /// A monthly plan: each payment adds a month to `months`, issued with the same key.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recurring: bool,
    /// Bought to move the drive to its tier ([`crate::period::change_due`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub change_tier: bool,
    /// The drive was moved to its tier: its months are the drive's like any other.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub changed: bool,
}

impl fmt::Debug for VaultGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultGrant")
            .field("checkout_id", &self.checkout_id)
            .field("tier", &self.tier)
            .field("months", &self.months)
            .field("issue_key", &"<hidden>")
            .field("issued", &self.issued)
            .field("next_issue_at", &self.next_issue_at)
            .field("ended", &self.ended)
            .field("recurring", &self.recurring)
            .field("change_tier", &self.change_tier)
            .field("changed", &self.changed)
            .finish()
    }
}

impl VaultGrant {
    /// The grant of `checkout_id`'s `months` of `tier`, against `issue_key`, nothing issued yet.
    #[must_use]
    pub fn new(checkout_id: &str, tier: &str, months: u32, issue_key: &str) -> VaultGrant {
        VaultGrant {
            checkout_id: checkout_id.trim().to_string(),
            tier: tier.trim().to_string(),
            months,
            issue_key: issue_key.trim().to_string(),
            issued: 0,
            next_issue_at: None,
            ended: None,
            recurring: false,
            change_tier: false,
            changed: false,
        }
    }

    /// Months may still come: no final refusal, and a month not issued yet or a next one
    /// scheduled (a monthly plan paid up waits for its next payment).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.ended.is_none() && (self.issued < self.months || self.next_issue_at.is_some())
    }

    /// A move to its tier that has not happened yet.
    #[must_use]
    pub fn changes_tier(&self) -> bool {
        self.change_tier && !self.changed
    }

    /// Whether its next month may be asked for at `now`.
    #[must_use]
    pub fn may_ask(&self, now: u64) -> bool {
        self.is_open() && self.next_issue_at.is_none_or(|at| at <= now)
    }

    /// `other`'s word on the same grant joined in: the furthest schedule, the most months (a
    /// monthly plan's grow), an end or a change either saw.
    fn join(&mut self, other: &VaultGrant) {
        if other.issued > self.issued
            || (other.issued == self.issued && other.next_issue_at > self.next_issue_at)
        {
            self.issued = other.issued;
            self.next_issue_at = other.next_issue_at;
        }
        self.months = self.months.max(other.months);
        if self.ended.is_none() {
            self.ended.clone_from(&other.ended);
        }
        self.recurring |= other.recurring;
        self.change_tier |= other.change_tier;
        self.changed |= other.changed;
    }
}

/// A paid checkout of the drive as the price lock of its next renewal (`renews`: the token
/// server keeps that checkout's price for one renewal). Kept with the drive's period tokens,
/// so any device of the drive renews it; `used_by` names the renewal that took it, which is
/// the drive's lock after it. `Debug` shows no issue key.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenewalLock {
    pub checkout_id: String,
    pub tier: String,
    /// The checkout's issue key: what proves the lock is the drive's.
    pub issue_key: String,
    /// The signed price ladder version it was paid under, when the token server said it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ladder_version: Option<u64>,
    /// When it was kept (seconds since 1970): the newest unused lock of a tier is the one.
    #[serde(default)]
    pub kept_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_by: Option<String>,
}

impl fmt::Debug for RenewalLock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RenewalLock")
            .field("checkout_id", &self.checkout_id)
            .field("tier", &self.tier)
            .field("issue_key", &"<hidden>")
            .field("ladder_version", &self.ladder_version)
            .field("kept_at", &self.kept_at)
            .field("used_by", &self.used_by)
            .finish()
    }
}

/// What a drive's vault holds. It only grows: [`VaultState::join`] is a union, and a spent
/// nonce or a done request stays spent or done whichever copy is joined in later.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultState {
    // Crate-visible so the kit can build a partial state with `..VaultState::default()`.
    #[serde(default)]
    pub(crate) format: u32,
    /// The tokens kept, oldest first (spent ones are taken out).
    #[serde(default)]
    pub tokens: Vec<PeriodToken>,
    /// The nonces of the tokens redeemed, or refused as useless.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spent: Vec<String>,
    /// The checkouts' grants.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grants: Vec<VaultGrant>,
    /// The issue requests kept before they were sent, until their answer came.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<IssueRequest>,
    /// The ids of the issue requests answered or given up ([`IssueRequest::id`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub done: Vec<String>,
    /// The drive's paid checkouts as price locks for its renewals.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locks: Vec<RenewalLock>,
}

impl fmt::Debug for VaultState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultState")
            .field("tokens", &self.tokens)
            .field("spent", &self.spent.len())
            .field("grants", &self.grants)
            .field("issues", &self.issues)
            .field("done", &self.done.len())
            .field("locks", &self.locks)
            .finish()
    }
}

impl VaultState {
    /// A change that adds `tokens`.
    #[must_use]
    pub fn with_tokens(tokens: &[PeriodToken]) -> VaultState {
        VaultState {
            tokens: tokens.to_vec(),
            ..VaultState::default()
        }
    }

    /// A change that spends the token `nonce`.
    #[must_use]
    pub fn with_spent(nonce: &str) -> VaultState {
        VaultState {
            spent: vec![nonce.to_string()],
            ..VaultState::default()
        }
    }

    /// A change that keeps (or updates) `grant`.
    #[must_use]
    pub fn with_grant(grant: VaultGrant) -> VaultState {
        VaultState {
            grants: vec![grant],
            ..VaultState::default()
        }
    }

    /// `other` joined in, then what it spent or finished taken out.
    pub fn join(&mut self, other: &VaultState) {
        for token in &other.tokens {
            if !self.tokens.iter().any(|t| t.nonce == token.nonce) {
                self.tokens.push(token.clone());
            }
        }
        for nonce in &other.spent {
            if !self.spent.contains(nonce) {
                self.spent.push(nonce.clone());
            }
        }
        for grant in &other.grants {
            match self.grants.iter_mut().find(|g| g.checkout_id == grant.checkout_id) {
                Some(kept) => kept.join(grant),
                None => self.grants.push(grant.clone()),
            }
        }
        for request in &other.issues {
            if !self.issues.iter().any(|r| r.id() == request.id()) {
                self.issues.push(request.clone());
            }
        }
        for id in &other.done {
            if !self.done.contains(id) {
                self.done.push(id.clone());
            }
        }
        for lock in &other.locks {
            match self.locks.iter_mut().find(|l| l.checkout_id == lock.checkout_id) {
                Some(kept) => {
                    if kept.used_by.is_none() {
                        kept.used_by.clone_from(&lock.used_by);
                    }
                    kept.ladder_version = kept.ladder_version.or(lock.ladder_version);
                }
                None => self.locks.push(lock.clone()),
            }
        }
        let spent = &self.spent;
        self.tokens.retain(|t| !spent.contains(&t.nonce));
        let done = &self.done;
        self.issues.retain(|r| !done.contains(&r.id()));
    }

    /// Whether joining `other` in would change nothing.
    #[must_use]
    pub fn holds(&self, other: &VaultState) -> bool {
        let mut joined = self.clone();
        joined.join(other);
        joined == *self
    }

    /// The tokens of `tier` not spent, oldest first.
    #[must_use]
    pub fn unspent(&self, tier: &str) -> Vec<&PeriodToken> {
        self.tokens.iter().filter(|t| t.tier == tier).collect()
    }

    /// The grant of `checkout_id`.
    #[must_use]
    pub fn grant(&self, checkout_id: &str) -> Option<&VaultGrant> {
        self.grants.iter().find(|g| g.checkout_id == checkout_id)
    }

    /// The lock the drive's next renewal of `tier` names: the newest one no renewal took.
    #[must_use]
    pub fn renewal_lock(&self, tier: &str) -> Option<&RenewalLock> {
        self.locks
            .iter()
            .filter(|l| l.tier == tier && l.used_by.is_none())
            .max_by_key(|l| l.kept_at)
    }

    /// The issue requests of `checkout_id` still waiting for their answer.
    #[must_use]
    pub fn open_issues(&self, checkout_id: &str) -> Vec<&IssueRequest> {
        self.issues
            .iter()
            .filter(|r| r.checkout_id() == checkout_id)
            .collect()
    }

    fn stamped(mut self) -> VaultState {
        self.format = VAULT_FORMAT;
        self
    }
}

/// A place a drive's period tokens are kept.
pub trait TokenVault: Send + Sync {
    /// What it holds (nothing yet: empty).
    ///
    /// # Errors
    ///
    /// It cannot be read (no answer, a damaged or foreign object).
    fn read(&self) -> CloudResult<VaultState>;

    /// Joins `change` into what it holds: read, joined, written back; a writer that came in
    /// between is read and joined again, nothing of it lost. What it holds then.
    ///
    /// # Errors
    ///
    /// It cannot be read or written.
    fn join(&self, change: &VaultState) -> CloudResult<VaultState>;

    /// Where it is, for an error's words.
    fn place(&self) -> &'static str;
}

/// The vault on this computer: `<dir>/<drive id>.json`, readable by this user only (a file of an
/// older version holds its tokens only, and reads as them). Changed under a lock of its own, so
/// two windows lose none of each other's changes. When its file is gone or does not read, the
/// second copy beside it ([`LocalVault::second_copy`]) is what it holds, and the next change
/// writes it back.
///
/// With a [`VaultSeal`] the file is SEALED: the issue keys (bearers for months a customer paid
/// for) and the period tokens are ciphertext under a random key that lives in the keyring and
/// nowhere else (AZ-K13). A file written before the seal is read as it is and sealed by the next
/// change; a keyring that cannot give the key refuses ([`CloudError::Keyring`]) - the vault is
/// never written in the clear instead.
#[derive(Debug, Clone)]
pub struct LocalVault {
    path: PathBuf,
    dir: PathBuf,
    drive_id: String,
    /// This is the second copy of a vault, which has no copy of its own.
    is_copy: bool,
    /// What seals the files; none: they are plain JSON (a build without the encryption, the
    /// tests of the plain format).
    #[cfg(feature = "encryption")]
    seal: Option<VaultSeal>,
}

/// What seals the vault files of this computer: the keyring that holds the random key of each
/// drive's vault ([`azul_storage::footprint::KeyEntry::VaultKey`]). `Debug` shows no key.
#[cfg(feature = "encryption")]
#[derive(Debug, Clone)]
pub struct VaultSeal {
    keyring: crate::shared::SharedKeyring,
}

#[cfg(feature = "encryption")]
impl VaultSeal {
    /// Seals with the keys `keyring` holds.
    #[must_use]
    pub fn new(keyring: crate::shared::SharedKeyring) -> VaultSeal {
        VaultSeal { keyring }
    }

    /// The same, through another keyring over the same entries (a background job's, which never
    /// prompts).
    #[must_use]
    pub fn through(&self, keyring: crate::shared::SharedKeyring) -> VaultSeal {
        VaultSeal { keyring }
    }

    /// The key of `drive_id`'s vault files. `create`: made - random, kept in the keyring under
    /// the entry's lock, so two windows make one - when there is none; else `None` for none.
    ///
    /// # Errors
    ///
    /// The keyring cannot be read or does not take the new key (unavailable, refused, locked), or
    /// the entry holds no key.
    fn key(
        &self,
        drive_id: &str,
        create: bool,
    ) -> CloudResult<
        Option<azul_storage::crypto::Zeroizing<[u8; azul_storage::crypto::blob::KEY_LEN]>>,
    > {
        use azul_storage::footprint::KeyEntry;

        let name = KeyEntry::VaultKey.name(drive_id);
        if let Some(text) = self.keyring.get(&name)? {
            return decode_key(&text, &name).map(Some);
        }
        if !create {
            return Ok(None);
        }
        let _held = self.keyring.lock(&name)?;
        if let Some(text) = self.keyring.get(&name)? {
            return decode_key(&text, &name).map(Some);
        }
        let key = azul_storage::crypto::blob::new_key()
            .map_err(|e| CloudError::failed(format!("a vault key could not be made: {e}")))?;
        let text = azul_storage::crypto::Zeroizing::new(encode_key(&key));
        self.keyring.set(&name, &text)?;
        Ok(Some(key))
    }
}

/// A vault key as the keyring keeps it: base64url of its 32 bytes.
#[cfg(feature = "encryption")]
fn encode_key(key: &[u8; azul_storage::crypto::blob::KEY_LEN]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key)
}

/// The key `text` holds; an entry that is no key is an error, never "no key" (a vault sealed under
/// a key that was replaced by text must not be started over).
#[cfg(feature = "encryption")]
fn decode_key(
    text: &str,
    entry: &str,
) -> CloudResult<azul_storage::crypto::Zeroizing<[u8; azul_storage::crypto::blob::KEY_LEN]>> {
    use base64::Engine as _;
    let bytes = azul_storage::crypto::Zeroizing::new(
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(text.trim())
            .unwrap_or_default(),
    );
    let key: [u8; azul_storage::crypto::blob::KEY_LEN] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| CloudError::failed(format!("the keyring entry {entry} is no vault key")))?;
    Ok(azul_storage::crypto::Zeroizing::new(key))
}

/// How long a change waits for another window's change of the same file.
const LOCAL_WAIT: Duration = Duration::from_secs(30);

impl LocalVault {
    /// The vault of `drive_id` at `path` in the folder `dir`.
    #[must_use]
    pub fn new(dir: &Path, path: PathBuf, drive_id: &str) -> LocalVault {
        LocalVault {
            path,
            dir: dir.to_path_buf(),
            drive_id: drive_id.to_string(),
            is_copy: false,
            #[cfg(feature = "encryption")]
            seal: None,
        }
    }

    /// This vault with its files sealed by `seal`.
    #[cfg(feature = "encryption")]
    #[must_use]
    pub fn sealed(mut self, seal: VaultSeal) -> LocalVault {
        self.seal = Some(seal);
        self
    }

    /// The file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The vault's second copy on this computer (`<dir>/<drive id>.copy.json`, readable by this
    /// user only): what a grant is kept in beside the first when no vault of the drive can take
    /// it ([`DriveTokens::keep_second_copy`]).
    #[must_use]
    pub fn second_copy(&self) -> LocalVault {
        LocalVault {
            path: self.copy_path(),
            dir: self.dir.clone(),
            drive_id: self.drive_id.clone(),
            is_copy: true,
            #[cfg(feature = "encryption")]
            seal: self.seal.clone(),
        }
    }

    /// The file of the second copy.
    fn copy_path(&self) -> PathBuf {
        self.dir.join(format!("{}.copy.json", self.drive_id))
    }

    /// The folder of the locks a change of the vaults is made under, `<dir>/locks`.
    fn locks(&self) -> LockDir {
        LockDir::new(self.dir.join("locks"))
    }

    /// Every file this drive's vault on this computer is in: the vault, its second copy and the
    /// lock a change of them is made under (a removed drive leaves none of them).
    #[must_use]
    pub fn files(&self) -> Vec<PathBuf> {
        vec![
            self.path.clone(),
            self.second_copy().path,
            self.locks().file_of(&self.drive_id),
        ]
    }

    /// What the vault holds, and whether it is the second copy that says so: the file when it
    /// reads; else - the file gone, or cut, or damaged - the second copy beside it, when that
    /// reads (the vault only grows, so a copy that is older lacks what a newer file held and
    /// holds nothing else). A file that does not read and has no copy that does is an error,
    /// never an empty vault; and a missing file with a copy that does not read is one too.
    fn read_file(&self) -> CloudResult<(VaultState, bool)> {
        let first = self.read_vault(&self.path);
        if let Ok(Some(state)) = first {
            return Ok((state, false));
        }
        if self.is_copy {
            return first.map(|state| (state.unwrap_or_default(), false));
        }
        match self.read_vault(&self.copy_path()) {
            Ok(Some(copy)) => Ok((copy, true)),
            Ok(None) => first.map(|state| (state.unwrap_or_default(), false)),
            Err(copy_error) => first.and(Err(copy_error)),
        }
    }

    /// The vault in the file `path`: `None` when there is none. A sealed file needs this vault's
    /// seal and the key in the keyring; a plain file (written before the seal) reads as it is.
    fn read_vault(&self, path: &Path) -> CloudResult<Option<VaultState>> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).with_context(|| path.display().to_string()),
        };
        let unreadable = |why: &dyn fmt::Display| {
            CloudError::failed(format!("{} cannot be read: {why}", path.display()))
        };
        #[cfg(feature = "encryption")]
        if azul_storage::crypto::blob::is_sealed(&bytes) {
            let Some(seal) = &self.seal else {
                return Err(unreadable(
                    &"it is sealed, and this build opens no sealed vault",
                ));
            };
            let Some(key) = seal.key(&self.drive_id, false)? else {
                return Err(unreadable(&"its key is not in the keyring any more"));
            };
            let plain = azul_storage::crypto::blob::open(&key, &self.context(path), &bytes)
                .map_err(|e| unreadable(&e))?;
            return serde_json::from_slice(&plain)
                .map(Some)
                .map_err(|e| unreadable(&e));
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| unreadable(&e))
    }

    /// What binds a sealed file to its place: this crate, the drive and the file's role.
    #[cfg(feature = "encryption")]
    fn context(&self, path: &Path) -> String {
        let role = if path == self.copy_path() {
            "copy"
        } else {
            "vault"
        };
        format!("azcloud-kit/period-{role}|{}", self.drive_id)
    }

    /// Whether the file at the vault's path is a plain one that a seal is to replace.
    #[cfg(feature = "encryption")]
    fn is_plain_while_sealing(&self) -> bool {
        self.seal.is_some()
            && std::fs::read(&self.path)
                .is_ok_and(|bytes| !azul_storage::crypto::blob::is_sealed(&bytes))
    }

    #[cfg(not(feature = "encryption"))]
    fn is_plain_while_sealing(&self) -> bool {
        false
    }

    /// Writes the vault `state` to this vault's file: sealed with the keyring's key when this
    /// vault has a seal - the key made first when there is none - else as plain JSON.
    fn write_vault(&self, state: &VaultState) -> CloudResult<()> {
        #[cfg(feature = "encryption")]
        if let Some(seal) = &self.seal {
            let key = seal
                .key(&self.drive_id, true)?
                .ok_or_else(|| CloudError::failed("the vault's key could not be made"))?;
            let plain = azul_storage::crypto::Zeroizing::new(
                serde_json::to_vec(state)
                    .map_err(|e| CloudError::failed(format!("{}: {e}", self.path.display())))?,
            );
            let sealed = azul_storage::crypto::blob::seal(&key, &self.context(&self.path), &plain)
                .map_err(|e| CloudError::failed(format!("{}: {e}", self.path.display())))?;
            return write_atomic(&self.path, &sealed, true)
                .with_context(|| self.path.display().to_string());
        }
        write_json(&self.path, state, true)
    }
}

impl TokenVault for LocalVault {
    fn read(&self) -> CloudResult<VaultState> {
        self.read_file().map(|(state, _)| state)
    }

    fn join(&self, change: &VaultState) -> CloudResult<VaultState> {
        let _lock = self.locks().lock(&self.drive_id, LOCAL_WAIT)?;
        let (mut state, from_copy) = self.read_file()?;
        // What came from the copy is written to the file whether or not the change adds to it:
        // the file heals with the next change - and a plain file a seal is to replace is sealed
        // by the next change too.
        if !from_copy
            && state.holds(change)
            && state.format == VAULT_FORMAT
            && !self.is_plain_while_sealing()
        {
            return Ok(state);
        }
        state.join(change);
        let state = state.stamped();
        create_private_dir(&self.dir).with_context(|| self.dir.display().to_string())?;
        self.write_vault(&state)?;
        Ok(state)
    }

    fn place(&self) -> &'static str {
        "this computer"
    }
}

#[cfg(feature = "encryption")]
pub use in_drive::{DriveVault, UnlockedDrive};

#[cfg(feature = "encryption")]
mod in_drive {
    use std::{fmt, sync::Arc};

    use azul_storage::{
        crypto::{
            azl1::{self, PublicHeader, WriteOptions, HEADER_LEN},
            device,
            keys::load_key_chain,
            DriveKey, KeyId, ObjectId,
        },
        keyring::KeyringStore,
        Drive, DriveError, Limit, Precondition,
    };

    use super::{TokenVault, VaultState};
    use crate::error::{CloudError, CloudResult};

    /// A drive with its key on this device: its bucket (below the encryption) and the drive key.
    /// Made only where the key is held - what a vault in the drive needs.
    #[derive(Clone)]
    pub struct UnlockedDrive {
        drive_id: String,
        bucket: Arc<dyn Drive>,
        key: DriveKey,
    }

    impl fmt::Debug for UnlockedDrive {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("UnlockedDrive")
                .field("drive_id", &self.drive_id)
                .finish_non_exhaustive()
        }
    }

    impl UnlockedDrive {
        /// `drive_id`'s bucket with its `key`.
        #[must_use]
        pub fn new(drive_id: &str, bucket: Arc<dyn Drive>, key: DriveKey) -> UnlockedDrive {
            UnlockedDrive {
                drive_id: drive_id.trim().to_string(),
                bucket,
                key,
            }
        }

        /// The drive `drive_id` with the key `keyring` holds (or this device's member wrap in
        /// `bucket` opens); `None` when this device has no key of it (a plain drive, or one
        /// waiting for a join code or the recovery code).
        ///
        /// # Errors
        ///
        /// The bucket's or the keyring's refusal, a damaged wrap.
        pub fn open(
            drive_id: &str,
            bucket: Arc<dyn Drive>,
            keyring: &dyn KeyringStore,
        ) -> CloudResult<Option<UnlockedDrive>> {
            let key = device::unlock(bucket.as_ref(), keyring, drive_id)?;
            Ok(key.map(|key| UnlockedDrive::new(drive_id, bucket, key)))
        }

        #[must_use]
        pub fn drive_id(&self) -> &str {
            &self.drive_id
        }
    }

    /// The vault in the drive: the object [`DriveVault::KEY`], the vault's JSON as an AZL1
    /// object of the drive key - the operator holds the bucket, not the key, so neither the
    /// tokens nor the issue keys are readable there.
    ///
    /// Every version is sealed under the same object id with a fresh file key, so the storage can
    /// put an older valid version back (AZ-C15). That costs a delay, nothing else: the state only
    /// grows by unions, so an older version lacks what a newer one held and holds nothing else; a
    /// device that ever read the newer one keeps it in this computer's vault, and
    /// [`DriveTokens::state`](super::DriveTokens::state) joins every vault it can read and writes
    /// the union back into the ones that lack it - the replayed object is overwritten at the next
    /// look. A token the replay brings back is still spent here (the union keeps the spent nonce)
    /// and is refused by the token server where it is not. What a replay can still do - hide a
    /// version from a device that has no copy of it to heal from - the storage can do by deleting
    /// the object, which this design never relied on it not doing. A revision counter would only
    /// add an alarm nobody can act on.
    pub struct DriveVault {
        drive: UnlockedDrive,
        object: ObjectId,
    }

    /// How often a write that lost to another device's is read, joined and tried again.
    const RACES: usize = 8;

    /// The most the vault's object may weigh when its HEAD said nothing: its state is a few
    /// hundred bytes a month a drive has been paid for.
    const VAULT_CEILING: u64 = 16 << 20;

    impl DriveVault {
        /// The vault's object in the bucket, among the drive's own (`.azlin/`).
        pub const KEY: &'static str = ".azlin/period/v1";

        /// The vault of `drive`.
        #[must_use]
        pub fn new(drive: &UnlockedDrive) -> DriveVault {
            let derived = blake3::derive_key("azlin period vault v1", drive.drive_id.as_bytes());
            let mut id = [0_u8; 16];
            id.copy_from_slice(&derived[..16]);
            DriveVault {
                drive: drive.clone(),
                object: ObjectId(id),
            }
        }

        /// What the object holds, its entity tag (`None`: no object yet) and whether it is sealed
        /// under a drive key older than the current one (a rotation since it was written): the
        /// next write puts it under the current key, whether or not it has anything new.
        fn fetch(&self) -> CloudResult<(VaultState, Option<String>, bool)> {
            let bucket = &self.drive.bucket;
            let (etag, size) = match bucket.head(Self::KEY) {
                // a write is only ever conditional on the version read
                Ok(info) => (
                    info.etag.ok_or_else(|| {
                        CloudError::failed(
                            "the bucket names no entity tag of the drive's period vault",
                        )
                    })?,
                    info.size,
                ),
                Err(DriveError::NotFound { .. }) => {
                    return Ok((VaultState::default(), None, false))
                }
                Err(e) => return Err(e.into()),
            };
            // Within the size the HEAD just said: the bucket is not trusted to stop (X-DELTA-AZDRIVE-KEYS-43).
            let limit = Limit::as_the_head_said("the drive's period vault", size, VAULT_CEILING);
            let bytes = match bucket.get_limited(Self::KEY, limit) {
                Ok(bytes) => bytes,
                // deleted in between: the next write makes it again
                Err(DriveError::NotFound { .. }) => {
                    return Ok((VaultState::default(), None, false))
                }
                Err(e) => return Err(e.into()),
            };
            let (state, older) = self.open(&bytes)?;
            Ok((state, Some(etag), older))
        }

        /// The drive key that wrapped the object's file key: the current one, else an older one
        /// of the drive's key chain (the current key opens it) - a rotation re-wraps the index and
        /// the member files and leaves this object under the key it was written with
        /// (X-DELTA-AZDRIVE-KEYS-5).
        fn older_key(&self, id: &KeyId) -> Result<DriveKey, String> {
            let chain = load_key_chain(self.drive.bucket.as_ref())
                .map_err(|e| e.to_string())?
                .ok_or_else(|| String::from("the drive has no key chain"))?;
            chain
                .open(&self.drive.drive_id, &self.drive.key)
                .into_iter()
                .find(|key| key.id() == *id)
                .ok_or_else(|| String::from("it is sealed under a drive key the key chain lacks"))
        }

        /// The vault the sealed `bytes` hold, and whether they are under an older drive key.
        fn open(&self, bytes: &[u8]) -> CloudResult<(VaultState, bool)> {
            let damaged = |why: String| {
                CloudError::failed(format!("the drive's period vault does not open: {why}"))
            };
            if bytes.len() < HEADER_LEN {
                return Err(damaged(String::from("it is too short")));
            }
            let header =
                PublicHeader::parse(&bytes[..HEADER_LEN]).map_err(|e| damaged(e.to_string()))?;
            let older = header.wrapped_key.key_id != self.drive.key.id();
            let wrapper = if older {
                self.older_key(&header.wrapped_key.key_id)
                    .map_err(damaged)?
            } else {
                self.drive.key.clone()
            };
            let file_key = wrapper
                .unwrap_file_key(&header.wrapped_key, &self.object)
                .map_err(|e| damaged(e.to_string()))?;
            let plain = azl1::decrypt(bytes, &self.object, &file_key)
                .map_err(|e| damaged(e.to_string()))?;
            let state = serde_json::from_slice(&plain).map_err(|e| damaged(e.to_string()))?;
            Ok((state, older))
        }

        fn seal(&self, state: &VaultState) -> CloudResult<Vec<u8>> {
            let plain = serde_json::to_vec(state)
                .map_err(|e| CloudError::failed(format!("the period vault: {e}")))?;
            let (sealed, _) = azl1::encrypt(&plain, self.object, &self.drive.key, &WriteOptions::default())
                .map_err(|e| CloudError::failed(format!("the period vault is not sealed: {e}")))?;
            Ok(sealed)
        }
    }

    impl TokenVault for DriveVault {
        fn read(&self) -> CloudResult<VaultState> {
            Ok(self.fetch()?.0)
        }

        fn join(&self, change: &VaultState) -> CloudResult<VaultState> {
            for _ in 0..RACES {
                let (mut state, etag, older) = self.fetch()?;
                // An object under an older drive key is written again under the current one
                // even when it holds all of `change`: it opens through the key chain meanwhile,
                // and a vault that depended on that file for ever would be lost with it.
                if etag.is_some() && !older && state.holds(change) {
                    return Ok(state);
                }
                state.join(change);
                let state = state.stamped();
                let condition = match etag {
                    Some(etag) => Precondition::Matches(etag),
                    None => Precondition::Absent,
                };
                match self.drive.bucket.put_if(Self::KEY, &self.seal(&state)?, &condition) {
                    Ok(_) => return Ok(state),
                    // another device wrote in between: read it, join once more
                    Err(DriveError::Conflict { .. }) => continue,
                    Err(e) => return Err(e.into()),
                }
            }
            Err(CloudError::failed(format!(
                "the drive's period vault changed {RACES} times while this device wrote to it"
            )))
        }

        fn place(&self) -> &'static str {
            "the drive"
        }
    }
}

/// Why a drive has no vault of its own to keep period tokens in (and so keeps them on this
/// computer only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoVault {
    /// This device holds no key of the drive: a plain drive, or one waiting for its join code or
    /// the recovery code.
    NoKey,
    /// The drive could not be opened to look: why (it is not open in the app, the bucket did not
    /// answer, a refused unlock).
    NotOpened(String),
}

impl fmt::Display for NoVault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NoVault::NoKey => f.write_str("this computer holds no key of the drive yet"),
            NoVault::NotOpened(why) => write!(f, "the drive could not be opened: {why}"),
        }
    }
}

/// What became of the drive's own vault when a change was joined ([`DriveTokens::join`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveKeep {
    /// The drive's vault has it: every device of the drive that holds its key reads it.
    Written,
    /// The drive has no vault to write to: why. It is on this computer only.
    NotAvailable(NoVault),
    /// The drive's vault was asked and did not take it (no answer, a damaged object): why.
    Failed(String),
}

/// Where a change landed. This computer's vault is always among them (else
/// [`DriveTokens::join`] failed); the question is whether it is anywhere else: a paid grant is
/// erased from the keyring only when it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    drive: DriveKeep,
    second_copy: bool,
}

impl Kept {
    /// What became of the drive's own vault.
    #[must_use]
    pub fn drive(&self) -> &DriveKeep {
        &self.drive
    }

    /// A second copy was written on this computer beside the first
    /// ([`DriveTokens::keep_second_copy`]).
    #[must_use]
    pub fn has_second_copy(&self) -> bool {
        self.second_copy
    }

    /// The change is in two places: the drive's vault has it, or - the drive having none - a
    /// second copy was written here. Only then may the other copy (the keyring's) go.
    #[must_use]
    pub fn is_redundant(&self) -> bool {
        match &self.drive {
            DriveKeep::Written => true,
            DriveKeep::NotAvailable(_) | DriveKeep::Failed(_) => self.second_copy,
        }
    }

    /// Why it is not in two places yet, for a sentence; empty when it is.
    #[must_use]
    pub fn why_not_redundant(&self) -> String {
        if self.is_redundant() {
            return String::new();
        }
        match &self.drive {
            DriveKeep::Written => String::new(),
            DriveKeep::NotAvailable(why) => format!("the drive keeps no vault for it yet: {why}"),
            DriveKeep::Failed(why) => format!("the drive's vault did not take it: {why}"),
        }
    }
}

/// A drive's period tokens over its vaults: the reads are joined, every change goes to each.
pub struct DriveTokens {
    drive_id: String,
    /// This computer's first: a change it keeps is never lost, whatever the drive's answer.
    vaults: Vec<Arc<dyn TokenVault>>,
    /// This computer's vault again, for its second copy.
    local: LocalVault,
    /// Why there is no vault of the drive among `vaults`, when there is none.
    no_vault: Option<NoVault>,
}

impl fmt::Debug for DriveTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let places: Vec<&str> = self.vaults.iter().map(|v| v.place()).collect();
        f.debug_struct("DriveTokens")
            .field("drive_id", &self.drive_id)
            .field("vaults", &places)
            .finish()
    }
}

impl DriveTokens {
    /// `drive_id`'s tokens in `local` and, when there is one, in `drive`. Without one the reason
    /// is [`NoVault::NoKey`] ([`DriveTokens::without_drive_vault`] names another).
    #[must_use]
    pub fn new(drive_id: &str, local: LocalVault, drive: Option<Arc<dyn TokenVault>>) -> DriveTokens {
        let no_vault = drive.is_none().then_some(NoVault::NoKey);
        let mut vaults: Vec<Arc<dyn TokenVault>> = vec![Arc::new(local.clone())];
        vaults.extend(drive);
        DriveTokens {
            drive_id: drive_id.to_string(),
            vaults,
            local,
            no_vault,
        }
    }

    /// `drive_id`'s tokens on this computer only, because `why` (the drive has no vault to keep
    /// them in): what a change that is joined says of it ([`Kept`]).
    #[must_use]
    pub fn without_drive_vault(drive_id: &str, local: LocalVault, why: NoVault) -> DriveTokens {
        let mut tokens = DriveTokens::new(drive_id, local, None);
        tokens.no_vault = Some(why);
        tokens
    }

    #[must_use]
    pub fn drive_id(&self) -> &str {
        &self.drive_id
    }

    /// Whether the tokens are kept in the drive too (else on this computer only).
    #[must_use]
    pub fn in_the_drive(&self) -> bool {
        self.vaults.len() > 1
    }

    /// Every vault's state joined; a vault that lacks what another holds gets it (the first
    /// look after an update moves this computer's tokens into the drive). A vault that cannot
    /// be read is left out while another answers.
    ///
    /// # Errors
    ///
    /// No vault can be read.
    pub fn state(&self) -> CloudResult<VaultState> {
        self.read_all().map(|(state, _)| state)
    }

    /// [`DriveTokens::state`] for a decision that rests on something being ABSENT - "this drive
    /// has no price lock": every vault the drive has must have been read. A vault that did not
    /// answer, or the drive's own vault that this device could not open
    /// ([`NoVault::NotOpened`]), is an error naming it: what is sought may be in it. A drive that
    /// has no vault of its own ([`NoVault::NoKey`]) has this computer's alone.
    ///
    /// # Errors
    ///
    /// No vault can be read, or one of them could not be.
    pub fn state_of_all(&self) -> CloudResult<VaultState> {
        let (state, mut unread) = self.read_all()?;
        if let Some(NoVault::NotOpened(why)) = &self.no_vault {
            unread.push(format!("the drive: {why}"));
        }
        if unread.is_empty() {
            Ok(state)
        } else {
            Err(CloudError::failed(format!(
                "the period tokens of {} were not read in every place ({}), so what they do not \
                 hold cannot be told",
                self.drive_id,
                unread.join("; ")
            )))
        }
    }

    /// Every vault's state joined, and why each vault that could not be read was left out.
    fn read_all(&self) -> CloudResult<(VaultState, Vec<String>)> {
        let mut read = Vec::new();
        let mut errors = Vec::new();
        for vault in &self.vaults {
            match vault.read() {
                Ok(state) => read.push((vault, state)),
                Err(e) => errors.push(format!("{}: {e}", vault.place())),
            }
        }
        if read.is_empty() {
            return Err(CloudError::failed(format!(
                "the period tokens of {} cannot be read ({})",
                self.drive_id,
                errors.join("; ")
            )));
        }
        let mut joined = VaultState::default();
        for (_, state) in &read {
            joined.join(state);
        }
        for (vault, state) in &read {
            if !state.holds(&joined) {
                // healed at the next look when it fails now
                let _ = vault.join(&joined);
            }
        }
        Ok((joined, errors))
    }

    /// Joins `change` into every vault; where it landed ([`Kept`]).
    ///
    /// # Errors
    ///
    /// This computer's vault did not keep it (a vault in the drive that did not answer gets it
    /// at the next look, from this computer's: the answer is [`DriveKeep::Failed`], not an
    /// error - whoever erases another copy of a paid grant asks [`Kept::is_redundant`]).
    pub fn join(&self, change: &VaultState) -> CloudResult<Kept> {
        let mut kept_here = Err(CloudError::failed("no vault"));
        let mut drive = None;
        for (i, vault) in self.vaults.iter().enumerate() {
            let outcome = vault.join(change).map(|_| ());
            if i == 0 {
                kept_here = outcome.map_err(|e| e.context(format!("the period tokens of {}", self.drive_id)));
            } else {
                drive = Some(match outcome {
                    Ok(()) => DriveKeep::Written,
                    Err(e) => DriveKeep::Failed(e.to_string()),
                });
            }
        }
        kept_here?;
        Ok(Kept {
            drive: drive.unwrap_or_else(|| {
                DriveKeep::NotAvailable(self.no_vault.clone().unwrap_or(NoVault::NoKey))
            }),
            second_copy: false,
        })
    }

    /// Everything the vaults hold written to this computer's second copy
    /// ([`LocalVault::second_copy`]) too, and joined into the drive's vault once more; where the
    /// grants are now. For a drive that has no vault of its own: the second place.
    ///
    /// # Errors
    ///
    /// The vaults cannot be read, or the second copy cannot be written.
    pub fn keep_second_copy(&self) -> CloudResult<Kept> {
        let state = self.state()?;
        self.local.second_copy().join(&state).map_err(|e| {
            e.context(format!(
                "the second copy of the period tokens of {}",
                self.drive_id
            ))
        })?;
        let mut kept = self.join(&state)?;
        kept.second_copy = true;
        Ok(kept)
    }

    /// Whether the drive's own vault holds everything this computer's vault holds, so that this
    /// computer's may go with the drive: what the drive lacks is written into it once more, and
    /// then the drive's vault is read and asked. A drive with no vault of its own (its key is not
    /// here, or it could not be opened) holds nothing: this computer's vault is the only copy of
    /// the months paid, the grants and the issue keys unless it is empty.
    ///
    /// # Errors
    ///
    /// This computer's vault, or the drive's, cannot be read: whether the months are kept
    /// elsewhere is not known, and not knowing is no answer.
    pub fn local_is_redundant(&self) -> CloudResult<bool> {
        let local = self.local.read()?;
        let held = match self.vaults.get(1) {
            Some(drive) => {
                // collapse-ok: a write that does not go through leaves the drive lacking what it
                // lacked, which the read below finds.
                let _ = drive.join(&local);
                drive.read()?
            }
            None => VaultState::default(),
        };
        Ok(held.holds(&local))
    }

    /// The tokens of `tier` not spent, oldest first.
    ///
    /// # Errors
    ///
    /// No vault can be read.
    pub fn unspent(&self, tier: &str) -> CloudResult<Vec<PeriodToken>> {
        Ok(self.state()?.unspent(tier).into_iter().cloned().collect())
    }

    /// Adds `tokens`.
    ///
    /// # Errors
    ///
    /// This computer's vault did not keep them.
    pub fn add_tokens(&self, tokens: &[PeriodToken]) -> CloudResult<()> {
        self.join(&VaultState::with_tokens(tokens)).map(|_| ())
    }

    /// Spends the token `nonce` (redeemed, or refused as useless): it is taken out everywhere.
    ///
    /// # Errors
    ///
    /// This computer's vault did not keep it.
    pub fn spend(&self, nonce: &str) -> CloudResult<()> {
        self.join(&VaultState::with_spent(nonce)).map(|_| ())
    }

    /// Keeps `grant` (or what it says new of one kept).
    ///
    /// # Errors
    ///
    /// This computer's vault did not keep it.
    pub fn keep_grant(&self, grant: VaultGrant) -> CloudResult<()> {
        self.join(&VaultState::with_grant(grant)).map(|_| ())
    }
}
