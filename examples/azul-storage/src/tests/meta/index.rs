//! The encrypted drive over the drive index (feature `encryption`).

use std::sync::Arc;

use crate::{
    crypto::DriveKey,
    encrypted::{EncryptedDrive, Expect, IndexChange, IndexEntry, NameIndex},
    meta::{
        merge::keep_both, open_encrypted_drive, pointer, MemoryBucket, MetaIndex, MetaRepo,
        RepoOptions,
    },
    Drive, DriveError, ListRequest, Precondition,
};

type Index = MetaIndex<MemoryBucket, DriveKey>;

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A device's encrypted drive over `bucket`, polling before every read.
fn device(bucket: &MemoryBucket, key: &DriveKey, id: &str, lazy: bool) -> EncryptedDrive<MemoryBucket> {
    let options = RepoOptions {
        cache_dir: None,
        lazy,
    };
    let repo = match MetaRepo::open_with(bucket.clone(), key.clone(), id, id, &options) {
        Ok(repo) => repo,
        Err(_) => MetaRepo::create_with(bucket.clone(), key.clone(), id, id, &options).unwrap(),
    };
    let index: Arc<dyn NameIndex> = Arc::new(Index::new(repo).with_poll_every(0));
    EncryptedDrive::new(bucket.clone(), key.clone(), index)
}

fn names(drive: &dyn Drive, prefix: &str) -> (Vec<String>, Vec<String>) {
    let page = drive.list(&ListRequest::folder(prefix)).unwrap();
    (
        page.folders,
        page.objects.into_iter().map(|o| o.key).collect(),
    )
}

#[test]
fn a_pointer_reads_back_as_the_entry_it_was_written_from() {
    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    let drive = device(&bucket, &key, "laptop", false);
    drive.put("docs/a.txt", b"the bytes of a").unwrap();
    let index = drive_index_entry(&bucket, &key, "docs/a.txt");
    let text = pointer::encode(&index);
    assert!(text.starts_with(pointer::MAGIC.as_bytes()));
    assert_eq!(pointer::decode(&text).unwrap(), index);
    let marker = IndexEntry {
        size: 0,
        modified: None,
        object: None,
    };
    assert_eq!(pointer::decode(&pointer::encode(&marker)).unwrap(), marker);
    assert!(pointer::decode(b"something else\nsize 1\n").is_err());
}

/// The index entry of `path` as a second device sees it.
fn drive_index_entry(bucket: &MemoryBucket, key: &DriveKey, path: &str) -> IndexEntry {
    let repo = MetaRepo::open(bucket.clone(), key.clone(), "reader", "Reader").unwrap();
    Index::new(repo).get(path).unwrap().unwrap()
}

#[test]
fn files_written_through_the_encrypted_drive_are_listed_and_read_back_without_a_list() {
    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    let drive = device(&bucket, &key, "laptop", false);
    drive.put("docs/a.txt", b"alpha").unwrap();
    drive.put("docs/sub/b.txt", b"beta").unwrap();
    drive.put("top.txt", b"top").unwrap();
    drive.create_folder("empty/").unwrap();

    assert_eq!(drive.get("docs/a.txt").unwrap(), b"alpha");
    assert_eq!(drive.head("docs/sub/b.txt").unwrap().size, 4);
    assert_eq!(
        names(&drive, ""),
        (
            vec!["docs/".to_string(), "empty/".to_string()],
            vec!["top.txt".to_string()]
        )
    );
    assert_eq!(
        names(&drive, "docs/"),
        (
            vec!["docs/sub/".to_string()],
            vec!["docs/".to_string(), "docs/a.txt".to_string()]
        )
    );
    let all = drive.list(&ListRequest::recursive("docs/")).unwrap();
    let keys: Vec<String> = all.objects.into_iter().map(|o| o.key).collect();
    assert_eq!(keys, ["docs/", "docs/a.txt", "docs/sub/", "docs/sub/b.txt"]);
    assert_eq!(bucket.counts().lists, 0);
}

#[test]
fn another_device_sees_the_files_and_a_stale_conditional_write_is_a_conflict() {
    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    let laptop = device(&bucket, &key, "laptop", false);
    laptop.put("report.docx", b"v1").unwrap();
    let phone = device(&bucket, &key, "phone", false);
    let seen = phone.head("report.docx").unwrap().etag.unwrap();
    assert_eq!(phone.get("report.docx").unwrap(), b"v1");

    laptop.put("report.docx", b"v2 from the laptop").unwrap();
    // The phone's change was based on v1: refused, nothing written (D52).
    let stale = phone.put_if("report.docx", b"v2 from the phone", &Precondition::Matches(seen));
    assert!(matches!(stale, Err(DriveError::Conflict { .. })));
    assert_eq!(phone.get("report.docx").unwrap(), b"v2 from the laptop");
    // Based on what is there now, it goes through.
    let now = phone.head("report.docx").unwrap().etag.unwrap();
    phone
        .put_if("report.docx", b"v3 from the phone", &Precondition::Matches(now))
        .unwrap();
    assert_eq!(laptop.get("report.docx").unwrap(), b"v3 from the phone");
    assert!(matches!(
        laptop.put_if("report.docx", b"x", &Precondition::Absent),
        Err(DriveError::Conflict { .. })
    ));
}

