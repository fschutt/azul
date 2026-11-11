//! The keys on a device: the drive key and this device's member key in the OS keyring, and the
//! flows that put them there.
//!
//! Keyring entries (one per drive, next to the drive's session at
//! [`crate::config::keyring_key`]):
//! - [`drive_key_entry`] `azul-storage/drive-key/<drive>`: the drive key;
//! - [`member_key_entry`] `azul-storage/member-key/<drive>`: this device's X25519 member secret
//!   for the drive (one per drive, so two drives cannot be linked by a device's key), with the
//!   id of the drive key this device holds (its pin) or, before it held one, the id a claim
//!   code named;
//! - [`planned_key_entry`] `azul-storage/drive-key-planned/<drive>`: a drive key made before
//!   its drive existed, which [`setup_new_drive`] takes.
//!
//! The flows, each over the drive's bucket (the inner drive, below the encryption) and the
//! keyring:
//! - [`setup_new_drive`]: the first device of a drive - a new drive key, a new recovery code (for
//!   the recovery sheet; it is never stored), the recovery wrap and this device's member wrap
//!   in the bucket, the keys in the keyring.
//! - [`unlock`]: the drive key from the keyring, else from this device's member wrap in the
//!   bucket (opened with its member secret); `None` when this device has no wrap.
//!
//! Anyone in control of the storage can seal a drive key of their own to a member's public key
//! and put it in that member's key file. So a device keeps the id of the drive key it first
//! held - made, taken from a join code or opened with the recovery code - beside its member
//! key, and never takes a wrap of another key ([`DriveError::KeyRefused`]). A device from
//! before these pins takes its wrap only when the drive's index opens with it; on an empty
//! drive nothing says which key is the drive's, so it asks for the recovery code or a join
//! code instead. Join and claim codes carry the drive key's id too, so even a device's first
//! key is checked against something the server did not hand out.
//! - [`enroll`]: this device's member wrap, made from a drive key it holds.
//! - [`seal_invite`] / [`adopt_invite`]: a second device gets the drive key through its join
//!   code. The inviting device seals the drive key to a one-time X25519 key
//!   (`.azlin/keys/invite-<id>.key`) whose secret rides in the join code; the joining device
//!   opens it, enrols itself and deletes the invite wrap, so the code opens the key once.
//! - [`recover`]: the recovery code opens the recovery wrap; this device is enrolled.
//!
//! The record of the members is the drive index's policy (`.azlin/policy.toml` and
//! `.azlin/keys/` in the metadata repository, `crate::meta::policy`). A device that holds the
//! drive key reads the members from it ([`members`]), and [`enroll`] records a member there
//! before its key file goes into the bucket. The bucket's key files are the copy a device
//! without the drive key opens: [`unlock`], [`adopt_invite`] and [`recover`] start from
//! them, because the repository is sealed with the drive key. [`unlock`] then asks the
//! policy, so a device it no longer names stays out. A drive whose index has no policy yet
//! gets one at its first member change, from its key files. A bucket without an index keeps
//! its key files as the record. Invites and the recovery wrap are no members: they stay in
//! the bucket alone.
//!
//! A join code (like the recovery code) is a bearer secret: whoever holds it reads the drive.
//! It already grants the bucket; with the invite secret it grants the data too. Pass it by a
//! file or a QR code, never by a channel others read.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use super::{
    hex_array,
    keys::{
        load_member_wrap, load_member_wraps, load_recovery_wrap, member_key_file,
        store_member_wrap, store_recovery_wrap, MemberSecret, MemberWrap, RecoveryCode,
        RecoveryKdf, RecoveryWrap, INVITE_PREFIX, KEYS_PREFIX, RECOVERY_KEY_FILE,
    },
    keystate::{self, Guard, Keeps, KeyringFacts, Op, Read, Way},
    to_hex, CryptoError, DriveKey, KeyId, KEY_LEN,
};
use crate::{
    footprint::KeyEntry,
    keyring::{KeyringError, KeyringStore},
    meta::policy::{KeyCheck, MemberChange, MemberRecord},
    Drive, DriveError, ListRequest, Precondition,
};

/// The keyring entry of a drive's key.
#[must_use]
pub fn drive_key_entry(drive: &str) -> String {
    KeyEntry::DriveKey.name(drive)
}

/// The keyring entry of this device's member secret for a drive.
#[must_use]
pub fn member_key_entry(drive: &str) -> String {
    KeyEntry::MemberKey.name(drive)
}

/// The keyring entry of a drive key made before its drive existed: a checkout keeps one for
/// the drive it pays for, so the claim code printed before the drive exists names the key's id
/// ([`plan_drive_key`]); [`setup_new_drive`] takes it.
#[must_use]
pub fn planned_key_entry(drive: &str) -> String {
    KeyEntry::PlannedKey.name(drive)
}

/// The keyring entries a key rotation keeps both keys in until it is done
/// ([`crate::rotation`]): the drive key before it and the one after it.
#[must_use]
pub fn rotation_key_entries(drive: &str) -> (String, String) {
    (
        KeyEntry::RotationPrevious.name(drive),
        KeyEntry::RotationNext.name(drive),
    )
}

/// Keeps `key` under the keyring entry `entry` (a rotation's two keys).
pub(crate) fn store_key_at(
    keyring: &dyn KeyringStore,
    entry: &str,
    key: &DriveKey,
) -> Result<(), DriveError> {
    store_key(keyring, entry, KIND_DRIVE_KEY, key.as_bytes())
}

/// The drive key under the keyring entry `entry`; `None` when there is none.
pub(crate) fn load_key_at(
    keyring: &dyn KeyringStore,
    entry: &str,
) -> Result<Option<DriveKey>, DriveError> {
    Ok(load_key(keyring, entry, KIND_DRIVE_KEY)?.map(|bytes| DriveKey::from_bytes(*bytes)))
}

/// Removes the keyring entry `entry`.
pub(crate) fn delete_key_at(keyring: &dyn KeyringStore, entry: &str) -> Result<(), DriveError> {
    keyring.delete(entry).map_err(|e| keyring_error(entry, e))
}

/// Evidence that a drive key is in the keyring: stored under an entry and read back equal
/// ([`stash`]), or read from one ([`held`]). A recovery wrap seals a drive key; one published
/// before the keyring holds that key is a wrap whose key can be lost with the next failed call -
/// a paid drive with data no recovery code opens - so [`publish_recovery_wrap`] takes one of
/// these and nothing else makes one: stash, then publish, then drop.
#[derive(Debug)]
#[must_use]
pub struct Stashed {
    entry: String,
    key_id: KeyId,
}

impl Stashed {
    /// The id of the key that is stashed.
    #[must_use]
    pub fn key_id(&self) -> KeyId {
        self.key_id
    }

    /// The keyring entry it is in.
    #[must_use]
    pub fn entry(&self) -> &str {
        &self.entry
    }
}

/// Keeps `key` under the keyring entry `entry` and reads it back: the evidence that it is kept.
///
/// # Errors
///
/// The keyring does not take the key, or does not give it back as it was.
pub fn stash(
    keyring: &dyn KeyringStore,
    entry: &str,
    key: &DriveKey,
) -> Result<Stashed, DriveError> {
    store_key_at(keyring, entry, key)?;
    match load_key_at(keyring, entry)? {
        Some(read) if &read == key => Ok(Stashed {
            entry: entry.to_string(),
            key_id: key.id(),
        }),
        _ => Err(DriveError::Io(String::from(
            "the keyring did not keep the new drive key",
        ))),
    }
}

/// The drive key the keyring holds under `entry`, with the evidence that it does; `None` when
/// there is none.
///
/// # Errors
///
/// The keyring cannot be read, or holds something that is no key.
pub fn held(
    keyring: &dyn KeyringStore,
    entry: &str,
) -> Result<Option<(DriveKey, Stashed)>, DriveError> {
    Ok(load_key_at(keyring, entry)?.map(|key| {
        let stashed = Stashed {
            entry: entry.to_string(),
            key_id: key.id(),
        };
        (key, stashed)
    }))
}

/// Where a recovery wrap goes in the bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Publish<'f> {
    /// The drive's first recovery wrap, which must be new: a second device that set the same
    /// drive up first wins ([`DriveError::Conflict`]).
    FirstNew,
    /// The first recovery wrap again: the earlier try of this device's setup (its code was
    /// never shown), a rotation's new one, a new code's.
    FirstReplace,
    /// A further recovery code's file ([`add_recovery_code`]), which must be new.
    Extra(&'f str),
}

