//! The drive an Azlin account's mail lives in: the one place AzMail and the Azlin Bridge open it,
//! so the mail objects (`mail/<Folder>/<name>.eml`, the markers under `mail/.state/`) go through
//! the encryption when the drive is encrypted. Plain Rust, no azul types: the keyring comes in.
//!
//! With the feature `encryption` the bucket is wrapped in azul-storage's `AutoEncrypted`: the
//! first call decides whether the drive is encrypted (this computer keeps its key - AzDrive's
//! entry for the same drive id, the keyring is shared - or the bucket holds key files); an
//! encrypted one then goes through the drive index (the bucket's encrypted metadata
//! repository); one that is not encrypted is read and written by NO call (an Azlin drive is
//! encrypted in AzDrive first). Without the feature (a build for tests, never shipped) an Azlin
//! drive is opened through no call at all: every one answers why (X-DELTA-AZDRIVE-KEYS-23) - the
//! bare bucket would hand the mail and the WebDAV files to the storage in the clear.
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
    rotation, Precondition,
};
use azul_storage::{keyring::KeyringStore, meta::RestoreSource, Drive, DriveError, Transport};

use crate::azlin::{AzlinSession, PlainBucket};

/// Where the drive index keeps this computer's copies of encrypted drives
/// ([`set_index_cache_root`]; `None`: in memory, read anew on every start).
static INDEX_CACHE_ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The name the drive index gives this device ([`set_device_name`]; `None`: "AzMail").
static DEVICE_NAME: Mutex<Option<String>> = Mutex::new(None);

/// The device name of a process that never set one.
pub const DEFAULT_DEVICE_NAME: &str = "AzMail";

/// The provider of encrypted drives' indexes: the drive's encrypted metadata repository
/// (azul-storage's `meta` module), this computer's copy of it under the cache root. `restore` is
/// where the drive's restore announcement comes from (the token server's word, as the session's
/// last sign-in answered it): a copy that is kept between runs and has no source refuses a drive
/// the operator restored, for good (X-DELTA-AZDRIVE-SYNC-3: AzMail, the bridge and the command line
/// built the provider without it, and only AzDrive's drives could be restored).
#[cfg(feature = "encryption")]
fn index_provider(restore: RestoreSource) -> Option<Arc<dyn IndexProvider>> {
    let root = INDEX_CACHE_ROOT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let name = DEVICE_NAME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .unwrap_or_else(|| DEFAULT_DEVICE_NAME.to_string());
    Some(Arc::new(azul_storage::meta::MetaIndexProvider::new(
        azul_storage::meta::IndexProfile {
            device_name: name,
            cache_root: root,
            lazy: false,
            packing: false,
            restore,
            reclaim: azul_storage::meta::ReclaimOptions::shadow(),
        },
    )))
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

/// `bucket`, the Azlin drive `drive_id`'s, as the mail's drive. An Azlin drive is encrypted: the
/// mail opens one that is ([`AutoEncrypted::encrypted_only`]) and reads or writes nothing of one
/// that is not (a new drive, an older one, a bucket that hides its key files, which cannot be
/// told apart) - it is encrypted in AzDrive first. `restore` is where the drive's restore
/// announcement comes from ([`crate::azlin::AzlinSession::restore_source`]). `bucket` is the
/// session's ([`PlainBucket`], what [`AzlinSession::open_bucket`] gives) or a drive's bytes in the
/// plain: an `Arc` of the bridge's account drive, of a test's folder (X-DELTA-STRUCT4-6).
pub fn wrap(
    bucket: impl Into<PlainBucket>,
    drive_id: &str,
    keyring: Arc<dyn KeyringStore>,
    restore: RestoreSource,
) -> Arc<dyn Drive> {
    #[cfg(feature = "encryption")]
    {
        wrap_auto(bucket, drive_id, keyring, restore)
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _: PlainBucket = bucket.into();
        let _ = (drive_id, keyring, restore);
        Arc::new(NoEncryption)
    }
}

/// The mail's drive in a build without the encryption: an Azlin drive is encrypted, and this build
/// can open none, so it reads and writes nothing of it - AzDrive's `Slot::open` refuses the same
/// way (`azlin_needs_encryption`). A bare bucket in its place served the mail folders and the
/// WebDAV files in the clear.
#[cfg(not(feature = "encryption"))]
struct NoEncryption;

#[cfg(not(feature = "encryption"))]
impl NoEncryption {
    fn refused() -> DriveError {
        DriveError::Unsupported(String::from(
            "this build has no encryption, and an Azlin drive is encrypted: nothing of it is read \
             or written",
        ))
    }
}

#[cfg(not(feature = "encryption"))]
impl Drive for NoEncryption {
    fn list(&self, _: &azul_storage::ListRequest) -> Result<azul_storage::ListPage, DriveError> {
        Err(NoEncryption::refused())
    }
    fn get(&self, _: &str) -> Result<Vec<u8>, DriveError> {
        Err(NoEncryption::refused())
    }
    fn get_range(&self, _: &str, _: azul_storage::ByteRange) -> Result<Vec<u8>, DriveError> {
        Err(NoEncryption::refused())
    }
    fn put(&self, _: &str, _: &[u8]) -> Result<(), DriveError> {
        Err(NoEncryption::refused())
    }
    fn delete(&self, _: &str) -> Result<(), DriveError> {
        Err(NoEncryption::refused())
    }
    fn head(&self, _: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
        Err(NoEncryption::refused())
    }
}

/// `bucket` as [`wrap`] makes it, with its type: the bridge files drops into it
/// ([`receive_drops_into`]) and serves its files through the same open drive.
#[cfg(feature = "encryption")]
#[must_use]
pub fn wrap_auto(
    bucket: impl Into<PlainBucket>,
    drive_id: &str,
    keyring: Arc<dyn KeyringStore>,
    restore: RestoreSource,
) -> Arc<AutoEncrypted> {
    Arc::new(
        AutoEncrypted::new(
            bucket.into().into_bytes(),
            drive_id,
            keyring,
            index_provider(restore),
        )
        .encrypted_only(),
    )
}

/// Why the drops of a drive were not opened in a round although the drive has a drop key file: the
/// file is no drop key this computer's drive key opens. The mail waits in the bucket - nothing is
/// deleted - and is opened once the file is a key this computer's drive key opens (after a key
/// change that stopped is finished, through the previous drop keys), or, for a damaged file, never by
/// anything this build has. The sync goes on: it is no error of the drive (X-DELTA-KEYS4-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropsWait {
    /// The file is sealed under ANOTHER drive key than the one this computer holds, and no key change
    /// of this computer's own is unfinished: this computer's key is an old one - another computer changed
    /// the drive's keys - and the mail is filed once this computer has the current key.
    OtherKey,
    /// The file is sealed under another drive key than the one this computer holds, and a key change of THIS
    /// computer is not finished (`rotation::unfinished`): it holds the newest key, and the new drop key is
    /// made in the change's last steps - the file is still the one before it. The way out is to finish the
    /// change in AzDrive, not another key (X-DELTA-KEYS5-3).
    KeyChangeUnfinished,
    /// The file names this computer's key and does not open with it: damaged. Nothing will open it - not
    /// now and not in a later round - and the Worker goes on sealing new mail to the public half it was
    /// given: that mail is unreadable until the file is replaced, which `enable_drop` (and so `azcloud
    /// mail-drop`), `disable_drop` and a key change all refuse to do for a file nobody can read
    /// (X-DELTA-KEYS5-3; the deliberate repair is registered as X-DELTA-KEYSTATE6-1).
    Damaged,
}

