//! Moving a plaintext drive into its encrypted namespace: everything moved and readable, a
//! stopped run resumed without moving a file twice, an interrupted move finished, other
//! contents kept and reported, names an encrypted drive cannot take, a file that changes while
//! it moves.

use std::{
    cell::{Cell, RefCell},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
};

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{device::setup_new_drive, keys::RecoveryKdf, DriveKey, ObjectId},
    encrypted::{EncryptedDrive, Expect, MemoryIndex},
    keyring::MemoryKeyring,
    migrate::{is_plaintext_key, migrate, MigrationState},
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

const MIB: usize = 1 << 20;

fn text(len: usize) -> Vec<u8> {
    b"A plaintext file of the drive before it was encrypted. "
        .iter()
        .copied()
        .cycle()
        .take(len)
        .collect()
}

const MAIL: &[u8] = b"Subject: hello\r\n\r\nthe first mail";
const README: &[u8] = b"# readme";

/// A drive's bucket from before encryption: a mail, a big document, a folder marker, a file at
/// the root.
fn plaintext_bucket() -> Arc<MemBucket> {
    let bucket = Arc::new(MemBucket::new());
    bucket.set("mail/inbox/1.eml", MAIL.to_vec());
    bucket.set("docs/report.txt", text(3 * MIB + 7));
    bucket.set("photos/", Vec::new());
    bucket.set("readme.md", README.to_vec());
    bucket
}

/// The bucket's drive key (the drive set up for encryption).
fn drive_key(bucket: &MemBucket) -> DriveKey {
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    setup_new_drive(bucket, &MemoryKeyring::new(), "d_1", kdf)
        .unwrap()
        .0
}

fn encrypted(bucket: &Arc<MemBucket>) -> EncryptedDrive<Arc<MemBucket>> {
    let key = drive_key(bucket);
    EncryptedDrive::new(bucket.clone(), key, Arc::new(MemoryIndex::new()))
}

fn run(drive: &EncryptedDrive<impl Drive>, state: &mut MigrationState) -> bool {
    migrate(drive, state, &mut |_| Ok(()), &|| false).unwrap()
}

#[test]
fn every_plaintext_file_moves_and_reads_back_through_the_encryption() {
    let bucket = plaintext_bucket();
    let drive = encrypted(&bucket);
    let mut state = MigrationState::default();
    let saves = RefCell::new(Vec::new());
    let done = migrate(
        &drive,
        &mut state,
        &mut |s| {
            saves.borrow_mut().push(s.clone());
            Ok(())
        },
        &|| false,
    )
    .unwrap();
    assert!(done && state.done);
    assert_eq!(state.moved, 4);
    assert_eq!(
        state.bytes,
        (MAIL.len() + 3 * MIB + 7 + README.len()) as u64
    );
    assert!(state.skipped.is_empty());
    assert_eq!(saves.borrow().len(), 5, "after every file, and at the end");

    assert_eq!(drive.get("mail/inbox/1.eml").unwrap(), MAIL);
    assert_eq!(drive.get("docs/report.txt").unwrap(), text(3 * MIB + 7));
    assert_eq!(drive.get("readme.md").unwrap(), README);
    assert!(drive.head("photos/").is_ok(), "the folder marker is in the index");
    for key in bucket.keys() {
        assert!(!is_plaintext_key(&key), "{key} is still plaintext");
    }
    assert_eq!(
        MigrationState::from_json(&state.to_json()).unwrap(),
        state
    );
    // Nothing left to do: a second run moves nothing.
    let mut again = state.clone();
    assert!(run(&drive, &mut again));
    assert_eq!(again.moved, 4);
}

