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
use zeroize::Zeroizing;

use super::{
    hex_array,
    keys::{
        load_member_wrap, load_member_wraps, load_recovery_wrap, member_key_file,
        store_member_wrap, store_recovery_wrap, MemberSecret, MemberWrap, RecoveryCode,
        RecoveryKdf, RecoveryWrap, INVITE_PREFIX, KEYS_PREFIX, RECOVERY_KEY_FILE,
    },
    to_hex, CryptoError, DriveKey, KeyId, KEY_LEN,
};
use crate::{
    keyring::{KeyringError, KeyringStore},
    meta::policy::{KeyCheck, MemberChange, MemberRecord},
    Drive, DriveError, ListRequest, Precondition,
};

/// The keyring entry of a drive's key.
#[must_use]
pub fn drive_key_entry(drive: &str) -> String {
    format!("azul-storage/drive-key/{drive}")
}

/// The keyring entry of this device's member secret for a drive.
#[must_use]
pub fn member_key_entry(drive: &str) -> String {
    format!("azul-storage/member-key/{drive}")
}

/// The keyring entry of a drive key made before its drive existed: a checkout keeps one for
/// the drive it pays for, so the claim code printed before the drive exists names the key's id
/// ([`plan_drive_key`]); [`setup_new_drive`] takes it.
#[must_use]
pub fn planned_key_entry(drive: &str) -> String {
    format!("azul-storage/drive-key-planned/{drive}")
}

