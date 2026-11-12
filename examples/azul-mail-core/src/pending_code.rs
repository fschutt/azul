//! The recovery code of a drive AzMail made and may not have shown (X-DELTA-KEYS2-4).
//!
//! "Create a new drive" makes the drive at the token server and encrypts it as it is made: the
//! recovery wrap goes into the bucket and the drive key into the keyring, and only then does the code
//! reach the page, shown once and stored nowhere. A stop between the two - the app killed, the window
//! closed, the computer off - leaves an encrypted drive whose recovery code nobody saw. The order is
//! record first, publish after (the order AzDrive's "I was hacked" has: it saves that the wrap may
//! change before the job that changes it): [`note`] writes a marker BEFORE the wrap is published,
//! [`clear`] takes it away once the page has the code, and the next start [`read`]s it - a marker that
//! is still there says the code was never shown, and the notice names the drive.
//!
//! The marker holds no code and no key: the keys are in the keyring on this computer, and the drive is
//! empty. It says which drive, so that nothing is lost but the paper the code would have been written
//! on - and the user is told so.

use std::path::{Path, PathBuf};

use azul_storage::durable::{self, Mode};
use serde::{Deserialize, Serialize};

#[cfg(feature = "encryption")]
use azul_storage::{crypto::keys::RecoveryKdf, keyring::KeyringStore, Drive};

/// The marker's file in AzMail's data folder.
pub const FILE: &str = "pending-recovery-code.json";

/// What the marker says: the drive whose recovery code may not have been shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    /// The drive the token server made.
    pub drive_id: String,
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
    })
    .map_err(|e| e.to_string())?;
    durable::replace(&path_of(root), &text, Mode::Private)
        .map_err(|e| format!("{}: {e}", path_of(root).display()))
}