impl std::fmt::Display for DropsWait {
    /// The sentence a log line says (the windows say it in their own words).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            DropsWait::OtherKey => {
                "incoming mail waits on the drive: its drop key is sealed under another drive key \
                 than this computer holds, so it is filed once this computer has the current key"
            }
            DropsWait::KeyChangeUnfinished => {
                "incoming mail waits on the drive: this computer's own key change is not finished, so \
                 the drive's drop key is still the one before it - finish the key change in AzDrive \
                 (I was hacked: new keys) and the mail is filed; nothing was deleted"
            }
            DropsWait::Damaged => {
                "incoming mail cannot be filed: the drive's drop key file is damaged, so it cannot be \
                 opened, and neither can any mail sent to that key until the file is replaced - this \
                 build cannot replace it; nothing was deleted"
            }
        })
    }
}

/// What filing the drops of a drive did: how many messages were filed, and - when the drive's drop
/// key could not be used this round - why the mail waits ([`DropsWait`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Filed {
    pub delivered: u64,
    pub waiting: Option<DropsWait>,
}

impl Filed {
    /// Nothing filed, nothing waiting: a plain drive, a drive without incoming mail.
    #[must_use]
    pub fn nothing() -> Filed {
        Filed::default()
    }

    /// Nothing filed, and `why` is the reason mail waits in the bucket.
    #[must_use]
    pub fn waiting(why: DropsWait) -> Filed {
        Filed {
            delivered: 0,
            waiting: Some(why),
        }
    }
}