/// Puts a recovery wrap into the bucket, for a drive key the keyring holds (`stashed`): the wrap
/// must seal exactly that key. The only way a wrap leaves this device.
///
/// A wrap published without a stash cannot be written:
///
/// ```compile_fail,E0061
/// use azul_storage::crypto::device::{publish_recovery_wrap, Publish};
/// use azul_storage::crypto::keys::RecoveryWrap;
/// use azul_storage::Drive;
///
/// fn publish(bucket: &dyn Drive, wrap: &RecoveryWrap) {
///     // no `Stashed`: the key this wrap seals was not seen in the keyring
///     let _ = publish_recovery_wrap(bucket, wrap, Publish::FirstNew);
/// }
/// ```
///
/// # Errors
///
/// A wrap of another key than the stashed one (`Io`: nothing was written), the bucket's refusal,
/// [`DriveError::Conflict`] for a wrap that had to be new and is not.
pub fn publish_recovery_wrap(
    bucket: &dyn Drive,
    stashed: &Stashed,
    wrap: &RecoveryWrap,
    how: Publish<'_>,
) -> Result<(), DriveError> {
    if wrap.drive_key_id != stashed.key_id {
        return Err(DriveError::Io(format!(
            "the recovery wrap seals the drive key {}, and the key the keyring holds is {}",
            wrap.drive_key_id, stashed.key_id
        )));
    }
    match how {
        Publish::FirstNew => put_new(bucket, RECOVERY_KEY_FILE, &wrap.to_bytes()),
        Publish::FirstReplace => store_recovery_wrap(bucket, wrap),
        Publish::Extra(file) => put_new(bucket, file, &wrap.to_bytes()),
    }
}

/// A key as the keyring keeps it (JSON): what it is and its bytes in hex. The hex text is wiped
/// when the value is dropped (AZ-C7): the string that held the key must not outlive it in freed
/// heap.
#[derive(Serialize, Deserialize)]
struct StoredKey {
    kind: String,
    key: String,
    /// A member key's pin: the id of the drive key this device holds for the drive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    drive_key_id: Option<String>,
    /// A member key's commitment: the id of the drive key a claim code named, while this
    /// device has not held the key yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    committed: Option<String>,
}

impl Drop for StoredKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// A key entry read: the key's bytes, and a member key's pin and commitment.
struct Entry {
    bytes: Zeroizing<[u8; KEY_LEN]>,
    drive_key_id: Option<KeyId>,
    committed: Option<KeyId>,
}

/// This device's member key for a drive and what it knows of the drive's key: the id of the
/// key it holds (its pin), else the id a claim code named.
struct Member {
    secret: MemberSecret,
    pin: Option<KeyId>,
    committed: Option<KeyId>,
}

impl Member {
    /// The id a drive key must have for this device to take it from a wrap.
    fn expected(&self) -> Option<KeyId> {
        self.pin.or(self.committed)
    }
}

const KIND_DRIVE_KEY: &str = "drive-key";
const KIND_MEMBER_KEY: &str = "member-key";

/// The member id an invite's one-time key has in the bucket.
fn invite_member(invite: &MemberSecret) -> String {
    format!("{INVITE_PREFIX}{}", invite.public().id())
}

/// A keyring's refusal as a drive's error (never the secret).
fn keyring_error(entry: &str, e: KeyringError) -> DriveError {
    match e {
        KeyringError::Denied => DriveError::Denied {
            message: format!("the keyring refused {entry}"),
        },
        KeyringError::Unavailable => DriveError::Unsupported(String::from(
            "keeping the drive's key needs a keyring, and this system has none",
        )),
        KeyringError::Failed(why) => DriveError::Io(format!("the keyring ({entry}): {why}")),
    }
}

/// Whether `keyring` takes a drive's key: the refusal a key written to it would get (AZ-K7: a plain
/// file takes none unless the user said it may), asked BEFORE anything that cannot be taken back
/// is done at a server - a join spends the code's one-time token, a lockdown locks the other
/// devices out, a sign-up makes the drive - so that the refusal at the first key written leaves
/// nothing half done (X-DELTA-AZDRIVE-KEYS-25). `what` names the key in the sentence.
///
/// # Errors
///
/// [`DriveError::InvalidConfig`] with the keyring's reason.
pub fn check_keeps_keys(keyring: &dyn KeyringStore, what: &str) -> Result<(), DriveError> {
    let protection = keyring.protection();
    if protection.keeps_key_material() {
        return Ok(());
    }
    Err(DriveError::InvalidConfig(format!(
        "{what} is not kept: {}",
        protection.refusal()
    )))
}

/// What the keyring says of the drive's keys, as the key permit's evidence ([`keystate`], C7): the
/// ids under `drive-key`, `drive-key-next` and `drive-key-planned`, each a read of its own - one that
/// fails is [`Read::Failed`], never an entry that is not there. Three local reads, nothing written.
/// The keyring's protection is not asked ([`keeps_keys_fact`]): only a guard that consults it says so.
#[must_use]
pub fn key_facts(keyring: &dyn KeyringStore, drive: &str) -> KeyringFacts {
    let (_, next_entry) = rotation_key_entries(drive);
    KeyringFacts {
        drive_key: keystate::key_id(&load_drive_key(keyring, drive)),
        next_key: keystate::key_id(&load_key_at(keyring, &next_entry)),
        planned_key: keystate::key_id(&load_key_at(keyring, &planned_key_entry(drive))),
        keeps_keys: Read::Unasked,
    }
}

/// Whether `keyring` takes key material, as the key permit's evidence: what [`check_keeps_keys`]
/// decides.
#[must_use]
pub fn keeps_keys_fact(keyring: &dyn KeyringStore) -> Read<Keeps> {
    let protection = keyring.protection();
    Read::Found(if protection.keeps_key_material() {
        Keeps::Yes
    } else {
        Keeps::No(protection.refusal())
    })
}

fn store_key(
    keyring: &dyn KeyringStore,
    entry: &str,
    kind: &str,
    bytes: &[u8; KEY_LEN],
) -> Result<(), DriveError> {
    store_entry(keyring, entry, kind, bytes, None, None)
}

fn store_entry(
    keyring: &dyn KeyringStore,
    entry: &str,
    kind: &str,
    bytes: &[u8; KEY_LEN],
    drive_key_id: Option<&KeyId>,
    committed: Option<&KeyId>,
) -> Result<(), DriveError> {
    // Key material goes into a keyring that keeps it from the other programs of this user, or
    // into a plain file the user said may hold it (AZ-K7): the one place a key is written.
    check_keeps_keys(keyring, &format!("the key {entry}"))?;
    // A key that cannot be written down is an error, never an empty entry in its place; the
    // serializer's message is not passed on (it could quote the key).
    let text = Zeroizing::new(
        serde_json::to_string(&StoredKey {
            kind: kind.to_string(),
            key: to_hex(bytes),
            drive_key_id: drive_key_id.map(KeyId::to_hex),
            committed: committed.map(KeyId::to_hex),
        })
        .map_err(|_| DriveError::Io(format!("the key {entry} could not be written down")))?,
    );
    keyring
        .set(entry, &text)
        .map_err(|e| keyring_error(entry, e))
}

/// The key bytes of the keyring entry `entry` when it holds a key of `kind`.
fn load_key(
    keyring: &dyn KeyringStore,
    entry: &str,
    kind: &str,
) -> Result<Option<Zeroizing<[u8; KEY_LEN]>>, DriveError> {
    Ok(load_entry(keyring, entry, kind)?.map(|stored| stored.bytes))
}

