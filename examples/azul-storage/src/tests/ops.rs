//! Folder operations (create, rename, delete a folder, list everything under
//! one) and drive-to-drive copies: on a folder on disk, and through the
//! default implementations every other drive gets from the six basic calls.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Mutex,
    },
};

use super::TempDir;
use crate::{
    ops::{exists, folder_exists, list_all},
    transfer::copy_object,
    ByteRange, Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo,
};

/// A bucket in memory with S3 semantics: a "folder" is a common prefix, a
/// key ending in `/` is a folder marker object. Pages hold at most two
/// entries, so every caller has to follow the continuation token. Only the
/// six basic calls: everything else is the trait's default.
#[derive(Default)]
struct Bucket {
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
    puts: AtomicU32,
    gets: AtomicU32,
    ranges: AtomicU32,
}

impl Bucket {
    fn with(keys: &[(&str, &[u8])]) -> Self {
        let bucket = Bucket::default();
        for (key, bytes) in keys {
            bucket
                .objects
                .lock()
                .unwrap()
                .insert((*key).to_string(), bytes.to_vec());
        }
        bucket
    }

    fn keys(&self) -> Vec<String> {
        self.objects.lock().unwrap().keys().cloned().collect()
    }
}

impl Drive for Bucket {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let objects = self.objects.lock().unwrap();
        let mut entries: Vec<(String, Option<ObjectInfo>)> = Vec::new();
        for (key, bytes) in objects.iter() {
            if !key.starts_with(&request.prefix) {
                continue;
            }
            let rest = &key[request.prefix.len()..];
            match request
                .delimiter
                .as_deref()
                .and_then(|d| rest.find(d).map(|i| (i, d.len())))
            {
                Some((i, len)) => {
                    let folder = format!("{}{}", request.prefix, &rest[..i + len]);
                    if !entries.iter().any(|(k, _)| *k == folder) {
                        entries.push((folder, None));
                    }
                }
                None => entries.push((
                    key.clone(),
                    Some(ObjectInfo {
                        key: key.clone(),
                        size: bytes.len() as u64,
                        modified: Some(1),
                        etag: None,
                    }),
                )),
            }
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        if let Some(after) = request.continuation.as_deref() {
            entries.retain(|(k, _)| k.as_str() > after);
        }
        let page_size = 2;
        let next = (entries.len() > page_size).then(|| entries[page_size - 1].0.clone());
        entries.truncate(page_size);
        let mut page = ListPage {
            next,
            ..ListPage::default()
        };
        for (key, info) in entries {
            match info {
                Some(info) => page.objects.push(info),
                None => page.folders.push(key),
            }
        }
        Ok(page)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        self.objects
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| DriveError::NotFound {
                key: key.to_string(),
            })
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.ranges.fetch_add(1, Ordering::SeqCst);
        let bytes = self
            .objects
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| DriveError::NotFound {
                key: key.to_string(),
            })?;
        let start = range.start as usize;
        if start >= bytes.len() {
            return Err(DriveError::InvalidRange {
                key: key.to_string(),
            });
        }
        let end = range
            .end
            .map_or(bytes.len() - 1, |e| (e as usize).min(bytes.len() - 1));
        Ok(bytes[start..=end].to_vec())
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.puts.fetch_add(1, Ordering::SeqCst);
        self.objects
            .lock()
            .unwrap()
            .insert(key.to_string(), bytes.to_vec());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.objects.lock().unwrap().remove(key);
        Ok(())
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.objects
            .lock()
            .unwrap()
            .get(key)
            .map(|bytes| ObjectInfo {
                key: key.to_string(),
                size: bytes.len() as u64,
                modified: Some(1),
                etag: None,
            })
            .ok_or_else(|| DriveError::NotFound {
                key: key.to_string(),
            })
    }
}

fn local(tmp: &TempDir) -> LocalDrive {
    let drive = LocalDrive::new(tmp.path().join("drive"));
    drive.put("docs/a.txt", b"alpha").unwrap();
    drive.put("docs/sub/b.txt", b"beta").unwrap();
    drive.put("readme.txt", b"hello").unwrap();
    drive
}

#[test]
fn list_all_follows_every_page_and_returns_every_object_under_the_prefix() {
    let bucket = Bucket::with(&[
        ("a/1", b"1"),
        ("a/2", b"2"),
        ("a/x/3", b"3"),
        ("a/x/y/4", b"4"),
        ("b/5", b"5"),
    ]);
    let all = list_all(&bucket, "a/").unwrap();
    let keys: Vec<&str> = all.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, vec!["a/1", "a/2", "a/x/3", "a/x/y/4"]);
    assert_eq!(list_all(&bucket, "").unwrap().len(), 5);
}

#[test]
fn exists_tells_a_missing_key_from_a_present_one() {
    let bucket = Bucket::with(&[("a/1", b"1"), ("f/", b"")]);
    assert!(exists(&bucket, "a/1").unwrap());
    assert!(!exists(&bucket, "a/2").unwrap());
    assert!(
        folder_exists(&bucket, "a/").unwrap(),
        "a folder with an object in it"
    );
    assert!(
        folder_exists(&bucket, "f/").unwrap(),
        "an empty folder's marker"
    );
    assert!(!folder_exists(&bucket, "g/").unwrap());
}

