//! The encrypted drive over the drive index (feature `encryption`).

use std::sync::{Arc, Mutex};

use super::{TempDir, Unplugged};
use crate::{
    crypto::DriveKey,
    encrypted::{
        EncryptedDrive, Expect, IndexChange, IndexEntry, IndexPage, IndexProvider, NameIndex,
    },
    meta::{
        keys, merge::keep_both, open_encrypted_drive, pointer, Bucket, IndexProfile, Maintenance,
        MemoryBucket, MetaIndex, MetaIndexProvider, MetaRepo, RepoOptions, RestoreSource,
    },
    restore::RestoreNotice,
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
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
        ..RepoOptions::default()
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

#[test]
fn the_provider_opens_a_drives_index_and_keeps_this_devices_copy_and_id() {
    let dir = TempDir::new("meta-provider");
    let provider = MetaIndexProvider::new(IndexProfile {
        cache_root: Some(dir.path().to_path_buf()),
        ..IndexProfile::in_memory("Laptop")
    });
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    let key = DriveKey::generate().unwrap();
    let index = provider
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index);
    drive.put("a.txt", b"a").unwrap();
    let id = std::fs::read_to_string(dir.path().join("device-id")).unwrap();

    let again = provider
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    assert!(again.get("a.txt").unwrap().is_some());
    assert_eq!(std::fs::read_to_string(dir.path().join("device-id")).unwrap(), id);
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != "device-id")
        .collect();
    // One drive's copy, its folder named by a hash of the drive's id.
    assert_eq!(names.len(), 1, "{names:?}");
    assert_eq!(names[0].len(), 32);
    assert!(names[0].bytes().all(|b| b.is_ascii_hexdigit()));
}

#[test]
fn the_provider_maintains_a_drives_index_while_a_copy_of_it_is_open() {
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    let key = DriveKey::generate().unwrap();
    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Laptop"));
    let rules = Maintenance {
        compact_at_packs: 2,
        ..Maintenance::default()
    };
    assert_eq!(
        provider.maintain(Arc::clone(&bucket), &key, &rules).unwrap(),
        None,
        "a bucket without an index has nothing to maintain"
    );

    let index = provider
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index);
    for name in ["a.txt", "b.txt", "c.txt"] {
        drive.put(name, name.as_bytes()).unwrap();
    }
    let done = provider
        .maintain(Arc::clone(&bucket), &key, &rules)
        .unwrap()
        .expect("no other device holds the lease");
    assert!(done.compacted, "three packs folded into one");

    // A copy opened afterwards reads every file; the one that was open goes on writing.
    let again = provider
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    assert!(again.get("c.txt").unwrap().is_some());
    drive.put("d.txt", b"d").unwrap();
    assert_eq!(drive.get("a.txt").unwrap(), b"a.txt");
    assert_eq!(drive.get("d.txt").unwrap(), b"d");
}

/// AZ-M4. A maintenance round that knows its drive refuses a manifest older than the one this
/// computer's copy of the drive's index has seen: a round that opened the store from nothing took
/// whatever the bucket served, and its sweep retired the objects of the newer state the older
/// manifest does not name - the rollback made permanent.
#[test]
fn az_m4_a_maintenance_round_of_a_drive_refuses_a_manifest_older_than_its_copy_has_seen() {
    let dir = TempDir::new("meta-maintain-floor");
    let raw = MemoryBucket::new();
    let bucket: Arc<dyn Drive> = Arc::new(raw.clone());
    let key = DriveKey::generate().unwrap();
    let provider = MetaIndexProvider::new(IndexProfile {
        cache_root: Some(dir.path().to_path_buf()),
        ..IndexProfile::in_memory("Laptop")
    });
    let index = provider
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index);
    drive.put("a.txt", b"a").unwrap();
    let older = raw.read(keys::MANIFEST).unwrap().unwrap().0;
    drive.put("b.txt", b"b").unwrap();
    // The storage serves the older manifest again, and nobody announced a restore.
    raw.overwrite(keys::MANIFEST, &older);

    let error = provider
        .maintain_drive(
            "drive-1",
            Arc::clone(&bucket),
            &key,
            &Maintenance::default(),
        )
        .unwrap_err();
    assert!(
        matches!(&error, DriveError::Protocol(text) if text.contains("older index")),
        "{error:?}"
    );
    // Nothing was retired on the strength of it.
    let swept: Vec<_> = raw
        .read(keys::MANIFEST)
        .unwrap()
        .map(|(bytes, _)| bytes)
        .into_iter()
        .collect();
    assert_eq!(swept, [older], "the manifest is as the storage served it");
}