/// Encrypts the new drive `drive_id` as it is made - the recovery wrap into `bucket`, the drive key and
/// this computer's member key into `keyring` - and hands back the recovery code, for the page to show
/// once. Recorded first, published after (X-DELTA-KEYS2-4): the marker ([`note`]) is written before
/// anything of the setup reaches the bucket or the keyring, so that a stop between the wrap and the
/// page is found by the next start; a marker that cannot be written is the reason to publish nothing.
/// A setup that fails takes the marker away again (the page reports it as "not encrypted"); one that
/// succeeds leaves it for [`clear`], which the page asks for once it has the code.
///
/// # Errors
///
/// The sentence for a drive that was made and is not encrypted, nothing published when the marker
/// could not be written.
#[cfg(feature = "encryption")]
pub fn encrypt_new_drive(
    root: &Path,
    bucket: &dyn Drive,
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
    match azul_storage::crypto::device::setup_new_drive(bucket, keyring, drive_id, kdf) {
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

/// Takes the marker away: the page has the code. A marker that is not there is as good.
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
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    /// X-DELTA-KEYS2-4. The marker goes down before the wrap is published and says which drive; a
    /// start finds it until the page has taken it away.
    #[test]
    fn x_delta_keys2_4_a_drive_whose_code_was_never_shown_is_found_by_the_next_start() {
        let dir = TempDir::new("azmail-pending-found");
        let root = dir.path();
        assert_eq!(read(root), Ok(None), "nothing pending");
        note(root, "d_made").unwrap();
        assert_eq!(
            read(root).unwrap(),
            Some(Pending {
                drive_id: String::from("d_made")
            })
        );
        clear(root).unwrap();
        assert_eq!(read(root), Ok(None), "the page took it away");
        clear(root).unwrap();
    }

    /// A marker nobody can read is a marker, not "nothing pending": the start says there is something
    /// it cannot read, it never reads a failed read as a clean one.
    #[test]
    fn x_delta_keys2_4_an_unreadable_marker_is_not_nothing_pending() {
        let dir = TempDir::new("azmail-pending-unreadable");
        std::fs::write(dir.path().join(FILE), b"{ not json").unwrap();
        assert!(read(dir.path()).is_err(), "{:?}", read(dir.path()));
    }

    /// The marker holds the drive and nothing else: no code, no key.
    #[test]
    fn x_delta_keys2_4_the_marker_holds_no_code_and_no_key() {
        let dir = TempDir::new("azmail-pending-plain");
        note(dir.path(), "d_made").unwrap();
        let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
        assert_eq!(text, r#"{"drive_id":"d_made"}"#);
    }

    /// A folder that cannot be written is the caller's reason not to publish the wrap: the error comes
    /// back, no marker is there.
    #[test]
    fn x_delta_keys2_4_a_marker_that_cannot_be_written_says_so() {
        let dir = TempDir::new("azmail-pending-unwritable");
        // A file where the folder should be.
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, b"a file").unwrap();
        assert!(note(&blocked.join("inner"), "d_made").is_err());
    }

    #[cfg(feature = "encryption")]
    mod setup {
        use std::sync::Mutex;

        use azul_storage::{
            keyring::MemoryKeyring, meta::MemoryBucket, Drive, DriveError, Precondition,
        };

        use super::*;

        /// A bucket that notes, at every write it is given, whether the marker of the new drive's
        /// recovery code was on the disk already: the order "record first, publish after" as the bucket
        /// sees it.
        struct Watching {
            inner: MemoryBucket,
            root: PathBuf,
            marker_at_each_write: Mutex<Vec<bool>>,
            /// A write fails (a setup that cannot finish).
            refuses: bool,
        }

        impl Watching {
            fn over(root: &Path) -> Watching {
                Watching {
                    inner: MemoryBucket::new(),
                    root: root.to_path_buf(),
                    marker_at_each_write: Mutex::new(Vec::new()),
                    refuses: false,
                }
            }

            fn saw(&self) {
                let marked = matches!(read(&self.root), Ok(Some(_)));
                self.marker_at_each_write.lock().unwrap().push(marked);
            }
        }

        impl azul_storage::DriveDelegate for Watching {
            fn below(&self) -> &dyn Drive {
                &self.inner
            }
            fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
                self.saw();
                if self.refuses {
                    return Err(DriveError::Transport(String::from(
                        "the connection dropped",
                    )));
                }
                self.inner.put(key, bytes)
            }
            fn put_if(
                &self,
                key: &str,
                bytes: &[u8],
                condition: &Precondition,
            ) -> Result<Option<String>, DriveError> {
                self.saw();
                if self.refuses {
                    return Err(DriveError::Transport(String::from(
                        "the connection dropped",
                    )));
                }
                self.inner.put_if(key, bytes, condition)
            }
        }

        azul_storage::delegate_drive!(Watching);

        fn kdf() -> RecoveryKdf {
            RecoveryKdf::with_cost(64, 1, 1).unwrap()
        }

        /// X-DELTA-KEYS2-4. The setup of a new drive used to publish the recovery wrap and only then
        /// hand the code to the page, with nothing written down in between: a stop there left a
        /// drive whose code nobody saw. The marker goes down first: every write the setup makes
        /// into the bucket finds it on the disk, and when the setup is done it is still there for the
        /// page to take away.
        #[test]
        fn x_delta_keys2_4_the_marker_is_on_the_disk_before_the_first_write_of_the_setup() {
            let dir = TempDir::new("azmail-encrypt-order");
            let bucket = Watching::over(dir.path());
            let keyring = MemoryKeyring::new();
            let code = encrypt_new_drive(dir.path(), &bucket, &keyring, "d_made", kdf()).unwrap();
            assert!(!code.is_empty(), "the code for the page");
            let seen = bucket.marker_at_each_write.lock().unwrap().clone();
            assert!(!seen.is_empty(), "the setup wrote into the bucket");
            assert!(
                seen.iter().all(|marked| *marked),
                "a write reached the bucket before the marker was written: {seen:?}"
            );
            assert_eq!(
                read(dir.path()).unwrap(),
                Some(Pending {
                    drive_id: String::from("d_made")
                }),
                "the page has not taken it yet"
            );
            clear(dir.path()).unwrap();
            assert_eq!(read(dir.path()), Ok(None));
        }

        /// A marker that cannot be written is the reason to publish nothing: no object reached the
        /// bucket and no key the keyring.
        #[test]
        fn x_delta_keys2_4_a_marker_that_cannot_be_written_stops_the_setup_before_it_publishes_anything(
        ) {
            let dir = TempDir::new("azmail-encrypt-blocked");
            let blocked = dir.path().join("blocked");
            std::fs::write(&blocked, b"a file").unwrap();
            let bucket = Watching::over(&blocked);
            let keyring = MemoryKeyring::new();
            let error =
                encrypt_new_drive(&blocked.join("inner"), &bucket, &keyring, "d_made", kdf())
                    .unwrap_err();
            assert!(error.contains("nothing was published"), "{error}");
            assert!(
                bucket.marker_at_each_write.lock().unwrap().is_empty(),
                "no write reached the bucket"
            );
            assert!(
                bucket.inner.objects().is_empty(),
                "nothing is in the bucket"
            );
            assert!(keyring.names().is_empty(), "nothing is in the keyring");
        }

        /// A setup that fails takes the marker away: the page reports the drive as not encrypted, and
        /// the next start must not say a code was never shown for a drive nobody was told is fine.
        #[test]
        fn x_delta_keys2_4_a_setup_that_fails_takes_its_marker_back() {
            let dir = TempDir::new("azmail-encrypt-failed");
            let mut bucket = Watching::over(dir.path());
            bucket.refuses = true;
            let keyring = MemoryKeyring::new();
            let error =
                encrypt_new_drive(dir.path(), &bucket, &keyring, "d_made", kdf()).unwrap_err();
            assert!(error.contains("not encrypted"), "{error}");
            assert_eq!(read(dir.path()), Ok(None), "the note went with the failure");
        }
    }
}
