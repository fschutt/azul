use std::{
    collections::VecDeque,
    fs::File,
    sync::{Arc, Barrier, Mutex},
    time::Duration,
};

use super::{every_bucket, TempDir};
use crate::{
    limits::Limit,
    meta::{Bucket, DriveBucket, Fetched, FolderBucket, MemoryBucket, MetaError},
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

fn is_conflict(result: &Result<Option<String>, MetaError>) -> bool {
    matches!(result, Err(MetaError::Conflict { .. }))
}

#[test]
fn an_object_is_created_once_and_a_second_create_is_a_conflict() {
    for b in every_bucket() {
        let first = b.bucket.create("m/a", b"one").unwrap();
        assert!(first.is_some(), "{}", b.name);
        assert!(is_conflict(&b.bucket.create("m/a", b"two")), "{}", b.name);
        let (bytes, version) = b.bucket.read("m/a").unwrap().unwrap();
        assert_eq!(bytes, b"one", "{}", b.name);
        assert_eq!(Some(version), first, "{}", b.name);
    }
}

#[test]
fn of_two_replaces_from_the_same_version_exactly_one_wins() {
    for b in every_bucket() {
        let v1 = b.bucket.create("m/manifest", b"base").unwrap().unwrap();
        let mine = b.bucket.replace("m/manifest", b"mine", &v1);
        let theirs = b.bucket.replace("m/manifest", b"theirs", &v1);
        assert!(mine.is_ok(), "{}", b.name);
        assert!(is_conflict(&theirs), "{}", b.name);
        assert_eq!(b.bucket.read("m/manifest").unwrap().unwrap().0, b"mine", "{}", b.name);
    }
}

#[test]
fn a_replace_of_a_missing_object_is_a_conflict() {
    for b in every_bucket() {
        assert!(is_conflict(&b.bucket.replace("m/x", b"x", "v1")), "{}", b.name);
        assert_eq!(b.bucket.read("m/x").unwrap(), None, "{}", b.name);
    }
}

#[test]
fn a_conditional_read_of_an_unchanged_object_is_not_modified() {
    for b in every_bucket() {
        let v1 = b.bucket.create("m/manifest", b"base").unwrap().unwrap();
        assert_eq!(
            b.bucket.read_if_changed("m/manifest", &v1).unwrap(),
            Fetched::NotModified,
            "{}",
            b.name
        );
    }
}

#[test]
fn a_conditional_read_after_a_write_brings_the_new_bytes_and_version() {
    for b in every_bucket() {
        let v1 = b.bucket.create("m/manifest", b"base").unwrap().unwrap();
        let v2 = b.bucket.replace("m/manifest", b"next", &v1).unwrap().unwrap();
        assert_ne!(v1, v2, "{}", b.name);
        assert_eq!(
            b.bucket.read_if_changed("m/manifest", &v1).unwrap(),
            Fetched::Changed {
                bytes: b"next".to_vec(),
                version: v2
            },
            "{}",
            b.name
        );
        assert_eq!(
            b.bucket.read_if_changed("m/gone", &v1).unwrap(),
            Fetched::Missing,
            "{}",
            b.name
        );
    }
}

#[test]
fn a_range_read_brings_the_bytes_of_the_range() {
    for b in every_bucket() {
        b.bucket.create("m/pack", b"0123456789").unwrap();
        assert_eq!(
            b.bucket.read_range("m/pack", ByteRange::new(2, Some(4))).unwrap(),
            b"234",
            "{}",
            b.name
        );
        assert_eq!(
            b.bucket.read_range("m/pack", ByteRange::new(7, None)).unwrap(),
            b"789",
            "{}",
            b.name
        );
        assert!(b.bucket.read_range("m/pack", ByteRange::new(10, None)).is_err(), "{}", b.name);
    }
}

#[test]
fn removing_an_object_twice_is_fine() {
    for b in every_bucket() {
        b.bucket.create("m/log", b"entry").unwrap();
        b.bucket.remove("m/log").unwrap();
        b.bucket.remove("m/log").unwrap();
        assert_eq!(b.bucket.read("m/log").unwrap(), None, "{}", b.name);
        assert!(b.bucket.create("m/log", b"again").unwrap().is_some(), "{}", b.name);
    }
}

#[test]
fn of_eight_threads_swapping_from_the_same_version_exactly_one_wins() {
    for b in every_bucket() {
        let bucket: Arc<dyn Bucket> = Arc::from(b.bucket);
        let v1 = bucket.create("m/manifest", b"base").unwrap().unwrap();
        let barrier = Arc::new(Barrier::new(8));
        let threads: Vec<_> = (0..8u8)
            .map(|i| {
                let bucket = Arc::clone(&bucket);
                let barrier = Arc::clone(&barrier);
                let v1 = v1.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    bucket.replace("m/manifest", &[i], &v1).is_ok()
                })
            })
            .collect();
        let wins = threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|won| *won)
            .count();
        assert_eq!(wins, 1, "{}", b.name);
    }
}