/// The keyring entry `entry` when it holds a key of `kind`.
fn load_entry(
    keyring: &dyn KeyringStore,
    entry: &str,
    kind: &str,
) -> Result<Option<Entry>, DriveError> {
    let Some(text) = keyring.get(entry).map_err(|e| keyring_error(entry, e))? else {
        return Ok(None);
    };
    let text = Zeroizing::new(text);
    // The parser's message is not passed on: it could quote the key.
    let mut stored: StoredKey = serde_json::from_str(&text).map_err(|_| {
        DriveError::InvalidConfig(format!("the keyring entry {entry} holds no key"))
    })?;
    // The hex text moves into a wiped string (the stored value's own is left empty).
    let hex = Zeroizing::new(std::mem::take(&mut stored.key));
    if stored.kind != kind {
        return Err(DriveError::InvalidConfig(format!(
            "the keyring entry {entry} holds a {} where a {kind} belongs",
            stored.kind
        )));
    }
    let bytes = hex_array::<KEY_LEN>(&hex).ok_or_else(|| {
        DriveError::InvalidConfig(format!("the keyring entry {entry} holds a damaged key"))
    })?;
    // A damaged id is refused, never read as no pin: that would trust the next wrap.
    let id = |text: Option<String>| -> Result<Option<KeyId>, DriveError> {
        text.map(|text| {
            KeyId::from_hex(&text).ok_or_else(|| {
                DriveError::InvalidConfig(format!(
                    "the keyring entry {entry} holds a damaged drive key id"
                ))
            })
        })
        .transpose()
    };
    Ok(Some(Entry {
        bytes: Zeroizing::new(bytes),
        drive_key_id: id(std::mem::take(&mut stored.drive_key_id))?,
        committed: id(std::mem::take(&mut stored.committed))?,
    }))
}

fn load_member(keyring: &dyn KeyringStore, drive: &str) -> Result<Option<Member>, DriveError> {
    Ok(
        load_entry(keyring, &member_key_entry(drive), KIND_MEMBER_KEY)?.map(|stored| Member {
            secret: MemberSecret::from_bytes(*stored.bytes),
            pin: stored.drive_key_id,
            committed: stored.committed,
        }),
    )
}

fn store_member(keyring: &dyn KeyringStore, drive: &str, member: &Member) -> Result<(), DriveError> {
    store_entry(
        keyring,
        &member_key_entry(drive),
        KIND_MEMBER_KEY,
        &member.secret.to_bytes(),
        member.pin.as_ref(),
        member.committed.as_ref(),
    )
}

/// The refusal of a drive key this device does not take.
fn refused(drive: &str, reason: String) -> DriveError {
    DriveError::KeyRefused {
        drive: drive.to_string(),
        reason,
    }
}

/// The id of the drive key this device holds for the drive (its pin); `None` before it held
/// one, and on a device from before pins.
pub fn pinned_drive_key(keyring: &dyn KeyringStore, drive: &str) -> Result<Option<KeyId>, DriveError> {
    Ok(load_member(keyring, drive)?.and_then(|member| member.pin))
}

