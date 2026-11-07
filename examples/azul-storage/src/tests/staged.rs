//! A new data object between its upload and the index change that names it (ARCH28 RF44, the
//! class of the lost-answer flaw; X-AR5a-15, AZ-C13): it is taken back out of the bucket only when
//! the change was refused for good, and the question whether the change landed is put to the
//! drive, never to a copy of it. The four places that used to carry a compensation of their own -
//! `bind`, `rewrite`, `promote` and the batch of large files - each get a test.

use std::{
    io::Cursor,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::mem_bucket::MemBucket;
use crate::{
    crypto::DriveKey,
    encrypted::{
        EncryptedDrive, Expect, IndexChange, IndexEntry, IndexPage, MemoryIndex, NameIndex, Rewrite,
    },
    meta::Certainty,
    packing::PackOptions,
    BatchStream, ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
    ServiceError,
};

/// An index whose change lands and whose answer says "refused" (a lost answer whose repeat met the
/// version it had made), and whose cheap `get` answers from a copy that has not heard of the
/// change. `get_fresh` is the drive itself.
struct LandedAndDenied {
    inner: MemoryIndex,
    /// The next `apply` lands, and answers `Conflict`.
    deny_next: AtomicBool,
    /// `get` answers "nothing there" while this holds.
    stale: AtomicBool,
}

impl LandedAndDenied {
    fn new() -> LandedAndDenied {
        LandedAndDenied {
            inner: MemoryIndex::new(),
            deny_next: AtomicBool::new(false),
            stale: AtomicBool::new(false),
        }
    }

    /// The next change lands and answers `Conflict`; from then on the copy has not heard of it
    /// (until [`LandedAndDenied::recover`]).
    fn lose_the_answer(&self) {
        self.deny_next.store(true, Ordering::SeqCst);
    }

    fn recover(&self) {
        self.stale.store(false, Ordering::SeqCst);
    }
}

impl NameIndex for LandedAndDenied {
    fn get(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        if self.stale.load(Ordering::SeqCst) {
            return Ok(None);
        }
        self.inner.get(path)
    }

    fn get_fresh(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        self.inner.get(path)
    }

    fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError> {
        self.inner.list(request)
    }

    fn apply(&self, changes: Vec<IndexChange>) -> Result<Vec<crate::crypto::ObjectId>, DriveError> {
        let key = match changes.first() {
            Some(IndexChange::Put { path, .. } | IndexChange::Remove { path, .. }) => path.clone(),
            None => String::new(),
        };
        let done = self.inner.apply(changes)?;
        if self.deny_next.swap(false, Ordering::SeqCst) {
            self.stale.store(true, Ordering::SeqCst);
            return Err(DriveError::Conflict { key });
        }
        Ok(done)
    }
}

fn drive_over(
    bucket: Arc<MemBucket>,
    index: &Arc<LandedAndDenied>,
) -> EncryptedDrive<Arc<MemBucket>> {
    let as_index: Arc<dyn NameIndex> = index.clone();
    EncryptedDrive::new(bucket, DriveKey::generate().unwrap(), as_index)
}

fn data_objects(bucket: &MemBucket) -> Vec<String> {
    bucket
        .keys()
        .into_iter()
        .filter(|key| key.starts_with("data/"))
        .collect()
}

#[test]
fn x_ar5a_15_certainty_names_what_is_known_of_a_change_by_its_error() {
    let refused = [
        DriveError::Conflict { key: "a".into() },
        DriveError::Denied {
            message: "no".into(),
        },
    ];
    for e in &refused {
        assert_eq!(e.certainty(), Certainty::Refused, "{e:?}");
    }
    let never_made = [
        DriveError::InvalidKey {
            key: "a".into(),
            reason: "x",
        },
        DriveError::Locked { drive: "d".into() },
        DriveError::Unsupported("x".into()),
        DriveError::Cancelled,
    ];
    for e in &never_made {
        assert_eq!(e.certainty(), Certainty::NotAttempted, "{e:?}");
    }
    let may_have_landed = [
        DriveError::Transport("reset".into()),
        DriveError::Service(ServiceError::default()),
        DriveError::Io("x".into()),
        DriveError::Protocol("x".into()),
        DriveError::NotFound { key: "a".into() },
        DriveError::Corrupt {
            key: "a".into(),
            reason: "x".into(),
        },
    ];
    for e in &may_have_landed {
        assert_eq!(e.certainty(), Certainty::Unknown, "{e:?}");
    }
}

#[test]
fn x_ar5a_15_bind_keeps_the_object_of_a_change_that_landed_and_answered_conflict() {
    let bucket = Arc::new(MemBucket::with_conditional_writes());
    let index = Arc::new(LandedAndDenied::new());
    let drive = drive_over(bucket.clone(), &index);
    index.lose_the_answer();
    drive
        .write("a.txt", b"the only copy", Expect::Any)
        .expect("the change landed: the index names the object, whatever a stale copy says");
    index.recover();
    assert_eq!(drive.get("a.txt").unwrap(), b"the only copy");
    assert_eq!(data_objects(&bucket).len(), 1);
}

#[test]
fn x_ar5a_15_rewrite_keeps_the_object_of_a_change_that_landed_and_answered_conflict() {
    let bucket = Arc::new(MemBucket::with_conditional_writes());
    let index = Arc::new(LandedAndDenied::new());
    let drive = drive_over(bucket.clone(), &index);
    drive.write("a.txt", b"version one", Expect::Any).unwrap();
    let entry = drive.entry("a.txt").unwrap();
    index.lose_the_answer();
    let mut body = Cursor::new(b"version one".to_vec());
    let rewritten = drive
        .rewrite(
            "a.txt",
            &entry,
            &mut body,
            drive.options(),
            &|_| true,
            &|| {},
        )
        .unwrap();
    assert!(
        matches!(rewritten, Rewrite::Replaced(_)),
        "the change landed: the path names the new object"
    );
    index.recover();
    assert_eq!(drive.get("a.txt").unwrap(), b"version one");
}

#[test]
fn x_ar5a_15_promote_keeps_the_object_of_a_change_that_landed_and_answered_conflict() {
    let bucket = Arc::new(MemBucket::with_conditional_writes());
    let index = Arc::new(LandedAndDenied::new());
    let drive = drive_over(bucket.clone(), &index).with_packing(PackOptions::default());
    drive.write("small.txt", b"small", Expect::Any).unwrap();
    assert!(drive
        .entry("small.txt")
        .unwrap()
        .object
        .unwrap()
        .packed
        .is_some());
    index.lose_the_answer();
    let own = drive
        .promote("small.txt")
        .expect("the change landed: the file names its own object");
    index.recover();
    assert!(own.packed.is_none());
    assert_eq!(drive.get("small.txt").unwrap(), b"small");
}

#[test]
fn x_ar5a_15_a_batch_of_large_files_keeps_the_object_of_a_change_that_landed_and_answered_conflict()
{
    let bucket = Arc::new(MemBucket::with_conditional_writes());
    let index = Arc::new(LandedAndDenied::new());
    let drive = drive_over(bucket.clone(), &index);
    let mut body = Cursor::new(b"a large file, in a batch".to_vec());
    let mut items = vec![BatchStream::new(
        "big.bin",
        &mut body as &mut (dyn std::io::Read + Send),
    )];
    index.lose_the_answer();
    let results = drive.put_from_many(&mut items);
    assert!(
        results[0].is_ok(),
        "the change landed: the index names the object ({:?})",
        results[0]
    );
    index.recover();
    assert_eq!(drive.get("big.bin").unwrap(), b"a large file, in a batch");
}

#[test]
fn x_ar5a_15_the_object_of_a_change_the_index_refused_for_good_goes_in_every_one_of_the_four_places(
) {
    // bind: the path must not be there.
    let bucket = Arc::new(MemBucket::with_conditional_writes());
    let index = Arc::new(LandedAndDenied::new());
    let drive = drive_over(bucket.clone(), &index);
    drive.write("taken.txt", b"first", Expect::Any).unwrap();
    let before = data_objects(&bucket);
    let refused = drive.write("taken.txt", b"second", Expect::Absent);
    assert!(
        matches!(refused, Err(DriveError::Conflict { .. })),
        "{refused:?}"
    );
    assert_eq!(
        data_objects(&bucket),
        before,
        "bind: the new object left again"
    );

    // rewrite: the path was written meanwhile (the entry the pass read is not the file's now).
    let entry = drive.entry("taken.txt").unwrap();
    drive.write("taken.txt", b"third", Expect::Any).unwrap();
    let before = data_objects(&bucket);
    let mut body = Cursor::new(b"first".to_vec());
    let outcome = drive
        .rewrite(
            "taken.txt",
            &entry,
            &mut body,
            drive.options(),
            &|_| true,
            &|| {},
        )
        .unwrap();
    assert!(matches!(outcome, Rewrite::Conflict));
    assert_eq!(
        data_objects(&bucket),
        before,
        "rewrite: the new object left again"
    );

    // the batch: the key must not be there.
    let mut body = Cursor::new(b"another file".to_vec());
    let absent = Precondition::Absent;
    let mut items =
        vec![
            BatchStream::new("taken.txt", &mut body as &mut (dyn std::io::Read + Send))
                .when(&absent),
        ];
    let results = drive.put_from_many(&mut items);
    assert!(
        matches!(results[0], Err(DriveError::Conflict { .. })),
        "{:?}",
        results[0]
    );
    assert_eq!(
        data_objects(&bucket),
        before,
        "the batch: the new object left again"
    );
}

/// A bucket that answers every conditional write "precondition failed" and stores nothing: a proxy
/// that refuses for a reason of its own.
struct Refuses412(MemBucket);

impl Drive for Refuses412 {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.0.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.0.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.0.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.0.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.0.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.0.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        _bytes: &[u8],
        _condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        Err(DriveError::Conflict {
            key: key.to_string(),
        })
    }
}

