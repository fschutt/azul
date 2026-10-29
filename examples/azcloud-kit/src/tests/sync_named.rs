//! A folder synced to a drive BY NAME ([`Target::Named`], an encrypted drive's way): every file
//! under its own key, the drive's own index naming it, no index of the sync in the bucket.
//! Plain buckets in memory stand in for the drive (an S3 drive on the fake service); one that
//! names each file's BLAKE3 in its metadata stands in for an encrypted drive's index.
//! And a drive as the plain sync's store ([`DriveStore`]).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use azul_storage::{
    testing::TempDir, ByteRange, Credentials, Drive, DriveError, ListPage, ListRequest,
    ObjectInfo, Precondition, S3Config, S3Drive, CONTENT_HASH_METADATA,
};

use super::{
    fake_s3::{FakeS3, S3Handle, BUCKET},
    S3,
};
use crate::sync::{
    drive_store::DriveStore,
    fetch_file,
    local::{hash_bytes, path_of},
    problems::Code,
    remote::index_key,
    sync_folder, sync_to, Conditional, LocalRoot, RemoteStore, RunHooks, SyncOptions, SyncReport,
    Target,
};

const PREFIX: &str = "Documents/";

/// An S3 drive on the fake service.
pub(super) fn s3_drive(s3: &Arc<FakeS3>) -> Arc<dyn Drive> {
    let config = S3Config {
        endpoint: S3.to_string(),
        region: String::from("us-east-1"),
        bucket: BUCKET.to_string(),
        path_style: true,
    };
    let drive = S3Drive::new(
        config,
        Credentials::new("AKID1", "secret-of-AKID1"),
        Box::new(S3Handle(s3.clone())),
    )
    .unwrap()
    .with_clock(|| 1_791_450_900);
    Arc::new(drive)
}

/// [`s3_drive`] in parts of 1 MiB whose file uploads keep their state in `resume` (they resume
/// with the parts the bucket lacks).
pub(super) fn s3_drive_resuming(s3: &Arc<FakeS3>, resume: &Path) -> Arc<dyn Drive> {
    let config = S3Config {
        endpoint: S3.to_string(),
        region: String::from("us-east-1"),
        bucket: BUCKET.to_string(),
        path_style: true,
    };
    let drive = S3Drive::new(
        config,
        Credentials::new("AKID1", "secret-of-AKID1"),
        Box::new(S3Handle(s3.clone())),
    )
    .unwrap()
    .with_clock(|| 1_791_450_900)
    .with_part_size(1024 * 1024)
    .with_resume_dir(resume);
    Arc::new(drive)
}

/// A drive that names each file's BLAKE3 in its metadata, as an encrypted drive's index does.
struct NamesHashes(Arc<dyn Drive>);

impl Drive for NamesHashes {
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
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.0.put_if(key, bytes, condition)
    }
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        let bytes = self.0.get(key)?;
        Ok(vec![(CONTENT_HASH_METADATA.to_string(), hash_bytes(&bytes))])
    }
}

struct Device {
    name: &'static str,
    folder: TempDir,
    state: TempDir,
}

impl Device {
    fn new(name: &'static str) -> Device {
        Device {
            name,
            folder: TempDir::new(&format!("azcloud-named-{name}")),
            state: TempDir::new(&format!("azcloud-named-state-{name}")),
        }
    }

    fn root(&self) -> LocalRoot {
        LocalRoot::folder(self.folder.path())
    }

    fn index(&self) -> PathBuf {
        self.state.path().join("index.json")
    }

    fn opts(&self) -> SyncOptions {
        SyncOptions::new(PREFIX, "d-test", self.name).unwrap()
    }

    fn run(&self, drive: &dyn Drive, hooks: &RunHooks<'_>) -> SyncReport {
        sync_to(Target::Named(drive), &self.root(), &self.index(), &self.opts(), hooks).unwrap()
    }