#[test]
fn a_stopped_migration_resumes_from_its_state_without_moving_a_file_twice() {
    let bucket = plaintext_bucket();
    let key = drive_key(&bucket);
    let index = Arc::new(MemoryIndex::new());
    let first = EncryptedDrive::new(bucket.clone(), key.clone(), index.clone());
    let mut state = MigrationState::default();
    let saves = Cell::new(0u32);
    let saved = RefCell::new(String::new());
    let done = migrate(
        &first,
        &mut state,
        &mut |s| {
            saves.set(saves.get() + 1);
            *saved.borrow_mut() = s.to_json();
            Ok(())
        },
        &|| saves.get() >= 2,
    )
    .unwrap();
    assert!(!done);
    assert_eq!(state.moved, 2);

    // Another process, from the state file.
    let mut resumed = MigrationState::from_json(&saved.borrow()).unwrap();
    assert_eq!(resumed.moved, 2);
    let second = EncryptedDrive::new(bucket.clone(), key, index.clone());
    assert!(run(&second, &mut resumed));
    assert_eq!(resumed.moved, 4);
    assert_eq!(index.len(), 4);
    let objects = bucket
        .keys()
        .iter()
        .filter(|k| ObjectId::from_bucket_key(k).is_some())
        .count();
    assert_eq!(objects, 3, "one object per file (the folder needs none)");
}

#[test]
fn a_move_a_crash_interrupted_is_finished_not_written_again() {
    let bucket = plaintext_bucket();
    let drive = encrypted(&bucket);
    // The crash: written encrypted, the plaintext not deleted yet.
    drive.write("readme.md", README, Expect::Absent).unwrap();
    let id = drive.entry("readme.md").unwrap().object.unwrap().id;
    let mut state = MigrationState::default();
    assert!(run(&drive, &mut state));
    assert_eq!(state.moved, 4);
    assert_eq!(
        drive.entry("readme.md").unwrap().object.unwrap().id,
        id,
        "the object the earlier run wrote"
    );
    assert!(bucket.object("readme.md").is_none());
}

#[test]
fn other_encrypted_contents_under_the_same_name_keep_both_and_are_reported_once() {
    let bucket = plaintext_bucket();
    let drive = encrypted(&bucket);
    drive
        .write("readme.md", b"# a newer readme", Expect::Absent)
        .unwrap();
    let mut state = MigrationState::default();
    assert!(run(&drive, &mut state));
    assert!(state.skipped.contains_key("readme.md"), "{state:?}");
    assert_eq!(bucket.object("readme.md").unwrap(), README, "the plaintext stays");
    assert_eq!(drive.get("readme.md").unwrap(), b"# a newer readme");
    assert_eq!(state.moved, 3);

    // The next run does not read it again.
    let ranges = bucket.ranges().len();
    assert!(run(&drive, &mut state));
    assert_eq!(bucket.ranges().len(), ranges);
}

#[test]
fn names_an_encrypted_drive_cannot_take_stay_and_are_reported() {
    let bucket = plaintext_bucket();
    bucket.set("odd//double.txt", b"x".to_vec());
    bucket.set("a/../climb.txt", b"y".to_vec());
    let drive = encrypted(&bucket);
    let mut state = MigrationState::default();
    assert!(run(&drive, &mut state));
    assert_eq!(state.moved, 4);
    assert_eq!(state.skipped.len(), 2, "{state:?}");
    assert!(bucket.object("odd//double.txt").is_some());
    assert!(bucket.object("a/../climb.txt").is_some());
}

/// A bucket where another device rewrites the mail right after its bytes were read.
struct RewrittenWhileRead {
    inner: Arc<MemBucket>,
    rewritten: AtomicBool,
}

impl Drive for RewrittenWhileRead {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let bytes = self.inner.get_range(key, range)?;
        if key == "mail/inbox/1.eml" && !self.rewritten.swap(true, Ordering::SeqCst) {
            self.inner.set(key, b"Subject: hello\r\n\r\nedited".to_vec());
        }
        Ok(bytes)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.inner.put_if(key, bytes, condition)
    }
}

#[test]
fn a_file_that_changes_while_it_moves_keeps_its_new_bytes_and_moves_next_time() {
    let bucket = plaintext_bucket();
    let key = drive_key(&bucket);
    let drive = EncryptedDrive::new(
        RewrittenWhileRead {
            inner: bucket.clone(),
            rewritten: AtomicBool::new(false),
        },
        key,
        Arc::new(MemoryIndex::new()),
    );
    let mut state = MigrationState::default();
    assert!(!run(&drive, &mut state), "not done: the mail changed");
    assert_eq!(state.moved, 3);
    assert!(
        matches!(drive.get("mail/inbox/1.eml"), Err(DriveError::NotFound { .. })),
        "the stale copy was taken back"
    );
    assert_eq!(
        bucket.object("mail/inbox/1.eml").unwrap(),
        b"Subject: hello\r\n\r\nedited"
    );
    assert!(run(&drive, &mut state));
    assert_eq!(state.moved, 4);
    assert_eq!(
        drive.get("mail/inbox/1.eml").unwrap(),
        b"Subject: hello\r\n\r\nedited"
    );
}