/// X-DELTA-SYNC4-10 (the floor). A purge that knows its drive refuses a manifest older than the one
/// this computer's copy of the drive's index has seen, as the maintenance round does (AZ-M4): a
/// rolled-back head makes an object that a later commit names again a candidate, and the purge
/// deletes candidates. RED on the old code: `purge` had no drive id and opened the repository with no
/// floor at all.
#[test]
fn x_delta_sync4_10_a_purge_of_a_drive_refuses_a_manifest_older_than_its_copy_has_seen() {
    let dir = TempDir::new("meta-purge-floor");
    let raw = MemoryBucket::new();
    raw.set_server_clock(crate::time::now_unix);
    let bucket: Arc<dyn Drive> = Arc::new(raw.clone());
    let key = DriveKey::generate().unwrap();
    let provider = MetaIndexProvider::new(IndexProfile {
        cache_root: Some(dir.path().to_path_buf()),
        ..IndexProfile::in_memory("Laptop")
    });
    let index = provider
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index);
    drive.put("a.txt", b"a, one").unwrap();
    drive.put("a.txt", b"a, two").unwrap();
    let older = raw.read(keys::MANIFEST).unwrap().unwrap().0;
    drive.put("b.txt", b"b").unwrap();
    // The storage serves the older manifest again, and nobody announced a restore.
    raw.overwrite(keys::MANIFEST, &older);
    let objects = |bucket: &MemoryBucket| {
        bucket
            .objects()
            .into_iter()
            .filter(|(key, _)| key.starts_with("data/"))
            .count()
    };
    let before = objects(&raw);

    let confirmation = crate::meta::reclaim::PurgeConfirmation::lab(crate::time::now_unix() - 60);
    let error = provider
        .purge("drive-1", Arc::clone(&bucket), &key, &confirmation)
        .unwrap_err();
    assert!(
        matches!(&error, DriveError::Protocol(text) if text.contains("older index")),
        "{error:?}"
    );
    assert_eq!(
        objects(&raw),
        before,
        "nothing was erased on the strength of it"
    );
}

/// A drive that answers everything but a listing: its manifest is missing, and whether anything
/// else is there cannot be told.
struct Unlistable(MemoryBucket);

impl Drive for Unlistable {
    fn list(&self, _: &ListRequest) -> Result<ListPage, DriveError> {
        Err(DriveError::Transport(
            "the listing got no answer".to_string(),
        ))
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
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.0.put_if(key, bytes, condition)
    }
}

/// AZ-M11. "No manifest" is not "no repository" and a listing that failed is not "nothing is
/// there": a drive that cannot be listed is not called empty, and no index is created over it.
#[test]
fn az_m11_a_drive_that_cannot_be_listed_is_not_called_empty() {
    let key = DriveKey::generate().unwrap();
    let raw = MemoryBucket::new();
    let shared: Arc<dyn Drive> = Arc::new(Unlistable(raw.clone()));
    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Phone"));
    let opened = provider.open_index("d_1", shared, &key);
    assert!(
        matches!(opened, Err(DriveError::Transport(_))),
        "the failed listing is the answer: {:?}",
        opened.err()
    );
    assert!(raw.objects().is_empty(), "no index was created");
}

