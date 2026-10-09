use std::{
    fs::File,
    sync::{Arc, Barrier},
    time::{Duration, SystemTime},
};

use super::{every_bucket, TempDir};
use crate::{
    meta::{Bucket, Fetched, FolderBucket, MemoryBucket, MetaError},
    ByteRange, Drive, ListRequest,
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
fn a_folder_bucket_takes_over_the_lock_of_a_writer_that_died() {
    let dir = TempDir::new("meta-folder-lock");
    let bucket = FolderBucket::new(dir.path());
    let v1 = bucket.create("m/manifest", b"base").unwrap().unwrap();
    let lock = dir.path().join("m").join("manifest.lock");
    let file = File::create(&lock).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(120))
        .unwrap();
    drop(file);
    assert!(bucket.replace("m/manifest", b"next", &v1).is_ok());
    assert!(!lock.exists());
}

#[test]
fn a_folder_bucket_leaves_no_lock_behind() {
    let dir = TempDir::new("meta-folder-nolock");
    let bucket = FolderBucket::new(dir.path());
    let v1 = bucket.create("m/manifest", b"base").unwrap().unwrap();
    assert!(bucket.replace("m/manifest", b"x", "not-the-version").is_err());
    bucket.replace("m/manifest", b"next", &v1).unwrap();
    let names: Vec<String> = std::fs::read_dir(dir.path().join("m"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["manifest".to_string()]);
}
