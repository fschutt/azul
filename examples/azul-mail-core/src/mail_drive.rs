//! The drive an Azlin account's mail lives in: the one place AzMail and the Azlin Bridge open it,
//! so the mail objects (`mail/<Folder>/<name>.eml`, the markers under `mail/.state/`) go through
//! the encryption when the drive is encrypted. Plain Rust, no azul types: the keyring comes in.
//!
//! With the feature `encryption` the bucket is wrapped in azul-storage's `AutoEncrypted`: the
//! first call decides whether the drive is encrypted (this computer keeps its key - AzDrive's
//! entry for the same drive id, the keyring is shared - or the bucket holds key files); an
//! encrypted one then goes through the drive index (the bucket's encrypted metadata
//! repository); a plain one is used as it is. Without the feature the bucket is used as it is,
//! as before.
//!
//! Incoming mail of an encrypted drive arrives as DROPS: the customer's Cloudflare Email Worker
//! seals each message to the drive's drop key (azul-storage's `crypto::drops`, AZD1) and puts it
//! at `.azlin/drop/<random>`. [`receive_drops`] (AzMail's Send/Receive, before the folders
//! sync) and [`receive_drops_into`] (the bridge, before it lists a folder) file them into the
//! drive's encrypted `mail/<Folder>/` under their stable names and delete them; both are
//! [`file_drops`].
//!
//! The drive index names this device in its commits and conflict copies:
//! [`set_device_name`] (AzMail's is "AzMail", the bridge's "Azlin Bridge").

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[cfg(feature = "encryption")]
use azul_storage::{
    crypto::{
        device,
        drops::{self, Dropped},
    },
    encrypted::{open_encrypted, AutoEncrypted, IndexProvider},
    Precondition,
};
use azul_storage::{keyring::KeyringStore, Drive, DriveError, Transport};

use crate::azlin::AzlinSession;

/// Where the drive index keeps this computer's copies of encrypted drives
/// ([`set_index_cache_root`]; `None`: in memory, read anew on every start).
static INDEX_CACHE_ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The name the drive index gives this device ([`set_device_name`]; `None`: "AzMail").
static DEVICE_NAME: Mutex<Option<String>> = Mutex::new(None);

/// The device name of a process that never set one.
pub const DEFAULT_DEVICE_NAME: &str = "AzMail";

/// The provider of encrypted drives' indexes: the drive's encrypted metadata repository
/// (azul-storage's `meta` module), this computer's copy of it under the cache root.
#[cfg(feature = "encryption")]
fn index_provider() -> Option<Arc<dyn IndexProvider>> {
    let root = INDEX_CACHE_ROOT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let name = DEVICE_NAME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .unwrap_or_else(|| DEFAULT_DEVICE_NAME.to_string());
    Some(Arc::new(
        azul_storage::meta::MetaIndexProvider::new(&name).with_cache_root(root),
    ))
}

/// Names this device in the drive index's commits and conflict copies ("AzMail", "Azlin
/// Bridge"). Set once at start, before a drive is opened.
pub fn set_device_name(name: &str) {
    *DEVICE_NAME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(name.to_string());
}

/// Keeps the drive index's copies of encrypted drives under `root` between runs (`None`: in
/// memory). AzMail sets it once at start.
pub fn set_index_cache_root(root: Option<PathBuf>) {
    *INDEX_CACHE_ROOT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = root;
}

/// `bucket`, the Azlin drive `drive_id`'s, as the mail's drive.
pub fn wrap(
    bucket: Arc<dyn Drive>,
    drive_id: &str,
    keyring: Arc<dyn KeyringStore>,
) -> Arc<dyn Drive> {
    #[cfg(feature = "encryption")]
    {
        Arc::new(AutoEncrypted::new(bucket, drive_id, keyring, index_provider()))
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (drive_id, keyring);
        bucket
    }
}

/// `bucket` as [`wrap`] makes it, with its type: the bridge files drops into it
/// ([`receive_drops_into`]) and serves its files through the same open drive.
#[cfg(feature = "encryption")]
#[must_use]
pub fn wrap_auto(
    bucket: Arc<dyn Drive>,
    drive_id: &str,
    keyring: Arc<dyn KeyringStore>,
) -> Arc<AutoEncrypted> {
    Arc::new(AutoEncrypted::new(bucket, drive_id, keyring, index_provider()))
}