#[test]
fn renaming_and_deleting_folders_change_the_index_only() {
    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    let drive = device(&bucket, &key, "laptop", false);
    drive.put("old/a.txt", b"a").unwrap();
    drive.put("old/deep/b.txt", b"b").unwrap();
    let data_objects = |bucket: &MemoryBucket| {
        bucket
            .objects()
            .into_iter()
            .filter(|(k, _)| k.starts_with("data/"))
            .count()
    };
    let before = data_objects(&bucket);
    drive.rename("old/", "new/").unwrap();
    assert_eq!(data_objects(&bucket), before);
    assert_eq!(drive.get("new/deep/b.txt").unwrap(), b"b");
    assert!(matches!(drive.get("old/a.txt"), Err(DriveError::NotFound { .. })));
    assert_eq!(names(&drive, "").0, vec!["new/".to_string()]);
    drive.delete_folder("new/").unwrap();
    assert_eq!(names(&drive, ""), (Vec::new(), Vec::new()));
    // The index keeps the history: the old versions stay restorable.
    assert_eq!(data_objects(&bucket), before);
}

#[test]
fn a_batch_whose_expectation_fails_changes_nothing() {
    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    let drive = device(&bucket, &key, "laptop", false);
    drive.put("a.txt", b"a").unwrap();
    let repo = MetaRepo::open(bucket.clone(), key.clone(), "tool", "Tool").unwrap();
    let index = Index::new(repo).with_poll_every(0);
    let entry = index.get("a.txt").unwrap().unwrap();
    let result = index.apply(vec![
        IndexChange::Put {
            path: "b.txt".to_string(),
            entry: entry.clone(),
            expect: Expect::Absent,
        },
        IndexChange::Put {
            path: "a.txt".to_string(),
            entry,
            expect: Expect::Absent,
        },
    ]);
    assert!(matches!(result, Err(DriveError::Conflict { key }) if key == "a.txt"));
    assert_eq!(index.get("b.txt").unwrap(), None);
    // The drive's own paths are no drive paths.
    let reserved = index.apply(vec![IndexChange::Remove {
        path: ".azlin/policy.toml".to_string(),
        expect: Expect::Any,
    }]);
    assert!(matches!(reserved, Err(DriveError::InvalidKey { .. })));
    assert_eq!(index.get(".azlin/policy.toml").unwrap(), None);
}

#[test]
fn the_bucket_of_an_encrypted_drive_over_the_index_holds_no_name() {
    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    let drive = device(&bucket, &key, "Felix laptop", false);
    drive.put("Holiday photos/beach.jpg", b"JPEG bytes of the beach").unwrap();
    drive.rename("Holiday photos/beach.jpg", "Holiday photos/sea.jpg").unwrap();
    for (name, bytes) in bucket.objects() {
        assert!(name.starts_with(".azlin/meta/") || name.starts_with("data/"), "{name}");
        for secret in [&b"Holiday"[..], b"beach", b"sea.jpg", b"Felix", b"JPEG bytes"] {
            assert!(!contains(&bytes, secret), "{name} holds {secret:?}");
            assert!(!contains(name.as_bytes(), secret), "{name}");
        }
    }
}

#[test]
fn a_lazy_device_lists_a_folder_without_reading_a_whole_pack() {
    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    {
        // One import of many files: one pack of several chunks.
        let mut repo = MetaRepo::create(bucket.clone(), key.clone(), "laptop", "Laptop").unwrap();
        let laptop = device(&bucket, &key, "laptop-writer", false);
        laptop.put("docs/a.txt", b"alpha").unwrap();
        repo.pull().unwrap();
        let entry = drive_index_entry(&bucket, &key, "docs/a.txt");
        let changes: Vec<crate::meta::Change> = (0..2000)
            .map(|i| {
                let mut copy = entry.clone();
                copy.size = i;
                crate::meta::Change::Put {
                    path: format!("photos/{i:04}.jpg"),
                    id: repo.write_blob(&pointer::encode(&copy)),
                }
            })
            .collect();
        repo.commit(&changes, "import", &mut keep_both).unwrap();
    }
    let before = bucket.whole_reads().len();
    let tablet = device(&bucket, &key, "tablet", true);
    let (folders, files) = names(&tablet, "docs/");
    assert!(folders.is_empty());
    assert_eq!(files, ["docs/", "docs/a.txt"]);
    assert_eq!(tablet.get("docs/a.txt").unwrap(), b"alpha");
    let reads = bucket.whole_reads();
    let tablet_reads = &reads[before..];
    assert!(tablet_reads.iter().all(|k| !k.ends_with(".pack")), "{tablet_reads:?}");
    assert!(bucket.counts().range_reads > 0);
    assert_eq!(bucket.counts().lists, 0);
}

#[test]
fn an_encrypted_drive_opens_over_a_bucket_with_or_without_an_index() {
    let key = DriveKey::generate().unwrap();
    let bucket = Arc::new(MemoryBucket::new());
    let first = open_encrypted_drive(Arc::clone(&bucket), key.clone(), "laptop", "Laptop", None, false)
        .unwrap();
    first.put("a.txt", b"a").unwrap();
    let second =
        open_encrypted_drive(Arc::clone(&bucket), key, "phone", "Phone", None, true).unwrap();
    assert_eq!(second.get("a.txt").unwrap(), b"a");
}
