//! The drive an Azlin account's mail lives in: the one place AzMail opens it, so the mail
//! objects (`mail/<Folder>/<name>.eml`, the markers under `mail/.state/`) go through the
//! encryption when the drive is encrypted. Plain Rust, no azul types: the keyring comes in.
//!
//! With the feature `encryption` (off until the drive index - the bucket's encrypted metadata
//! repository - is in) the bucket is wrapped in azul-storage's `AutoEncrypted`: the first call
//! decides whether the drive is encrypted (this computer keeps its key - AzDrive's entry for the
//! same drive id, the keyring is shared - or the bucket holds key files); an encrypted one then
//! needs the drive index, which this build does not have yet, and says so; a plain one is used
//! as it is. Without the feature the bucket is used as it is, as before.
//!
//! Incoming mail of an encrypted drive arrives as DROPS: the customer's Cloudflare Email Worker
//! seals each message to the drive's drop key (azul-storage's `crypto::drops`, AZD1) and puts it
//! at `.azlin/drop/<random>`. [`receive_drops`] (Send/Receive, before the folders sync) files
//! them into the drive's encrypted `mail/<Folder>/` under their stable names and deletes them.

use std::sync::Arc;

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

/// The provider of encrypted drives' indexes: `None` until the drive index is in.
#[cfg(feature = "encryption")]
fn index_provider() -> Option<Arc<dyn IndexProvider>> {
    None
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
        let Some(drive_key) = device::load_drive_key(keyring, &session.drive_id)? else {
            return Ok(0);
        };
        let bucket: Arc<dyn Drive> = Arc::new(session.open_drive(endpoint_override, transport)?);
        let Some(secret) = drops::load_drop_key(bucket.as_ref(), &drive_key, &session.drive_id)?
        else {
            return Ok(0);
        };
        let drive = open_encrypted(
            Arc::clone(&bucket),
            keyring,
            &session.drive_id,
            provider.as_ref(),
        )?;
        let mut delivered = drops::ingest(bucket.as_ref(), &secret, &session.drive_id, &mut |dropped| {
            file_drop(&drive, dropped)
        })?
        .delivered;
        // After a key rotation: the drops the Worker sealed to the old drop key before it got
        // the new one.
        if let Some(previous) =
            drops::load_previous_drop_key(bucket.as_ref(), &drive_key, &session.drive_id)?
        {
            delivered += drops::ingest(bucket.as_ref(), &previous, &session.drive_id, &mut |dropped| {
                file_drop(&drive, dropped)
            })?
            .delivered;
        }
        Ok(delivered)
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
pub(crate) fn file_drop(drive: &dyn Drive, dropped: &Dropped) -> Result<(), DriveError> {
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
    /// without the drive index it is refused.
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
            Err(DriveError::Unsupported(_))
        ));
        assert!(!tmp.path().join("mail").exists(), "nothing written");
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
}
