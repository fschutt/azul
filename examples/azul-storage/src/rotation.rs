//! "I was hacked": a new drive key for an encrypted drive (feature `encryption`). After the
//! account's lockdown (every other device's token, key and link revoked at the token server),
//! [`rotate`] takes the old drive key out of every use:
//!
//! 1. A new drive key K2 goes into this device's keyring, beside the old one K1
//!    ([`device::rotation_key_entries`]); the journal `.azlin/keys/_rotation.key` (ids and the
//!    step reached, never a key: K1 may be in the attacker's hands, so nothing sealed with it may
//!    carry K2) says a rotation is under way.
//! 2. The index is sealed under K2 ([`IndexProvider::rekey`]) - first, so that an index that
//!    cannot be rekeyed stops the rotation before anything changed (the journal and K2 go
//!    again, the drive stays as it was).
//! 3. Every index entry's file key is re-wrapped from K1 to K2 in ONE index change, each
//!    expecting its object unchanged (a file written meanwhile is caught and the pass runs
//!    again). The objects stay: their header wraps keep K1, which only a recovery from the
//!    headers would read - the index is the authority. "Re-encrypt everything"
//!    ([`reencrypt_pass`]) writes every file into a new object with a new file key, so nothing
//!    in the bucket opens with K1 any more: recommended after a compromise.
//! 4. This device gets a NEW member key (the old one may have leaked with K1) and its wrap is
//!    sealed to K2 (the keyring's drive key becomes K2); every other member wrap and every open
//!    invite leaves the bucket - the other devices come back with new join codes.
//! 5. A new recovery code, its wrap under K2 in the old one's place: the code comes back for the
//!    recovery sheet (a resumed rotation makes a new one: the last code returned is the one).
//! 6. Incoming mail: a new drop key (the old one kept, sealed with K2, for drops the Worker
//!    sealed before it got the new public key - [`drops::rotate_drop_key`]).
//! 7. Every share is revoked (its manifest deleted).
//! 8. The journal and K1 go.
//!
//! Every step can be run again: a rotation that stopped (a crash, no network) resumes from the
//! journal on the device that started it (the one with K2), with the same call.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    crypto::{
        device,
        drops::{self, DropPublic},
        keys::{store_recovery_wrap, RecoveryCode, RecoveryKdf, RecoveryWrap, KEYS_PREFIX},
        DriveKey, KeyId,
    },
    encrypted::{EncryptedDrive, Expect, IndexChange, IndexEntry, IndexProvider, NameIndex, Rewrite},
    keyring::KeyringStore,
    ops::list_all,
    sharing::revoke_all_shares,
    Drive, DriveError, ListRequest,
};

/// The rotation's journal in the bucket.
pub const JOURNAL_FILE: &str = ".azlin/keys/_rotation.key";
/// How often the re-wrap pass runs again when files changed under it.
const REWRAP_ATTEMPTS: usize = 5;

/// How far a rotation got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// K2 is in the keyring; nothing in the bucket changed yet.
    Started,
    /// The index is sealed under K2 (its entries may still be wrapped with K1).
    Rekeyed,
    /// Every index entry is wrapped with K2.
    Rewrapped,
    /// This device's wrap is K2's, the others are gone.
    Members,
}

/// The journal: which keys and how far (no key).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub format: String,
    pub version: u32,
    pub kind: String,
    /// The old drive key's id (hex).
    pub old: String,
    /// The new drive key's id (hex).
    pub new: String,
    pub phase: Phase,
}

const JOURNAL_FORMAT: &str = "azlin-drive-key";
const JOURNAL_KIND: &str = "rotation";

impl Journal {
    fn new(old: &KeyId, new: &KeyId) -> Journal {
        Journal {
            format: JOURNAL_FORMAT.to_string(),
            version: 1,
            kind: JOURNAL_KIND.to_string(),
            old: old.to_hex(),
            new: new.to_hex(),
            phase: Phase::Started,
        }
    }
}