#[test]
fn only_a_drives_own_files_are_plaintext() {
    assert!(is_plaintext_key("mail/inbox/1.eml"));
    assert!(is_plaintext_key("data/notes.txt"), "a folder of the user named data");
    assert!(!is_plaintext_key(".azlin/keys/recovery.key"));
    assert!(!is_plaintext_key(".azlin/meta/manifest"));
    let id = ObjectId::generate().unwrap();
    assert!(!is_plaintext_key(&id.bucket_key()));
    assert!(MigrationState::from_json("{\"format\": \"other\", \"moved\": 0, \"bytes\": 0}").is_err());
}

/// A bucket where another device saves a new version of the mail right after the migration looked
/// at it for the last time: the window between its HEAD and its DELETE.
struct SavedAfterTheLook {
    inner: Arc<MemBucket>,
    saved: AtomicBool,
}

impl Drive for SavedAfterTheLook {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let info = self.inner.head(key)?;
        // The look that decides says "unchanged"; right after it the other device saves.
        if key == "mail/inbox/1.eml" && !self.saved.swap(true, Ordering::SeqCst) {
            self.inner.set(
                key,
                b"Subject: hello\r\n\r\nwritten by another device".to_vec(),
            );
        }
        Ok(info)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.inner.put_if(key, bytes, condition)
    }
}

/// X-DELTA-SYNC4-16. The migration deleted the plaintext object by name after a HEAD that compared
/// its tag, size and date: a file another device saved under the name inside the round trip between
/// the HEAD and the DELETE was deleted. The DELETE names the version the listing showed; a changed
/// object stays, the encrypted copy of the old bytes goes, and the next run moves what is there. RED
/// on the old code: the bytes the other device wrote are gone from the bucket.
#[test]
fn x_delta_sync4_16_a_file_saved_between_the_last_look_and_the_delete_is_not_deleted() {
    let bucket = plaintext_bucket();
    let key = drive_key(&bucket);
    let drive = EncryptedDrive::new(
        SavedAfterTheLook {
            inner: bucket.clone(),
            saved: AtomicBool::new(false),
        },
        key,
        Arc::new(MemoryIndex::new()),
    );
    let mut state = MigrationState::default();
    let done = run(&drive, &mut state);
    assert_eq!(
        bucket
            .object("mail/inbox/1.eml")
            .expect("the plaintext object the other device saved stays"),
        b"Subject: hello\r\n\r\nwritten by another device"
    );
    assert!(!done, "not done: the mail changed");
    assert!(
        matches!(
            drive.get("mail/inbox/1.eml"),
            Err(DriveError::NotFound { .. })
        ),
        "the encrypted copy of the old bytes was taken back"
    );
    // The next run moves what is there now.
    assert!(run(&drive, &mut state));
    assert_eq!(
        drive.get("mail/inbox/1.eml").unwrap(),
        b"Subject: hello\r\n\r\nwritten by another device"
    );
}

/// A bucket whose first look at the mail after the migration wrote its encrypted copy is the moment
/// another device, with the drive open encrypted already, saves the same name encrypted - and the
/// plaintext object changes too, so that this run takes its own copy back.
struct EncryptedElsewhere {
    inner: Arc<MemBucket>,
    other: EncryptedDrive<Arc<MemBucket>>,
    fired: AtomicBool,
}

impl Drive for EncryptedElsewhere {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let info = self.inner.head(key)?;
        if key == "mail/inbox/1.eml" && !self.fired.swap(true, Ordering::SeqCst) {
            self.other
                .put(key, b"written encrypted by another device")
                .unwrap();
            self.inner
                .set(key, b"Subject: hello\r\n\r\nedited meanwhile".to_vec());
        }
        Ok(info)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.inner.put_if(key, bytes, condition)
    }
}