/// Files the drops of the drive `drive_id` (in its plain `bucket`) into its encrypted mail
/// folders and deletes them; says how many ([`Filed`]). `open` gives the encrypted drive, asked
/// only when there is a drop key (so a drive without incoming mail costs one keyring look and one
/// read). Nothing for a drive whose key this computer does not keep or that has no drop key. A drop
/// that does not open (sealed to an older drop key, damaged) stays in the bucket.
///
/// A drop key file that this computer's drive key does not open - sealed under another drive key
/// (a key change of this computer's own, stopped between its Members step and the new drop key, has
/// the newest key and the file of the one before it), or damaged - is no error of the drive: no
/// drop is opened this round, the mail waits in the bucket and the answer says why
/// ([`Filed::waiting`]); the sync of the folders goes on, and the drops are filed once the file is
/// the key's (X-DELTA-KEYS4-1). The writers of the drop key (`enable_drop`, `disable_drop`) still
/// refuse such a file with their typed error: a drop key is never written over what nobody can read.
#[cfg(feature = "encryption")]
pub fn file_drops(
    bucket: &dyn Drive,
    drive_id: &str,
    keyring: &dyn KeyringStore,
    open: &mut dyn FnMut() -> Result<Arc<dyn Drive>, DriveError>,
) -> Result<Filed, DriveError> {
    let Some(drive_key) = device::load_drive_key(keyring, drive_id)? else {
        return Ok(Filed::nothing());
    };
    let secret = match drops::load_drop_key(bucket, &drive_key, drive_id) {
        Ok(Some(secret)) => secret,
        Ok(None) => return Ok(Filed::nothing()),
        // A file sealed under another drive key: a key change of this computer's own that is not finished
        // (it holds the NEWEST key, the file is the one before it) is told from an old key here - the way out
        // is not the same (X-DELTA-KEYS5-3). A keyring that cannot say is the keyring's error, as above.
        Err(DriveError::KeyRefused { .. }) => {
            let wait = if rotation::unfinished(keyring, drive_id)? {
                DropsWait::KeyChangeUnfinished
            } else {
                DropsWait::OtherKey
            };
            return Ok(Filed::waiting(wait));
        }
        Err(DriveError::Corrupt { .. }) => return Ok(Filed::waiting(DropsWait::Damaged)),
        Err(e) => return Err(e),
    };
    let drive = open()?;
    let mut delivered = drops::ingest(bucket, &secret, drive_id, &mut |dropped| {
        file_drop(drive.as_ref(), dropped)
    })?
    .delivered;
    // After key rotations: the drops the Worker sealed to an old drop key before it got the new
    // one - the key of the last rotation and the ones before it (a second rotation before the
    // Worker was updated must not strand the mail sealed to the first key).
    for previous in drops::load_previous_drop_keys(bucket, &drive_key, drive_id)? {
        delivered += drops::ingest(bucket, &previous, drive_id, &mut |dropped| {
            file_drop(drive.as_ref(), dropped)
        })?
        .delivered;
    }
    Ok(Filed {
        delivered,
        waiting: None,
    })
}

/// [`file_drops`] for a drive opened with [`wrap_auto`]: the drops of its bucket into the same
/// open drive (the bridge, before it lists a folder). Nothing for a plain drive.
#[cfg(feature = "encryption")]
pub fn receive_drops_into(
    auto: &Arc<AutoEncrypted>,
    keyring: &dyn KeyringStore,
) -> Result<Filed, DriveError> {
    let bucket = Arc::clone(auto.bucket());
    let drive: Arc<dyn Drive> = Arc::clone(auto) as Arc<dyn Drive>;
    file_drops(bucket.as_ref(), auto.drive(), keyring, &mut || Ok(Arc::clone(&drive)))
}