/// The rotation under way in `bucket`, if any (another device sees it and stops writing).
pub fn pending(bucket: &dyn Drive) -> Result<Option<Journal>, DriveError> {
    let bytes = match bucket.get(JOURNAL_FILE) {
        Ok(bytes) => bytes,
        Err(DriveError::NotFound { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    let journal: Journal = serde_json::from_slice(&bytes).map_err(|_| DriveError::Corrupt {
        key: JOURNAL_FILE.to_string(),
        reason: String::from("not a rotation journal"),
    })?;
    if journal.format != JOURNAL_FORMAT || journal.kind != JOURNAL_KIND || journal.version != 1 {
        return Err(DriveError::Corrupt {
            key: JOURNAL_FILE.to_string(),
            reason: String::from("a rotation journal of another kind or version"),
        });
    }
    Ok(Some(journal))
}

fn save(bucket: &dyn Drive, journal: &Journal) -> Result<(), DriveError> {
    let mut bytes = serde_json::to_vec_pretty(journal).unwrap_or_default();
    bytes.push(b'\n');
    bucket.put(JOURNAL_FILE, &bytes)
}

/// What a rotation did.
pub struct Rotated {
    /// The new recovery code, for the recovery sheet: stored nowhere.
    pub recovery_code: RecoveryCode,
    /// The new drive key's id.
    pub drive_key: KeyId,
    /// Index entries re-wrapped.
    pub rewrapped: usize,
    /// Member wraps and invites removed (the other devices).
    pub members_removed: usize,
    /// The new drop public key for the mail Worker, when incoming mail is on.
    pub drop_key: Option<DropPublic>,
    /// Shares revoked.
    pub shares_revoked: usize,
}

impl std::fmt::Debug for Rotated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rotated")
            .field("drive_key", &self.drive_key)
            .field("rewrapped", &self.rewrapped)
            .field("members_removed", &self.members_removed)
            .field("drop_key", &self.drop_key)
            .field("shares_revoked", &self.shares_revoked)
            .finish_non_exhaustive()
    }
}

/// Every entry of `index`, across all pages.
fn all_entries(index: &dyn NameIndex) -> Result<Vec<(String, IndexEntry)>, DriveError> {
    let mut out = Vec::new();
    let mut request = ListRequest::recursive("");
    loop {
        let page = index.list(&request)?;
        out.extend(page.entries);
        match page.next {
            Some(token) => request = request.with_continuation(token),
            None => return Ok(out),
        }
    }
}

/// Step 2: every entry's file key from `old` to `new`, in one index change; again while files
/// change under it. Returns how many entries were re-wrapped.
pub fn rewrap_index(index: &dyn NameIndex, old: &DriveKey, new: &DriveKey) -> Result<usize, DriveError> {
    let (old_id, new_id) = (old.id(), new.id());
    let mut total = 0;
    for _ in 0..REWRAP_ATTEMPTS {
        let mut changes = Vec::new();
        for (path, entry) in all_entries(index)? {
            let Some(object) = entry.object.as_ref() else {
                continue;
            };
            if object.wrapped_key.key_id == new_id {
                continue;
            }
            if object.wrapped_key.key_id != old_id {
                return Err(DriveError::Corrupt {
                    key: path,
                    reason: String::from("its key is wrapped by neither the old nor the new drive key"),
                });
            }
            let file_key = old
                .unwrap_file_key(&object.wrapped_key, &object.id)
                .map_err(|e| e.for_key(&path))?;
            let wrapped = new
                .wrap_file_key(&file_key, &object.id)
                .map_err(|e| e.for_key(&path))?;
            let id = object.id;
            let mut rewrapped = entry.clone();
            if let Some(o) = rewrapped.object.as_mut() {
                o.wrapped_key = wrapped;
            }
            changes.push(IndexChange::Put {
                path,
                entry: rewrapped,
                expect: Expect::Object(id),
            });
        }
        if changes.is_empty() {
            return Ok(total);
        }
        let count = changes.len();
        match index.apply(changes) {
            Ok(_) => total += count,
            Err(DriveError::Conflict { .. }) => {}
            Err(e) => return Err(e),
        }
    }
    Err(DriveError::Conflict {
        key: String::from("the drive kept changing during the key rotation: try again"),
    })
}

/// Step 4: every member wrap and invite but this device's own leaves the bucket.
fn remove_other_members(bucket: &dyn Drive, keep: &str) -> Result<usize, DriveError> {
    let mut removed = 0;
    for object in list_all(bucket, KEYS_PREFIX)? {
        let Some(name) = object
            .key
            .strip_prefix(KEYS_PREFIX)
            .and_then(|rest| rest.strip_suffix(".key"))
        else {
            continue;
        };
        if name == "recovery" || name.starts_with('_') || name == keep || name.contains('/') {
            continue;
        }
        match bucket.delete(&object.key) {
            Ok(()) | Err(DriveError::NotFound { .. }) => removed += 1,
            Err(e) => return Err(e),
        }
    }
    Ok(removed)
}

/// A key the rotation needs is not on this device.
fn missing(which: &str) -> DriveError {
    DriveError::Denied {
        message: format!(
            "the key rotation's {which} drive key is not on this device: finish it on the device \
             that started it"
        ),
    }
}

/// Rotates the drive key of `drive` (its plain `bucket`; the index from `provider`; the keys
/// in `keyring`), or resumes the rotation the bucket's journal names. See the module docs.
pub fn rotate(
    bucket: Arc<dyn Drive>,
    keyring: &dyn KeyringStore,
    drive: &str,
    provider: &dyn IndexProvider,
    kdf: RecoveryKdf,
) -> Result<Rotated, DriveError> {
    let (previous_entry, next_entry) = device::rotation_key_entries(drive);
    let (old, new, mut journal) = match pending(bucket.as_ref())? {
        None => {
            let Some(old) = device::load_drive_key(keyring, drive)? else {
                return Err(missing("old"));
            };
            let new = DriveKey::generate().map_err(|e| e.for_key(drive))?;
            device::store_key_at(keyring, &previous_entry, &old)?;
            device::store_key_at(keyring, &next_entry, &new)?;
            // The keyring must give K2 back before anything is wrapped with it.
            if device::load_key_at(keyring, &next_entry)?.as_ref() != Some(&new) {
                return Err(DriveError::Io(String::from(
                    "the keyring did not keep the new drive key",
                )));
            }
            let journal = Journal::new(&old.id(), &new.id());
            save(bucket.as_ref(), &journal)?;
            (old, new, journal)
        }
        Some(journal) => {
            let new = device::load_key_at(keyring, &next_entry)?
                .filter(|k| k.id().to_hex() == journal.new)
                .ok_or_else(|| missing("new"))?;
            let old = device::load_key_at(keyring, &previous_entry)?
                .filter(|k| k.id().to_hex() == journal.old)
                .ok_or_else(|| missing("old"))?;
            (old, new, journal)
        }
    };
    if journal.phase < Phase::Rekeyed {
        match provider.rekey(drive, Arc::clone(&bucket), &old, &new) {
            Ok(()) => {}
            Err(DriveError::Unsupported(why)) => {
                // Nothing changed yet: the drive stays as it was, K2 goes.
                let _ = bucket.delete(JOURNAL_FILE);
                device::delete_key_at(keyring, &previous_entry)?;
                device::delete_key_at(keyring, &next_entry)?;
                return Err(DriveError::Unsupported(why));
            }
            Err(e) => return Err(e),
        }
        journal.phase = Phase::Rekeyed;
        save(bucket.as_ref(), &journal)?;
    }
    let mut rewrapped = 0;
    if journal.phase < Phase::Rewrapped {
        let index = provider.open_index(drive, Arc::clone(&bucket), &new)?;
        rewrapped = rewrap_index(index.as_ref(), &old, &new)?;
        journal.phase = Phase::Rewrapped;
        save(bucket.as_ref(), &journal)?;
    }
    let mut members_removed = 0;
    if journal.phase < Phase::Members {
        // A new member key for this device, its wrap sealed to K2; K2 becomes the keyring's
        // drive key.
        device::delete_key_at(keyring, &device::member_key_entry(drive))?;
        let me = device::enroll(bucket.as_ref(), keyring, drive, &new)?;
        members_removed = remove_other_members(bucket.as_ref(), &me)?;
        journal.phase = Phase::Members;
        save(bucket.as_ref(), &journal)?;
    }
    // Again on every resume: the code returned last is the one the wrap holds.
    let recovery_code = RecoveryCode::generate().map_err(|e| e.for_key(drive))?;
    let wrap = RecoveryWrap::seal(&new, drive, &recovery_code, kdf).map_err(|e| e.for_key(drive))?;
    store_recovery_wrap(bucket.as_ref(), &wrap)?;
    let drop_key = drops::rotate_drop_key(bucket.as_ref(), &old, &new, drive)?;
    let shares_revoked = revoke_all_shares(bucket.as_ref())?;
    // Done: the journal and the old key go; K2 is the keyring's drive key (step 4).
    match bucket.delete(JOURNAL_FILE) {
        Ok(()) | Err(DriveError::NotFound { .. }) => {}
        Err(e) => return Err(e),
    }
    device::delete_key_at(keyring, &previous_entry)?;
    device::delete_key_at(keyring, &next_entry)?;
    Ok(Rotated {
        recovery_code,
        drive_key: new.id(),
        rewrapped,
        members_removed,
        drop_key,
        shares_revoked,
    })
}

// ==== Re-encrypt everything ====

/// The `format` of a saved [`ReencryptState`].
pub const REENCRYPT_FORMAT: &str = "azul-storage.reencryption";

/// Where "re-encrypt everything" is: what the caller saves after every file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReencryptState {
    pub format: String,
    /// Files modified before this (seconds since 1970) are the pass's: what was written after it
    /// is in new objects already.
    pub before: u64,
    /// The last path done (paths come in the index's order).
    #[serde(default)]
    pub cursor: Option<String>,
    /// Files written into new objects.
    #[serde(default)]
    pub done: u64,
    /// Files that could not be read (damaged): left, counted.
    #[serde(default)]
    pub failed: u64,
}

