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

use crate::{
    error::{CloudError, CloudResult, Context},
    lock::LockDir,
    period::{IssueRequest, PeriodToken},
    state::{create_private_dir, read_json, write_json},
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
    #[serde(default)]
    format: u32,
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
/// two windows lose none of each other's changes.
#[derive(Debug, Clone)]
pub struct LocalVault {
    path: PathBuf,
    dir: PathBuf,
    drive_id: String,
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
        }
    }

    /// The file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn read_file(&self) -> CloudResult<VaultState> {
        Ok(read_json::<VaultState>(&self.path)?.unwrap_or_default())
    }
}

impl TokenVault for LocalVault {
    fn read(&self) -> CloudResult<VaultState> {
        self.read_file()
    }

    fn join(&self, change: &VaultState) -> CloudResult<VaultState> {
        let _lock = LockDir::new(self.dir.join("locks")).lock(&self.drive_id, LOCAL_WAIT)?;
        let mut state = self.read_file()?;
        if state.holds(change) && state.format == VAULT_FORMAT {
            return Ok(state);
        }
        state.join(change);
        let state = state.stamped();
        create_private_dir(&self.dir).with_context(|| self.dir.display().to_string())?;
        write_json(&self.path, &state, true)?;
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
            device, DriveKey, ObjectId,
        },
        keyring::KeyringStore,
        Drive, DriveError, Precondition,
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
    pub struct DriveVault {
        drive: UnlockedDrive,
        object: ObjectId,
    }

    /// How often a write that lost to another device's is read, joined and tried again.
    const RACES: usize = 8;

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

        /// What the object holds and its entity tag (`None`: no object yet).
        fn fetch(&self) -> CloudResult<(VaultState, Option<String>)> {
            let bucket = &self.drive.bucket;
            let etag = match bucket.head(Self::KEY) {
                // a write is only ever conditional on the version read
                Ok(info) => info.etag.ok_or_else(|| {
                    CloudError::failed("the bucket names no entity tag of the drive's period vault")
                })?,
                Err(DriveError::NotFound { .. }) => return Ok((VaultState::default(), None)),
                Err(e) => return Err(e.into()),
            };
            let bytes = match bucket.get(Self::KEY) {
                Ok(bytes) => bytes,
                // deleted in between: the next write makes it again
                Err(DriveError::NotFound { .. }) => return Ok((VaultState::default(), None)),
                Err(e) => return Err(e.into()),
            };
            Ok((self.open(&bytes)?, Some(etag)))
        }

        fn open(&self, bytes: &[u8]) -> CloudResult<VaultState> {
            let damaged = |why: String| {
                CloudError::failed(format!("the drive's period vault does not open: {why}"))
            };
            if bytes.len() < HEADER_LEN {
                return Err(damaged(String::from("it is too short")));
            }
            let header = PublicHeader::parse(&bytes[..HEADER_LEN]).map_err(|e| damaged(e.to_string()))?;
            let file_key = self
                .drive
                .key
                .unwrap_file_key(&header.wrapped_key, &self.object)
                .map_err(|e| damaged(e.to_string()))?;
            let plain = azl1::decrypt(bytes, &self.object, &file_key).map_err(|e| damaged(e.to_string()))?;
            serde_json::from_slice(&plain).map_err(|e| damaged(e.to_string()))
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
                let (mut state, etag) = self.fetch()?;
                if etag.is_some() && state.holds(change) {
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

/// A drive's period tokens over its vaults: the reads are joined, every change goes to each.
pub struct DriveTokens {
    drive_id: String,
    /// This computer's first: a change it keeps is never lost, whatever the drive's answer.
    vaults: Vec<Arc<dyn TokenVault>>,
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
    /// `drive_id`'s tokens in `local` and, when there is one, in `drive`.
    #[must_use]
    pub fn new(drive_id: &str, local: LocalVault, drive: Option<Arc<dyn TokenVault>>) -> DriveTokens {
        let mut vaults: Vec<Arc<dyn TokenVault>> = vec![Arc::new(local)];
        vaults.extend(drive);
        DriveTokens {
            drive_id: drive_id.to_string(),
            vaults,
        }
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
        Ok(joined)
    }

    /// Joins `change` into every vault.
    ///
    /// # Errors
    ///
    /// This computer's vault did not keep it (a vault in the drive that did not answer gets it
    /// at the next look, from this computer's).
    pub fn join(&self, change: &VaultState) -> CloudResult<()> {
        let mut kept_here = Err(CloudError::failed("no vault"));
        for (i, vault) in self.vaults.iter().enumerate() {
            let outcome = vault.join(change).map(|_| ());
            if i == 0 {
                kept_here = outcome.map_err(|e| e.context(format!("the period tokens of {}", self.drive_id)));
            }
        }
        kept_here
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
        self.join(&VaultState::with_tokens(tokens))
    }

    /// Spends the token `nonce` (redeemed, or refused as useless): it is taken out everywhere.
    ///
    /// # Errors
    ///
    /// This computer's vault did not keep it.
    pub fn spend(&self, nonce: &str) -> CloudResult<()> {
        self.join(&VaultState::with_spent(nonce))
    }

    /// Keeps `grant` (or what it says new of one kept).
    ///
    /// # Errors
    ///
    /// This computer's vault did not keep it.
    pub fn keep_grant(&self, grant: VaultGrant) -> CloudResult<()> {
        self.join(&VaultState::with_grant(grant))
    }
}