#[test]
fn a_folder_created_on_a_local_drive_is_a_directory_that_lists_as_a_folder() {
    let tmp = TempDir::new("ops-mkdir");
    let drive = local(&tmp);
    drive.create_folder("docs/new/").unwrap();
    assert!(tmp.path().join("drive").join("docs").join("new").is_dir());
    let page = drive.list(&ListRequest::folder("docs/")).unwrap();
    assert!(page.folders.contains(&"docs/new/".to_string()), "{page:?}");
    assert!(folder_exists(&drive, "docs/new/").unwrap());
    // An existing folder is fine; a name without the trailing / is not a folder.
    drive.create_folder("docs/new/").unwrap();
    assert!(drive.create_folder("docs/other").is_err());
    assert!(drive.create_folder("").is_err(), "the root exists already");
}

#[test]
fn a_folder_created_on_a_bucket_is_an_empty_marker_object() {
    let bucket = Bucket::default();
    bucket.create_folder("photos/2024/").unwrap();
    assert_eq!(bucket.keys(), vec!["photos/2024/".to_string()]);
    assert_eq!(bucket.get("photos/2024/").unwrap(), Vec::<u8>::new());
    assert!(bucket.create_folder("photos").is_err());
}

#[test]
fn renaming_a_local_file_moves_it_and_refuses_an_existing_target() {
    let tmp = TempDir::new("ops-rename-file");
    let drive = local(&tmp);
    drive.rename("docs/a.txt", "docs/c.txt").unwrap();
    assert!(matches!(
        drive.get("docs/a.txt"),
        Err(DriveError::NotFound { .. })
    ));
    assert_eq!(drive.get("docs/c.txt").unwrap(), b"alpha");
    // Into another folder, which is created.
    drive.rename("docs/c.txt", "archive/2024/c.txt").unwrap();
    assert_eq!(drive.get("archive/2024/c.txt").unwrap(), b"alpha");
    // Never over something that is there.
    assert!(drive.rename("readme.txt", "archive/2024/c.txt").is_err());
    assert_eq!(drive.get("readme.txt").unwrap(), b"hello");
}

#[test]
fn renaming_a_local_folder_moves_everything_under_it() {
    let tmp = TempDir::new("ops-rename-folder");
    let drive = local(&tmp);
    drive.rename("docs/", "papers/").unwrap();
    assert!(!folder_exists(&drive, "docs/").unwrap());
    assert_eq!(drive.get("papers/a.txt").unwrap(), b"alpha");
    assert_eq!(drive.get("papers/sub/b.txt").unwrap(), b"beta");
    // A folder never lands on an existing folder or file.
    drive.create_folder("taken/").unwrap();
    assert!(drive.rename("papers/", "taken/").is_err());
    assert!(drive.rename("papers/", "readme.txt/").is_err());
    assert!(
        drive.rename("papers/", "papers/inside/").is_err(),
        "into itself"
    );
}

#[test]
fn the_default_rename_copies_then_deletes_every_object_under_a_folder() {
    let bucket = Bucket::with(&[
        ("docs/", b""),
        ("docs/a.txt", b"alpha"),
        ("docs/sub/b.txt", b"beta"),
        ("other.txt", b"x"),
    ]);
    bucket.rename("docs/", "papers/").unwrap();
    assert_eq!(
        bucket.keys(),
        vec![
            "other.txt".to_string(),
            "papers/".to_string(),
            "papers/a.txt".to_string(),
            "papers/sub/b.txt".to_string(),
        ]
    );
    bucket.rename("other.txt", "papers/other.txt").unwrap();
    assert!(!exists(&bucket, "other.txt").unwrap());
    assert!(bucket.rename("papers/a.txt", "papers/other.txt").is_err());
    assert!(bucket.rename("missing.txt", "x.txt").is_err());
}

#[test]
fn deleting_a_folder_removes_everything_under_it_but_never_the_root() {
    let tmp = TempDir::new("ops-rmdir");
    let drive = local(&tmp);
    drive.delete_folder("docs/").unwrap();
    assert!(!tmp.path().join("drive").join("docs").exists());
    assert_eq!(drive.get("readme.txt").unwrap(), b"hello");
    assert!(drive.delete_folder("").is_err());
    assert!(
        drive.delete_folder("readme.txt").is_err(),
        "not a folder name"
    );

    let bucket = Bucket::with(&[
        ("docs/", b""),
        ("docs/a.txt", b"alpha"),
        ("docs/sub/b.txt", b"beta"),
        ("docsx.txt", b"x"),
    ]);
    bucket.delete_folder("docs/").unwrap();
    assert_eq!(bucket.keys(), vec!["docsx.txt".to_string()]);
    assert!(bucket.delete_folder("").is_err());
}