/// A bucket out of reach: every request fails the way a lost connection does.
struct OutOfReach;

fn no_connection() -> DriveError {
    DriveError::Transport("no connection to the storage".to_string())
}

impl Drive for OutOfReach {
    fn list(&self, _: &ListRequest) -> Result<ListPage, DriveError> {
        Err(no_connection())
    }
    fn get(&self, _: &str) -> Result<Vec<u8>, DriveError> {
        Err(no_connection())
    }
    fn get_range(&self, _: &str, _: ByteRange) -> Result<Vec<u8>, DriveError> {
        Err(no_connection())
    }
    fn put(&self, _: &str, _: &[u8]) -> Result<(), DriveError> {
        Err(no_connection())
    }
    fn delete(&self, _: &str) -> Result<(), DriveError> {
        Err(no_connection())
    }
    fn head(&self, _: &str) -> Result<ObjectInfo, DriveError> {
        Err(no_connection())
    }
}

#[test]
fn a_copy_sealed_before_a_rotation_opens_offline_in_the_rotation_window_only() {
    let dir = TempDir::new("meta-window");
    let provider = MetaIndexProvider::new(IndexProfile {
        cache_root: Some(dir.path().to_path_buf()),
        ..IndexProfile::in_memory("Phone")
    });
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    let old = DriveKey::generate().unwrap();
    let new = DriveKey::generate().unwrap();
    let index = provider
        .open_index("drive-1", Arc::clone(&bucket), &old)
        .unwrap();
    EncryptedDrive::new(Arc::clone(&bucket), old.clone(), index)
        .put("a.txt", b"a")
        .unwrap();

    let away: Arc<dyn Drive> = Arc::new(OutOfReach);
    assert!(
        provider
            .open_index("drive-1", Arc::clone(&away), &new)
            .is_err(),
        "the new key alone does not open the copy"
    );
    let index = provider
        .open_index_in_window("drive-1", Arc::clone(&away), &new, &old)
        .unwrap();
    assert!(index.get("a.txt").unwrap().is_some());
    // The window is the rotation's: a key from no rotation opens nothing.
    let stranger = DriveKey::generate().unwrap();
    assert!(provider
        .open_index_in_window("drive-1", away, &new, &stranger)
        .is_err());
}

/// F5 asks the drive again (RECOVERY17 at azdrive_add_e2e 6d: an encrypted drive's F5 answered
/// from this device's copy and never reached the node that refused it). A listing marked
/// `refreshed` pulls at once, its poll time aside: another device's change shows, a node that
/// refuses says so; a lost connection still answers from the copy.
#[test]
fn a_refreshed_listing_asks_the_bucket_again_before_its_poll_time() {
    let bucket = Arc::new(Unplugged::default());
    let key = DriveKey::generate().unwrap();
    let laptop = MetaIndex::new(
        MetaRepo::create(bucket.clone(), key.clone(), "laptop", "Laptop").unwrap(),
    )
    .with_poll_every(0);
    let phone = MetaIndex::new(MetaRepo::open(bucket.clone(), key, "phone", "Phone").unwrap())
        .with_poll_every(3600);
    let paths = |page: IndexPage| -> Vec<String> {
        page.entries.into_iter().map(|(path, _)| path).collect()
    };
    let root = || ListRequest::folder("");
    assert!(paths(phone.list(&root()).unwrap()).is_empty());
    laptop
        .apply(vec![IndexChange::Put {
            path: "new.txt".to_string(),
            entry: IndexEntry {
                size: 3,
                modified: Some(1_760_000_000),
                object: None,
            },
            expect: Expect::Absent,
        }])
        .unwrap();
    assert!(
        paths(phone.list(&root()).unwrap()).is_empty(),
        "within the poll time: the copy"
    );
    assert_eq!(
        paths(phone.list(&root().refreshed()).unwrap()),
        vec!["new.txt".to_string()],
        "F5: the bucket"
    );
    bucket.set_refusing(true);
    assert!(phone.list(&root()).is_ok(), "within the poll time: the copy");
    match phone.list(&root().refreshed()) {
        Err(DriveError::Service(refused)) => {
            assert_eq!(refused.azlin_error, Some(crate::AzlinError::ReadOnlyUnpaid));
            assert_eq!(refused.request_id.as_deref(), Some("REQ-UNPAID"));
        }
        other => panic!("F5 says the node's refusal, not {other:?}"),
    }
    bucket.set_refusing(false);
    bucket.set_down(true);
    assert_eq!(
        paths(phone.list(&root().refreshed()).unwrap()),
        vec!["new.txt".to_string()],
        "no connection: the copy answers"
    );
}