impl ReencryptState {
    /// A pass for the files modified before `before` (the rotation's end).
    #[must_use]
    pub fn new(before: u64) -> ReencryptState {
        ReencryptState {
            format: REENCRYPT_FORMAT.to_string(),
            before,
            cursor: None,
            done: 0,
            failed: 0,
        }
    }

    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Result<ReencryptState, DriveError> {
        let state: ReencryptState = serde_json::from_str(text).map_err(|e| {
            DriveError::InvalidConfig(format!("not a re-encryption state (line {})", e.line()))
        })?;
        if state.format != REENCRYPT_FORMAT {
            return Err(DriveError::InvalidConfig(format!(
                "not a re-encryption state (format {:?})",
                state.format
            )));
        }
        Ok(state)
    }
}

/// "Re-encrypt everything": every file of `drive` (opened with the new drive key) modified
/// before `state.before` is written into a new object with a new file key (its date kept), the
/// old object deleted - afterwards nothing in the bucket opens with the old drive key. `save`
/// gets the state after every file, `stop` is asked before every file; `Ok(true)` when every
/// file is done. A file the user changed meanwhile keeps the user's version (it is in a new
/// object anyway).
pub fn reencrypt_pass<D: Drive>(
    drive: &EncryptedDrive<D>,
    state: &mut ReencryptState,
    save: &mut dyn FnMut(&ReencryptState) -> Result<(), DriveError>,
    stop: &dyn Fn() -> bool,
) -> Result<bool, DriveError> {
    for (path, entry) in drive.all_entries("")? {
        let due = entry.object.is_some()
            && entry.modified.unwrap_or(0) < state.before
            && state.cursor.as_deref().map_or(true, |done| path.as_str() > done);
        if !due {
            continue;
        }
        if stop() {
            return Ok(false);
        }
        let result = drive.open_reader(&path).and_then(|mut reader| {
            drive.rewrite(&path, &entry, &mut reader, drive.options(), &|_| true, &|| {})
        });
        match result {
            Ok(Rewrite::Replaced(_)) => state.done += 1,
            Ok(Rewrite::Declined | Rewrite::Conflict) | Err(DriveError::NotFound { .. }) => {}
            Err(DriveError::Corrupt { .. }) => state.failed += 1,
            Err(e) => return Err(e),
        }
        state.cursor = Some(path);
        save(state)?;
    }
    Ok(true)
}