/// X-DELTA-SYNC6-8. The migration took its own encrypted copy back by NAME (`Expect::Any`) when the
/// plaintext object changed: another device that already had the drive open encrypted, and saved the
/// same name after our write landed, lost its newer entry (an index without history releases the
/// object at once). The undo names the object this run wrote: the other device's entry stays, the
/// plaintext stays for the next run. RED on the old code: the other device's file is gone.
#[test]
fn x_delta_sync6_8_the_undo_of_the_migrations_copy_never_removes_another_devices_newer_entry() {
    let bucket = plaintext_bucket();
    let key = drive_key(&bucket);
    let index: Arc<MemoryIndex> = Arc::new(MemoryIndex::new());
    let other = EncryptedDrive::new(bucket.clone(), key.clone(), index.clone());
    let drive = EncryptedDrive::new(
        EncryptedElsewhere {
            inner: bucket.clone(),
            other,
            fired: AtomicBool::new(false),
        },
        key.clone(),
        index.clone(),
    );
    let mut state = MigrationState::default();
    let done = run(&drive, &mut state);
    assert!(!done, "not done: the mail changed while it moved");
    let elsewhere = EncryptedDrive::new(bucket.clone(), key, index);
    assert_eq!(
        elsewhere
            .get("mail/inbox/1.eml")
            .expect("the entry the other device saved is still named"),
        b"written encrypted by another device"
    );
    assert_eq!(
        bucket
            .object("mail/inbox/1.eml")
            .expect("the plaintext stays for the next run"),
        b"Subject: hello\r\n\r\nedited meanwhile"
    );
}

/// A bucket where another device saves a new version of the mail right after the SECOND look at it:
/// the HEAD inside the delete, the round trip before the DELETE itself.
struct SavedInsideTheDelete {
    inner: Arc<MemBucket>,
    looks: AtomicU32,
}

impl Drive for SavedInsideTheDelete {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let info = self.inner.head(key)?;
        if key == "mail/inbox/1.eml" && self.looks.fetch_add(1, Ordering::SeqCst) == 1 {
            self.inner.set(
                key,
                b"Subject: hello\r\n\r\nsaved inside the round trip".to_vec(),
            );
        }
        Ok(info)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.inner.put_if(key, bytes, condition)
    }
}

/// X-DELTA-SYNC6-4 - a PIN, not a fix: what the migration's version delete does and does not
/// guarantee on a bucket. The delete checks the tag with a HEAD and then deletes (a general-purpose
/// bucket takes no condition on a delete; the trait's `delete_many` and S3's DeleteObjects are both
/// check-then-act), so the window is the one round trip between the HEAD and the DELETE. A save
/// BEFORE the HEAD is caught (`x_delta_sync4_16_*`); a save INSIDE the round trip goes with the
/// object, and the encrypted copy of the old bytes is the only one left. The window is named in the
/// doc of `delete_unchanged` and in the module's premise (the other devices have stopped writing
/// plaintext). When a bucket with a conditional delete is used, this test is the one to turn around.
#[test]
fn x_delta_sync6_4_pin_a_save_inside_the_round_trip_of_the_versioned_delete_is_not_guarded() {
    let bucket = plaintext_bucket();
    let key = drive_key(&bucket);
    let drive = EncryptedDrive::new(
        SavedInsideTheDelete {
            inner: bucket.clone(),
            looks: AtomicU32::new(0),
        },
        key,
        Arc::new(MemoryIndex::new()),
    );
    let mut state = MigrationState::default();
    assert!(run(&drive, &mut state), "every file moved");
    assert_eq!(
        bucket.object("mail/inbox/1.eml"),
        None,
        "PINNED (X-DELTA-SYNC6-4): the object another device saved inside the round trip between the \
         HEAD and the DELETE went with the old one - the delete is check-then-act on a bucket"
    );
    assert_eq!(
        drive.get("mail/inbox/1.eml").unwrap(),
        MAIL,
        "and the encrypted copy holds the bytes the migration read"
    );
}