/// Files the drops of the drive `drive_id` (in its plain `bucket`) into its encrypted mail
/// folders and deletes them; returns how many. `open` gives the encrypted drive, asked only when
/// there is a drop key (so a drive without incoming mail costs one keyring look and one read).
/// 0 for a drive whose key this computer does not keep or that has no drop key. A drop that does
/// not open (sealed to an older drop key, damaged) stays in the bucket.
#[cfg(feature = "encryption")]
pub fn file_drops(
    bucket: &dyn Drive,
    drive_id: &str,
    keyring: &dyn KeyringStore,
    open: &mut dyn FnMut() -> Result<Arc<dyn Drive>, DriveError>,
) -> Result<u64, DriveError> {
    let Some(drive_key) = device::load_drive_key(keyring, drive_id)? else {
        return Ok(0);
    };
    let Some(secret) = drops::load_drop_key(bucket, &drive_key, drive_id)? else {
        return Ok(0);
    };
    let drive = open()?;
    let mut delivered = drops::ingest(bucket, &secret, drive_id, &mut |dropped| {
        file_drop(drive.as_ref(), dropped)
    })?
    .delivered;
    // After a key rotation: the drops the Worker sealed to the old drop key before it got the
    // new one.
    if let Some(previous) = drops::load_previous_drop_key(bucket, &drive_key, drive_id)? {
        delivered += drops::ingest(bucket, &previous, drive_id, &mut |dropped| {
            file_drop(drive.as_ref(), dropped)
        })?
        .delivered;
    }
    Ok(delivered)
}

/// [`file_drops`] for a drive opened with [`wrap_auto`]: the drops of its bucket into the same
/// open drive (the bridge, before it lists a folder). 0 for a plain drive.
#[cfg(feature = "encryption")]
pub fn receive_drops_into(auto: &Arc<AutoEncrypted>, keyring: &dyn KeyringStore) -> Result<u64, DriveError> {
    let bucket = Arc::clone(auto.bucket());
    let drive: Arc<dyn Drive> = Arc::clone(auto) as Arc<dyn Drive>;
    file_drops(bucket.as_ref(), auto.drive(), keyring, &mut || Ok(Arc::clone(&drive)))
}

/// Files the drive's incoming-mail drops into its encrypted mail folders (see the module
/// documentation) and deletes them; returns how many. 0 - and no request at all - for a plain
/// drive, a drive whose key this computer does not keep, a build without the feature or without
/// the drive index; 0 after one listing for a drive without incoming mail. A drop that does not
/// open (sealed to an older drop key, damaged) stays in the bucket.
pub fn receive_drops(
    session: &AzlinSession,
    endpoint_override: Option<&str>,
    transport: Box<dyn Transport>,
    keyring: &dyn KeyringStore,
) -> Result<u64, DriveError> {
    #[cfg(feature = "encryption")]
    {
        let Some(provider) = index_provider() else {
            return Ok(0);
        };
        if device::load_drive_key(keyring, &session.drive_id)?.is_none() {
            return Ok(0);
        }
        let bucket: Arc<dyn Drive> = Arc::new(session.open_drive(endpoint_override, transport)?);
        file_drops(bucket.as_ref(), &session.drive_id, keyring, &mut || {
            Ok(Arc::new(open_encrypted(
                Arc::clone(&bucket),
                keyring,
                &session.drive_id,
                provider.as_ref(),
            )?) as Arc<dyn Drive>)
        })
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (session, endpoint_override, transport, keyring);
        Ok(0)
    }
}

/// One drop's message into `drive` (the encrypted drive) at `mail/<Folder>/<stamp>-<hash>.eml`:
/// a name that is there already is this message (a drop filed twice), not an error.
#[cfg(feature = "encryption")]
pub fn file_drop(drive: &dyn Drive, dropped: &Dropped) -> Result<(), DriveError> {
    let name = crate::azlin::object_name(&dropped.raw, dropped.received);
    let key = crate::azlin::message_key(dropped.folder.name(), &name);
    match drive.put_if(&key, &dropped.raw, &Precondition::Absent) {
        Ok(_) | Err(DriveError::Conflict { .. }) => Ok(()),
        Err(e) => Err(e),
    }
}