#[test]
fn the_memory_bucket_runs_the_hook_before_the_swap_checks_its_version() {
    let bucket = MemoryBucket::new();
    let v1 = bucket.create("m/manifest", b"base").unwrap().unwrap();
    let other = bucket.clone();
    let known = v1.clone();
    bucket.before_next_replace("m/manifest", move || {
        other.replace("m/manifest", b"theirs", &known).unwrap();
    });
    assert!(is_conflict(&bucket.replace("m/manifest", b"mine", &v1)));
    assert_eq!(bucket.read("m/manifest").unwrap().unwrap().0, b"theirs");
    // The hook ran once: the next swap from the current version wins.
    let current = bucket.read("m/manifest").unwrap().unwrap().1;
    assert!(bucket.replace("m/manifest", b"mine", &current).is_ok());
    assert_eq!(bucket.counts().conflicts, 1);
}

#[test]
fn the_memory_bucket_counts_a_not_modified_read() {
    let bucket = MemoryBucket::new();
    let v1 = bucket.create("m/manifest", b"base").unwrap().unwrap();
    bucket.read_if_changed("m/manifest", &v1).unwrap();
    let counts = bucket.counts();
    assert_eq!(counts.conditional_reads, 1);
    assert_eq!(counts.not_modified, 1);
    assert_eq!(counts.reads, 0);
    assert_eq!(counts.lists, 0);
}

#[test]
fn the_memory_bucket_lists_like_s3() {
    let bucket = MemoryBucket::new();
    for key in ["a/1", "a/2", "b", "c/d/e"] {
        Drive::put(&bucket, key, b"x").unwrap();
    }
    let page = bucket.list(&ListRequest::folder("")).unwrap();
    assert_eq!(page.folders, vec!["a/".to_string(), "c/".to_string()]);
    assert_eq!(page.objects.len(), 1);
    assert_eq!(page.objects[0].key, "b");
    assert!(page.objects[0].etag.is_some());
    let first = bucket.list(&ListRequest::recursive("").with_max_keys(3)).unwrap();
    assert_eq!(first.objects.len(), 3);
    let next = first.next.clone().unwrap();
    let rest = bucket
        .list(&ListRequest::recursive("").with_max_keys(3).with_continuation(next))
        .unwrap();
    assert_eq!(rest.objects.len(), 1);
    assert_eq!(rest.objects[0].key, "c/d/e");
    assert_eq!(rest.next, None);
}

#[test]
fn a_folder_bucket_refuses_a_key_that_climbs_out_of_its_folder() {
    let dir = TempDir::new("meta-folder-key");
    let bucket = FolderBucket::new(dir.path());
    assert!(matches!(
        bucket.create("../outside", b"x"),
        Err(MetaError::Drive(_))
    ));
    assert!(matches!(bucket.read("/etc/passwd"), Err(MetaError::Drive(_))));
}

#[test]
fn a_lock_file_a_dead_writer_left_behind_is_no_obstacle() {
    let dir = TempDir::new("meta-folder-lock");
    let bucket = FolderBucket::new(dir.path());
    let v1 = bucket.create("m/manifest", b"base").unwrap().unwrap();
    // A lock file nobody holds a lock on: what a crash leaves.
    File::create(dir.path().join("m").join("manifest.lock")).unwrap();
    assert!(bucket.replace("m/manifest", b"next", &v1).is_ok());
}

#[test]
fn a_folder_bucket_waits_for_the_writer_that_holds_the_lock() {
    let dir = TempDir::new("meta-folder-wait");
    let bucket = FolderBucket::new(dir.path());
    let v1 = bucket.create("m/manifest", b"base").unwrap().unwrap();
    let held = File::options()
        .read(true)
        .write(true)
        .open(dir.path().join("m").join("manifest.lock"))
        .unwrap();
    held.lock().unwrap();
    let waiting = {
        let bucket = bucket.clone();
        std::thread::spawn(move || bucket.replace("m/manifest", b"next", &v1).is_ok())
    };
    std::thread::sleep(Duration::from_millis(100));
    assert!(!waiting.is_finished());
    drop(held);
    assert!(waiting.join().unwrap());
}