/// The keyring entries a key rotation keeps both keys in until it is done
/// ([`crate::rotation`]): the drive key before it and the one after it.
#[must_use]
pub fn rotation_key_entries(drive: &str) -> (String, String) {
    (
        format!("azul-storage/drive-key-previous/{drive}"),
        format!("azul-storage/drive-key-next/{drive}"),
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

/// A key as the keyring keeps it (JSON): what it is and its bytes in hex.
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
    let text = Zeroizing::new(
        serde_json::to_string(&StoredKey {
            kind: kind.to_string(),
            key: to_hex(bytes),
            drive_key_id: drive_key_id.map(KeyId::to_hex),
            committed: committed.map(KeyId::to_hex),
        })
        .unwrap_or_default(),
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
    let stored: StoredKey = serde_json::from_str(&text).map_err(|_| {
        DriveError::InvalidConfig(format!("the keyring entry {entry} holds no key"))
    })?;
    let hex = Zeroizing::new(stored.key);
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
        drive_key_id: id(stored.drive_key_id)?,
        committed: id(stored.committed)?,
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
    member_secret(keyring, drive)?;
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
}

/// A claim code names the id of the drive's key before this device holds it: every key the
/// device gets for the drive from then on - from its wrap, a join code or the recovery code -
/// must have that id (a join code that names another one is the newer word). A device that
/// holds the drive's key already keeps its pin.
pub fn commit_drive_key(keyring: &dyn KeyringStore, drive: &str, id: &KeyId) -> Result<(), DriveError> {
    member_secret(keyring, drive)?;
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

/// This device's member secret for the drive: the keyring's, else a new one, kept there
/// before it is used.
pub fn member_secret(keyring: &dyn KeyringStore, drive: &str) -> Result<MemberSecret, DriveError> {
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

/// Removes the drive's keys from this device (the drive was removed, or the device signs out).
pub fn forget_keys(keyring: &dyn KeyringStore, drive: &str) -> Result<(), DriveError> {
    for entry in [
        drive_key_entry(drive),
        member_key_entry(drive),
        planned_key_entry(drive),
    ] {
        keyring
            .delete(&entry)
            .map_err(|e| keyring_error(&entry, e))?;
    }
    Ok(())
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
pub fn setup_new_drive(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    kdf: RecoveryKdf,
) -> Result<(DriveKey, RecoveryCode), DriveError> {
    if is_encrypted(bucket)? {
        return Err(DriveError::InvalidConfig(format!(
            "\"{drive}\" is encrypted already: open it with this device's key, an invite or the \
             recovery code"
        )));
    }
    let planned = planned_key_entry(drive);
    let drive_key = match load_key_at(keyring, &planned)? {
        Some(key) => key,
        None => DriveKey::generate().map_err(|e| e.for_key(drive))?,
    };
    let code = RecoveryCode::generate().map_err(|e| e.for_key(drive))?;
    let recovery =
        RecoveryWrap::seal(&drive_key, drive, &code, kdf).map_err(|e| e.for_key(drive))?;
    // The recovery wrap first: a drive must never hold data no recovery code opens.
    put_new(bucket, RECOVERY_KEY_FILE, &recovery.to_bytes()).map_err(|e| match e {
        DriveError::Conflict { .. } => DriveError::Conflict {
            key: String::from(RECOVERY_KEY_FILE),
        },
        other => other,
    })?;
    enroll(bucket, keyring, drive, &drive_key)?;
    delete_key_at(keyring, &planned)?;
    Ok((drive_key, code))
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

/// A joining device takes the drive key its join code's invite opens, enrols itself and
/// deletes the invite wrap (the code opens the key once). `key_id` is the id of the drive key
/// the join code names (a code from before has none): a seal of another key is refused
/// (`KeyRefused`), and so is one of another key than a claim code named when the join code
/// names none. `Denied` when the invite was used already or withdrawn.
pub fn adopt_invite(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    invite: &MemberSecret,
    key_id: Option<&KeyId>,
) -> Result<DriveKey, DriveError> {
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
    enroll(bucket, keyring, drive, &drive_key)?;
    bucket.delete(&file)?;
    Ok(drive_key)
}

/// Withdraws an invite that was not used (its wrap leaves the bucket).
pub fn withdraw_invite(bucket: &dyn Drive, invite: &MemberSecret) -> Result<(), DriveError> {
    let member = invite_member(invite);
    let file = member_key_file(&member).map_err(|e| e.for_key(&member))?;
    bucket.delete(&file)
}

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
    // The first code's wrap, then a further code's (a second kit), until one opens.
    let wrap = load_recovery_wrap(bucket)?;
    let mut opened = match wrap.open(drive, code) {
        Ok(drive_key) => Some(drive_key),
        Err(CryptoError::WrongKey) => None,
        Err(other) => return Err(other.for_key(RECOVERY_KEY_FILE)),
    };
    if opened.is_none() {
        for file in extra_recovery_files(bucket)? {
            let bytes = match bucket.get(&file) {
                Ok(bytes) => bytes,
                Err(DriveError::NotFound { .. }) => continue,
                Err(e) => return Err(e),
            };
            let wrap = RecoveryWrap::parse(&bytes).map_err(|e| e.for_key(&file))?;
            match wrap.open(drive, code) {
                Ok(drive_key) => {
                    opened = Some(drive_key);
                    break;
                }
                Err(CryptoError::WrongKey) => {}
                Err(other) => return Err(other.for_key(&file)),
            }
        }
    }
    let drive_key = opened.ok_or_else(|| DriveError::Denied {
        message: String::from("the recovery code does not open this drive"),
    })?;
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

// ==== Trusted contacts' keys (super::contacts) ====

/// The keyring entry of a contact key - the key this device made to hold one owner's share of a
/// recovery code - by its public key's id.
#[must_use]
pub fn contact_key_entry(id: &str) -> String {
    format!("azul-storage/contact-key/{id}")
}

/// The keyring entry of a drive's recovery request key, kept while trusted contacts answer.
#[must_use]
pub fn request_key_entry(drive: &str) -> String {
    format!("azul-storage/recovery-request/{drive}")
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
    loop {
        let page = bucket.list(&request)?;
        for object in &page.objects {
            let key = object.key.as_str();
            let member = key
                .strip_prefix(KEYS_PREFIX)
                .and_then(|rest| rest.strip_suffix(".key"))
                .unwrap_or("");
            let counts = !member.is_empty()
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
            Some(next) => request.continuation = Some(next),
            None => break,
        }
    }
    Ok(count)
}

// ==== Further recovery codes (a second emergency kit, D51) ====

/// The start of a further recovery code's key file: `.azlin/keys/recovery-<16 hex>.key` (the
/// first code's is [`RECOVERY_KEY_FILE`]). A key rotation removes them with the other wraps.
pub const EXTRA_RECOVERY_PREFIX: &str = ".azlin/keys/recovery-";

/// One more recovery code of a drive this device holds the key of, sealed like the first in a
/// wrap of its own; its file and the code (for its sheet: stored nowhere).
pub fn add_recovery_code(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    kdf: RecoveryKdf,
) -> Result<(String, RecoveryCode), DriveError> {
    let drive_key = load_drive_key(keyring, drive)?.ok_or_else(|| DriveError::Locked {
        drive: drive.to_string(),
    })?;
    let code = RecoveryCode::generate().map_err(|e| e.for_key(drive))?;
    let wrap = RecoveryWrap::seal(&drive_key, drive, &code, kdf).map_err(|e| e.for_key(drive))?;
    let mut id = [0u8; 8];
    super::random_bytes(&mut id).map_err(|e| e.for_key(drive))?;
    let file = format!("{EXTRA_RECOVERY_PREFIX}{}.key", to_hex(&id));
    put_new(bucket, &file, &wrap.to_bytes())?;
    Ok((file, code))
}

/// Removes a further recovery code's wrap: its kit no longer opens the drive (a file that is
/// gone already is no error). Refused for any other file - the first code's, a member's.
pub fn remove_recovery_code(bucket: &dyn Drive, file: &str) -> Result<(), DriveError> {
    let named = file
        .strip_prefix(EXTRA_RECOVERY_PREFIX)
        .and_then(|rest| rest.strip_suffix(".key"))
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit()));
    if !named {
        return Err(DriveError::InvalidConfig(format!(
            "{file} is no further recovery code's key file"
        )));
    }
    match bucket.delete(file) {
        Ok(()) | Err(DriveError::NotFound { .. }) => Ok(()),
        Err(e) => Err(e),
    }
}

/// The key files of the drive's further recovery codes.
fn extra_recovery_files(bucket: &dyn Drive) -> Result<Vec<String>, DriveError> {
    Ok(crate::ops::list_all(bucket, EXTRA_RECOVERY_PREFIX)?
        .into_iter()
        .map(|object| object.key)
        .filter(|key| key.ends_with(".key"))
        .collect())
}

