//! The keys on a device: the drive key and this device's member key in the OS keyring, and the
//! flows that put them there.
//!
//! Keyring entries (one per drive, next to the drive's session at
//! [`crate::config::keyring_key`]):
//! - [`drive_key_entry`] `azul-storage/drive-key/<drive>`: the drive key;
//! - [`member_key_entry`] `azul-storage/member-key/<drive>`: this device's X25519 member secret
//!   for the drive (one per drive, so two drives cannot be linked by a device's key).
//!
//! The flows, each over the drive's bucket (the inner drive, below the encryption) and the
//! keyring:
//! - [`setup_new_drive`]: the first device of a drive - a new drive key, a new recovery code (for
//!   the recovery sheet; it is never stored), the recovery wrap and this device's member wrap
//!   in the bucket, the keys in the keyring.
//! - [`unlock`]: the drive key from the keyring, else from this device's member wrap in the
//!   bucket (opened with its member secret); `None` when this device has no wrap.
//! - [`enroll`]: this device's member wrap, made from a drive key it holds.
//! - [`seal_invite`] / [`adopt_invite`]: a second device gets the drive key through its join
//!   code. The inviting device seals the drive key to a one-time X25519 key
//!   (`.azlin/keys/invite-<id>.key`) whose secret rides in the join code; the joining device
//!   opens it, enrols itself and deletes the invite wrap, so the code opens the key once.
//! - [`recover`]: the recovery code opens the recovery wrap; this device is enrolled.
//!
//! A join code (like the recovery code) is a bearer secret: whoever holds it reads the drive.
//! It already grants the bucket; with the invite secret it grants the data too. Pass it by a
//! file or a QR code, never by a channel others read.

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::{
    hex_array,
    keys::{
        load_member_wrap, load_recovery_wrap, member_key_file, store_member_wrap,
        store_recovery_wrap, MemberSecret, MemberWrap, RecoveryCode, RecoveryKdf, RecoveryWrap,
        KEYS_PREFIX, RECOVERY_KEY_FILE,
    },
    to_hex, CryptoError, DriveKey, KEY_LEN,
};
use crate::{
    keyring::{KeyringError, KeyringStore},
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
}

const KIND_DRIVE_KEY: &str = "drive-key";
const KIND_MEMBER_KEY: &str = "member-key";

/// The member id an invite's one-time key has in the bucket.
fn invite_member(invite: &MemberSecret) -> String {
    format!("invite-{}", invite.public().id())
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
    let text = Zeroizing::new(
        serde_json::to_string(&StoredKey {
            kind: kind.to_string(),
            key: to_hex(bytes),
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
    Ok(Some(Zeroizing::new(bytes)))
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
    Ok(load_key(keyring, &member_key_entry(drive), KIND_MEMBER_KEY)?
        .map(|bytes| MemberSecret::from_bytes(*bytes)))
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
    for entry in [drive_key_entry(drive), member_key_entry(drive)] {
        keyring
            .delete(&entry)
            .map_err(|e| keyring_error(&entry, e))?;
    }
    Ok(())
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

/// The first device of a drive: a new drive key and recovery code. The recovery wrap (derived
/// with `kdf`) and this device's member wrap go into the bucket, the drive key and the member
/// secret into the keyring. Returns the drive key, and the recovery code for the recovery
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
    let drive_key = DriveKey::generate().map_err(|e| e.for_key(drive))?;
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
    Ok((drive_key, code))
}

/// Seals `drive_key` to this device's member key (made and kept in the keyring when there is
/// none yet), puts the wrap into the bucket and keeps the drive key in the keyring. Returns
/// this device's member id.
pub fn enroll(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    drive_key: &DriveKey,
) -> Result<String, DriveError> {
    let secret = member_secret(keyring, drive)?;
    let member = secret.public().id();
    let wrap = MemberWrap::seal(drive_key, drive, &member, &secret.public())
        .map_err(|e| e.for_key(drive))?;
    store_member_wrap(bucket, &wrap)?;
    store_drive_key(keyring, drive, drive_key)?;
    Ok(member)
}

/// The drive key on this device: the keyring's, else the one this device's member wrap in the
/// bucket holds (opened with its member secret, then kept in the keyring). `None` when this
/// device has neither: it needs an invite ([`adopt_invite`]) or the recovery code ([`recover`]).
pub fn unlock(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
) -> Result<Option<DriveKey>, DriveError> {
    if let Some(key) = load_drive_key(keyring, drive)? {
        return Ok(Some(key));
    }
    let Some(secret) = load_member_secret(keyring, drive)? else {
        return Ok(None);
    };
    let member = secret.public().id();
    let wrap = match load_member_wrap(bucket, &member) {
        Ok(wrap) => wrap,
        Err(DriveError::NotFound { .. }) => return Ok(None),
        Err(e) => return Err(e),
    };
    let file = member_key_file(&member).map_err(|e| e.for_key(drive))?;
    let drive_key = wrap.open(drive, &secret).map_err(|e| e.for_key(&file))?;
    store_drive_key(keyring, drive, &drive_key)?;
    Ok(Some(drive_key))
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
/// deletes the invite wrap (the code opens the key once). `Denied` when the invite was used
/// already or withdrawn.
pub fn adopt_invite(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    invite: &MemberSecret,
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
/// its own, the key in the keyring). `Denied` for a wrong code.
pub fn recover(
    bucket: &dyn Drive,
    keyring: &dyn KeyringStore,
    drive: &str,
    code: &RecoveryCode,
) -> Result<DriveKey, DriveError> {
    let wrap = load_recovery_wrap(bucket)?;
    let drive_key = wrap.open(drive, code).map_err(|e| match e {
        CryptoError::WrongKey => DriveError::Denied {
            message: String::from("the recovery code does not open this drive"),
        },
        other => other.for_key(RECOVERY_KEY_FILE),
    })?;
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