/// Files the drive's incoming-mail drops into its encrypted mail folders (see the module
/// documentation) and deletes them; says how many ([`Filed`]). Nothing - and no request at all -
/// for a plain drive, a drive whose key this computer does not keep, a build without the feature
/// or without the drive index; nothing after one listing for a drive without incoming mail. A drop
/// that does not open (sealed to an older drop key, damaged) stays in the bucket; a drop key that
/// this computer's key does not open leaves every drop where it is and says so
/// ([`file_drops`]).
pub fn receive_drops(
    session: &AzlinSession,
    endpoint_override: Option<&str>,
    transport: Box<dyn Transport>,
    keyring: &dyn KeyringStore,
) -> Result<Filed, DriveError> {
    #[cfg(feature = "encryption")]
    {
        let Some(provider) = index_provider(session.restore_source()) else {
            return Ok(Filed::nothing());
        };
        if device::load_drive_key(keyring, &session.drive_id)?.is_none() {
            return Ok(Filed::nothing());
        }
        let bucket = session
            .open_bucket(endpoint_override, transport)?
            .into_bytes();
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
        Ok(Filed::nothing())
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
    let bucket = session.open_bucket(endpoint_override, transport)?;
    Ok(wrap(
        bucket,
        &session.drive_id,
        keyring,
        session.restore_source(),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use azul_storage::{
        keyring::MemoryKeyring, meta::RestoreSource, testing::TempDir, Drive, LocalDrive,
    };

    use super::wrap;

    /// A test's drive has no token server to announce a restore.
    fn no_restore() -> RestoreSource {
        RestoreSource::none("a test has no token server to ask")
    }

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

    /// X-DELTA-AZDRIVE-KEYS-23: without the feature the bucket WAS the mail's drive as it is - the
    /// mail folders and the WebDAV files of an Azlin drive in the clear, where AzDrive refuses such a
    /// build at `Slot::open`. It is opened through no call now: every one answers why.
    #[cfg(not(feature = "encryption"))]
    #[test]
    fn x_delta_azdrive_keys_23_a_build_without_the_encryption_reads_and_writes_nothing_of_an_azlin_drive(
    ) {
        use azul_storage::{DriveError, ListRequest};
        let tmp = TempDir::new("azmail-mail-drive");
        let bucket = LocalDrive::without_manifest(tmp.path());
        bucket.put("mail/Inbox/1.eml", b"Subject: hi\r\n\r\nhello").unwrap();
        let drive = wrap(
            Arc::new(bucket),
            "d_1",
            Arc::new(MemoryKeyring::new()),
            no_restore(),
        );
        assert!(matches!(
            drive.get("mail/Inbox/1.eml"),
            Err(DriveError::Unsupported(_))
        ));
        assert!(matches!(
            drive.put("mail/Sent/2.eml", b"sent"),
            Err(DriveError::Unsupported(_))
        ));
        assert!(drive.list(&ListRequest::recursive("")).is_err());
        assert!(drive.head("mail/Inbox/1.eml").is_err());
        assert!(!tmp.path().join("mail/Sent").exists(), "nothing written");
    }

    /// With it, an Azlin drive is encrypted-only (C1/K8, X-AR5b-1): a bucket that lists plaintext
    /// files and no key - a drive made before encryption, or a hostile bucket that hides its key
    /// files, the two look the same to a device that holds no key - is neither read nor written by
    /// the mail until it is encrypted: what it lists is its storage's word alone.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_ar5b_1_a_drive_that_is_not_encrypted_is_neither_read_nor_written_by_the_mail() {
        use azul_storage::DriveError;
        let tmp = TempDir::new("azmail-mail-drive");
        let bucket = LocalDrive::without_manifest(tmp.path());
        bucket.put("mail/Inbox/1.eml", b"Subject: hi\r\n\r\nhello").unwrap();
        let drive = wrap(
            Arc::new(bucket),
            "d_1",
            Arc::new(MemoryKeyring::new()),
            no_restore(),
        );
        assert!(matches!(
            drive.get("mail/Inbox/1.eml"),
            Err(DriveError::Denied { .. })
        ));
        assert!(matches!(
            drive.put("mail/Sent/2.eml", b"sent"),
            Err(DriveError::Denied { .. })
        ));
        assert!(!tmp.path().join("mail/Sent").exists(), "nothing written");
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
            no_restore(),
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
        let drive = wrap(bucket.clone(), "d_new", keyring, no_restore());
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
        let drive = wrap(bucket, "d_cache", keyring, no_restore());
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
        // What an app asks before it turns incoming mail on: the key permit's `DropKey`.
        let permit =
            device::drop_key_permit(bucket.as_ref(), keyring.as_ref(), "d_drops", &drive_key)
                .unwrap();
        let public = drops::enable_drop(&permit, bucket.as_ref(), &drive_key, "d_drops").unwrap();
        let raw = b"From: ben@example.net\r\nSubject: Lunch\r\n\r\nNoon.\r\n";
        let key = drops::new_drop_key().unwrap();
        let sealed =
            drops::seal_drop(&public, "d_drops", &key, 1_791_619_200, DropFolder::Inbox, raw)
                .unwrap();
        bucket.put(&key, &sealed).unwrap();

        let auto = super::wrap_auto(bucket.clone(), "d_drops", keyring.clone(), no_restore());
        assert_eq!(
            super::receive_drops_into(&auto, keyring.as_ref()).unwrap(),
            super::Filed {
                delivered: 1,
                waiting: None
            }
        );
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
        assert_eq!(
            super::receive_drops_into(&auto, keyring.as_ref()).unwrap(),
            super::Filed::nothing()
        );

        let plain = super::wrap_auto(
            Arc::new(MemoryBucket::new()),
            "d_plain",
            keyring.clone(),
            no_restore(),
        );
        assert_eq!(
            super::receive_drops_into(&plain, keyring.as_ref()).unwrap(),
            super::Filed::nothing()
        );
    }

    /// A drive whose incoming mail is on: its key (K1) in the returned keyring, its drop key sealed under K1, one
    /// message the Worker sealed to that drop key waiting in the bucket as a drop.
    #[cfg(feature = "encryption")]
    fn a_drive_with_one_waiting_drop(
        drive: &str,
    ) -> (
        Arc<azul_storage::meta::MemoryBucket>,
        Arc<MemoryKeyring>,
        azul_storage::crypto::DriveKey,
        &'static [u8],
    ) {
        use azul_storage::{
            crypto::{
                device,
                drops::{self, DropFolder},
                keys::RecoveryKdf,
            },
            meta::MemoryBucket,
        };
        let keyring = Arc::new(MemoryKeyring::new());
        let bucket = Arc::new(MemoryBucket::new());
        let cheap = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        device::setup_new_drive(bucket.as_ref(), keyring.as_ref(), drive, cheap).unwrap();
        let k1 = device::load_drive_key(keyring.as_ref(), drive)
            .unwrap()
            .unwrap();
        let permit =
            device::drop_key_permit(bucket.as_ref(), keyring.as_ref(), drive, &k1).unwrap();
        let public = drops::enable_drop(&permit, bucket.as_ref(), &k1, drive).unwrap();
        let raw: &'static [u8] = b"From: ben@example.net\r\nSubject: Lunch\r\n\r\nNoon.\r\n";
        let key = drops::new_drop_key().unwrap();
        let sealed =
            drops::seal_drop(&public, drive, &key, 1_791_619_200, DropFolder::Inbox, raw).unwrap();
        bucket.put(&key, &sealed).unwrap();
        (bucket, keyring, k1, raw)
    }

    /// X-DELTA-KEYS4-1: the device that holds the NEWEST key while its own "I was hacked" is stopped between its
    /// Members step and the new drop key (the keyring's drive key is K2, `drive-key-previous` K1, `drive-key-next`
    /// K2, `_drop.key` is still sealed under K1) meets a drop key file its key does not open. Since 6011d4f59 that
    /// was `Err(KeyRefused)` out of `file_drops`, and AzMail's whole Send/Receive ended on it before the drive was
    /// opened - on a device whose key was NOT replaced - with a sentence that said it was. Now no drop is opened
    /// that round, the drive is not even opened, the drops and the file stay as they are, and the writers of the
    /// drop key still refuse. The answer says it is THIS computer's own key change that is not finished
    /// ([`super::DropsWait::KeyChangeUnfinished`]: the way out is AzDrive's "I was hacked" again, not another key -
    /// X-DELTA-KEYS5-3). After the rotation's last step (the previous drop key, the new `_drop.key`) the mail
    /// waiting in the bucket is filed through the previous drop keys.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_keys4_1_a_drop_key_of_the_key_before_stops_nothing_and_the_mail_is_filed_after_the_rotation_ends(
    ) {
        use azul_storage::{
            crypto::{
                device,
                drops::{self, DROP_KEY_FILE},
                DriveKey,
            },
            ops::list_all,
            DriveError,
        };
        let _turn = provider_turn();
        super::set_index_cache_root(None);
        let drive_id = "d_k41_window";
        let (bucket, keyring, k1, raw) = a_drive_with_one_waiting_drop(drive_id);
        // The device's own key change: the drive key is K2 now, with K1 as the key before and K2 as the new one
        // (R1, R2, R9: the entries a rotation keeps until its last step); it has not replaced the drop key yet.
        let k2 = DriveKey::generate().unwrap();
        device::store_drive_key(keyring.as_ref(), drive_id, &k2).unwrap();
        let (previous_entry, next_entry) = device::rotation_key_entries(drive_id);
        let _previous = device::stash(keyring.as_ref(), &previous_entry, &k1).unwrap();
        let _next = device::stash(keyring.as_ref(), &next_entry, &k2).unwrap();
        assert!(
            azul_storage::rotation::unfinished(keyring.as_ref(), drive_id).unwrap(),
            "premise: this computer's own key change is not finished"
        );
        let drop_key_file = bucket.get(DROP_KEY_FILE).unwrap();
        let opened = std::cell::Cell::new(0u32);
        let filed = super::file_drops(bucket.as_ref(), drive_id, keyring.as_ref(), &mut || {
            opened.set(opened.get() + 1);
            Err(DriveError::Unsupported(String::from(
                "the drive is not opened this round",
            )))
        });
        assert_eq!(
            filed.expect("no error of the drive: the sync goes on"),
            super::Filed::waiting(super::DropsWait::KeyChangeUnfinished)
        );
        assert_eq!(
            opened.get(),
            0,
            "no drop key to ingest with: the drive is not opened for it"
        );
        assert_eq!(
            bucket.get(DROP_KEY_FILE).unwrap(),
            drop_key_file,
            "the file is as it was"
        );
        assert_eq!(
            list_all(bucket.as_ref(), ".azlin/drop/").unwrap().len(),
            1,
            "the drop waits"
        );
        // What the sync does next: the drive opens (the newest key opens its index) and its folders list.
        let drive = wrap(bucket.clone(), drive_id, keyring.clone(), no_restore());
        assert!(
            drive
                .list(&azul_storage::ListRequest::recursive("mail/"))
                .is_ok(),
            "the sync goes on: the drive opens with the key this computer holds"
        );
        // The writers keep their refusal: turning incoming mail on or off is refused while the key change is not
        // over (and for a file of another key).
        assert!(
            matches!(
                device::drop_key_permit(bucket.as_ref(), keyring.as_ref(), drive_id, &k2),
                Err(DriveError::Denied { .. } | DriveError::KeyRefused { .. })
            ),
            "turning incoming mail on or off is still refused"
        );
        // The bridge's way: the same answer, through the open drive.
        let auto = super::wrap_auto(bucket.clone(), drive_id, keyring.clone(), no_restore());
        assert_eq!(
            super::receive_drops_into(&auto, keyring.as_ref()).unwrap(),
            super::Filed::waiting(super::DropsWait::KeyChangeUnfinished)
        );
        // The rotation's last step: the drop key before it is kept under K2 and the new one replaces the file.
        drops::rotate_drop_key(bucket.as_ref(), &k1, &k2, drive_id)
            .unwrap()
            .expect("incoming mail was on");
        assert_eq!(
            super::receive_drops_into(&auto, keyring.as_ref()).unwrap(),
            super::Filed {
                delivered: 1,
                waiting: None
            },
            "the mail that waited is filed through the previous drop key"
        );
        let name = crate::azlin::object_name(raw, 1_791_619_200);
        assert_eq!(auto.get(&format!("mail/Inbox/{name}")).unwrap(), raw);
        assert!(
            list_all(bucket.as_ref(), ".azlin/drop/")
                .unwrap()
                .is_empty(),
            "the drop is gone"
        );
    }

    /// X-DELTA-KEYS5-3 (A): the other half of the sentence. A computer whose key is an OLD one - another computer
    /// changed the drive's keys, and its drop key is sealed under the new key - waits for the current key, and the
    /// answer says so ([`super::DropsWait::OtherKey`]); the computer of the test above, whose OWN key change is
    /// not finished, is told to finish it. One sentence for both told the newest computer that it lacked the key.
    /// The mail is not filed, nothing is deleted, the drive is not opened, and the file is as it was.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_keys5_3_a_computer_with_an_old_key_waits_for_the_current_one_and_says_so() {
        use azul_storage::{
            crypto::{
                device,
                drops::{self, DROP_KEY_FILE},
                DriveKey,
            },
            ops::list_all,
            DriveError,
        };
        let _turn = provider_turn();
        super::set_index_cache_root(None);
        let drive_id = "d_k53_old_key";
        let (bucket, keyring, k1, _raw) = a_drive_with_one_waiting_drop(drive_id);
        // Another computer changed the drive's keys: the drop key is K2's now. This computer still holds K1.
        let k2 = DriveKey::generate().unwrap();
        drops::rotate_drop_key(bucket.as_ref(), &k1, &k2, drive_id)
            .unwrap()
            .expect("incoming mail was on");
        let drop_key_file = bucket.get(DROP_KEY_FILE).unwrap();
        assert!(
            !azul_storage::rotation::unfinished(keyring.as_ref(), drive_id).unwrap(),
            "premise: no key change of this computer is unfinished"
        );
        let opened = std::cell::Cell::new(0u32);
        let filed = super::file_drops(bucket.as_ref(), drive_id, keyring.as_ref(), &mut || {
            opened.set(opened.get() + 1);
            Err(DriveError::Unsupported(String::from(
                "the drive is not opened this round",
            )))
        })
        .expect("no error of the drive: the sync goes on");
        assert_eq!(filed, super::Filed::waiting(super::DropsWait::OtherKey));
        assert_eq!(filed.delivered, 0, "nothing was filed");
        assert_eq!(opened.get(), 0, "the drive is not opened for it");
        assert_eq!(
            bucket.get(DROP_KEY_FILE).unwrap(),
            drop_key_file,
            "the file is as it was"
        );
        assert_eq!(
            list_all(bucket.as_ref(), ".azlin/drop/").unwrap().len(),
            1,
            "the drop waits: nothing was deleted"
        );
        assert!(
            matches!(
                device::drop_key_permit(bucket.as_ref(), keyring.as_ref(), drive_id, &k1),
                Err(DriveError::KeyRefused { .. })
            ),
            "turning incoming mail on or off is refused for a file of another key"
        );
    }

    /// X-DELTA-KEYS5-3, a keyring that cannot say: whether this computer's own key change is unfinished is asked of the
    /// keyring AFTER the drop key file was found to be another key's. A keyring that breaks right there is unknown,
    /// and unknown is neither "old key" nor "unfinished": the answer is the keyring's error - the sync stops on it as
    /// it does when the first read of the keyring fails - and nothing is said of a wait, deleted or filed.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_keys5_3_a_keyring_that_cannot_say_whether_the_key_change_is_unfinished_is_an_error_and_no_wait(
    ) {
        use azul_storage::{
            crypto::{
                device,
                drops::{self, DROP_KEY_FILE},
                DriveKey,
            },
            keyring::{KeyringError, KeyringStore},
            ops::list_all,
            DriveError,
        };
        use std::sync::atomic::{AtomicBool, Ordering};
        /// A keyring that answers like the one it holds until the flag is set.
        struct Breaks {
            inner: Arc<MemoryKeyring>,
            broken: Arc<AtomicBool>,
        }
        impl Breaks {
            fn failing(&self) -> Result<(), KeyringError> {
                if self.broken.load(Ordering::SeqCst) {
                    return Err(KeyringError::Failed(String::from("the keychain is locked")));
                }
                Ok(())
            }
        }
        impl KeyringStore for Breaks {
            fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
                self.failing()?;
                KeyringStore::get(self.inner.as_ref(), key)
            }
            fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
                self.failing()?;
                KeyringStore::set(self.inner.as_ref(), key, secret)
            }
            fn delete(&self, key: &str) -> Result<(), KeyringError> {
                self.failing()?;
                KeyringStore::delete(self.inner.as_ref(), key)
            }
        }
        /// A bucket whose read of the drop key file breaks the keyring: the moment between the two reads.
        struct BreaksTheKeyringAtTheDropKey {
            inner: Arc<azul_storage::meta::MemoryBucket>,
            broken: Arc<AtomicBool>,
        }
        impl Drive for BreaksTheKeyringAtTheDropKey {
            fn list(
                &self,
                request: &azul_storage::ListRequest,
            ) -> Result<azul_storage::ListPage, DriveError> {
                self.inner.list(request)
            }
            fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
                if key == DROP_KEY_FILE {
                    self.broken.store(true, Ordering::SeqCst);
                }
                self.inner.get(key)
            }
            fn get_range(
                &self,
                key: &str,
                range: azul_storage::ByteRange,
            ) -> Result<Vec<u8>, DriveError> {
                self.inner.get_range(key, range)
            }
            fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
                self.inner.put(key, bytes)
            }
            fn delete(&self, key: &str) -> Result<(), DriveError> {
                self.inner.delete(key)
            }
            fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
                self.inner.head(key)
            }
        }
        let _turn = provider_turn();
        super::set_index_cache_root(None);
        let drive_id = "d_k53_keyring";
        let (bucket, keyring, k1, _raw) = a_drive_with_one_waiting_drop(drive_id);
        // Another computer changed the drive's keys: the drop key is K2's now, and this computer holds K1.
        let k2 = DriveKey::generate().unwrap();
        drops::rotate_drop_key(bucket.as_ref(), &k1, &k2, drive_id)
            .unwrap()
            .expect("incoming mail was on");
        let drop_key_file = bucket.get(DROP_KEY_FILE).unwrap();
        let broken = Arc::new(AtomicBool::new(false));
        let breaks = Breaks {
            inner: Arc::clone(&keyring),
            broken: Arc::clone(&broken),
        };
        let watching = BreaksTheKeyringAtTheDropKey {
            inner: Arc::clone(&bucket),
            broken: Arc::clone(&broken),
        };
        // The keyring breaks when the drop key file is read: the question "is a key change of this computer's own
        // unfinished" meets a keyring that cannot answer.
        let filed = super::file_drops(&watching, drive_id, &breaks, &mut || {
            Err(DriveError::Unsupported(String::from("not opened")))
        });
        assert!(
            broken.load(Ordering::SeqCst),
            "premise: the keyring broke when the drop key was read"
        );
        assert!(
            matches!(
                filed,
                Err(DriveError::Io(_) | DriveError::Denied { .. } | DriveError::Unsupported(_))
            ),
            "the keyring's error, not a wait: {filed:?}"
        );
        assert!(device::load_drive_key(keyring.as_ref(), drive_id)
            .unwrap()
            .is_some());
        assert_eq!(
            bucket.get(DROP_KEY_FILE).unwrap(),
            drop_key_file,
            "the file is as it was"
        );
        assert_eq!(
            list_all(bucket.as_ref(), ".azlin/drop/").unwrap().len(),
            1,
            "the drop waits: nothing was deleted or filed"
        );
        // And with a keyring that stays whole the same drive says the wait - the control at the same size.
        broken.store(false, Ordering::SeqCst);
        let whole = Breaks {
            inner: Arc::clone(&keyring),
            broken: Arc::new(AtomicBool::new(false)),
        };
        let filed = super::file_drops(bucket.as_ref(), drive_id, &whole, &mut || {
            Err(DriveError::Unsupported(String::from("not opened")))
        })
        .expect("no error of the drive");
        assert_eq!(filed, super::Filed::waiting(super::DropsWait::OtherKey));
    }

    /// X-DELTA-KEYS5-3: the three waits are three sentences - each says what to do, in the words of the log line
    /// the bridge prints - and the damaged one says it will not be opened and that mail sent to its key is
    /// unreadable until the file is replaced, which nothing here can do (a wait with no end is no "waits").
    #[test]
    fn x_delta_keys5_3_each_wait_has_its_own_sentence_and_the_damaged_one_is_no_wait() {
        let other = super::DropsWait::OtherKey.to_string();
        let unfinished = super::DropsWait::KeyChangeUnfinished.to_string();
        let damaged = super::DropsWait::Damaged.to_string();
        assert!(
            other.contains("another drive key")
                && other.contains("once this computer has the current key")
        );
        assert!(
            unfinished.contains("own key change is not finished") && unfinished.contains("AzDrive"),
            "{unfinished}"
        );
        assert!(
            !unfinished.contains("another drive key"),
            "the newest computer is not told it lacks the key: {unfinished}"
        );
        assert!(
            damaged.contains("damaged")
                && damaged.contains("cannot be opened")
                && damaged.contains("until the file is replaced")
                && damaged.contains("cannot replace it")
                && damaged.contains("nothing was deleted"),
            "{damaged}"
        );
        assert!(
            !damaged.contains("waits"),
            "a file nothing will open does not wait: {damaged}"
        );
        assert!(
            other != unfinished && unfinished != damaged && other != damaged,
            "three different sentences"
        );
    }

    /// X-DELTA-KEYS4-1: a drop key file that names this computer's key and does not open with it (damaged) is no
    /// error of the drive either: the sync goes on, the answer says so, nothing is deleted - and the writers of
    /// the drop key (the permit) still refuse it with their typed error, for nothing may write over a key
    /// nobody can read.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_keys4_1_a_damaged_drop_key_file_stops_nothing_and_the_writers_still_refuse() {
        use azul_storage::{
            crypto::{
                device,
                drops::{DropKeyFile, DROP_KEY_FILE},
            },
            ops::list_all,
            DriveError,
        };
        let _turn = provider_turn();
        super::set_index_cache_root(None);
        let drive_id = "d_k41_damaged";
        let (bucket, keyring, k1, _raw) = a_drive_with_one_waiting_drop(drive_id);
        let mut file = DropKeyFile::parse(&bucket.get(DROP_KEY_FILE).unwrap()).unwrap();
        file.sealed[0] ^= 1;
        bucket.put(DROP_KEY_FILE, &file.to_bytes()).unwrap();
        let damaged = bucket.get(DROP_KEY_FILE).unwrap();
        let opened = std::cell::Cell::new(0u32);
        let filed = super::file_drops(bucket.as_ref(), drive_id, keyring.as_ref(), &mut || {
            opened.set(opened.get() + 1);
            Err(DriveError::Unsupported(String::from(
                "the drive is not opened this round",
            )))
        });
        assert_eq!(
            filed.expect("no error of the drive: the sync goes on"),
            super::Filed::waiting(super::DropsWait::Damaged)
        );
        assert_eq!(opened.get(), 0);
        assert_eq!(
            bucket.get(DROP_KEY_FILE).unwrap(),
            damaged,
            "left as it was"
        );
        assert_eq!(
            list_all(bucket.as_ref(), ".azlin/drop/").unwrap().len(),
            1,
            "the drop waits"
        );
        assert!(
            matches!(
                device::drop_key_permit(bucket.as_ref(), keyring.as_ref(), drive_id, &k1),
                Err(DriveError::Corrupt { .. })
            ),
            "the writers refuse a damaged file: {:?}",
            device::drop_key_permit(bucket.as_ref(), keyring.as_ref(), drive_id, &k1).err()
        );
    }

    /// X-DELTA-KEYS4-1: what is NOT taken for "waiting" stays an error: a drop key read that fails (the bucket
    /// does not answer) is the bucket's own error, the sync stops on it as it did.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_keys4_1_a_drop_key_that_cannot_be_read_is_still_an_error() {
        use azul_storage::{crypto::drops::DROP_KEY_FILE, DriveError};
        struct CutOffRead(Arc<azul_storage::meta::MemoryBucket>);
        impl Drive for CutOffRead {
            fn list(
                &self,
                request: &azul_storage::ListRequest,
            ) -> Result<azul_storage::ListPage, DriveError> {
                self.0.list(request)
            }
            fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
                if key == DROP_KEY_FILE {
                    return Err(DriveError::Transport(String::from(
                        "the connection was cut",
                    )));
                }
                self.0.get(key)
            }
            fn get_range(
                &self,
                key: &str,
                range: azul_storage::ByteRange,
            ) -> Result<Vec<u8>, DriveError> {
                self.0.get_range(key, range)
            }
            fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
                self.0.put(key, bytes)
            }
            fn delete(&self, key: &str) -> Result<(), DriveError> {
                self.0.delete(key)
            }
            fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
                self.0.head(key)
            }
        }
        let _turn = provider_turn();
        let drive_id = "d_k41_cut";
        let (bucket, keyring, _k1, _raw) = a_drive_with_one_waiting_drop(drive_id);
        let cut = CutOffRead(bucket);
        let filed = super::file_drops(&cut, drive_id, keyring.as_ref(), &mut || {
            Err(DriveError::Unsupported(String::from("not asked")))
        });
        assert!(matches!(filed, Err(DriveError::Transport(_))), "{filed:?}");
    }

    /// X-DELTA-AZDRIVE-SYNC-3 (AZ-M4). AzMail keeps this computer's copy of the drive index between
    /// runs; the operator restores the drive from its lagging off-site copy and the token server
    /// announces it in the sign-in. A mail drive made with the session's announcement takes the
    /// restore once and shows the restored folders; one made with no source refuses the drive for
    /// good ("an older index than this device has seen"). RED not run: `wrap` took no source, so
    /// the second case was the only one AzMail had.
    #[cfg(feature = "encryption")]
    #[test]
    fn x_delta_azdrive_sync_3_the_mail_takes_a_restore_its_session_announces_and_refuses_one_nobody_did(
    ) {
        use azul_storage::{
            crypto::{device, keys::RecoveryKdf},
            meta::{keys, MemoryBucket},
            restore::RestoreNotice,
            ListRequest,
        };
        let _turn = provider_turn();
        let tmp = TempDir::new("azmail-restore");
        super::set_index_cache_root(Some(tmp.path().to_path_buf()));
        let keyring = Arc::new(MemoryKeyring::new());
        let bucket = Arc::new(MemoryBucket::new());
        let cheap = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        device::setup_new_drive(bucket.as_ref(), keyring.as_ref(), "d_restore", cheap).unwrap();
        let drive = wrap(bucket.clone(), "d_restore", keyring.clone(), no_restore());
        drive.put("mail/Inbox/1.eml", b"one").unwrap();
        let older = bucket
            .objects()
            .into_iter()
            .find(|(key, _)| key == keys::MANIFEST)
            .map(|(_, bytes)| bytes)
            .expect("the index's manifest");
        drive.put("mail/Inbox/2.eml", b"two").unwrap();
        drop(drive);
        // The operator restores the bucket from the off-site copy that lags: the older manifest.
        bucket.overwrite(keys::MANIFEST, &older);

        let refused = wrap(bucket.clone(), "d_restore", keyring.clone(), no_restore());
        let error = refused
            .list(&ListRequest::recursive("mail/"))
            .expect_err("a restored drive nobody announced is a rollback");
        assert!(error.to_string().contains("older index"), "{error}");

        let mut session = crate::azlin::AzlinSession::with_token("d_restore", "dt_f.0.t");
        session.restore = Some(RestoreNotice {
            epoch: 1,
            ..RestoreNotice::default()
        });
        let taken = wrap(
            bucket.clone(),
            "d_restore",
            keyring,
            session.restore_source(),
        );
        let page = taken.list(&ListRequest::recursive("mail/")).unwrap();
        let files: Vec<&str> = page
            .objects
            .iter()
            .map(|o| o.key.as_str())
            .filter(|key| key.ends_with(".eml"))
            .collect();
        super::set_index_cache_root(None);
        assert_eq!(
            files,
            ["mail/Inbox/1.eml"],
            "the restored drive: the second message was after the off-site copy"
        );
    }
}