#[test]
fn an_index_whose_bucket_cannot_be_reached_still_answers_reads_from_the_copy() {
    let bucket = Arc::new(Unplugged::default());
    let key = DriveKey::generate().unwrap();
    let repo = MetaRepo::create(bucket.clone(), key, "laptop", "Laptop").unwrap();
    let index = MetaIndex::new(repo).with_poll_every(0);
    let entry = IndexEntry {
        size: 1,
        modified: Some(1_760_000_000),
        object: None,
    };
    index
        .apply(vec![IndexChange::Put {
            path: "docs/a.txt".to_string(),
            entry: entry.clone(),
            expect: Expect::Absent,
        }])
        .unwrap();

    bucket.set_down(true);
    assert_eq!(index.get("docs/a.txt").unwrap(), Some(entry.clone()));
    let page = index.list(&ListRequest::folder("docs/")).unwrap();
    assert_eq!(page.entries.len(), 2, "the folder's marker and its file");
    // A change needs the bucket.
    let change = IndexChange::Put {
        path: "docs/b.txt".to_string(),
        entry,
        expect: Expect::Absent,
    };
    assert!(matches!(
        index.apply(vec![change]),
        Err(DriveError::Transport(_))
    ));
}

/// AZ-M11. A drive that lost only its manifest keeps its log, its packs and its files. A device
/// that opened it used to create a new, empty index over them, and the next sweep retired
/// everything the old index was.
#[test]
fn az_m11_an_index_that_lost_only_its_manifest_is_not_replaced_by_an_empty_one() {
    use crate::meta::{keys, Bucket};

    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    let drive = device(&bucket, &key, "laptop", false);
    drive.put("docs/a.txt", b"the only copy").unwrap();
    let before = bucket.objects().len();
    bucket.remove(keys::MANIFEST).unwrap();

    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Phone"));
    let shared: Arc<dyn Drive> = Arc::new(bucket.clone());
    let opened = provider.open_index("d_1", Arc::clone(&shared), &key);
    assert!(
        matches!(opened, Err(DriveError::Corrupt { .. })),
        "a new index over the old one's files is no answer: {:?}",
        opened.err()
    );
    assert!(
        bucket.objects().iter().all(|(k, _)| k != keys::MANIFEST),
        "nothing was created"
    );
    assert_eq!(
        bucket.objects().len(),
        before - 1,
        "and nothing else was touched"
    );
}

#[test]
fn az_m11_a_drive_with_data_objects_and_no_index_at_all_is_not_a_new_drive() {
    let key = DriveKey::generate().unwrap();
    let bucket = MemoryBucket::new();
    // What an operator's restore brought back: the data objects, not the index.
    bucket
        .put("data/ab/0123456789abcdef0123456789abcdef", b"sealed bytes")
        .unwrap();
    let shared: Arc<dyn Drive> = Arc::new(bucket.clone());
    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Phone"));
    assert!(matches!(
        provider.open_index("d_1", Arc::clone(&shared), &key),
        Err(DriveError::Corrupt { .. })
    ));

    // A user's own folder called "data" is no data object: a new drive still gets its index.
    let plain = MemoryBucket::new();
    plain.put("data/notes.txt", b"the user's own file").unwrap();
    let shared: Arc<dyn Drive> = Arc::new(plain.clone());
    assert!(provider
        .open_index("d_2", Arc::clone(&shared), &key)
        .is_ok());

    // And an empty bucket does.
    let empty: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    assert!(provider.open_index("d_3", empty, &key).is_ok());
}

