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
//!    sealed to K2 (the keyring's drive key becomes K2); every other member leaves the index's
//!    policy (the record of the members, `crate::meta::policy`), and every other member wrap
//!    and every open invite leaves the bucket - the other devices come back with new join
//!    codes.
//! 5. A new recovery code, its wrap under K2 in the old one's place: the code comes back for the
//!    recovery sheet (a resumed rotation makes a new one: the last code returned is the one).
//! 6. Incoming mail: a new drop key (the old one kept, sealed with K2, for drops the Worker
//!    sealed before it got the new public key - [`drops::rotate_drop_key`]).
//! 7. Every share is revoked (its manifest deleted).
//! 8. The journal goes, and K1 leaves the keyring.
//!
//! K1 is not lost, though: the KEY CHAIN keeps it ([`crate::crypto::keys::KeyChain`], the file
//! `.azlin/keys/_chain.key`), sealed under K2, so everyone who holds K2 - every device that
//! joins again, the recovery code - opens it (step 2, before anything lets go of K1). The history
//! of the drive (the index keeps every state), its trash and the versions a restore puts back
//! hold file keys wrapped by K1; they still open through the chain. So a restore as of a time
//! before the rotation brings back readable files. The price is honest: history older than the
//! rotation stays readable to anyone who held K1 (and can fetch the ciphertext), until "re-encrypt
//! everything" has rewritten the files and the old versions have expired. The chain is never
//! pruned here: a link goes only when no object that needs its key is left.
//!
//! The journal says how far a rotation got, and gates the steps a resume skips, so it is
//! authenticated: its MAC is keyed with K2, which only the device that started the rotation
//! keeps. A journal that anyone else wrote (whoever can write to the bucket can plant one)
//! is no rotation of this device: it is neither resumed nor does it stop the lockdown
//! ([`resumable`]), and a rotation that starts replaces it.
//!
//! Every step can be run again: a rotation that stopped (a crash, no network) resumes from the
//! journal on the device that started it (the one with K2), with the same call.

// panic = "abort" in every shipped profile, and this module reads bytes other people wrote: no
// unwrap, expect, panic, todo or unreachable in its shipped code (ARCH28 RF50).
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented
    )
)]

use std::{collections::BTreeSet, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    crypto::{
        device,
        drops::{self, DropPublic},
        keys::{
            load_key_chain, store_key_chain, RecoveryCode, RecoveryKdf, RecoveryWrap, KEYS_PREFIX,
        },
        DriveKey, KeyId,
    },
    encrypted::{EncryptedDrive, Expect, IndexChange, IndexEntry, IndexProvider, NameIndex, Rewrite},
    keyring::KeyringStore,
    meta::policy::{MemberChange, MemberRecord},
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

/// The journal: which keys and how far (no key), and a MAC under the new key
/// ([`Journal::authentic`]).
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
    /// BLAKE3 keyed hash (hex) of the journal's fields under a key derived from the NEW drive
    /// key: whoever writes to the bucket can plant a journal, only the device that keeps the new
    /// key can make one that verifies. Absent from a journal of before this field: it verifies
    /// nowhere.
    #[serde(default)]
    pub mac: String,
}

const JOURNAL_FORMAT: &str = "azlin-drive-key";
const JOURNAL_KIND: &str = "rotation";
/// BLAKE3 `derive_key` context of the journal's MAC key (from the new drive key).
const JOURNAL_MAC_CONTEXT: &str = "Azlin 2026-11-04 rotation journal: authentication";

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Started => "started",
            Phase::Rekeyed => "rekeyed",
            Phase::Rewrapped => "rewrapped",
            Phase::Members => "members",
        }
    }
}

impl Journal {
    fn new(drive: &str, old: &DriveKey, new: &DriveKey) -> Journal {
        let mut journal = Journal {
            format: JOURNAL_FORMAT.to_string(),
            version: 1,
            kind: JOURNAL_KIND.to_string(),
            old: old.id().to_hex(),
            new: new.id().to_hex(),
            phase: Phase::Started,
            mac: String::new(),
        };
        journal.seal(drive, new);
        journal
    }

    /// The MAC of the journal's fields for the drive `drive` under `new`.
    fn mac_of(&self, drive: &str, new: &DriveKey) -> blake3::Hash {
        let key = new.derive(JOURNAL_MAC_CONTEXT);
        let mut hasher = blake3::Hasher::new_keyed(&key);
        for part in [
            self.format.as_str(),
            self.kind.as_str(),
            drive,
            self.old.as_str(),
            self.new.as_str(),
            self.phase.name(),
        ] {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part.as_bytes());
        }
        hasher.finalize()
    }

    /// Sets the MAC after a change of the fields (the phase).
    fn seal(&mut self, drive: &str, new: &DriveKey) {
        self.mac = self.mac_of(drive, new).to_hex().to_string();
    }

    /// Whether this device wrote the journal: its MAC verifies under `new`, the key it names.
    /// A journal that anyone who can write to the bucket planted does not.
    #[must_use]
    pub fn authentic(&self, drive: &str, new: &DriveKey) -> bool {
        if new.id().to_hex() != self.new {
            return false;
        }
        // `blake3::Hash` compares in constant time.
        blake3::Hash::from_hex(&self.mac).is_ok_and(|mac| mac == self.mac_of(drive, new))
    }
}