/// A bucket whose conditional write lands and answers "precondition failed" (the repeat of a
/// request whose first answer was lost met the object it had made).
struct LandsThenRefuses(MemBucket);

impl Drive for LandsThenRefuses {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.0.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.0.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.0.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.0.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.0.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.0.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        _condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.0.put(key, bytes)?;
        Err(DriveError::Conflict {
            key: key.to_string(),
        })
    }
}

#[test]
fn az_c13_a_precondition_failure_for_an_object_that_is_not_there_is_not_a_write_that_landed() {
    let bucket = Arc::new(Refuses412(MemBucket::with_conditional_writes()));
    let drive = EncryptedDrive::new(
        bucket.clone(),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    );
    let put = drive.put("a.txt", b"never stored");
    assert!(put.is_err(), "the storage holds no such object: {put:?}");
    assert!(
        matches!(drive.entry("a.txt"), Err(DriveError::NotFound { .. })),
        "nothing may name an object that is not in the bucket"
    );
}

#[test]
fn az_c13_a_precondition_failure_for_an_object_of_the_length_sent_is_the_write_itself() {
    let bucket = Arc::new(LandsThenRefuses(MemBucket::with_conditional_writes()));
    let drive = EncryptedDrive::new(
        bucket.clone(),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    );
    drive
        .put("a.txt", b"stored once, answered twice")
        .expect("the repeat met the object the first request made");
    assert_eq!(drive.get("a.txt").unwrap(), b"stored once, answered twice");
}
