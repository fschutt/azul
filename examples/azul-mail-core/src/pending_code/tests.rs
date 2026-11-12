//! The tests of [`super`]: the marker, and the order of "Create a new drive" - the marker goes down before
//! the setup publishes anything (X-DELTA-KEYS2-4). A file of its own: the drive-classification test of
//! azul-storage (`tests/drive_coverage.rs`) reads every non-test file of the tree for a drive that wraps
//! another, and the bucket this one watches is a test's.

use azul_storage::testing::TempDir;

use super::*;

/// X-DELTA-KEYS2-4. The marker goes down before the wrap is published and says which drive; a
/// start finds it until the page has taken it away.
#[test]
fn x_delta_keys2_4_a_drive_whose_code_was_never_shown_is_found_by_the_next_start() {
    let dir = TempDir::new("pending-found");
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
    let dir = TempDir::new("pending-unreadable");
    std::fs::write(dir.path().join(FILE), b"{ not json").unwrap();
    assert!(read(dir.path()).is_err(), "{:?}", read(dir.path()));
}

/// The marker holds the drive and nothing else: no code, no key.
#[test]
fn x_delta_keys2_4_the_marker_holds_no_code_and_no_key() {
    let dir = TempDir::new("pending-plain");
    note(dir.path(), "d_made").unwrap();
    let text = std::fs::read_to_string(dir.path().join(FILE)).unwrap();
    assert_eq!(text, r#"{"drive_id":"d_made"}"#);
}

/// A folder that cannot be written is the caller's reason not to publish the wrap: the error comes
/// back, no marker is there.
#[test]
fn x_delta_keys2_4_a_marker_that_cannot_be_written_says_so() {
    let dir = TempDir::new("pending-unwritable");
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
        let dir = TempDir::new("encrypt-order");
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
        let dir = TempDir::new("encrypt-blocked");
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, b"a file").unwrap();
        let bucket = Watching::over(&blocked);
        let keyring = MemoryKeyring::new();
        let error = encrypt_new_drive(&blocked.join("inner"), &bucket, &keyring, "d_made", kdf())
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
        let dir = TempDir::new("encrypt-failed");
        let mut bucket = Watching::over(dir.path());
        bucket.refuses = true;
        let keyring = MemoryKeyring::new();
        let error = encrypt_new_drive(dir.path(), &bucket, &keyring, "d_made", kdf()).unwrap_err();
        assert!(error.contains("not encrypted"), "{error}");
        assert_eq!(read(dir.path()), Ok(None), "the note went with the failure");
    }
}
