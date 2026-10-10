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

use std::sync::Arc;

#[cfg(feature = "encryption")]
use azul_storage::encrypted::{AutoEncrypted, IndexProvider};
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
}
