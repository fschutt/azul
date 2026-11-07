//! What an index releases, and how long a drive keeps it (ARCH28 RF44; AZ-U22, X-AR5a-21): an
//! index without history releases objects for deleting at once, and a delete the bucket refuses is
//! kept and tried again, not dropped; an index that keeps history releases only what its history
//! still names, and the drive deletes none of it; the spans a drive keeps things have one owner.

use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use super::mem_bucket::MemBucket;
use crate::{
    crypto::DriveKey,
    encrypted::{EncryptedDrive, Expect, IndexChange, MemoryIndex, NameIndex},
    meta::{MemoryBucket, MetaIndex, MetaRepo, Released, RepoOptions, RETENTION},
    packing::{PackOptions, RETIRED_GRACE_SECS},
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

/// A bucket whose first `failing` deletes fail as a lost connection does.
struct FlakyDelete {
    inner: MemBucket,
    failing: AtomicU32,
}

impl Drive for FlakyDelete {
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
        let failing = self
            .failing
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
        if failing.is_ok() {
            return Err(DriveError::Transport(String::from("connection reset")));
        }
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

fn data_objects(bucket: &MemBucket) -> Vec<String> {
    bucket
        .keys()
        .into_iter()
        .filter(|key| key.starts_with("data/"))
        .collect()
}

#[test]
fn az_u22_an_object_the_bucket_would_not_delete_after_the_index_released_it_is_kept_and_tried_again(
) {
    let bucket = Arc::new(FlakyDelete {
        inner: MemBucket::with_conditional_writes(),
        failing: AtomicU32::new(0),
    });
    let drive = EncryptedDrive::new(
        bucket.clone(),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    );
    drive.put("a.txt", b"version one").unwrap();
    bucket.failing.store(1, Ordering::SeqCst);
    // The overwrite releases version one; the bucket refuses to delete it. The write is done.
    drive.put("a.txt", b"version two").unwrap();
    assert_eq!(drive.get("a.txt").unwrap(), b"version two");
    let kept = drive.undeleted();
    assert_eq!(
        kept.len(),
        1,
        "named by nothing, counted in the quota, remembered"
    );
    assert_eq!(
        data_objects(&bucket.inner).len(),
        2,
        "both versions are in the bucket"
    );

    // The bucket works again: the next release carries the object along.
    drive.put("b.txt", b"another file").unwrap();
    assert_eq!(drive.undeleted().len(), 0);
    assert_eq!(
        data_objects(&bucket.inner).len(),
        2,
        "version two and b.txt"
    );

    // And a round of upkeep tries again too.
    bucket.failing.store(1, Ordering::SeqCst);
    drive.put("a.txt", b"version three").unwrap();
    assert_eq!(drive.undeleted().len(), 1);
    assert_eq!(drive.retry_undeleted(), 0);
    assert_eq!(
        data_objects(&bucket.inner).len(),
        2,
        "version three and b.txt"
    );
}

type Index = MetaIndex<MemoryBucket, DriveKey>;

#[test]
fn x_ar5a_21_an_index_that_keeps_history_releases_only_what_its_history_still_names() {
    let key = DriveKey::generate().unwrap();
    let meta = MemoryBucket::new();
    let repo = MetaRepo::create_with(
        meta.clone(),
        key.clone(),
        "laptop",
        "Laptop",
        &RepoOptions::default(),
    )
    .unwrap();
    let index: Arc<dyn NameIndex> = Arc::new(Index::new(repo).with_poll_every(0));
    let drive = EncryptedDrive::new(meta.clone(), key, Arc::clone(&index));
    drive.put("a.txt", b"version one").unwrap();
    let first = drive.entry("a.txt").unwrap().object.unwrap().id;
    drive.put("a.txt", b"version two").unwrap();
    let second = drive.entry("a.txt").unwrap();
    let data = |bucket: &MemoryBucket| -> Vec<String> {
        bucket
            .objects()
            .into_iter()
            .map(|(key, _)| key)
            .filter(|key| key.starts_with("data/"))
            .collect()
    };
    // The overwrite released version one: the drive deleted nothing, the history names it.
    let objects = data(&meta);
    assert_eq!(objects.len(), 2, "{objects:?}");
    assert!(objects.contains(&first.bucket_key()));

    // The change that removes version two releases it, held by the history.
    let released = index
        .apply(vec![IndexChange::Remove {
            path: String::from("a.txt"),
            expect: Expect::Any,
        }])
        .unwrap();
    assert!(released.delete_now.is_empty(), "{released:?}");
    assert_eq!(
        released.held_by_history,
        vec![second.bucket_object().unwrap()]
    );
    assert_eq!(data(&meta).len(), 2, "still both in the bucket");
}

#[test]
fn x_ar5a_21_a_released_set_adds_up_and_says_when_it_is_empty() {
    let mut released = Released::none();
    assert!(released.is_empty());
    let a = crate::crypto::ObjectId::generate().unwrap();
    let b = crate::crypto::ObjectId::generate().unwrap();
    released.extend(Released::delete_now(vec![a]));
    released.extend(Released::held(vec![b]));
    assert_eq!(released.delete_now, vec![a]);
    assert_eq!(released.held_by_history, vec![b]);
    assert!(!released.is_empty());
}

#[test]
fn x_ar5a_21_the_spans_a_drive_keeps_things_have_one_owner() {
    assert_eq!(RETENTION.trash, Duration::from_secs(14 * 86_400));
    assert_eq!(RETENTION.versions, Duration::from_secs(90 * 86_400));
    assert_eq!(
        (RETENTION.trash_days(), RETENTION.versions_days()),
        (14, 90)
    );
    // The grace of a retired pack is the versions' span, not a span of its own (it was thirty
    // days, shorter than the ninety a version can be restored from).
    assert_eq!(RETIRED_GRACE_SECS, RETENTION.versions.as_secs());
    assert_eq!(
        PackOptions::default().retired_grace_secs,
        RETENTION.versions.as_secs()
    );
}