    fn sync(&self, drive: &dyn Drive) -> SyncReport {
        self.run(drive, &RunHooks::default())
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let path = path_of(self.folder.path(), key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn read(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(path_of(self.folder.path(), key)).ok()
    }

    fn tree(&self) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        walk(self.folder.path(), "", &mut out);
        out
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let key = format!("{prefix}{}", entry.file_name().to_string_lossy());
        // The sync's own bookkeeping (the folder's marker, its trash) is no file of the user.
        if key == ".azlin" {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            walk(&entry.path(), &format!("{key}/"), out);
        } else {
            out.insert(key, fs::read(entry.path()).unwrap());
        }
    }
}

#[test]
fn a_folder_synced_by_name_keeps_each_file_under_its_own_key_and_no_sync_index() {
    let s3 = FakeS3::new();
    let drive = s3_drive(&s3);
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("notes/a.md", b"alpha");
    a.write("b.txt", b"beta");
    let sent = a.sync(&*drive);
    assert_eq!(sent.files_up, 2, "{}", sent.summary());
    assert_eq!(s3.read("Documents/notes/a.md").as_deref(), Some(&b"alpha"[..]));
    assert_eq!(s3.read("Documents/b.txt").as_deref(), Some(&b"beta"[..]));
    // No plain index and no blobs: the folder's record is the one file of the sync's own (on
    // an encrypted drive sealed like the others, its name in the sealed index).
    assert_eq!(
        s3.keys()
            .into_iter()
            .filter(|k| k.contains(".azlin"))
            .collect::<Vec<_>>(),
        vec![String::from("Documents/.azlin/sync/record.json")],
        "{:?}",
        s3.keys()
    );
    let got = b.sync(&*drive);
    assert_eq!(got.files_down, 2, "{}", got.summary());
    assert_eq!(b.tree(), a.tree());
}

/// Only `.azlin/` at the prefix's root is a sync's bookkeeping: a user's folder of that name
/// deeper down syncs as any other.
#[test]
fn a_users_azlin_folder_below_the_root_syncs_by_name_too() {
    let s3 = FakeS3::new();
    let drive = s3_drive(&s3);
    let a = Device::new("dev-a-nested");
    let b = Device::new("dev-b-nested");
    a.write("projects/.azlin/notes.txt", b"not the root's .azlin");
    a.sync(&*drive);
    assert_eq!(
        s3.read("Documents/projects/.azlin/notes.txt").as_deref(),
        Some(&b"not the root's .azlin"[..])
    );
    let got = b.sync(&*drive);
    assert_eq!(got.files_down, 1, "{}", got.summary());
    assert_eq!(
        b.read("projects/.azlin/notes.txt").as_deref(),
        Some(&b"not the root's .azlin"[..])
    );
    let again = a.sync(&*drive);
    assert_eq!((again.files_up, again.files_down), (0, 0), "{}", again.summary());
}

#[test]
fn devices_synced_by_name_converge_on_edits_and_deletes() {
    let s3 = FakeS3::new();
    let drive = s3_drive(&s3);
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("a.md", b"alpha");
    a.write("old.txt", b"old");
    a.sync(&*drive);
    b.sync(&*drive);

    b.write("a.md", b"alpha, edited on b");
    b.write("new.md", b"new on b");
    fs::remove_file(path_of(b.folder.path(), "old.txt")).unwrap();
    let sent = b.sync(&*drive);
    assert_eq!(sent.files_up, 2, "{}", sent.summary());
    assert_eq!(sent.deleted_there, 1);
    assert!(s3.read("Documents/old.txt").is_none());
    let got = a.sync(&*drive);
    assert_eq!(got.files_down, 2, "{}", got.summary());
    assert_eq!(got.deleted_here, 1);
    assert_eq!(a.tree(), b.tree());
    let again = a.sync(&*drive);
    assert_eq!((again.files_up, again.files_down), (0, 0), "{}", again.summary());
}

#[test]
fn an_edit_made_on_the_drive_during_a_run_is_never_overwritten() {
    let s3 = FakeS3::new();
    let drive = s3_drive(&s3);
    let a = Device::new("dev-a");
    a.write("x.md", b"x1");
    a.sync(&*drive);
    a.write("x.md", b"x2 from a");
    // Another device writes x.md between a's read of the drive and a's upload.
    s3.before_next_cas(|s| {
        s.write("Documents/x.md", b"from elsewhere".to_vec());
    });
    let got = a.sync(&*drive);
    assert!(
        got.changed_during_sync.iter().any(|line| line.starts_with("x.md")),
        "{:?}",
        got.changed_during_sync
    );
    assert_eq!(s3.read("Documents/x.md").as_deref(), Some(&b"from elsewhere"[..]));
    assert_eq!(a.read("x.md").as_deref(), Some(&b"x2 from a"[..]), "mine stays here");

    let hooks = RunHooks {
        hold_conflicts: true,
        ..RunHooks::default()
    };
    let asked = a.run(&*drive, &hooks);
    assert_eq!(asked.held.len(), 1, "{}", asked.summary());
    assert_eq!(asked.held[0].key, "x.md");
}

#[test]
fn a_drive_that_names_the_content_hash_needs_no_download_to_agree() {
    let s3 = FakeS3::new();
    let drive = NamesHashes(s3_drive(&s3));
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("same.txt", b"the same on both");
    a.write("docs/also.txt", b"also the same");
    a.sync(&drive);
    // b has the same files before its first sync.
    b.write("same.txt", b"the same on both");
    b.write("docs/also.txt", b"also the same");
    let got = b.sync(&drive);
    assert_eq!(got.files_down, 0, "{}", got.summary());
    assert_eq!(got.files_up, 0);
    assert!(got.conflicts.is_empty());
    assert_eq!(got.unchanged, 2);
}

#[test]
fn a_version_this_device_wrote_is_known_without_reading_it_again() {
    let s3 = FakeS3::new();
    let drive = s3_drive(&s3);
    let a = Device::new("dev-a");
    a.write("one.txt", b"1");
    a.write("two.txt", b"22");
    a.sync(&*drive);
    s3.clear_log();
    let again = a.sync(&*drive);
    assert_eq!(again.unchanged, 2, "{}", again.summary());
    assert_eq!((again.files_up, again.files_down), (0, 0));
    assert_eq!(s3.count("GET"), 0, "{:?}", s3.log.lock().unwrap());
    assert_eq!(s3.count("PUT"), 0);
}

#[test]
fn a_cloud_only_file_of_a_drive_synced_by_name_comes_down_when_opened() {
    let s3 = FakeS3::new();
    let drive = s3_drive(&s3);
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("big.bin", b"a big file");
    a.sync(&*drive);
    let nothing = |_key: &str, _size: u64| false;
    let hooks = RunHooks {
        fetch: Some(&nothing),
        ..RunHooks::default()
    };
    let got = b.run(&*drive, &hooks);
    assert_eq!(got.cloud_only, vec![String::from("big.bin")]);
    assert!(b.read("big.bin").is_none());
    let path = fetch_file(Target::Named(&*drive), &b.root(), &b.index(), &b.opts(), "big.bin")
        .unwrap();
    assert_eq!(fs::read(path).unwrap(), b"a big file");
    let again = b.sync(&*drive);
    assert_eq!((again.files_up, again.files_down), (0, 0), "{}", again.summary());
    assert!(again.cloud_only.is_empty());
}

/// Forgets everything the device kept of its pairing (a re-login, a lost state folder): its
/// folder stays as it is.
fn lose_state(device: &Device) {
    for entry in fs::read_dir(device.state.path()).unwrap().flatten() {
        if entry.file_type().unwrap().is_dir() {
            fs::remove_dir_all(entry.path()).unwrap();
        } else {
            fs::remove_file(entry.path()).unwrap();
        }
    }
}

/// A file one device deleted is no new file to a device that lost its state and still holds
/// it (D5): the drive keeps the delete, the copy stays here and is not sent again.
#[test]
fn a_file_deleted_on_the_drive_is_not_sent_again_by_a_device_that_lost_its_state() {
    let s3 = FakeS3::new();
    let drive = NamesHashes(s3_drive(&s3));
    let a = Device::new("dev-a-tomb");
    let b = Device::new("dev-b-tomb");
    a.write("keep.txt", b"kept");
    a.write("gone.txt", b"deleted on a");
    a.sync(&drive);
    b.sync(&drive);
    fs::remove_file(path_of(a.folder.path(), "gone.txt")).unwrap();
    let sent = a.sync(&drive);
    assert_eq!(sent.deleted_there, 1, "{}", sent.summary());
    lose_state(&b);
    let got = b.sync(&drive);
    assert!(s3.read("Documents/gone.txt").is_none(), "{}", got.summary());
    assert_eq!(b.read("gone.txt").as_deref(), Some(&b"deleted on a"[..]), "kept here");
    assert!(
        got.problems
            .iter()
            .any(|p| p.code == Code::DeletedElsewhereKept && p.key == "gone.txt"),
        "{:?}",
        got.problems
    );
    let again = a.sync(&drive);
    assert_eq!(again.files_down, 0, "{}", again.summary());
    assert!(a.read("gone.txt").is_none());
}

/// The time a file was last changed comes back with it from a drive synced by name, to the
/// nanosecond, and is no change for the next run.
#[test]
fn a_files_modification_time_travels_through_a_drive_synced_by_name() {
    let s3 = FakeS3::new();
    let drive = NamesHashes(s3_drive(&s3));
    let a = Device::new("dev-a-time");
    let b = Device::new("dev-b-time");
    a.write("photo.jpg", b"taken long ago");
    let taken = UNIX_EPOCH + Duration::new(1_600_000_000, 123_456_789);
    fs::File::options()
        .write(true)
        .open(path_of(a.folder.path(), "photo.jpg"))
        .unwrap()
        .set_modified(taken)
        .unwrap();
    a.sync(&drive);
    b.sync(&drive);
    let got = fs::metadata(path_of(b.folder.path(), "photo.jpg"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(got, taken);
    let again = b.sync(&drive);
    assert_eq!((again.files_up, again.files_down), (0, 0), "{}", again.summary());
    let back = a.sync(&drive);
    assert_eq!((back.files_up, back.files_down), (0, 0), "{}", back.summary());
}

/// The executable bit is part of a file's content on a drive synced by name too.
#[cfg(unix)]
#[test]
fn the_executable_bit_travels_through_a_drive_synced_by_name() {
    use std::os::unix::fs::PermissionsExt;

    let s3 = FakeS3::new();
    let drive = NamesHashes(s3_drive(&s3));
    let a = Device::new("dev-a-exec");
    let b = Device::new("dev-b-exec");
    a.write("run.sh", b"#!/bin/sh\necho hi\n");
    a.write("plain.txt", b"not a program");
    let script = path_of(a.folder.path(), "run.sh");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    a.sync(&drive);
    b.sync(&drive);
    let mode = |device: &Device, key: &str| {
        fs::metadata(path_of(device.folder.path(), key))
            .unwrap()
            .permissions()
            .mode()
    };
    assert_ne!(mode(&b, "run.sh") & 0o100, 0, "{:o}", mode(&b, "run.sh"));
    assert_eq!(mode(&b, "plain.txt") & 0o111, 0, "{:o}", mode(&b, "plain.txt"));
    let again = b.sync(&drive);
    assert_eq!((again.files_up, again.files_down), (0, 0), "{}", again.summary());
    // The bit taken away on a reaches b as a change of the file.
    fs::set_permissions(&script, fs::Permissions::from_mode(0o644)).unwrap();
    let sent = a.sync(&drive);
    assert_eq!(sent.files_up, 1, "{}", sent.summary());
    b.sync(&drive);
    assert_eq!(mode(&b, "run.sh") & 0o111, 0, "{:o}", mode(&b, "run.sh"));
}

/// A mass delete the user confirmed on one device travels with its deletes: another device
/// applies it (into its trash) without asking its own user again (D1).
#[test]
fn a_mass_delete_confirmed_on_one_device_is_not_asked_again_on_another() {
    let s3 = FakeS3::new();
    let drive = s3_drive(&s3);
    let a = Device::new("dev-a-mass");
    let b = Device::new("dev-b-mass");
    for i in 0..12 {
        a.write(&format!("docs/{i}.txt"), format!("doc {i}").as_bytes());
    }
    a.sync(&*drive);
    b.sync(&*drive);
    fs::remove_dir_all(path_of(a.folder.path(), "docs")).unwrap();
    let mut confirmed = a.opts();
    confirmed.allow_mass_delete = true;
    let hooks = RunHooks::default();
    let sent = sync_to(Target::Named(&*drive), &a.root(), &a.index(), &confirmed, &hooks).unwrap();
    assert_eq!(sent.deleted_there, 12, "{}", sent.summary());
    let hooks = RunHooks {
        hold_mass_delete: true,
        ..RunHooks::default()
    };
    let got = b.run(&*drive, &hooks);
    assert!(got.mass_delete.is_none(), "{}", got.summary());
    assert_eq!(got.deleted_here, 12, "{}", got.summary());
    assert!(b.tree().is_empty(), "{:?}", b.tree().keys().collect::<Vec<_>>());
}

/// An empty folder travels through a drive synced by name too, and so does its delete.
#[test]
fn an_empty_folder_travels_through_a_drive_synced_by_name() {
    let s3 = FakeS3::new();
    let drive = NamesHashes(s3_drive(&s3));
    let a = Device::new("dev-a-dirs");
    let b = Device::new("dev-b-dirs");
    a.write("notes/a.md", b"alpha");
    fs::create_dir_all(a.folder.path().join("photos/2026")).unwrap();
    a.sync(&drive);
    b.sync(&drive);
    assert!(b.folder.path().join("photos/2026").is_dir());
    let again = b.sync(&drive);
    assert_eq!((again.files_up, again.files_down), (0, 0), "{}", again.summary());
    fs::remove_dir_all(a.folder.path().join("photos")).unwrap();
    a.sync(&drive);
    b.sync(&drive);
    assert!(!b.folder.path().join("photos").exists());
    assert_eq!(b.read("notes/a.md").as_deref(), Some(&b"alpha"[..]));
}

// ==== A drive as the plain sync's store ====

#[test]
fn a_drive_as_a_store_answers_an_unchanged_object_without_its_bytes() {
    let s3 = FakeS3::new();
    let store = DriveStore::new(s3_drive(&s3));
    assert_eq!(store.get_unless("k.json", None).unwrap(), Conditional::NotFound);
    let first = s3.write("k.json", b"{}".to_vec());
    let first = first.trim_matches('"').to_string();
    match store.get_unless("k.json", None).unwrap() {
        Conditional::Found { body, etag } => {
            assert_eq!(body, b"{}");
            assert_eq!(etag.as_deref(), Some(first.as_str()));
        }
        other => panic!("{other:?}"),
    }
    s3.clear_log();
    assert_eq!(
        store.get_unless("k.json", Some(&first)).unwrap(),
        Conditional::NotModified
    );
    assert_eq!(s3.count("GET"), 0, "a HEAD answers it");
    s3.write("k.json", b"{\"a\": 1}".to_vec());
    assert!(matches!(
        store.get_unless("k.json", Some(&first)).unwrap(),
        Conditional::Found { .. }
    ));
}

#[test]
fn a_drive_as_a_store_writes_conditionally() {
    let s3 = FakeS3::new();
    let store = DriveStore::new(s3_drive(&s3));
    let first = store.put_if("k", b"1", None).unwrap().expect("the first write wins");
    assert_eq!(store.put_if("k", b"1 again", None).unwrap(), None, "it is there already");
    let second = store
        .put_if("k", b"2", Some(&first))
        .unwrap()
        .expect("the version read is still there");
    assert_ne!(second, first);
    assert_eq!(store.put_if("k", b"3", Some(&first)).unwrap(), None, "a stale version loses");
    assert_eq!(s3.read("k").as_deref(), Some(&b"2"[..]));
    assert_eq!(store.head("k").unwrap(), Some(1));
    assert_eq!(store.head("nope").unwrap(), None);
    assert_eq!(store.list("").unwrap().len(), 1);
}

#[test]
fn the_plain_sync_runs_through_a_drive() {
    let s3 = FakeS3::new();
    let store = DriveStore::new(s3_drive(&s3));
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("notes/a.md", b"alpha");
    let opts = SyncOptions::new("sync/", "d-test", "dev-a").unwrap();
    let sent = sync_folder(&store, &a.root(), &a.index(), &opts).unwrap();
    assert!(sent.index_written, "{}", sent.summary());
    assert!(s3.read(&index_key("sync/")).is_some());
    let opts_b = SyncOptions::new("sync/", "d-test", "dev-b").unwrap();
    let got = sync_folder(&store, &b.root(), &b.index(), &opts_b).unwrap();
    assert_eq!(got.files_down, 1, "{}", got.summary());
    assert_eq!(b.tree(), a.tree());
    // An unchanged drive: one HEAD of the index, nothing written.
    s3.clear_log();
    let again = sync_folder(&store, &b.root(), &b.index(), &opts_b).unwrap();
    assert!(!again.index_written);
    assert_eq!(s3.count("GET"), 0, "{:?}", s3.log.lock().unwrap());
    assert_eq!(s3.count("PUT"), 0);
}