/// The id of the drive key a claim code named for the drive, while this device has not held
/// the key yet.
pub fn committed_drive_key(
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Option<KeyId>, DriveError> {
    Ok(load_member(keyring, drive)?.and_then(|member| member.committed))
}

/// Pins the drive key `id` on this device: from now on it takes no wrap of another key for the
/// drive. The member key is made when there is none yet.
fn pin(keyring: &dyn KeyringStore, drive: &str, id: &KeyId) -> Result<(), DriveError> {
    changing(keyring, &member_key_entry(drive), || {
        member_secret_held(keyring, drive)?;
        let Some(mut member) = load_member(keyring, drive)? else {
            return Err(DriveError::Io(format!(
                "the keyring did not keep the member key of {drive}"
            )));
        };
        if member.pin == Some(*id) && member.committed.is_none() {
            return Ok(());
        }
        member.pin = Some(*id);
        member.committed = None;
        store_member(keyring, drive, &member)
    })
}

/// A claim code names the id of the drive's key before this device holds it: every key the
/// device gets for the drive from then on - from its wrap, a join code or the recovery code -
/// must have that id (a join code that names another one is the newer word). A device that
/// holds the drive's key already keeps its pin.
pub fn commit_drive_key(keyring: &dyn KeyringStore, drive: &str, id: &KeyId) -> Result<(), DriveError> {
    changing(keyring, &member_key_entry(drive), || {
        member_secret_held(keyring, drive)?;
        let Some(mut member) = load_member(keyring, drive)? else {
            return Err(DriveError::Io(format!(
                "the keyring did not keep the member key of {drive}"
            )));
        };
        if member.pin.is_some() || member.committed == Some(*id) {
            return Ok(());
        }
        member.committed = Some(*id);
        store_member(keyring, drive, &member)
    })
}

/// Keeps `key` as the drive key the drive `drive` is set up with ([`setup_new_drive`]): a key
/// made before its drive existed, whose id a claim code names already.
pub fn plan_drive_key(keyring: &dyn KeyringStore, drive: &str, key: &DriveKey) -> Result<(), DriveError> {
    store_key(keyring, &planned_key_entry(drive), KIND_DRIVE_KEY, key.as_bytes())
}

/// Keeps `key` in the keyring as the drive's key.
pub fn store_drive_key(
    keyring: &dyn KeyringStore,
    drive: &str,
    key: &DriveKey,
) -> Result<(), DriveError> {
    store_key(keyring, &drive_key_entry(drive), KIND_DRIVE_KEY, key.as_bytes())
}

/// The drive's key from the keyring; `None` when this device keeps none.
pub fn load_drive_key(
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Option<DriveKey>, DriveError> {
    Ok(load_key(keyring, &drive_key_entry(drive), KIND_DRIVE_KEY)?
        .map(|bytes| DriveKey::from_bytes(*bytes)))
}

/// This device's member secret for the drive, from the keyring; `None` when it has none.
pub fn load_member_secret(
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Option<MemberSecret>, DriveError> {
    Ok(load_member(keyring, drive)?.map(|member| member.secret))
}

/// Runs `change` - a read, a decision and a write of the keyring entry `entry` - holding the
/// entry ([`KeyringStore::hold`]): two windows or two apps first-enrolling at once made two member
/// secrets, and the loser's wrap was sealed to a key no keyring holds (AZ-K19).
fn changing<T>(
    keyring: &dyn KeyringStore,
    entry: &str,
    change: impl FnOnce() -> Result<T, DriveError>,
) -> Result<T, DriveError> {
    let _held = keyring.hold(entry).map_err(|e| keyring_error(entry, e))?;
    change()
}

/// This device's member secret for the drive: the keyring's, else a new one, kept there
/// before it is used. The entry is held while it is made, so two enrolments at once share it.
pub fn member_secret(keyring: &dyn KeyringStore, drive: &str) -> Result<MemberSecret, DriveError> {
    changing(keyring, &member_key_entry(drive), || {
        member_secret_held(keyring, drive)
    })
}

/// [`member_secret`] by a caller that holds the member key's entry already.
fn member_secret_held(keyring: &dyn KeyringStore, drive: &str) -> Result<MemberSecret, DriveError> {
    if let Some(secret) = load_member_secret(keyring, drive)? {
        return Ok(secret);
    }
    let secret = MemberSecret::generate().map_err(|e| e.for_key(drive))?;
    store_key(
        keyring,
        &member_key_entry(drive),
        KIND_MEMBER_KEY,
        &secret.to_bytes(),
    )?;
    Ok(secret)
}

/// Every keyring entry a drive has on this device ([`KeyEntry::ALL`]): its session and drive
/// token, the drive key, this device's member key (with its pin), a key made before the drive's
/// setup finished, the two keys of a rotation under way, and the recovery request's key. A removed
/// drive leaves none of them behind ([`forget_keys`], and `footprint::forget_drive` for the rest
/// of its footprint).
#[must_use]
pub fn key_entries(drive: &str) -> Vec<String> {
    KeyEntry::ALL
        .iter()
        .map(|entry| entry.name(drive))
        .collect()
}

/// Removes the drive's keys from this device ([`key_entries`]; the device signs out). Every entry
/// is tried; the first refusal is the answer. A drive taken off a device goes through
/// `footprint::forget_drive`, which asks for a `RemovalProof` before it deletes a key.
pub fn forget_keys(keyring: &dyn KeyringStore, drive: &str) -> Result<(), DriveError> {
    let mut first = None;
    for entry in key_entries(drive) {
        if let Err(e) = keyring.delete(&entry) {
            first.get_or_insert(keyring_error(&entry, e));
        }
    }
    first.map_or(Ok(()), Err)
}

/// The drive's members (their ids, sorted) as a device that holds the drive key reads them:
/// the ones its index's policy names. A drive whose index has no policy yet, and a bucket
/// without an index, have their member key files as the record.
pub fn members(bucket: &dyn Drive, drive_key: &DriveKey) -> Result<Vec<String>, DriveError> {
    if let Some(mut record) = MemberRecord::open(bucket, drive_key)? {
        if let Some(members) = record.members()? {
            return Ok(members.policy.members.into_keys().collect());
        }
    }
    Ok(load_member_wraps(bucket)?.into_keys().collect())
}

/// Records `changes` of the drive's members in its index's policy, when the bucket holds an
/// index. A drive without a policy gets one from its key files in the same commit. A bucket
/// without an index keeps its key files as the record, so there is nothing to do.
fn record_members(
    bucket: &dyn Drive,
    drive_key: &DriveKey,
    changes: &[MemberChange],
) -> Result<(), DriveError> {
    match MemberRecord::open(bucket, drive_key)? {
        Some(mut record) => record.record(changes).map(drop),
        None => Ok(()),
    }
}

/// Whether the drive's record names `member`: its index's policy. A drive whose index has
/// no policy, and a bucket without an index, name every member with a key file.
fn is_member(bucket: &dyn Drive, drive_key: &DriveKey, member: &str) -> Result<bool, DriveError> {
    let Some(mut record) = MemberRecord::open(bucket, drive_key)? else {
        return Ok(true);
    };
    Ok(record
        .members()?
        .map_or(true, |members| members.contains(member)))
}

/// Whether the bucket holds an encrypted drive: key files under `.azlin/keys/`.
pub fn is_encrypted(bucket: &dyn Drive) -> Result<bool, DriveError> {
    let page = bucket.list(&ListRequest::recursive(KEYS_PREFIX).with_max_keys(1))?;
    Ok(!page.objects.is_empty())
}

/// Puts a key file that must be new: conditionally where the bucket can (two devices setting a
/// drive up at once: the second one learns it lost).
fn put_new(bucket: &dyn Drive, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
    match bucket.put_if(key, bytes, &Precondition::Absent) {
        Ok(_) => Ok(()),
        Err(DriveError::Unsupported(_)) => bucket.put(key, bytes),
        Err(e) => Err(e),
    }
}

/// The first device of a drive: a new drive key (the one planned for it, [`plan_drive_key`],
/// when there is one) and recovery code. The recovery wrap (derived with `kdf`) and this
/// device's member wrap go into the bucket, the drive key and the member secret into the
/// keyring. Returns the drive key, and the recovery code for the recovery
/// sheet: it is stored nowhere, so this is the one moment it can be shown. Refused for a bucket
/// that holds key files already (it is encrypted: [`unlock`], [`adopt_invite`] or
/// [`recover`] instead).
///
/// The order keeps a paid drive from being stranded: the drive key is kept in the keyring
/// (as the planned key) before ANYTHING goes into the bucket - the recovery wrap is published
/// only once this device holds the key it seals -, and the planned key stays until the device is
/// enrolled. A setup that stopped in between (no network, a denied keyring prompt, a kill) is run
/// again with the same call: the planned key is found, the recovery wrap of the earlier try (its
/// code was never shown) is replaced by a new one, and the device is enrolled. A bucket whose
/// recovery wrap holds another key is another device's drive (`KeyRefused`).
///
/// The last step that can fail is [`enroll`]: from the drive key in the keyring on, the recovery
/// wrap in the bucket is sealed to it and the code must reach the user, so nothing after it returns
/// an error. What the planted key's removal could not do is [`NewDrive::left_over`] of
/// [`setup_new_drive_reporting`], which this function leaves out; an app that shows the sheet
/// calls that one (X-DELTA-KEYS2-3).
pub fn setup_new_drive(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    kdf: RecoveryKdf,
) -> Result<(DriveKey, RecoveryCode), DriveError> {
    let made = setup_new_drive_reporting(bucket, keyring, drive, kdf)?;
    Ok((made.drive_key, made.code))
}

/// What [`setup_new_drive_reporting`] made.
pub struct NewDrive {
    /// The drive key, which this device holds.
    pub drive_key: DriveKey,
    /// The recovery code for the recovery sheet: stored nowhere.
    pub code: RecoveryCode,
    /// What the last step could not take away, each a sentence naming the keyring entry (never a
    /// key): the drive is set up and the code is its recovery code, so this is a leftover to clear
    /// and no failure - the planted key stays beside the drive key it is a copy of. Empty: nothing
    /// is left.
    pub left_over: Vec<String>,
}

impl std::fmt::Debug for NewDrive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewDrive")
            .field("drive_key", &self.drive_key.id())
            .field("left_over", &self.left_over)
            .finish_non_exhaustive()
    }
}

/// [`setup_new_drive`] that says what its last step left behind ([`NewDrive::left_over`]) instead
/// of dropping it.
pub fn setup_new_drive_reporting(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    kdf: RecoveryKdf,
) -> Result<NewDrive, DriveError> {
    let planned_entry = planned_key_entry(drive);
    // A key kept before this call: a checkout's, or the one an earlier try of this setup made.
    let planned = load_key_at(keyring, &planned_entry)?;
    let encrypted_already = || {
        DriveError::InvalidConfig(format!(
            "\"{drive}\" is encrypted already: open it with this device's key, an invite or the \
             recovery code"
        ))
    };
    let encrypted = is_encrypted(bucket)?;
    // The key permit's shadow (C7 step 2) beside this flow's guards: a bucket that lists key files
    // for a device with no planned key, and a keyring that keeps no key (the first key written
    // refuses). The guards decide; the shadow changes nothing.
    {
        let mut evidence = keystate::Evidence::new(drive, crate::time::now_unix());
        evidence.keyring = key_facts(keyring, drive);
        evidence.keyring.keeps_keys = keeps_keys_fact(keyring);
        evidence.bucket.encrypted = Read::Found(encrypted);
        let guard =
            if (encrypted && planned.is_none()) || check_keeps_keys(keyring, "the key").is_err() {
                Guard::Refuses
            } else if planned.is_some() {
                Guard::PermitsAs(Way::Finish)
            } else {
                Guard::PermitsAs(Way::New)
            };
        crate::key_shadow!(
            "device::setup_new_drive",
            Op::Setup,
            evidence,
            guard,
            known = [
                (
                    "active",
                    "X-DELTA-AZDRIVE-KEYS-19: this flow never looks at the drive key in the keyring - \
                     with the key held and a bucket that lists no key file it makes a second key and \
                     enroll keeps it in the key's place; the table refuses Setup on an active drive"
                ),
                (
                    "unknown",
                    "the flow reads only the planned key: a keyring that fails on the drive key or the \
                     new key alone does not stop it (the table refuses on any failed read)"
                ),
            ]
        );
    }
    if encrypted && planned.is_none() {
        return Err(encrypted_already());
    }
    let generated = planned.is_none();
    let (drive_key, stashed) = match planned {
        // Read from the keyring just now: held.
        Some(key) => {
            let stashed = Stashed {
                entry: planned_entry.clone(),
                key_id: key.id(),
            };
            (key, stashed)
        }
        None => {
            let key = DriveKey::generate().map_err(|e| e.for_key(drive))?;
            // Kept, and read back, before the bucket sees anything of it.
            let stashed = stash(keyring, &planned_entry, &key)?;
            (key, stashed)
        }
    };
    // The key files in the bucket must be the ones of this very key: an earlier try of this
    // device's setup (its recovery wrap), nothing else.
    let own_recovery = if encrypted {
        match load_recovery_wrap(bucket) {
            Ok(wrap) if wrap.drive_key_id == drive_key.id() => true,
            Ok(wrap) => {
                return Err(refused(
                    drive,
                    format!(
                        "the drive's recovery key file holds the drive key {}, and the key this \
                         device kept for it is {}",
                        wrap.drive_key_id,
                        drive_key.id()
                    ),
                ))
            }
            Err(DriveError::NotFound { .. }) => return Err(encrypted_already()),
            Err(e) => return Err(e),
        }
    } else {
        false
    };
    let code = RecoveryCode::generate().map_err(|e| e.for_key(drive))?;
    let recovery =
        RecoveryWrap::seal(&drive_key, drive, &code, kdf).map_err(|e| e.for_key(drive))?;
    // The recovery wrap before anything else is sealed to this key: a drive must never hold data
    // no recovery code opens.
    if own_recovery {
        // The earlier try's code was never shown: this one replaces it.
        publish_recovery_wrap(bucket, &stashed, &recovery, Publish::FirstReplace)?;
    } else if let Err(e) = publish_recovery_wrap(bucket, &stashed, &recovery, Publish::FirstNew) {
        // The answer of a PUT is not what the bucket holds: a write whose answer got lost lands,
        // and its resend is refused as "the object is there" (a conditional write is sent again
        // blind). So the bucket is looked at before the key made now goes (K3).
        match load_recovery_wrap(bucket) {
            // Our wrap is there: the PUT landed. It is put again the way an earlier try's wrap
            // is replaced (the same bytes, a plain write), and the setup goes on.
            Ok(wrap) if wrap.drive_key_id == drive_key.id() => {
                publish_recovery_wrap(bucket, &stashed, &recovery, Publish::FirstReplace)?;
            }
            // Another device was first with a wrap of another key: the key made now is for
            // nothing and goes again.
            Ok(_) => {
                if generated {
                    delete_key_at(keyring, &planned_entry)?;
                }
                return Err(DriveError::Conflict {
                    key: String::from(RECOVERY_KEY_FILE),
                });
            }
            // None there, or the bucket cannot be read: whether the key seals anything is not
            // known, so it stays (the planned key is found by the next call, which finishes the
            // setup) and the PUT's own failure is the answer.
            Err(_) => return Err(e),
        }
    }
    enroll(bucket, keyring, drive, &drive_key)?;
    // The drive key is the keyring's and the recovery wrap is in the bucket: the code is the
    // drive's now and must reach the user, so nothing from here on returns an error. The planted
    // key is a copy of the drive key; a keyring that refuses to delete it (a prompt answered No)
    // leaves a copy of what it keeps anyway, and is said (X-DELTA-KEYS2-3: the `?` here ended the
    // setup with an error after the wrap and the key existed, and the code was dropped).
    let mut left_over = Vec::new();
    if let Err(e) = delete_key_at(keyring, &planned_entry) {
        left_over.push(format!("{planned_entry}: {e}"));
    }
    Ok(NewDrive {
        drive_key,
        code,
        left_over,
    })
}

/// Whether this device has a setup of the drive under way: [`setup_new_drive`] kept the drive's key
/// (or a checkout planned it) and has not finished enrolling the device - a stop after the
/// recovery wrap went into the bucket, say. The same call, run again, finishes it.
pub fn setup_pending(keyring: &dyn KeyringStore, drive: &str) -> Result<bool, DriveError> {
    Ok(load_key_at(keyring, &planned_key_entry(drive))?.is_some())
}

/// Seals `drive_key` to this device's member key (made and kept in the keyring when there is
/// none yet), records this device as a member with that wrap in the index's policy (when the
/// bucket holds an index), puts the wrap into the bucket and keeps the drive key in the
/// keyring. This device holds the key from now on: it is pinned first, so whatever stops the
/// rest, no wrap of another key is taken afterwards. Returns this device's member id.
pub fn enroll(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    drive_key: &DriveKey,
) -> Result<String, DriveError> {
    let secret = member_secret(keyring, drive)?;
    pin(keyring, drive, &drive_key.id())?;
    let member = secret.public().id();
    let wrap = MemberWrap::seal(drive_key, drive, &member, &secret.public())
        .map_err(|e| e.for_key(drive))?;
    // The record first (the index's policy), then the key file this device opens while it
    // has no drive key.
    let change = MemberChange::Add {
        member: member.clone(),
        wrap: wrap.to_bytes(),
    };
    record_members(bucket, drive_key, &[change])?;
    store_member_wrap(bucket, &wrap)?;
    store_drive_key(keyring, drive, drive_key)?;
    Ok(member)
}

/// The drive key on this device: the keyring's, else the one this device's member wrap in the
/// bucket holds (opened with its member secret, then kept in the keyring). `None` when this
/// device has neither, when the index's policy no longer names it, and when nothing tells
/// whether its wrap holds the drive's key: it needs an invite ([`adopt_invite`]) or the
/// recovery code ([`recover`]). `KeyRefused` for a wrap of another key than the one this
/// device holds.
pub fn unlock(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Option<DriveKey>, DriveError> {
    if let Some(key) = load_drive_key(keyring, drive)? {
        pin_kept_key(keyring, drive, &key)?;
        return Ok(Some(key));
    }
    let Some(drive_key) = key_from_wrap(bucket, keyring, drive)? else {
        return Ok(None);
    };
    store_drive_key(keyring, drive, &drive_key)?;
    Ok(Some(drive_key))
}

/// A device from before pins keeps the drive key it holds as its pin: the keyring's key is its
/// own, not the bucket's word.
fn pin_kept_key(keyring: &dyn KeyringStore, drive: &str, key: &DriveKey) -> Result<(), DriveError> {
    match load_member(keyring, drive)? {
        Some(member) if member.pin.is_none() => pin(keyring, drive, &key.id()),
        _ => Ok(()),
    }
}

/// The drive key this device's member wrap in the bucket holds, opened with its member
/// secret. `None` without a member secret or a wrap, and when the index's policy no longer
/// names this device: the policy decides, whatever key file the bucket still holds.
///
/// The wrap must hold the key this device pinned (or the one its claim code named): the
/// storage can seal any key to a member's public key. A device without either takes the wrap
/// only when the drive's index opens with its key, and pins it then; on a drive without an
/// index nothing tells, so it gets `None` and asks for a code.
fn key_from_wrap(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Option<DriveKey>, DriveError> {
    let Some(me) = load_member(keyring, drive)? else {
        return Ok(None);
    };
    let member = me.secret.public().id();
    let wrap = match load_member_wrap(bucket, &member) {
        Ok(wrap) => wrap,
        Err(DriveError::NotFound { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    let file = member_key_file(&member).map_err(|e| e.for_key(drive))?;
    if let Some(expected) = me.expected() {
        if wrap.drive_key_id != expected {
            return Err(refused(
                drive,
                format!(
                    "{file} holds the drive key {}, and this device's is {expected}",
                    wrap.drive_key_id
                ),
            ));
        }
    }
    // The wrap names its key's id, and opening it checks the key against that id.
    let drive_key = wrap.open(drive, &me.secret).map_err(|e| e.for_key(&file))?;
    if me.expected().is_some() {
        if !is_member(bucket, &drive_key, &member)? {
            return Ok(None);
        }
        if me.pin.is_none() {
            pin(keyring, drive, &drive_key.id())?;
        }
        return Ok(Some(drive_key));
    }
    match MemberRecord::check(bucket, &drive_key)? {
        KeyCheck::NoIndex => Ok(None),
        KeyCheck::Refuses => Err(refused(
            drive,
            format!(
                "the drive's index does not open with the drive key {} that {file} holds",
                wrap.drive_key_id
            ),
        )),
        KeyCheck::Opens(mut record) => {
            let named = record
                .members()?
                .map_or(true, |members| members.contains(&member));
            if !named {
                return Ok(None);
            }
            pin(keyring, drive, &drive_key.id())?;
            Ok(Some(drive_key))
        }
    }
}

/// A device whose keyring kept another drive key than the one it pinned catches up: a key
/// change of its own stopped between the pin and the keyring (a rotation on this device).
/// When its member wrap in the bucket holds the pinned key, that key becomes the keyring's
/// drive key. Returns the new key and the one before it, for the rotation's window
/// ([`crate::encrypted::IndexProvider::open_index_in_window`]). `None` when the wrap holds
/// the keyring's key, or when there is no wrap: a rotation that removed this device leaves
/// it nothing to catch up from (it needs an invite or the recovery code). A wrap of any other
/// key is refused (`KeyRefused`): another device's rotation brings its new key with a join
/// code, never through the bucket alone.
pub fn catch_up(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Option<(DriveKey, DriveKey)>, DriveError> {
    let Some(previous) = load_drive_key(keyring, drive)? else {
        return Ok(None);
    };
    let Some(current) = key_from_wrap(bucket, keyring, drive)? else {
        return Ok(None);
    };
    if current == previous {
        return Ok(None);
    }
    store_drive_key(keyring, drive, &current)?;
    Ok(Some((current, previous)))
}

/// Seals `drive_key` to a new one-time key for a join code: the wrap goes into the bucket as
/// `.azlin/keys/invite-<id>.key`, the one-time secret comes back for the code
/// ([`invite_text`]).
pub fn seal_invite(
    bucket: &dyn Drive,
    drive: &str,
    drive_key: &DriveKey,
) -> Result<MemberSecret, DriveError> {
    let invite = MemberSecret::generate().map_err(|e| e.for_key(drive))?;
    let wrap = MemberWrap::seal(drive_key, drive, &invite_member(&invite), &invite.public())
        .map_err(|e| e.for_key(drive))?;
    store_member_wrap(bucket, &wrap)?;
    Ok(invite)
}

/// The text of an invite's one-time secret, for a join code (64 hex digits). A secret.
#[must_use]
pub fn invite_text(invite: &MemberSecret) -> Zeroizing<String> {
    Zeroizing::new(to_hex(&invite.to_bytes()[..]))
}

/// The invite secret of [`invite_text`]'s text.
#[must_use]
pub fn invite_from_text(text: &str) -> Option<MemberSecret> {
    hex_array::<KEY_LEN>(text.trim())
        .map(Zeroizing::new)
        .map(|bytes| MemberSecret::from_bytes(*bytes))
}

/// What [`adopt_invite_reporting`] did.
#[derive(Debug)]
#[must_use]
pub struct Adopted {
    /// The drive key; this device is enrolled with it and holds it.
    pub key: DriveKey,
    /// `Some`: the invite's wrap could not be deleted, so the join code still opens the key until
    /// it is withdrawn ([`withdraw_invite`] again, or a rotation). The join itself is done - a
    /// device that has the key is not told it has not.
    pub invite_left: Option<DriveError>,
}

/// A joining device takes the drive key its join code's invite opens, enrols itself and
/// deletes the invite wrap (the code opens the key once). `key_id` is the id of the drive key
/// the join code names (a code from before has none): a seal of another key is refused
/// (`KeyRefused`), and so is one of another key than a claim code named when the join code
/// names none. `Denied` when the invite was used already or withdrawn.
///
/// The invite is deleted AFTER the device has the key (make before break); a delete that fails
/// is not the join's failure - the key is held and kept - but it leaves the code live, and
/// [`Adopted::invite_left`] says so. This form drops that word; the apps ask for
/// [`adopt_invite_reporting`].
pub fn adopt_invite(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    invite: &MemberSecret,
    key_id: Option<&KeyId>,
) -> Result<DriveKey, DriveError> {
    adopt_invite_reporting(bucket, keyring, drive, invite, key_id).map(|adopted| adopted.key)
}

/// [`adopt_invite`] that says whether the invite is still live afterwards ([`Adopted`]).
pub fn adopt_invite_reporting(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    invite: &MemberSecret,
    key_id: Option<&KeyId>,
) -> Result<Adopted, DriveError> {
    let member = invite_member(invite);
    let file = member_key_file(&member).map_err(|e| e.for_key(drive))?;
    let wrap = match load_member_wrap(bucket, &member) {
        Ok(wrap) => wrap,
        Err(DriveError::NotFound { .. }) => {
            return Err(DriveError::Denied {
                message: String::from(
                    "the join code's key was used already or withdrawn: ask for a new code",
                ),
            })
        }
        Err(e) => return Err(e),
    };
    let named = match key_id {
        Some(id) => Some(*id),
        None => committed_drive_key(keyring, drive)?,
    };
    if let Some(named) = named {
        if wrap.drive_key_id != named {
            return Err(refused(
                drive,
                format!(
                    "the join code's seal holds the drive key {}, and the code names {named}",
                    wrap.drive_key_id
                ),
            ));
        }
    }
    let drive_key = wrap.open(drive, invite).map_err(|e| e.for_key(&file))?;
    shadow_recover("device::adopt_invite_reporting", keyring, drive);
    enroll(bucket, keyring, drive, &drive_key)?;
    // The device has the key. A wrap that is gone is as good as deleted; any other failure leaves
    // the invite live, and that is said instead of being turned into a failure of the join.
    Ok(Adopted {
        key: drive_key,
        invite_left: invite_left(bucket, &file),
    })
}

/// Deletes the invite's wrap `file` and says whether it is still in the bucket afterwards: `None`
/// when it is gone, else the failure. A delete whose answer was lost may have worked, so a failed
/// one looks again before it says the invite is live; a look that fails too leaves it unknown, and
/// unknown is reported as live: the join code is not trusted to be gone.
fn invite_left(bucket: &dyn Drive, file: &str) -> Option<DriveError> {
    match bucket.delete(file) {
        Ok(()) | Err(DriveError::NotFound { .. }) => None,
        Err(e) => match bucket.head(file) {
            Err(DriveError::NotFound { .. }) => None,
            Ok(_) | Err(_) => Some(e),
        },
    }
}

/// Withdraws an invite that was not used (its wrap leaves the bucket).
pub fn withdraw_invite(bucket: &dyn Drive, invite: &MemberSecret) -> Result<(), DriveError> {
    let member = invite_member(invite);
    let file = member_key_file(&member).map_err(|e| e.for_key(&member))?;
    bucket.delete(&file)
}

/// All the work the Argon2id derivations of ONE recovery together may ask for (KiB of memory times
/// passes): the first wrap and every further code's, each at the cost this software writes, with
/// room to spare. The cost of a wrap is the bucket's word and is paid before the code is known to
/// be right, so a bucket that lists the most further files (8) with the most each may ask for
/// cannot keep a device busy for minutes: the wraps after the budget are not tried.
pub const RECOVER_WORK_BUDGET: u64 =
    (1 + MAX_EXTRA_RECOVERY_CODES as u64) * RecoveryKdf::DEFAULT_WORK * 2;

/// The drive key the recovery code opens; this device is enrolled with it (a member wrap of
/// its own, the key in the keyring). `Denied` for a wrong code. Only its holder can seal a
/// wrap the code opens, so the key is the drive's, whichever key this device held before (a
/// rotation made a new one); a claim code's id still has to match (`KeyRefused`).
pub fn recover(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    code: &RecoveryCode,
) -> Result<DriveKey, DriveError> {
    recover_within(bucket, keyring, drive, code, RECOVER_WORK_BUDGET)
}

/// [`recover`] with `budget` as the most work its derivations may ask for together
/// ([`RECOVER_WORK_BUDGET`]).
pub fn recover_within(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    code: &RecoveryCode,
    budget: u64,
) -> Result<DriveKey, DriveError> {
    // The first code's wrap, then a further code's (a second kit), until one opens. A wrap that
    // is missing, damaged or asks for more than any wrap does is no wrap: the others are still
    // tried (a hostile bucket cannot keep a second kit from opening the drive by spoiling the
    // first), and what was wrong with the first is said when none opens.
    let mut first_problem = None;
    let mut spent = 0u64;
    let mut opened = match load_recovery_wrap(bucket) {
        Ok(wrap) => {
            spent = spent.saturating_add(wrap.kdf.work());
            match wrap.open(drive, code) {
                Ok(drive_key) => Some(drive_key),
                Err(CryptoError::WrongKey) => None,
                Err(CryptoError::Unsupported(why)) => {
                    first_problem = Some(CryptoError::Unsupported(why).for_key(RECOVERY_KEY_FILE));
                    None
                }
                Err(other) => return Err(other.for_key(RECOVERY_KEY_FILE)),
            }
        }
        Err(
            e @ (DriveError::NotFound { .. }
            | DriveError::Corrupt { .. }
            | DriveError::Unsupported(_)),
        ) => {
            first_problem = Some(e);
            None
        }
        Err(e) => return Err(e),
    };
    if opened.is_none() {
        for file in extra_recovery_files(bucket)? {
            let bytes = match crate::limits::named::KEY_FILE.read_from(bucket, &file) {
                Ok(bytes) => bytes,
                // Gone meanwhile, or bigger than any key file is (whatever kind of drive says
                // so: a bucket refuses it on its way, a folder after the read): no wrap, the
                // others are tried.
                Err(
                    DriveError::NotFound { .. }
                    | DriveError::Protocol(_)
                    | DriveError::Corrupt { .. },
                ) => continue,
                Err(e) => return Err(e),
            };
            // A file that holds no usable wrap is left out: a damaged one is junk, and one of a
            // cost this build does not derive with is said when no wrap opens (the code may be
            // right and the wrap one this build cannot try, which is not "the code is wrong").
            let wrap = match RecoveryWrap::parse(&bytes) {
                Ok(wrap) => wrap,
                Err(CryptoError::Unsupported(why)) => {
                    first_problem.get_or_insert(CryptoError::Unsupported(why).for_key(&file));
                    continue;
                }
                Err(_) => continue,
            };
            spent = spent.saturating_add(wrap.kdf.work());
            if spent > budget {
                first_problem.get_or_insert(DriveError::Unsupported(String::from(
                    "the drive's recovery key files ask for more work than one recovery spends: \
                     the rest were not tried",
                )));
                break;
            }
            match wrap.open(drive, code) {
                Ok(drive_key) => {
                    opened = Some(drive_key);
                    break;
                }
                Err(CryptoError::WrongKey | CryptoError::Unsupported(_)) => {}
                Err(other) => return Err(other.for_key(&file)),
            }
        }
    }
    let drive_key = match (opened, first_problem) {
        (Some(drive_key), _) => drive_key,
        (None, Some(problem)) => return Err(problem),
        (None, None) => {
            return Err(DriveError::Denied {
                message: String::from("the recovery code does not open this drive"),
            })
        }
    };
    shadow_recover("device::recover_within", keyring, drive);
    if let Some(named) = committed_drive_key(keyring, drive)? {
        if drive_key.id() != named {
            return Err(refused(
                drive,
                format!(
                    "the recovery code opens the drive key {}, and the claim code names {named}",
                    drive_key.id()
                ),
            ));
        }
    }
    enroll(bucket, keyring, drive, &drive_key)?;
    Ok(drive_key)
}

/// The key permit's shadow of a flow that brings the drive key to this device (C7 step 2): `Recover`,
/// asked once the bucket has shown that the drive is encrypted (a recovery wrap opened, an invite
/// opened) and before the key is written. The guard is the keyring's protection
/// ([`check_keeps_keys`]) - the first key written refuses where it keeps none - and nothing looks at
/// the keys this device already keeps. The table refuses to bring a key onto a device whose setup
/// or key change is unfinished.
fn shadow_recover(site: &'static str, keyring: &dyn KeyringStore, drive: &str) {
    let mut evidence = keystate::Evidence::new(drive, crate::time::now_unix());
    evidence.keyring = key_facts(keyring, drive);
    evidence.keyring.keeps_keys = keeps_keys_fact(keyring);
    evidence.bucket.encrypted = Read::Found(true);
    let guard = if check_keeps_keys(keyring, "the drive key").is_err() {
        Guard::Refuses
    } else {
        Guard::Permits
    };
    crate::key_shadow!(
        site,
        Op::Recover,
        evidence,
        guard,
        known = [
            (
                "planned",
                "the flow writes the recovered key over a device whose own setup did not finish: its \
                 planned key and the recovered one would pin two different drives' keys"
            ),
            (
                "rotating",
                "the flow brings a key onto a device that keeps the new key of an unfinished rotation \
                 (harmless for the old key's wrap, but the table says finish the change first)"
            ),
            (
                "unknown",
                "the flow reads none of the keyring's key entries before it writes: a keyring that \
                 cannot be read fails at the write (the table refuses on any failed read)"
            ),
        ]
    );
}

// ==== Trusted contacts' keys (super::contacts) ====

/// The keyring entry of a contact key - the key this device made to hold one owner's share of a
/// recovery code - by its public key's id.
#[must_use]
pub fn contact_key_entry(id: &str) -> String {
    crate::footprint::contact_key_entry(id)
}

/// The keyring entry of a drive's recovery request key, kept while trusted contacts answer.
#[must_use]
pub fn request_key_entry(drive: &str) -> String {
    KeyEntry::RequestKey.name(drive)
}

const KIND_CONTACT_KEY: &str = "contact-key";
const KIND_REQUEST_KEY: &str = "request-key";

/// A new contact key kept in the keyring; its public half (what the owner seals a share to).
pub fn new_contact_key(keyring: &dyn KeyringStore) -> Result<super::keys::MemberPublic, DriveError> {
    let secret = MemberSecret::generate().map_err(|e| e.for_key("a contact key"))?;
    let public = secret.public();
    store_key(
        keyring,
        &contact_key_entry(&public.id()),
        KIND_CONTACT_KEY,
        &secret.to_bytes(),
    )?;
    Ok(public)
}

/// The contact key whose public half is `public`; `None` when this device has none.
pub fn load_contact_key(
    keyring: &dyn KeyringStore,
    public: &super::keys::MemberPublic,
) -> Result<Option<MemberSecret>, DriveError> {
    let secret = load_key(keyring, &contact_key_entry(&public.id()), KIND_CONTACT_KEY)?
        .map(|bytes| MemberSecret::from_bytes(*bytes));
    // An entry under the id of another key is no key of this one.
    Ok(secret.filter(|secret| secret.public() == *public))
}

/// The drive's recovery request key: the keyring's, else a new one kept there.
pub fn request_key(keyring: &dyn KeyringStore, drive: &str) -> Result<MemberSecret, DriveError> {
    let entry = request_key_entry(drive);
    if let Some(bytes) = load_key(keyring, &entry, KIND_REQUEST_KEY)? {
        return Ok(MemberSecret::from_bytes(*bytes));
    }
    let secret = MemberSecret::generate().map_err(|e| e.for_key(drive))?;
    store_key(keyring, &entry, KIND_REQUEST_KEY, &secret.to_bytes())?;
    Ok(secret)
}

/// Removes the drive's recovery request key (the recovery is done).
pub fn forget_request_key(keyring: &dyn KeyringStore, drive: &str) -> Result<(), DriveError> {
    let entry = request_key_entry(drive);
    keyring.delete(&entry).map_err(|e| keyring_error(&entry, e))
}

/// Members of the drive other than this device - its other devices (and, in a shared drive,
/// the other people): the member key files in `.azlin/keys/`, the recovery code's and the
/// invites' left out.
pub fn other_devices(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<u32, DriveError> {
    let own = load_member_secret(keyring, drive)?
        .and_then(|secret| member_key_file(&secret.public().id()).ok());
    let mut request = ListRequest::recursive(KEYS_PREFIX);
    let mut count = 0u32;
    let mut pages = crate::ops::Pages::new();
    loop {
        let page = bucket.list(&request)?;
        for object in &page.objects {
            let key = object.key.as_str();
            let member = key
                .strip_prefix(KEYS_PREFIX)
                .and_then(|rest| rest.strip_suffix(".key"))
                .unwrap_or("");
            // `_rotation.key` and `_chain.key` are the drive's own files, not a device's.
            let counts = !member.is_empty()
                && !member.starts_with('_')
                && !member.contains('/')
                && key != RECOVERY_KEY_FILE
                && !key.starts_with(EXTRA_RECOVERY_PREFIX)
                && !member.starts_with("invite-")
                && own.as_deref() != Some(key);
            if counts {
                count = count.saturating_add(1);
            }
        }
        match page.next {
            // Bounded: a storage that lists member files for ever is refused (X-DELTA-AZDRIVE-KEYS-22).
            Some(next) => request = pages.next(request, next, count as usize)?,
            None => break,
        }
    }
    Ok(count)
}

// ==== Further recovery codes (a second emergency kit, D51) ====

/// The start of a further recovery code's key file: `.azlin/keys/recovery-<16 hex>.key` (the
/// first code's is [`RECOVERY_KEY_FILE`]). A key rotation removes them with the other wraps.
pub const EXTRA_RECOVERY_PREFIX: &str = ".azlin/keys/recovery-";

/// The most further recovery codes a drive holds (a second kit, a third): the recovery tries each
/// wrap's Argon2id in turn, so the bucket's say on how many there are is bounded.
pub const MAX_EXTRA_RECOVERY_CODES: usize = 8;

/// One more recovery code of a drive this device holds the key of, sealed like the first in a
/// wrap of its own; its file and the code (for its sheet: stored nowhere). A drive holds at
/// most [`MAX_EXTRA_RECOVERY_CODES`] of them.
pub fn add_recovery_code(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    kdf: RecoveryKdf,
) -> Result<(String, RecoveryCode), DriveError> {
    add_recovery_code_named(bucket, keyring, drive, kdf, |_| {
        let mut id = [0u8; 8];
        super::random_bytes(&mut id).map_err(|e| e.for_key(drive))?;
        Ok(format!("{EXTRA_RECOVERY_PREFIX}{}.key", to_hex(&id)))
    })
}

/// The key file of the further recovery code whose key at the token server is `public_key` (its
/// base64 text): `.azlin/keys/recovery-<16 hex digits of the text's hash>.key`. Named so, a kit's
/// wrap is found by anyone who sees the key at the server - a removal of the key from ANOTHER
/// computer than the one that made the kit finds the wrap too (X-DELTA-AZDRIVE-KEYS-28). Kits made
/// before had a random name, known only to the computer that made them.
#[must_use]
pub fn recovery_code_file_for(public_key: &str) -> String {
    let hash = blake3::hash(public_key.trim().as_bytes());
    format!(
        "{EXTRA_RECOVERY_PREFIX}{}.key",
        to_hex(&hash.as_bytes()[..8])
    )
}

/// [`add_recovery_code`] whose file is named by the code's key at the token server
/// ([`recovery_code_file_for`]); `public_key_of` says that key from the code.
pub fn add_recovery_code_for_key(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    kdf: RecoveryKdf,
    public_key_of: impl FnOnce(&RecoveryCode) -> String,
) -> Result<(String, RecoveryCode), DriveError> {
    add_recovery_code_named(bucket, keyring, drive, kdf, |code| {
        Ok(recovery_code_file_for(&public_key_of(code)))
    })
}

/// [`add_recovery_code`] in the file `file_of` names for the new code.
fn add_recovery_code_named(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    kdf: RecoveryKdf,
    file_of: impl FnOnce(&RecoveryCode) -> Result<String, DriveError>,
) -> Result<(String, RecoveryCode), DriveError> {
    let extra = extra_recovery_files(bucket)?.len();
    shadow_add_code(keyring, drive, extra);
    if extra >= MAX_EXTRA_RECOVERY_CODES {
        return Err(DriveError::Unsupported(format!(
            "a drive holds at most {MAX_EXTRA_RECOVERY_CODES} further recovery codes: remove one first"
        )));
    }
    let (drive_key, stashed) =
        held(keyring, &drive_key_entry(drive))?.ok_or_else(|| DriveError::Locked {
            drive: drive.to_string(),
        })?;
    let code = RecoveryCode::generate().map_err(|e| e.for_key(drive))?;
    let wrap = RecoveryWrap::seal(&drive_key, drive, &code, kdf).map_err(|e| e.for_key(drive))?;
    let file = file_of(&code)?;
    publish_recovery_wrap(bucket, &stashed, &wrap, Publish::Extra(&file))?;
    Ok((file, code))
}

/// The key permit's shadow of [`add_recovery_code`] (C7 step 2), asked before its two guards answer:
/// a drive holds at most [`MAX_EXTRA_RECOVERY_CODES`] further codes, and the device must hold the
/// drive key ([`held`] says `Locked` where it does not). Nothing looks at a key change that is
/// unfinished: a code added while the recovery wrap may be replaced is the finding of KEYS-7.
fn shadow_add_code(keyring: &dyn KeyringStore, drive: &str, extra: usize) {
    let mut evidence = keystate::Evidence::new(drive, crate::time::now_unix());
    evidence.keyring = key_facts(keyring, drive);
    evidence.bucket.extra_codes = Read::Found(u32::try_from(extra).unwrap_or(u32::MAX));
    let holds = matches!(evidence.keyring.drive_key, Read::Found(_));
    let guard = if extra >= MAX_EXTRA_RECOVERY_CODES || !holds {
        Guard::Refuses
    } else {
        Guard::Permits
    };
    crate::key_shadow!(
        "device::add_recovery_code_named",
        Op::AddCode,
        evidence,
        guard,
        known = [
            (
                "rotating",
                "X-DELTA-AZDRIVE-KEYS-7: no guard here refuses a code while a key change is \
                 unfinished (only the settings forget the code's check); the table refuses it, the \
                 recovery wrap may be replaced"
            ),
            (
                "unknown",
                "the flow reads only the drive key: a keyring that fails on the new key or the \
                 planned key alone does not stop it (the table refuses on any failed read)"
            ),
        ]
    );
}

/// Removes a further recovery code's wrap: its kit no longer opens the drive (a file that is
/// gone already is no error). Refused for any other file - the first code's, a member's.
pub fn remove_recovery_code(bucket: &dyn Drive, file: &str) -> Result<(), DriveError> {
    check_extra_recovery_file(file)?;
    match bucket.delete(file) {
        Ok(()) | Err(DriveError::NotFound { .. }) => Ok(()),
        Err(e) => Err(e),
    }
}

/// `file` names a further recovery code's key file and no other.
fn check_extra_recovery_file(file: &str) -> Result<(), DriveError> {
    let named = file
        .strip_prefix(EXTRA_RECOVERY_PREFIX)
        .and_then(|rest| rest.strip_suffix(".key"))
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit()));
    if named {
        Ok(())
    } else {
        Err(DriveError::InvalidConfig(format!(
            "{file} is no further recovery code's key file"
        )))
    }
}

/// The bytes of a further recovery code's key file, to be put back by
/// [`restore_recovery_code`] when the removal they were taken for is refused; `None` for a file
/// that is not there.
///
/// # Errors
///
/// A name that is no further recovery code's, the bucket's refusal, a file past the key files'
/// limit.
pub fn read_recovery_code(bucket: &dyn Drive, file: &str) -> Result<Option<Vec<u8>>, DriveError> {
    check_extra_recovery_file(file)?;
    match crate::limits::named::KEY_FILE.read_from(bucket, file) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(DriveError::NotFound { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Puts a further recovery code's key file (as [`read_recovery_code`] read it) back, only where
/// nothing is: a removal the token server refused leaves the kit as it was.
///
/// # Errors
///
/// A name that is no further recovery code's, the bucket's refusal; [`DriveError::Conflict`] when
/// something is there already.
pub fn restore_recovery_code(
    bucket: &dyn Drive,
    file: &str,
    bytes: &[u8],
) -> Result<(), DriveError> {
    check_extra_recovery_file(file)?;
    put_new(bucket, file, bytes)
}

/// The key files of the drive's further recovery codes that none of `claimed` names: copies of
/// kits that no key at the token server stands behind any more, or that were made before a kit's
/// copy was named by its key ([`recovery_code_file_for`]), so that no key can be mapped to them.
/// Each still opens the drive key for whoever holds its code and reaches the bucket, and only a
/// key change removes them all (X-DELTA-AZDRIVE-KEYS-28). A drive holds at most
/// [`MAX_EXTRA_RECOVERY_CODES`] of them (the add refuses more), which is also what is looked at.
///
/// # Errors
///
/// The bucket's refusal to list.
pub fn unclaimed_recovery_codes(
    bucket: &dyn Drive,
    claimed: &[String],
) -> Result<Vec<String>, DriveError> {
    let mut found = extra_recovery_files(bucket)?;
    found.retain(|file| !claimed.contains(file));
    Ok(found)
}

/// The key files of the drive's further recovery codes: the first [`MAX_EXTRA_RECOVERY_CODES`]
/// by name, found in a bounded number of listing pages (a bucket that lists endless names under
/// the prefix does not keep this reading for ever).
fn extra_recovery_files(bucket: &dyn Drive) -> Result<Vec<String>, DriveError> {
    const MAX_PAGES: usize = 16;
    let mut found = Vec::new();
    let mut request = ListRequest::recursive(EXTRA_RECOVERY_PREFIX);
    for _ in 0..MAX_PAGES {
        let page = bucket.list(&request)?;
        for object in page.objects {
            let named = object
                .key
                .strip_prefix(EXTRA_RECOVERY_PREFIX)
                .and_then(|rest| rest.strip_suffix(".key"))
                .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit()));
            if named {
                found.push(object.key);
            }
        }
        match page.next {
            Some(next) if found.len() < MAX_EXTRA_RECOVERY_CODES => {
                request.continuation = Some(next)
            }
            _ => break,
        }
    }
    found.sort();
    found.truncate(MAX_EXTRA_RECOVERY_CODES);
    Ok(found)
}

