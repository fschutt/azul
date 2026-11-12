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
//! 3. Every index entry's file key is re-wrapped from K1 to K2, in batches of the entries one pack
//!    of the writer's cap holds ([`rewrap_batch`]: one index change each, and so one pack each -
//!    the writer refuses a pack over an index object's limit, which one change of a drive of some
//!    600,000 files was), each entry expecting its object unchanged (a file written meanwhile is
//!    caught and the pass runs again; a stop between two batches leaves a half re-wrapped index,
//!    which opens through the key chain, and every repeat skips the entries already wrapped with
//!    K2). The objects stay: their header wraps keep K1, which only a recovery from the
//!    headers would read - the index is the authority. "Re-encrypt everything"
//!    ([`reencrypt_pass`]) writes every file into a new object with a new file key, so no
//!    current file opens with K1 any more - on a drive that keeps its history the old versions
//!    still do, until they expire or the user has them erased (see below): recommended after a
//!    compromise.
//! 4. This device gets a NEW member key (the old one may have leaked with K1) and its wrap is
//!    sealed to K2 (the keyring's drive key becomes K2); every other member leaves the index's
//!    policy (the record of the members, `crate::meta::policy`), and every other member wrap
//!    and every open invite leaves the bucket - the other devices come back with new join
//!    codes.
//! 5. Incoming mail: a new drop key (the old one kept, sealed with K2, for drops the Worker
//!    sealed before it got the new public key - [`drops::rotate_drop_key`]).
//! 6. Every share is revoked (its manifest deleted).
//! 7. A new recovery code, its wrap under K2 in the old one's place: the code comes back for the
//!    recovery sheet (a resumed rotation makes a new one: the last code returned is the one).
//!    This is the LAST step that can fail: a failure before it leaves the old wrap and the old
//!    code as they were, and a failure after it cannot happen, because nothing after it returns
//!    an error - the code is the drive's now and has to reach the user.
//! 8. The journal goes, and K1 leaves the keyring; what could not go is said in
//!    [`Rotated::left_over`], it is not a failure of the rotation.
//!
//! K1 is not lost, though: the KEY CHAIN keeps it ([`crate::crypto::keys::KeyChain`], the file
//! `.azlin/keys/_chain.key`), sealed under K2, so everyone who holds K2 - every device that
//! joins again, the recovery code - opens it (step 2, before anything lets go of K1; it is read
//! back and looked at again before the recovery wrap is replaced, because the bucket's writers
//! can delete the file). The history of the drive (the index keeps every state), its trash and
//! the versions a restore puts back hold file keys wrapped by K1; they still open through the
//! chain. So a restore as of a time before the rotation brings back readable files. The price is
//! honest: history older than the rotation stays readable to anyone who held K1 (and can fetch
//! the ciphertext), and so do the objects of a drive that keeps its history, until "re-encrypt
//! everything" has rewritten the files and the old versions have expired - nothing in the bucket
//! is out of K1's reach before that. The chain is never pruned here: a link goes only when no
//! object that needs its key is left.
//!
//! The journal says how far a rotation got, and gates the steps a resume skips, so it is
//! authenticated: its MAC is keyed with K2, which only the device that started the rotation
//! keeps. A journal that anyone else wrote (whoever can write to the bucket can plant one)
//! is no rotation of this device: it is neither resumed nor does it stop the lockdown
//! ([`resumable`]).
//!
//! K2 is never made again while one may seal data. The keyring's `drive-key-next` is the one
//! copy of K2 until step 4 makes it the drive key (no wrap in the bucket holds it before that),
//! and the index is sealed under it from step 2 on. So a rotation that finds K2 in the
//! keyring and no journal of its own - the journal was deleted, replaced by another device's, or
//! is junk - goes on with THAT K2 ([`Start::Adopt`]: a new journal, every step again) and does
//! not make a second one; and one that finds the index open with neither key this device holds
//! stops at once, before the account's lockdown ([`ask`]).
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
            load_key_chain, store_key_chain, KeyChain, RecoveryCode, RecoveryKdf, RecoveryWrap,
            KEYS_PREFIX,
        },
        keystate::{self, BucketFacts, Op, Permit, Read, Verdict, Way},
        DriveKey, KeyId,
    },
    encrypted::{EncryptedDrive, Expect, IndexChange, IndexEntry, IndexProvider, NameIndex, Rewrite},
    keyring::KeyringStore,
    meta::{
        policy::{KeyCheck, MemberChange, MemberRecord},
        wal,
    },
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
/// question whether it is this device's. A file over the size a journal has is
/// [`DriveError::Corrupt`], like one that does not parse.
pub fn pending(bucket: &dyn Drive) -> Result<Option<Journal>, DriveError> {
    let bytes = match crate::limits::named::JOURNAL.read_from(bucket, JOURNAL_FILE) {
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
/// is no rotation of this device (planted, from another device, damaged, too big, unreadable),
/// and a rotation whose new key this device lost. A caller that skips the account's lockdown
/// because a rotation is under way asks this (through [`ask`]), never [`pending`]: a
/// journal anyone can plant would skip the lockdown for the attacker.
///
/// The keyring is asked FIRST: a device without a new key of a rotation has no journal of its own,
/// whatever the bucket holds, and the bucket is not even read - a file planted there, of any size,
/// cannot stop "I was hacked" before its lockdown.
pub fn resumable(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Option<Journal>, DriveError> {
    let (_, next_entry) = device::rotation_key_entries(drive);
    let next = device::load_key_at(keyring, &next_entry);
    resumable_seen(bucket, &next, drive, &mut BucketFacts::default())
}

/// [`resumable`] over the read of `drive-key-next` that the caller made, noting what it read of the
/// bucket's journal in `seen`: the key permit's judge is told the facts the guard itself read, never a
/// second look.
fn resumable_seen(
    bucket: &dyn Drive,
    next: &Result<Option<DriveKey>, DriveError>,
    drive: &str,
    seen: &mut BucketFacts,
) -> Result<Option<Journal>, DriveError> {
    let Some(new) = next.clone()? else {
        return Ok(None);
    };
    let journal = match pending(bucket) {
        Ok(Some(journal)) => journal,
        Ok(None) => {
            seen.journal = Read::Absent;
            return Ok(None);
        }
        Err(DriveError::Corrupt { .. }) => {
            seen.journal = Read::Found(keystate::Journal::NotOwn);
            return Ok(None);
        }
        Err(e) => {
            seen.journal = Read::Failed(e.to_string());
            return Err(e);
        }
    };
    let own = journal.authentic(drive, &new);
    seen.journal = Read::Found(if own {
        keystate::Journal::Own
    } else {
        keystate::Journal::NotOwn
    });
    Ok(own.then_some(journal))
}

/// Whether a key change of this device is unfinished: the keyring holds the new key of a rotation
/// (`drive-key-next`) and it is not the drive key yet, so the index may be sealed under a key
/// the drive key does not open, and the keyring's copy of K2 is the only one. Nothing of such a
/// drive is deleted on the word of a flag that was made before: a removal of the drive keeps its
/// keys (X-DELTA-AZDRIVE-KEYS-2).
///
/// The new key may be the drive key already (step 4 of [`rotate`] makes it so) and the change still
/// not over: the keys it replaces (`drive-key-previous`) leave the keyring only with the last step,
/// after the recovery wrap was replaced. A new key that is the drive key WITH the previous key still
/// there is a rotation that went past its Members step - its journal may be gone, and the recovery
/// wrap in the bucket still seals the old key - and is unfinished; one WITHOUT it is a finished
/// rotation whose cleanup took the previous key and not the new key's entry (X-DELTA-STRUCT-KEYSTATE-1).
pub fn unfinished(keyring: &dyn KeyringStore, drive: &str) -> Result<bool, DriveError> {
    unfinished_in(&device::read_entries(keyring, drive))
}

/// [`unfinished`] over entries read already: the guard and the key permit's evidence are the same
/// reads ([`device::KeyEntries::facts`]).
pub(crate) fn unfinished_in(entries: &device::KeyEntries) -> Result<bool, DriveError> {
    let Some(next) = entries.next.clone()? else {
        return Ok(false);
    };
    let Some(current) = entries.drive.clone()? else {
        return Ok(true);
    };
    if current != next {
        return Ok(true);
    }
    Ok(entries.previous.clone()?.is_some())
}

/// Whether a rotation of this device got as far as the keyring: `drive-key-previous` or
/// `drive-key-next` is there (R1 and R2 write them first, R17 deletes them). From then on the drive's
/// index, its keys and its recovery wrap may have changed, and a recovery code typed back right
/// before is no witness for what the wrap holds now. A call of [`rotate`] that failed before - the
/// key permit, the account's lockdown, the first keyring write - changed nothing of the drive. A
/// keyring that cannot be read is an error: the caller takes it as "begun"
/// (X-DELTA-KEYS2-1).
pub fn begun(keyring: &dyn KeyringStore, drive: &str) -> Result<bool, DriveError> {
    let (previous_entry, next_entry) = device::rotation_key_entries(drive);
    for entry in [&previous_entry, &next_entry] {
        if device::load_key_at(keyring, entry)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// How a call of [`rotate`] starts, decided from the keyring and the bucket alone: nothing is
/// written for it.
pub enum Start {
    /// The journal this device wrote: the rotation goes on where it stopped.
    Resume(Journal),
    /// This device kept the new key of an earlier try (`drive-key-next`, not the drive's key yet),
    /// and the bucket's journal is not its own any more - deleted, replaced by another device's,
    /// junk. That key may seal the drive's index already, and nothing else holds it: it is the
    /// rotation's new key again, with a journal written anew, and every step runs again (each one
    /// is safe to repeat).
    Adopt { old: DriveKey, new: DriveKey },
    /// A rotation that starts: a new key is made.
    Fresh { old: DriveKey },
}

/// The sentence of a rotation that cannot start ([`opens_nothing`]); also what the key permit
/// says of a drive whose keys opened nothing ([`crate::crypto::keystate::Refusal::OpensNothing`]).
pub(crate) const OPENS_NOTHING: &str =
    "the drive's index opens with none of the drive keys this device holds: its keys were \
     changed from another device. Nothing was changed here: join the drive again with a \
     code from that device, or unlock it with its recovery code";

/// A rotation cannot start: the drive's index opens with none of the drive keys this device holds,
/// so the keys were changed from another device, and a rotation here could only fail half way.
pub(crate) fn opens_nothing() -> DriveError {
    DriveError::Denied {
        message: String::from(OPENS_NOTHING),
    }
}

/// Whether the drive's index (`bucket`'s repository) opens with one of `keys`, or there is no
/// index to ask (an empty drive: nothing in it is sealed with a key).
fn index_opens_with(bucket: &dyn Drive, keys: &[&DriveKey]) -> Result<bool, DriveError> {
    for key in keys {
        match MemberRecord::check(bucket, key)? {
            KeyCheck::Opens(_) | KeyCheck::NoIndex => return Ok(true),
            KeyCheck::Refuses => {}
        }
    }
    Ok(false)
}

/// [`index_opens_with`] that notes its answer in `seen` (the key permit's shadow, as
/// [`resumable_seen`]).
fn index_opens_seen(
    bucket: &dyn Drive,
    keys: &[&DriveKey],
    seen: &mut BucketFacts,
) -> Result<bool, DriveError> {
    let opens = index_opens_with(bucket, keys);
    seen.index_opens = Read::value(opens.clone());
    opens
}

/// How a call of [`rotate`] would start, and refuses with nothing changed where it could not
/// finish: no drive key on this device (`Denied`), or an index that opens with neither the drive
/// key nor the kept new key (`Denied`). Reads the keyring and the bucket; writes neither.
pub fn start(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Start, DriveError> {
    let entries = device::read_entries(keyring, drive);
    let started = start_seen(bucket, &entries, drive, &mut BucketFacts::default())?;
    // After the guard: the key permit's judge knows nothing of the writer's cap.
    if let Start::Fresh { old } = &started {
        index_is_resealable(bucket, old)?;
    }
    Ok(started)
}

/// The pre-flight of a rotation that starts (X-DELTA-KEYS2-2): R4 seals every pack of the index
/// again as one pack of its own, and the writer refuses a pack over [`wal::writable_max`]
/// ([`wal::check_writable`]). A drive written before the writer had a cap can hold such a pack (some
/// 600,000 files in one change): its R4 would stop with `Unsupported` after the account's lockdown
/// had locked the other devices out of a drive whose key is not replaced. It is refused here instead,
/// before the lockdown and before R1, in words, and nothing is changed. The re-wrap (R7) needs no
/// such check: it goes in batches ([`rewrap_batch`]) however many files the drive holds. A drive
/// without an index has no pack to seal again.
fn index_is_resealable(bucket: &dyn Drive, key: &DriveKey) -> Result<(), DriveError> {
    let KeyCheck::Opens(record) = MemberRecord::check(bucket, key)? else {
        return Ok(());
    };
    let (biggest, max) = (record.biggest_object(), wal::writable_max());
    if biggest > max {
        return Err(DriveError::Unsupported(format!(
            "the drive's index holds an object of {biggest} bytes, more than an index object may be \
             ({max} bytes), and a new drive key would have to write it again: the keys of this drive \
             cannot be changed here yet. Nothing was changed"
        )));
    }
    Ok(())
}

/// What `start` and `ask` decide on, from the keyring's entries (read once by the
/// caller) and what they read of the bucket on the way (`seen`).
fn start_seen(
    bucket: &dyn Drive,
    entries: &device::KeyEntries,
    drive: &str,
    seen: &mut BucketFacts,
) -> Result<Start, DriveError> {
    if let Some(journal) = resumable_seen(bucket, &entries.next, drive, seen)? {
        return Ok(Start::Resume(journal));
    }
    let Some(current) = entries.drive.clone()? else {
        return Err(missing("old"));
    };
    match entries.next.clone()? {
        // The drive key itself in `drive-key-next` is a rotation that went past its Members step
        // (its previous key still kept: `Again` for the key permit) or a finished one that left its
        // entry behind: nothing of it is resumed, a new rotation starts.
        Some(next) if next != current => {
            if !index_opens_seen(bucket, &[&current, &next], seen)? {
                return Err(opens_nothing());
            }
            Ok(Start::Adopt {
                old: current,
                new: next,
            })
        }
        _ => {
            if !index_opens_seen(bucket, &[&current], seen)? {
                return Err(opens_nothing());
            }
            Ok(Start::Fresh { old: current })
        }
    }
}

/// The evidence a key change hands the key permit's judge: the keyring's entries the guard read
/// (the same reads: nothing is read again), the keyring's protection (no read: a property of the
/// keyring), and what the guard itself read of the bucket.
fn evidence_of<'a>(
    drive: &'a str,
    keyring: &dyn KeyringStore,
    entries: &device::KeyEntries,
    seen: BucketFacts,
) -> keystate::Evidence<'a> {
    let mut evidence = keystate::Evidence::new(drive, crate::time::now_unix());
    evidence.keyring = entries.facts();
    evidence.keyring.keeps_keys = device::keeps_keys_fact(keyring);
    evidence.bucket = seen;
    evidence
}

/// What a key change is let do: the permit of the rotation itself and, when the account's lockdown is
/// still to run, the permit of that. Both are asked TOGETHER ([`ask`]): the lockdown locks every other
/// device out at the token server, so it must not run for a drive whose key cannot be replaced.
pub struct Rotation {
    rotate: Permit,
    lockdown: Option<Permit>,
}

impl Rotation {
    /// The permit [`rotate`] takes.
    #[must_use]
    pub fn permit(&self) -> &Permit {
        &self.rotate
    }

    /// The permit of the account's lockdown, which runs BEFORE [`rotate`]: `None` for a rotation of
    /// this device that resumes (it has had its lockdown), `Some` for every other way on.
    #[must_use]
    pub fn lockdown(&self) -> Option<&Permit> {
        self.lockdown.as_ref()
    }
}

/// The way a key change goes on, as the key permit says it.
fn way_of(started: &Start) -> Way {
    match started {
        Start::Resume(_) => Way::Resume,
        Start::Adopt { .. } => Way::Adopt,
        Start::Fresh { .. } => Way::New,
    }
}

/// The permit is no longer the answer: what it was asked on has changed.
fn changed_since_asked() -> DriveError {
    DriveError::Denied {
        message: String::from(
            "the keys of this drive changed since the key change was asked for: nothing was \
             changed here, ask again",
        ),
    }
}

/// May this device change the drive's key, and has the account's lockdown still to run? Reads the
/// keyring and the bucket, writes neither, and answers BEFORE the lockdown: a key change that cannot
/// finish - no drive key here, an index that opens with none of the keys this device holds, a keyring
/// or a bucket that cannot be read, a keyring that keeps no key material, an index the writer cannot
/// seal again - is refused HERE, while the other devices still have their access (X-DELTA-AZDRIVE-KEYS-1,
/// -6, -25; X-DELTA-KEYS2-2). A refusal keeps the words and the class the guards had: the keyring's or
/// the bucket's own error for a read that failed, `Denied` with the sentence of `opens_nothing`.
///
/// # Errors
///
/// As above.
pub fn ask(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Rotation, DriveError> {
    // A keyring that keeps no key material says so FIRST, whatever else is the matter with the drive on
    // this device (the user can allow it): the new key could not be written, and the lockdown would have
    // locked the others out for nothing (X-DELTA-AZDRIVE-KEYS-25).
    if let Some(reason) = keystate::keyring_refusal(drive, &device::keeps_keys_fact(keyring)) {
        return Err(reason.error(drive));
    }
    let entries = device::read_entries(keyring, drive);
    let mut seen = BucketFacts::default();
    let started = start_seen(bucket, &entries, drive, &mut seen)?;
    let evidence = evidence_of(drive, keyring, &entries, seen);
    let rotate = match keystate::ask(Op::Rotate, &evidence) {
        Verdict::Permit(permit) => permit,
        Verdict::Refuse(reason) => return Err(reason.error(drive)),
    };
    // The judge and the guard decide on the same reads: they must say the same way on.
    if rotate.way() != way_of(&started) {
        return Err(changed_since_asked());
    }
    // After the judge, which knows nothing of the writer's cap: before the lockdown, so that a drive
    // whose index the rotation cannot seal again is refused while the other devices have their access.
    if let Start::Fresh { old } = &started {
        index_is_resealable(bucket, old)?;
    }
    // A rotation of this device that resumes has had its lockdown; every other way on runs it.
    let lockdown = if matches!(started, Start::Resume(_)) {
        None
    } else {
        match keystate::ask(Op::Lockdown, &evidence) {
            Verdict::Permit(permit) => Some(permit),
            Verdict::Refuse(reason) => return Err(reason.error(drive)),
        }
    };
    Ok(Rotation { rotate, lockdown })
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
    /// Further recovery codes' wraps removed (`recovery-<id>.key`, the kits made with
    /// `add_recovery_code`): each sealed the old key, and its paper opens nothing now
    /// (X-DELTA-AZDRIVE-KEYS-27). Counted apart from the members: a user told "3 other computers
    /// removed" would not look for the kit in a drawer.
    pub codes_removed: usize,
    /// The new drop public key for the mail Worker, when incoming mail is on.
    pub drop_key: Option<DropPublic>,
    /// Shares revoked.
    pub shares_revoked: usize,
    /// What the rotation's last step could not take away, each a sentence naming the journal or
    /// the keyring entry (never a key): the keys ARE changed and the new recovery code is the
    /// drive's, so this is a leftover to clear and no failure. Empty: nothing is left.
    pub left_over: Vec<String>,
}

impl std::fmt::Debug for Rotated {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rotated")
            .field("drive_key", &self.drive_key)
            .field("rewrapped", &self.rewrapped)
            .field("members_removed", &self.members_removed)
            .field("codes_removed", &self.codes_removed)
            .field("drop_key", &self.drop_key)
            .field("shares_revoked", &self.shares_revoked)
            .field("left_over", &self.left_over)
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

/// How many entries one batch of the re-wrap changes (R7): the pack a batch writes is a quarter of
/// the smaller of the writer's cap ([`wal::writable_max`]) and a compaction's biggest pack
/// ([`wal::COMPACT_PACK_MAX`], so that a frozen drive's node still takes it) at the most when
/// each entry weighs [`wal::ENTRY_PACK_BYTES`]; the rest is for the trees of the folders the batch
/// writes again. Never fewer than one. The re-wrap used to be ONE change of every entry: for a
/// drive of some 600,000 files its pack was over the cap, `put_sealed_pack` refused it after the
/// index was sealed under K2, and every retry stopped there (X-DELTA-KEYS2-2).
pub(crate) fn rewrap_batch() -> usize {
    let pack = wal::writable_max().min(wal::COMPACT_PACK_MAX);
    usize::try_from(pack / 4 / wal::ENTRY_PACK_BYTES)
        .unwrap_or(usize::MAX)
        .max(1)
}

/// Step 2: every entry's file key from `old` to `new`, in batches ([`rewrap_batch`]); again while
/// files change under it. Returns how many entries were re-wrapped.
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
        // One change (one pack) per batch: a batch that lands is final, a batch whose entry changed
        // under it (`Conflict`: nothing of it changed) is seen again by the next pass, and a stop
        // in between leaves a half re-wrapped index that the next call goes on with.
        let batch = rewrap_batch();
        let mut rest = changes;
        while !rest.is_empty() {
            let later = rest.split_off(batch.min(rest.len()));
            let now = std::mem::replace(&mut rest, later);
            let count = now.len();
            match index.apply(now) {
                Ok(_) => total += count,
                Err(DriveError::Conflict { .. }) => {}
                Err(e) => return Err(e),
            }
        }
    }
    Err(DriveError::Conflict {
        key: String::from("the drive kept changing during the key rotation: try again"),
    })
}

/// The drive's key chain in `bucket` as a rotation takes it: the chain there is, a new one where
/// there is none - and where the file is damaged or too big to be one (whoever can write to the
/// bucket can put either there): nothing a damaged file holds can be opened or trusted, so the
/// rotation does not stop on it and the link it adds starts the chain anew. A file of a newer
/// version, and a bucket that cannot be read, stop it.
fn chain_of(bucket: &dyn Drive) -> Result<KeyChain, DriveError> {
    match load_key_chain(bucket) {
        Ok(Some(chain)) => Ok(chain),
        Ok(None) | Err(DriveError::Corrupt { .. }) => Ok(KeyChain::new()),
        Err(e) => Err(e),
    }
}

/// Seals the old drive key under the new one into the drive's key chain in `bucket` (a chain
/// that is there already keeps its links), and reads the chain back: the old key opens from it
/// with the new one, or this fails. Again for the same pair: one link. It is the one copy of K1
/// that outlives the keyring's, so nothing lets go of K1 on the word of a `put` alone.
pub(crate) fn chain_old_key(
    bucket: &dyn Drive,
    drive: &str,
    old: &DriveKey,
    new: &DriveKey,
) -> Result<(), DriveError> {
    let mut chain = chain_of(bucket)?;
    if chain.open(drive, new).contains(old) {
        // Already there, and it opens: the file as it is was just read, nothing to write.
        return Ok(());
    }
    chain.add(drive, old, new).map_err(|e| e.for_key(drive))?;
    store_key_chain(bucket, &chain)?;
    if chain_of(bucket)?.open(drive, new).contains(old) {
        Ok(())
    } else {
        Err(DriveError::Io(String::from(
            "the key chain does not hold the old drive key after it was written: the old key \
             stays on this device",
        )))
    }
}

/// Step 4: every member but this device leaves. First the index's record (`drive_key` opens
/// it): every member its policy names, and every wrap it holds. Then every member wrap and
/// invite in the bucket, and every further recovery code's wrap (`recovery-<id>`). Returns how many
/// members and invites were removed, from either, and how many further recovery codes.
pub(crate) fn remove_other_members(
    bucket: &dyn Drive,
    drive_key: &DriveKey,
    keep: &str,
) -> Result<(usize, usize), DriveError> {
    let mut removed = BTreeSet::new();
    let mut codes = BTreeSet::new();
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
                if name.starts_with("recovery-") {
                    codes.insert(name.to_string());
                } else {
                    removed.insert(name.to_string());
                }
            }
            Err(e) => return Err(e),
        }
    }
    Ok((removed.len(), codes.len()))
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
/// in `keyring`), or resumes the rotation the bucket's journal names, or goes on with the new
/// key of an earlier try whose journal is gone ([`Start`]). See the module docs.
///
/// `permit` is the rotation's own ([`ask`]): the key permit's word that this device may change the
/// drive's key, and how (a new key, the journal's rotation, the earlier try's key). A permit of
/// another operation or drive, or one whose way is not the way the keyring and the bucket say NOW,
/// changes nothing.
///
/// # Errors
///
/// A permit that does not fit (`Denied`), and everything a step of the rotation says.
pub fn rotate(
    permit: &Permit,
    bucket: Arc<dyn Drive>,
    keyring: &dyn KeyringStore,
    drive: &str,
    provider: &dyn IndexProvider,
    kdf: RecoveryKdf,
) -> Result<Rotated, DriveError> {
    permit.allows(Op::Rotate, drive)?;
    let (previous_entry, next_entry) = device::rotation_key_entries(drive);
    // Only a journal this device wrote is resumed: any other (planted by whoever can write to
    // the bucket, or a damaged one) is replaced by the journal of the rotation that goes on, and
    // a new key of an earlier try is never replaced by a second one (K2 may seal the index).
    let started = start(bucket.as_ref(), keyring, drive)?;
    if way_of(&started) != permit.way() {
        return Err(changed_since_asked());
    }
    let (old, new, stashed, mut journal) = match started {
        Start::Fresh { old } => {
            let new = DriveKey::generate().map_err(|e| e.for_key(drive))?;
            device::store_key_at(keyring, &previous_entry, &old)?;
            // The keyring must give K2 back before anything is wrapped with it.
            let stashed = device::stash(keyring, &next_entry, &new)?;
            let mut journal = Journal::new(drive, &old, &new);
            save(bucket.as_ref(), drive, &new, &mut journal)?;
            (old, new, stashed, journal)
        }
        Start::Adopt { old, new } => {
            // The old key is the keyring's drive key (the new one is not before the Members step);
            // `drive-key-previous` says so again, K2 stays where it is.
            device::store_key_at(keyring, &previous_entry, &old)?;
            let (_, stashed) = device::held(keyring, &next_entry)?
                .filter(|(k, _)| *k == new)
                .ok_or_else(|| missing("new"))?;
            let mut journal = Journal::new(drive, &old, &new);
            save(bucket.as_ref(), drive, &new, &mut journal)?;
            (old, new, stashed, journal)
        }
        Start::Resume(journal) => {
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
    let older: Vec<DriveKey> = chain_of(bucket.as_ref())?.open(drive, &old);
    if journal.phase < Phase::Rekeyed {
        match provider.rekey(drive, Arc::clone(&bucket), &old, &new) {
            Ok(()) => {}
            Err(DriveError::Unsupported(why)) => {
                // Nothing changed yet: the drive stays as it was, K2 goes.
                // collapse-ok: the journal is inert without K2 (its MAC is keyed with it) and the
                // next rotation replaces it.
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
    let (mut members_removed, mut codes_removed) = (0, 0);
    if journal.phase < Phase::Members {
        // A new member key for this device, its wrap sealed to K2; K2 becomes the keyring's
        // drive key.
        device::delete_key_at(keyring, &device::member_key_entry(drive))?;
        let me = device::enroll(bucket.as_ref(), keyring, drive, &new)?;
        (members_removed, codes_removed) = remove_other_members(bucket.as_ref(), &new, &me)?;
        journal.phase = Phase::Members;
        save(bucket.as_ref(), drive, &new, &mut journal)?;
    }
    // Everything that can fail and be run again comes BEFORE the recovery wrap, so a failure
    // leaves the old wrap and its code as they were (K31): K1 in the chain once more (the bucket's
    // writers can delete the file, and the keyring's copy of K1 goes below), the drop key, the
    // shares.
    chain_old_key(bucket.as_ref(), drive, &old, &new)?;
    let drop_key = drops::rotate_drop_key(bucket.as_ref(), &old, &new, drive)?;
    let shares_revoked = revoke_all_shares(bucket.as_ref())?;
    // Again on every resume: the code returned last is the one the wrap holds. The last step that
    // can fail; from the wrap on the code is the drive's and must reach the user, so nothing below
    // returns an error.
    let recovery_code = RecoveryCode::generate().map_err(|e| e.for_key(drive))?;
    let wrap = RecoveryWrap::seal(&new, drive, &recovery_code, kdf).map_err(|e| e.for_key(drive))?;
    device::publish_recovery_wrap(
        bucket.as_ref(),
        &stashed,
        &wrap,
        device::Publish::FirstReplace,
    )?;
    // Done: the journal goes and K1 leaves the keyring (the key chain keeps it, sealed under
    // K2); K2 is the keyring's drive key (step 4). What stays is said, not raised: the keys are
    // changed. Both entries are tried even when the journal stays, for a journal that stays
    // beside a K2 that stays would read as a rotation under way and skip the next lockdown.
    let mut left_over = Vec::new();
    match bucket.delete(JOURNAL_FILE) {
        Ok(()) | Err(DriveError::NotFound { .. }) => {}
        Err(e) => left_over.push(format!("{JOURNAL_FILE}: {e}")),
    }
    for entry in [&previous_entry, &next_entry] {
        if let Err(e) = device::delete_key_at(keyring, entry) {
            left_over.push(format!("{entry}: {e}"));
        }
    }
    Ok(Rotated {
        recovery_code,
        drive_key: new.id(),
        rewrapped,
        members_removed,
        codes_removed,
        drop_key,
        shares_revoked,
        left_over,
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
    /// When the user confirmed that the old versions of every file are erased too (seconds since
    /// 1970); `None`: they are not, and the drive's history stays restorable until the retention is
    /// over. Re-encrypting every file writes new objects with new keys; the old ones - which the
    /// old drive key still opens - stay in a drive that keeps its history, as the history of the
    /// files. Only this confirmation erases them ([`ReencryptState::purge_confirmation`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purge_confirmed_at: Option<u64>,
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
            purge_confirmed_at: None,
        }
    }

    /// The user confirmed, at `at`, that the old versions of every file are erased with the
    /// re-encryption: for good, the history before it cannot be restored afterwards. Record it
    /// here, in the state the pass saves, so a restart does not lose the answer.
    pub fn confirm_purge(&mut self, at: u64) {
        self.purge_confirmed_at = Some(at);
    }

    /// The confirmation the user gave, if they gave one: what a purge
    /// ([`crate::meta::MetaIndexProvider::purge`]) takes once the pass is over.
    #[must_use]
    pub fn purge_confirmation(&self) -> Option<crate::meta::reclaim::PurgeConfirmation> {
        self.purge_confirmed_at
            .map(crate::meta::reclaim::PurgeConfirmation::confirmed_by_the_user)
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
/// before `state.before` is written into a new object with a new file key (its date kept). On a
/// drive whose index keeps no history the old object is deleted - afterwards nothing in the
/// bucket opens with the old drive key. On a drive that keeps its history (the repository) the
/// old objects are the history of the files: they stay, restorable until the retention is over,
/// and go at once only when the user confirmed it ([`ReencryptState::confirm_purge`]; the caller
/// then runs [`crate::meta::MetaIndexProvider::purge`] when this returns `Ok(true)`) - they used to
/// stay for ever, or (the small files' packs) to go without asking. Small files packed together
/// are written into new packs at the end, every pack holding one of them at once. `save` gets the
/// state after every file (and after the packs, when there were any), `stop` is asked before every
/// file; `Ok(true)` when every file is done. A file the user changed meanwhile keeps the user's
/// version (it is in a new object anyway).
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