/// The journal in `bucket`, if any - whoever wrote it. Anyone who can write to the bucket can
/// plant one, so it says that a rotation may be under way, nothing more: [`resumable`] is the
/// question whether it is this device's.
pub fn pending(bucket: &dyn Drive) -> Result<Option<Journal>, DriveError> {
    let bytes = match bucket.get_limited(JOURNAL_FILE, crate::limits::named::JOURNAL) {
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

/// The rotation of THIS device that stopped, if any: the journal in `bucket`, when its MAC
/// verifies under the new key the keyring keeps for it. `None` for no journal, a journal that
/// is no rotation of this device (planted, from another device, damaged, unreadable), and a
/// rotation whose new key this device lost. A caller that skips the account's lockdown because
/// a rotation is under way asks this, never [`pending`]: a journal anyone can plant would skip
/// the lockdown for the attacker.
pub fn resumable(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Option<Journal>, DriveError> {
    let journal = match pending(bucket) {
        Ok(Some(journal)) => journal,
        Ok(None) | Err(DriveError::Corrupt { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    let (_, next_entry) = device::rotation_key_entries(drive);
    let Some(new) = device::load_key_at(keyring, &next_entry)? else {
        return Ok(None);
    };
    Ok(journal.authentic(drive, &new).then_some(journal))
}

fn save(
    bucket: &dyn Drive,
    drive: &str,
    new: &DriveKey,
    journal: &mut Journal,
) -> Result<(), DriveError> {
    journal.seal(drive, new);
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
    rewrap_index_with(index, old, &[], new)
}

/// [`rewrap_index`] for a drive that was rotated before: an entry the index holds can be wrapped
/// by one of the `older` keys (the key chain's) as well - a restore as of a time before the
/// earlier rotation puts such pointers back at the head.
pub fn rewrap_index_with(
    index: &dyn NameIndex,
    old: &DriveKey,
    older: &[DriveKey],
    new: &DriveKey,
) -> Result<usize, DriveError> {
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
            let wrapper = if object.wrapped_key.key_id == old_id {
                Some(old)
            } else {
                older
                    .iter()
                    .find(|key| key.id() == object.wrapped_key.key_id)
            };
            let Some(wrapper) = wrapper else {
                return Err(DriveError::Corrupt {
                    key: path,
                    reason: String::from(
                        "its key is wrapped by neither the old nor the new drive key",
                    ),
                });
            };
            // A packed file's key is its pack's, wrapped for the pack.
            let bound = object.bucket_object();
            let file_key = wrapper
                .unwrap_file_key(&object.wrapped_key, &bound)
                .map_err(|e| e.for_key(&path))?;
            let wrapped = new
                .wrap_file_key(&file_key, &bound)
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

/// Seals the old drive key under the new one into the drive's key chain in `bucket` (a chain
/// that is there already keeps its links). Again for the same pair: one link.
fn chain_old_key(
    bucket: &dyn Drive,
    drive: &str,
    old: &DriveKey,
    new: &DriveKey,
) -> Result<(), DriveError> {
    let mut chain = load_key_chain(bucket)?.unwrap_or_default();
    chain.add(drive, old, new).map_err(|e| e.for_key(drive))?;
    store_key_chain(bucket, &chain)
}

/// Step 4: every member but this device leaves. First the index's record (`drive_key` opens
/// it): every member its policy names, and every wrap it holds. Then every member wrap and
/// invite in the bucket. Returns how many members and invites were removed, from either.
fn remove_other_members(
    bucket: &dyn Drive,
    drive_key: &DriveKey,
    keep: &str,
) -> Result<usize, DriveError> {
    let mut removed = BTreeSet::new();
    if let Some(mut record) = MemberRecord::open(bucket, drive_key)? {
        let others: Vec<String> = record
            .members()?
            .map(|members| members.ids())
            .unwrap_or_default()
            .into_iter()
            .filter(|member| member != keep)
            .collect();
        if !others.is_empty() {
            let changes: Vec<MemberChange> = others
                .iter()
                .map(|member| MemberChange::Remove {
                    member: member.clone(),
                })
                .collect();
            record.record(&changes)?;
        }
        removed.extend(others);
    }
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
            Ok(()) | Err(DriveError::NotFound { .. }) => {
                removed.insert(name.to_string());
            }
            Err(e) => return Err(e),
        }
    }
    Ok(removed.len())
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
    // Only a journal this device wrote is resumed: any other (planted by whoever can write to
    // the bucket, or a damaged one) is replaced by the journal of a rotation that starts.
    let (old, new, stashed, mut journal) = match resumable(bucket.as_ref(), keyring, drive)? {
        None => {
            let Some(old) = device::load_drive_key(keyring, drive)? else {
                return Err(missing("old"));
            };
            let new = DriveKey::generate().map_err(|e| e.for_key(drive))?;
            device::store_key_at(keyring, &previous_entry, &old)?;
            // The keyring must give K2 back before anything is wrapped with it.
            let stashed = device::stash(keyring, &next_entry, &new)?;
            let mut journal = Journal::new(drive, &old, &new);
            save(bucket.as_ref(), drive, &new, &mut journal)?;
            (old, new, stashed, journal)
        }
        Some(journal) => {
            let (new, stashed) = device::held(keyring, &next_entry)?
                .filter(|(k, _)| k.id().to_hex() == journal.new)
                .ok_or_else(|| missing("new"))?;
            let old = device::load_key_at(keyring, &previous_entry)?
                .filter(|k| k.id().to_hex() == journal.old)
                .ok_or_else(|| missing("old"))?;
            (old, new, stashed, journal)
        }
    };
    // The keys from before K1, from the key chain of earlier rotations: the history can hold
    // file keys they wrapped.
    let older: Vec<DriveKey> = match load_key_chain(bucket.as_ref())? {
        Some(chain) => chain.open(drive, &old),
        None => Vec::new(),
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
        // K1 into the key chain, sealed under K2, before anything lets go of K1: the history
        // and the versions a restore puts back hold file keys it wrapped.
        chain_old_key(bucket.as_ref(), drive, &old, &new)?;
        journal.phase = Phase::Rekeyed;
        save(bucket.as_ref(), drive, &new, &mut journal)?;
    }
    let mut rewrapped = 0;
    if journal.phase < Phase::Rewrapped {
        let index = provider.open_index(drive, Arc::clone(&bucket), &new)?;
        rewrapped = rewrap_index_with(index.as_ref(), &old, &older, &new)?;
        journal.phase = Phase::Rewrapped;
        save(bucket.as_ref(), drive, &new, &mut journal)?;
    }
    let mut members_removed = 0;
    if journal.phase < Phase::Members {
        // A new member key for this device, its wrap sealed to K2; K2 becomes the keyring's
        // drive key.
        device::delete_key_at(keyring, &device::member_key_entry(drive))?;
        let me = device::enroll(bucket.as_ref(), keyring, drive, &new)?;
        members_removed = remove_other_members(bucket.as_ref(), &new, &me)?;
        journal.phase = Phase::Members;
        save(bucket.as_ref(), drive, &new, &mut journal)?;
    }
    // Again on every resume: the code returned last is the one the wrap holds.
    let recovery_code = RecoveryCode::generate().map_err(|e| e.for_key(drive))?;
    let wrap = RecoveryWrap::seal(&new, drive, &recovery_code, kdf).map_err(|e| e.for_key(drive))?;
    device::publish_recovery_wrap(
        bucket.as_ref(),
        &stashed,
        &wrap,
        device::Publish::FirstReplace,
    )?;
    let drop_key = drops::rotate_drop_key(bucket.as_ref(), &old, &new, drive)?;
    let shares_revoked = revoke_all_shares(bucket.as_ref())?;
    // Done: the journal goes and K1 leaves the keyring (the key chain keeps it, sealed under
    // K2); K2 is the keyring's drive key (step 4).
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
    /// Whether the packs of small files were written anew (after the files of their own).
    #[serde(default)]
    pub packs_done: bool,
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
            packs_done: false,
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
/// old object deleted - afterwards nothing in the bucket opens with the old drive key. Small
/// files packed together are written into new packs at the end, every pack holding one of them
/// at once, and the old packs leave the bucket. `save` gets the state after every file (and
/// after the packs, when there were any), `stop` is asked before every file; `Ok(true)` when
/// every file is done. A file the user changed meanwhile keeps the user's version (it is in a
/// new object anyway).
pub fn reencrypt_pass<D: Drive>(
    drive: &EncryptedDrive<D>,
    state: &mut ReencryptState,
    save: &mut dyn FnMut(&ReencryptState) -> Result<(), DriveError>,
    stop: &dyn Fn() -> bool,
) -> Result<bool, DriveError> {
    for (path, entry) in drive.all_entries("")? {
        let due = entry.object.as_ref().is_some_and(|o| o.packed.is_none())
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
    if !state.packs_done {
        let report = drive.reencrypt_packs(state.before, crate::time::now_unix())?;
        state.packs_done = true;
        if report.packs_read > 0 {
            save(state)?;
        }
    }
    Ok(true)
}