#[test]
fn a_folder_bucket_leaves_no_temporary_file_behind() {
    let dir = TempDir::new("meta-folder-notemp");
    let bucket = FolderBucket::new(dir.path());
    let v1 = bucket.create("m/manifest", b"base").unwrap().unwrap();
    assert!(bucket.replace("m/manifest", b"x", "not-the-version").is_err());
    bucket.replace("m/manifest", b"next", &v1).unwrap();
    let mut names: Vec<String> = std::fs::read_dir(dir.path().join("m"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["manifest", "manifest.lock"]);
}

// ==== AZ-M5: an object of the index is read for the size it has ====

/// A drive that keeps the limit of every `get_limited` and can say sizes of its own in a HEAD (the
/// next ones, then the true one).
struct Spy {
    inner: MemoryBucket,
    limits: Mutex<Vec<Limit>>,
    sizes: Mutex<VecDeque<u64>>,
}

impl Spy {
    fn holding(key: &str, bytes: &[u8]) -> Arc<Spy> {
        let inner = MemoryBucket::new();
        inner.create(key, bytes).unwrap();
        Arc::new(Spy {
            inner,
            limits: Mutex::new(Vec::new()),
            sizes: Mutex::new(VecDeque::new()),
        })
    }

    fn says(&self, sizes: &[u64]) {
        self.sizes.lock().unwrap().extend(sizes.iter().copied());
    }

    fn limits(&self) -> Vec<Limit> {
        self.limits.lock().unwrap().clone()
    }
}

impl Drive for Spy {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        Drive::get(&self.inner, key)
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        Drive::get_range(&self.inner, key, range)
    }

    fn get_limited(&self, key: &str, limit: Limit) -> Result<Vec<u8>, DriveError> {
        self.limits.lock().unwrap().push(limit);
        let bytes = Drive::get(&self.inner, key)?;
        limit.check_object(key, bytes.len() as u64)?;
        Ok(bytes)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        Drive::put(&self.inner, key, bytes)
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        Drive::delete(&self.inner, key)
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let mut info = Drive::head(&self.inner, key)?;
        if let Some(size) = self.sizes.lock().unwrap().pop_front() {
            info.size = size;
        }
        Ok(info)
    }

    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        Drive::put_if(&self.inner, key, bytes, condition)
    }
}

/// AZ-M5. An object of the index is read with the size its HEAD said, and a little room, as its
/// cap - so the transport's time follows the object - and not with the 256 MiB the format allows.
#[test]
fn az_m5_an_index_object_is_read_with_the_size_its_head_said_as_its_cap() {
    let spy = Spy::holding("m/pack", &vec![7u8; 1_000_000]);
    let bucket = DriveBucket::new(Arc::clone(&spy));
    let (bytes, _) = bucket.read("m/pack").unwrap().unwrap();
    assert_eq!(bytes.len(), 1_000_000);
    let limits = spy.limits();
    assert_eq!(limits.len(), 1);
    assert!(
        (1_000_000..2_000_000).contains(&limits[0].max),
        "the size and a little room, not 256 MiB: {:?}",
        limits[0]
    );
    // The conditional read of a version that moved reads the same way.
    let Fetched::Changed { bytes, .. } = bucket
        .read_if_changed("m/pack", "an older version")
        .unwrap()
    else {
        panic!("the version moved");
    };
    assert_eq!(bytes.len(), 1_000_000);
    assert!(spy.limits()[1].max < 2_000_000);
}

/// AZ-M5. An object whose HEAD says it is more than an index object may weigh is refused before a
/// byte of it is asked for.
#[test]
fn az_m5_an_index_object_its_head_says_is_too_big_is_refused_before_any_get() {
    let spy = Spy::holding("m/pack", b"small");
    spy.says(&[300 << 20, 300 << 20]);
    let bucket = DriveBucket::new(Arc::clone(&spy));
    let error = bucket.read("m/pack").unwrap_err();
    assert!(
        matches!(&error, MetaError::Drive(DriveError::Protocol(text)) if text.contains("an object of the drive index")),
        "{error:?}"
    );
    assert!(matches!(
        bucket.read_if_changed("m/pack", "another version"),
        Err(MetaError::Drive(DriveError::Protocol(_)))
    ));
    assert!(spy.limits().is_empty(), "no GET was sent");
}

/// AZ-M5. An object replaced by a longer one between the HEAD and the GET is asked about again
/// and read, where the cap made of the old size refused it for good.
#[test]
fn az_m5_an_object_replaced_by_a_longer_one_since_its_head_is_read_after_a_second_head() {
    let spy = Spy::holding("m/pack", &vec![9u8; 1_000_000]);
    spy.says(&[10]);
    let bucket = DriveBucket::new(Arc::clone(&spy));
    let (bytes, _) = bucket.read("m/pack").unwrap().unwrap();
    assert_eq!(bytes.len(), 1_000_000);
    let limits = spy.limits();
    assert_eq!(limits.len(), 2, "{limits:?}");
    assert!(
        limits[0].max < 1_000_000 && limits[1].max >= 1_000_000,
        "{limits:?}"
    );
}

/// AZ-M5. A drive that does not say a size (0) is read under the format's own cap, as before.
#[test]
fn az_m5_a_drive_that_says_no_size_is_read_under_the_formats_cap() {
    let spy = Spy::holding("m/pack", &vec![1u8; 5_000]);
    spy.says(&[0]);
    let bucket = DriveBucket::new(Arc::clone(&spy));
    assert_eq!(bucket.read("m/pack").unwrap().unwrap().0.len(), 5_000);
    assert_eq!(spy.limits()[0], crate::limits::named::INDEX_OBJECT);
}