#[test]
fn a_local_drive_names_the_file_of_a_key_and_a_bucket_names_none() {
    let tmp = TempDir::new("ops-local-path");
    let drive = local(&tmp);
    assert_eq!(
        drive.local_path("docs/a.txt"),
        Some(tmp.path().join("drive").join("docs").join("a.txt"))
    );
    assert_eq!(
        drive.local_path("docs/sub/"),
        Some(tmp.path().join("drive").join("docs").join("sub"))
    );
    assert_eq!(drive.local_path("../etc/passwd"), None);
    assert_eq!(Bucket::default().local_path("docs/a.txt"), None);
}

#[test]
fn copying_between_two_local_drives_copies_the_file_and_reports_the_bytes() {
    let tmp = TempDir::new("ops-copy-local");
    let source = local(&tmp);
    let target = LocalDrive::new(tmp.path().join("other"));
    let mut seen = Vec::new();
    let copied = copy_object(
        &source,
        "docs/a.txt",
        Some(5),
        &target,
        "in/a.txt",
        &mut |bytes| seen.push(bytes),
    )
    .unwrap();
    assert_eq!(copied, 5);
    assert_eq!(target.get("in/a.txt").unwrap(), b"alpha");
    assert_eq!(
        source.get("docs/a.txt").unwrap(),
        b"alpha",
        "a copy keeps the source"
    );
    assert_eq!(seen.last(), Some(&5));
}

#[test]
fn copying_from_a_bucket_to_a_local_drive_writes_the_file_in_ranged_chunks() {
    let tmp = TempDir::new("ops-copy-down");
    let big = vec![7u8; (crate::transfer::CHUNK as usize) + 10];
    let bucket = Bucket::with(&[("big.bin", big.as_slice()), ("small.txt", b"abc")]);
    let target = LocalDrive::new(tmp.path().join("down"));
    let mut seen = Vec::new();
    copy_object(
        &bucket,
        "big.bin",
        Some(big.len() as u64),
        &target,
        "big.bin",
        &mut |bytes| seen.push(bytes),
    )
    .unwrap();
    assert_eq!(target.get("big.bin").unwrap().len(), big.len());
    assert_eq!(bucket.ranges.load(Ordering::SeqCst), 2, "two chunks");
    assert_eq!(seen.last().copied(), Some(big.len() as u64));
    assert!(seen.len() >= 2, "progress after every chunk: {seen:?}");
    copy_object(
        &bucket,
        "small.txt",
        None,
        &target,
        "small.txt",
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(target.get("small.txt").unwrap(), b"abc");
}

#[test]
fn copying_into_a_bucket_puts_the_object_once() {
    let tmp = TempDir::new("ops-copy-up");
    let source = local(&tmp);
    let bucket = Bucket::default();
    copy_object(
        &source,
        "docs/sub/b.txt",
        Some(4),
        &bucket,
        "up/b.txt",
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(bucket.get("up/b.txt").unwrap(), b"beta");
    assert_eq!(bucket.puts.load(Ordering::SeqCst), 1);
    // Bucket to bucket: one get, one put.
    let other = Bucket::default();
    copy_object(&bucket, "up/b.txt", Some(4), &other, "b.txt", &mut |_| {}).unwrap();
    assert_eq!(other.get("b.txt").unwrap(), b"beta");
}

#[test]
fn a_local_object_has_its_path_and_creation_time_as_metadata_and_a_bucket_none() {
    let tmp = TempDir::new("ops-metadata");
    let drive = local(&tmp);
    let pairs = drive.metadata("docs/a.txt").unwrap();
    assert!(
        pairs
            .iter()
            .any(|(k, v)| k == "Location" && v.ends_with("a.txt")),
        "{pairs:?}"
    );
    assert!(matches!(
        drive.metadata("docs/missing.txt"),
        Err(DriveError::NotFound { .. })
    ));
    assert!(Bucket::with(&[("a", b"1")])
        .metadata("a")
        .unwrap()
        .is_empty());
}

#[test]
fn a_copy_within_one_drive_keeps_the_source_and_refuses_a_folder() {
    let tmp = TempDir::new("ops-copy-within");
    let drive = local(&tmp);
    drive.copy("docs/a.txt", "docs/copy/a.txt").unwrap();
    assert_eq!(drive.get("docs/copy/a.txt").unwrap(), b"alpha");
    assert_eq!(
        drive.get("docs/a.txt").unwrap(),
        b"alpha",
        "a copy keeps the source"
    );
    // A copy replaces a file that is there (conflicts are the caller's).
    drive.copy("readme.txt", "docs/copy/a.txt").unwrap();
    assert_eq!(drive.get("docs/copy/a.txt").unwrap(), b"hello");
    assert!(
        drive.copy("docs/", "elsewhere/").is_err(),
        "a folder is not one object"
    );
    assert!(matches!(
        drive.copy("docs/missing.txt", "x.txt"),
        Err(DriveError::NotFound { .. })
    ));

    // Without a copy of its own a drive gets and puts.
    let bucket = Bucket::with(&[("a", b"1")]);
    bucket.copy("a", "b/a").unwrap();
    assert_eq!(bucket.get("b/a").unwrap(), b"1");
    assert_eq!(bucket.get("a").unwrap(), b"1");
}