/// The mail's drive of `session` (its bucket at `endpoint_override` when one is configured),
/// its requests through `transport`, the drive's key (if it is encrypted) from `keyring`.
/// Sends nothing.
pub fn open(
    session: &AzlinSession,
    endpoint_override: Option<&str>,
    transport: Box<dyn Transport>,
    keyring: Arc<dyn KeyringStore>,
) -> Result<Arc<dyn Drive>, DriveError> {
    let bucket = session.open_drive(endpoint_override, transport)?;
    Ok(wrap(Arc::new(bucket), &session.drive_id, keyring))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use azul_storage::{keyring::MemoryKeyring, testing::TempDir, Drive, LocalDrive};

    use super::wrap;

    /// "We always encrypt and compress": the mail core's default build opens an encrypted
    /// account's drive through the encryption (AzMail and the bridge build on it).
    #[test]
    fn a_default_build_of_the_mail_core_goes_through_the_encryption() {
        assert!(
            cfg!(feature = "encryption"),
            "azul-mail-core's default features take `encryption`"
        );
    }

    /// The tests that open drives through the process-wide index provider (its cache root) take
    /// turns: one sets the cache root and counts what lands there.
    #[cfg(feature = "encryption")]
    static PROVIDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(feature = "encryption")]
    fn provider_turn() -> std::sync::MutexGuard<'static, ()> {
        PROVIDER.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn a_plain_drive_carries_the_mail_as_it_is() {
        let tmp = TempDir::new("azmail-mail-drive");
        let bucket = LocalDrive::without_manifest(tmp.path());
        bucket.put("mail/Inbox/1.eml", b"Subject: hi\r\n\r\nhello").unwrap();
        let drive = wrap(Arc::new(bucket), "d_1", Arc::new(MemoryKeyring::new()));
        assert_eq!(
            drive.get("mail/Inbox/1.eml").unwrap(),
            b"Subject: hi\r\n\r\nhello"
        );
        drive.put("mail/Sent/2.eml", b"sent").unwrap();
        assert_eq!(
            std::fs::read(tmp.path().join("mail/Sent/2.eml")).unwrap(),
            b"sent"
        );
    }

    /// With the feature, a drive this computer keeps the key of is never written as plaintext:
    /// over a folder that cannot hold the drive index (`.azlin/` is a folder's own) it is
    /// refused.
    #[cfg(feature = "encryption")]
    #[test]
    fn an_encrypted_drive_is_never_written_as_plaintext() {
        use azul_storage::{
            crypto::{device, DriveKey},
            DriveError,
        };
        let tmp = TempDir::new("azmail-mail-drive-encrypted");
        let keyring = Arc::new(MemoryKeyring::new());
        device::store_drive_key(keyring.as_ref(), "d_1", &DriveKey::generate().unwrap())
            .unwrap();
        let drive = wrap(
            Arc::new(LocalDrive::without_manifest(tmp.path())),
            "d_1",
            keyring,
        );
        assert!(matches!(
            drive.put("mail/Sent/2.eml", b"secret"),
            Err(DriveError::InvalidKey { .. } | DriveError::Unsupported(_))
        ));
        assert!(!tmp.path().join("mail").exists(), "nothing written");
    }

    /// "We always encrypt": the mail of an encrypted drive - new mail, a sent copy, a draft - goes
    /// through the drive's encryption by default: the bucket holds neither its names nor its text.
    #[cfg(feature = "encryption")]
    #[test]
    fn new_mail_of_an_encrypted_drive_reaches_the_bucket_as_ciphertext_only() {
        use azul_storage::{
            crypto::{device, keys::RecoveryKdf},
            meta::MemoryBucket,
        };
        let _turn = provider_turn();
        super::set_index_cache_root(None);
        let keyring = Arc::new(MemoryKeyring::new());
        let bucket = Arc::new(MemoryBucket::new());
        let cheap = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        device::setup_new_drive(bucket.as_ref(), keyring.as_ref(), "d_new", cheap).unwrap();
        let drive = wrap(bucket.clone(), "d_new", keyring);
        let sent = b"Subject: the quarterly plan\r\n\r\nhello\r\n";
        drive
            .put("mail/Sent/20261010T080000Z-0011223344556677.eml", sent)
            .unwrap();
        drive.put("mail/Drafts/20261010T090000Z-8899aabbccddeeff.eml", b"draft").unwrap();
        assert_eq!(
            drive
                .get("mail/Sent/20261010T080000Z-0011223344556677.eml")
                .unwrap(),
            sent
        );
        for (key, bytes) in bucket.objects() {
            assert!(!key.contains("mail") && !key.contains("Sent"), "{key}");
            assert!(
                !bytes.windows(9).any(|w| w == b"quarterly"),
                "{key} holds the mail's text"
            );
        }
    }

    /// With a cache root set, an encrypted drive's index keeps this computer's copy there
    /// between runs (as AzDrive's does).
    #[cfg(feature = "encryption")]
    #[test]
    fn an_encrypted_drives_index_keeps_its_copy_under_the_cache_root() {
        use azul_storage::{
            crypto::{device, keys::RecoveryKdf},
            meta::MemoryBucket,
        };
        let _turn = provider_turn();
        let tmp = TempDir::new("azmail-index-cache");
        super::set_index_cache_root(Some(tmp.path().to_path_buf()));
        let keyring = Arc::new(MemoryKeyring::new());
        let bucket = Arc::new(MemoryBucket::new());
        let cheap = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        device::setup_new_drive(bucket.as_ref(), keyring.as_ref(), "d_cache", cheap).unwrap();
        let drive = wrap(bucket, "d_cache", keyring);
        drive.put("mail/Inbox/1.eml", b"hello").unwrap();
        super::set_index_cache_root(None);
        let kept: Vec<String> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(kept.iter().any(|name| name == "device-id"), "{kept:?}");
        assert_eq!(kept.len(), 2, "the device id and the drive's copy: {kept:?}");
    }

    /// A drop's message lands under its stable name in the encrypted drive, once however often
    /// it is filed.
    #[cfg(feature = "encryption")]
    #[test]
    fn a_drop_is_filed_under_its_stable_name_once() {
        use azul_storage::{
            crypto::{
                drops::{DropFolder, Dropped},
                DriveKey, Zeroizing,
            },
            encrypted::{EncryptedDrive, MemoryIndex},
            ListRequest,
        };
        let tmp = TempDir::new("azmail-mail-drive-drops");
        let drive = EncryptedDrive::new(
            LocalDrive::without_manifest(tmp.path()),
            DriveKey::generate().unwrap(),
            Arc::new(MemoryIndex::new()),
        );
        let raw = b"From: a@example.com\r\nSubject: hi\r\n\r\nhello\r\n".to_vec();
        let dropped = Dropped {
            received: 1_791_619_200,
            folder: DropFolder::Spam,
            raw: Zeroizing::new(raw.clone()),
        };
        super::file_drop(&drive, &dropped).unwrap();
        super::file_drop(&drive, &dropped).unwrap();
        let name = crate::azlin::object_name(&raw, 1_791_619_200);
        assert!(name.starts_with("20261010T080000Z-"), "{name}");
        let key = format!("mail/Spam/{name}");
        assert_eq!(drive.get(&key).unwrap(), raw);
        let listed = drive.list(&ListRequest::recursive("mail/")).unwrap();
        assert_eq!(listed.objects.len(), 1, "one message, filed once");
    }

    /// The bridge's way: the drops in the bucket under an open `AutoEncrypted` drive go into that
    /// same drive, never into the bucket in the clear; a plain drive has none to file.
    #[cfg(feature = "encryption")]
    #[test]
    fn drops_are_filed_into_the_open_drive_they_belong_to() {
        let _turn = provider_turn();
        use azul_storage::{
            crypto::{
                device,
                drops::{self, DropFolder},
                keys::RecoveryKdf,
            },
            meta::MemoryBucket,
            ops::list_all,
        };
        let keyring = Arc::new(MemoryKeyring::new());
        let bucket = Arc::new(MemoryBucket::new());
        let cheap = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        device::setup_new_drive(bucket.as_ref(), keyring.as_ref(), "d_drops", cheap).unwrap();
        let drive_key = device::load_drive_key(keyring.as_ref(), "d_drops")
            .unwrap()
            .unwrap();
        let public = drops::enable_drop(bucket.as_ref(), &drive_key, "d_drops").unwrap();
        let raw = b"From: ben@example.net\r\nSubject: Lunch\r\n\r\nNoon.\r\n";
        let key = drops::new_drop_key().unwrap();
        let sealed =
            drops::seal_drop(&public, "d_drops", &key, 1_791_619_200, DropFolder::Inbox, raw)
                .unwrap();
        bucket.put(&key, &sealed).unwrap();

        let auto = super::wrap_auto(bucket.clone(), "d_drops", keyring.clone());
        assert_eq!(super::receive_drops_into(&auto, keyring.as_ref()).unwrap(), 1);
        let name = crate::azlin::object_name(raw, 1_791_619_200);
        assert_eq!(auto.get(&format!("mail/Inbox/{name}")).unwrap(), raw);
        assert!(
            list_all(bucket.as_ref(), ".azlin/drop/").unwrap().is_empty(),
            "the drop is gone"
        );
        assert!(
            list_all(bucket.as_ref(), "mail/").unwrap().is_empty(),
            "no message in the bucket in the clear"
        );
        assert_eq!(super::receive_drops_into(&auto, keyring.as_ref()).unwrap(), 0);

        let plain = super::wrap_auto(Arc::new(MemoryBucket::new()), "d_plain", keyring.clone());
        assert_eq!(super::receive_drops_into(&plain, keyring.as_ref()).unwrap(), 0);
    }
}
