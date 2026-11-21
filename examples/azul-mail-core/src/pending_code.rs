//! The recovery code of a drive AzMail made and may not have shown (X-DELTA-KEYS2-4).
//!
//! "Create a new drive" makes the drive at the token server and encrypts it as it is made: the
//! recovery wrap goes into the bucket and the drive key into the keyring, and only then does the code
//! reach the page, shown once and stored nowhere. A stop between the two - the app killed, the window
//! closed, the computer off - leaves an encrypted drive whose recovery code nobody saw. The order is
//! record first, publish after (the order AzDrive's "I was hacked" has: it saves that the wrap may
//! change before the job that changes it): [`note`] writes a marker BEFORE the wrap is published,
//! [`clear`] takes it away once the USER has said they saved the code ("I saved the code" on the
//! account page - the page has the code long before: closing the window or killing the app between
//! would leave a drive whose code nobody has and no marker to say so, X-DELTA-SYNC4-15), and the next
//! start [`read`]s it - a marker that is still there says the code was never confirmed, and the
//! notice names the drive.
//!
//! The marker holds no code and no key: the keys are in the keyring on this computer, and the drive is
//! empty. It says which drive, so that nothing is lost but the paper the code would have been written
//! on - and the user is told so.

use std::path::{Path, PathBuf};

use azul_storage::durable::{self, Mode};
use serde::{Deserialize, Serialize};

#[cfg(feature = "encryption")]
use azul_storage::{crypto::keys::RecoveryKdf, keyring::KeyringStore};

#[cfg(feature = "encryption")]
use crate::azlin::PlainBucket;

/// The marker's file in AzMail's data folder.
pub const FILE: &str = "pending-recovery-code.json";

/// What the marker says: the drive whose recovery code may not have been shown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    /// The drive the token server made.
    pub drive_id: String,
    /// The account page showed the code ([`shown`]): the marker also outlives a user who wrote the
    /// code down and pressed Finish without saying so, and what the next start says must then be
    /// true of THAT case - the code was shown and nobody confirmed it - not "was never shown"
    /// (X-DELTA-APPS3-5). A marker without the field (written before it existed, when the page took
    /// the marker away at once) says the code was never shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shown: bool,
}

fn path_of(root: &Path) -> PathBuf {
    root.join(FILE)
}

/// Writes the marker for `drive_id`: the code of this drive is about to be made and may never be
/// shown. Called BEFORE the wrap is published; a failure is the caller's reason not to publish it.
///
/// # Errors
///
/// The sentence for a marker that could not be written (the folder, the disk).
pub fn note(root: &Path, drive_id: &str) -> Result<(), String> {
    let text = serde_json::to_vec(&Pending {
        drive_id: drive_id.to_string(),
        shown: false,
    })
    .map_err(|e| e.to_string())?;
    durable::replace(&path_of(root), &text, Mode::Private)
        .map_err(|e| format!("{}: {e}", path_of(root).display()))
}

/// Encrypts the new drive `drive_id` as it is made - the recovery wrap into `bucket` (the session's bytes, plain:
/// [`PlainBucket`], what the setup of a drive's key files works on and a mail folder never does, X-DELTA-STRUCT4-6), the drive
/// key and this computer's member key into `keyring` - and hands back the recovery code, for the page to show once. Recorded first, published after (X-DELTA-KEYS2-4): the marker ([`note`]) is written before
/// anything of the setup reaches the bucket or the keyring, so that a stop between the wrap and the
/// page is found by the next start; a marker that cannot be written is the reason to publish nothing.
/// A setup that fails takes the marker away again (the page reports it as "not encrypted"); one that
/// succeeds leaves it for [`clear`], which the account page asks for once the user has said they saved
/// the code.
///
/// # Errors
///
/// The sentence for a drive that was made and is not encrypted, nothing published when the marker
/// could not be written.
#[cfg(feature = "encryption")]
pub fn encrypt_new_drive(
    root: &Path,
    bucket: &PlainBucket,
    keyring: &dyn KeyringStore,
    drive_id: &str,
    kdf: RecoveryKdf,
) -> Result<String, String> {
    note(root, drive_id).map_err(|why| {
        format!(
            "the drive was made but not encrypted: a note that its recovery code may not be shown \
             could not be kept ({why}), so nothing was published"
        )
    })?;
    match azul_storage::crypto::device::setup_new_drive(
        bucket.bytes().as_ref(),
        keyring,
        drive_id,
        kdf,
    ) {
        Ok((_, code)) => Ok(code.to_text().to_string()),
        Err(e) => {
            // The page reports a setup that failed as "not encrypted": the drive is one the user was
            // told is no good, and the note would tell the next start of a code never shown for it.
            // collapse-ok: a note that stays costs one notice at the next start.
            let _ = clear(root);
            Err(format!("the drive was made but not encrypted: {e}"))
        }
    }
}

/// Records that the account page now HAS the code of `drive_id` (the marker stays: only the user's
/// "I saved the code" takes it away, [`clear`]). A marker that is not there is left not there - the
/// user said so already - and so is one of another drive. Written like the marker itself.
///
/// # Errors
///
/// The sentence for a marker that cannot be read or written.
pub fn shown(root: &Path, drive_id: &str) -> Result<(), String> {
    match read(root)? {
        Some(pending) if pending.drive_id == drive_id => {
            let text = serde_json::to_vec(&Pending {
                shown: true,
                ..pending
            })
            .map_err(|e| e.to_string())?;
            durable::replace(&path_of(root), &text, Mode::Private)
                .map_err(|e| format!("{}: {e}", path_of(root).display()))
        }
        _ => Ok(()),
    }
}

/// Takes the marker away: the user has said they saved the code. A marker that is not there is as good.
///
/// # Errors
///
/// The sentence for a marker that is there and could not be removed.
pub fn clear(root: &Path) -> Result<(), String> {
    match std::fs::remove_file(path_of(root)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path_of(root).display())),
    }
}

/// The marker a start finds: the drive whose code may not have been shown, or `None` when no
/// marker is there.
///
/// # Errors
///
/// The sentence for a marker that is there and cannot be read - an unreadable marker is a marker:
/// it is never taken for "nothing pending".
pub fn read(root: &Path) -> Result<Option<Pending>, String> {
    let path = path_of(root);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<Pending>(&bytes)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests;