/// X-DELTA-AZDRIVE-SYNC-3 (AZ-M4). The provider an app builds is where its device says where its
/// drives' restores are announced, and the profile cannot be written without saying it: a provider
/// whose source is `RestoreSource::none` refuses a drive the operator restored from its lagging
/// off-site copy, for good; the same copy of the index under a provider whose source gives the
/// token server's announcement takes the restore once and reads the restored drive. RED not run:
/// AzMail, the bridge and the command line built the provider with `new(name)` and no source, and
/// only the first of the two cases existed for them.
#[test]
fn x_delta_azdrive_sync_3_a_provider_with_a_restore_source_takes_an_announced_restore_and_one_without_stays_refused(
) {
    let dir = TempDir::new("meta-provider-restore");
    let raw = MemoryBucket::new();
    let bucket: Arc<dyn Drive> = Arc::new(raw.clone());
    let key = DriveKey::generate().unwrap();
    let announced: Arc<Mutex<Option<RestoreNotice>>> = Arc::new(Mutex::new(None));
    let source = {
        let cell = Arc::clone(&announced);
        RestoreSource::new(move || cell.lock().unwrap().clone())
    };
    let profile = |restore: RestoreSource| IndexProfile {
        cache_root: Some(dir.path().to_path_buf()),
        restore,
        ..IndexProfile::in_memory("Laptop")
    };

    let provider = MetaIndexProvider::new(profile(source.clone()));
    let index = provider
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index);
    drive.put("a.txt", b"a").unwrap();
    let older = raw.read(keys::MANIFEST).unwrap().unwrap().0;
    drive.put("b.txt", b"b").unwrap();
    drop(drive);
    // The operator restored the bucket from the off-site copy that lags: the older manifest.
    raw.overwrite(keys::MANIFEST, &older);

    // A provider that was given no source: refused, and the copy is as it was.
    let without = MetaIndexProvider::new(profile(RestoreSource::none(
        "this test's app has no token server",
    )));
    let refused = without
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .err()
        .expect("a restored drive with no announcement is a rollback");
    assert!(
        matches!(&refused, DriveError::Protocol(text) if text.contains("older index")),
        "{refused:?}"
    );

    // The token server announces the restore: the provider with the source takes it, once.
    *announced.lock().unwrap() = Some(RestoreNotice {
        epoch: 1,
        ..RestoreNotice::default()
    });
    let with = MetaIndexProvider::new(profile(source.clone()));
    let index = with
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index);
    let page = drive.list(&ListRequest::recursive("")).unwrap();
    let files: Vec<&str> = page
        .objects
        .iter()
        .map(|o| o.key.as_str())
        .filter(|key| key.ends_with(".txt"))
        .collect();
    assert_eq!(
        files,
        ["a.txt"],
        "the restored drive; b.txt was after the copy"
    );
    // The same announcement again excuses nothing: the epoch this copy took is remembered.
    drop(drive);
    let again = MetaIndexProvider::new(profile(source));
    assert!(again
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .is_ok());
}

/// `RestoreSource::none` says why, and a set source does not.
#[test]
fn x_delta_azdrive_sync_3_a_restore_source_that_is_none_says_why() {
    let none = RestoreSource::none("a test");
    assert!(!none.is_set());
    assert_eq!(none.none_because(), Some("a test"));
    assert!(format!("{none:?}").contains("a test"), "{none:?}");
    let set = RestoreSource::new(|| None);
    assert!(set.is_set());
    assert_eq!(set.none_because(), None);
    assert_eq!(set.notice(), None);
    assert_eq!(none, RestoreSource::default(), "no source is no source");
}
