//! A folder synced to a drive BY NAME ([`Target::Named`], an encrypted drive's way): every file
//! under its own key, the drive's own index naming it, no index of the sync in the bucket.
//! Plain buckets in memory stand in for the drive (an S3 drive on the fake service); one that
//! names each file's BLAKE3 in its metadata stands in for an encrypted drive's index.
//! And a drive as the plain sync's store ([`DriveStore`]).

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
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
    sync_folder, sync_to,
    verify::{verify, VerifyReport},
    Conditional, LocalRoot, RemoteStore, RunHooks, SyncOptions, SyncReport, Target,
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

/// A mass delete the user confirmed on one device travels with its deletes, but the drive does not
/// say who wrote them ([`crate::sync::trust`], SYNC-1): another device asks its own user again,
/// even when it knows the writer's name as one of the account's devices - a name is no identity -
/// and applies it (into its trash) when that user confirms.
#[test]
fn a_mass_delete_confirmed_on_one_device_is_asked_again_on_another_and_applied_when_confirmed() {
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
    let sent = sync_to(
        Target::Named(&*drive),
        &a.root(),
        &a.index(),
        &confirmed,
        &hooks,
    )
    .unwrap();
    assert_eq!(sent.deleted_there, 12, "{}", sent.summary());
    let hooks = RunHooks {
        hold_mass_delete: true,
        ..RunHooks::default()
    };
    let asked = b.run(&*drive, &hooks);
    assert!(asked.mass_delete.is_some(), "{}", asked.summary());
    assert_eq!(asked.deleted_here, 0, "{}", asked.summary());
    // Knowing a's name as an own device changes nothing: the tombstone's `device` is a string its
    // writer chose.
    let mut own = b.opts();
    own.own_devices = vec![String::from("dev-a-mass")];
    let still = sync_to(Target::Named(&*drive), &b.root(), &b.index(), &own, &hooks).unwrap();
    assert!(still.mass_delete.is_some(), "{}", still.summary());
    assert_eq!(still.deleted_here, 0, "{}", still.summary());
    assert_eq!(b.tree().len(), 12);
    // The user's own answer on this device does apply it.
    let mut yes = b.opts();
    yes.allow_mass_delete = true;
    let got = sync_to(Target::Named(&*drive), &b.root(), &b.index(), &yes, &hooks).unwrap();
    assert!(got.mass_delete.is_none(), "{}", got.summary());
    assert_eq!(got.deleted_here, 12, "{}", got.summary());
    assert!(
        b.tree().is_empty(),
        "{:?}",
        b.tree().keys().collect::<Vec<_>>()
    );
}

/// X-DELTA-AZDRIVE-SYNC-1 on a drive synced by name: the folder record carries the tombstones.
/// A writer that names itself like the computer that reads them (`dev-b-forged` on both sides)
/// had `confirmed` believed by `writer == self.device`. RED not run: the old judge finds the
/// tombstones trusted and b deletes 12 files without a question.
#[test]
fn x_delta_azdrive_sync_1_a_writer_named_like_this_device_is_not_believed_on_a_named_drive() {
    let s3 = FakeS3::new();
    let drive = s3_drive(&s3);
    let stranger = Device::new("dev-stranger-forged");
    let b = Device::new("dev-b-forged");
    for i in 0..12 {
        stranger.write(&format!("docs/{i}.txt"), format!("doc {i}").as_bytes());
    }
    // The stranger syncs as "dev-b-forged": the name of the computer that will read its deletes.
    let mut as_b = stranger.opts();
    as_b.device = String::from("dev-b-forged");
    let hooks = RunHooks::default();
    sync_to(
        Target::Named(&*drive),
        &stranger.root(),
        &stranger.index(),
        &as_b,
        &hooks,
    )
    .unwrap();
    b.sync(&*drive);
    fs::remove_dir_all(path_of(stranger.folder.path(), "docs")).unwrap();
    as_b.allow_mass_delete = true;
    let sent = sync_to(
        Target::Named(&*drive),
        &stranger.root(),
        &stranger.index(),
        &as_b,
        &hooks,
    )
    .unwrap();
    assert_eq!(sent.deleted_there, 12, "{}", sent.summary());
    let hooks = RunHooks {
        hold_mass_delete: true,
        ..RunHooks::default()
    };
    let asked = b.run(&*drive, &hooks);
    assert!(asked.mass_delete.is_some(), "{}", asked.summary());
    assert_eq!(b.tree().len(), 12, "nothing was trashed");
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

/// A folder renamed on one device while a file in it is renamed on the other keeps both renames
/// on a drive synced by name too: the folder moves on the drive as one folder.
#[test]
fn a_folder_rename_and_a_rename_inside_it_on_another_device_compose_on_a_drive_synced_by_name() {
    let s3 = FakeS3::new();
    let drive = NamesHashes(s3_drive(&s3));
    let a = Device::new("dev-a-compose");
    let b = Device::new("dev-b-compose");
    a.write("dir/child.txt", b"child");
    a.sync(&drive);
    b.sync(&drive);
    fs::rename(a.folder.path().join("dir"), a.folder.path().join("dir2")).unwrap();
    fs::rename(
        b.folder.path().join("dir/child.txt"),
        b.folder.path().join("dir/child2.txt"),
    )
    .unwrap();
    a.sync(&drive);
    b.sync(&drive);
    a.sync(&drive);
    for device in [&a, &b] {
        assert_eq!(device.read("dir2/child2.txt").as_deref(), Some(&b"child"[..]));
        assert!(!device.folder.path().join("dir").exists());
    }
    assert_eq!(a.tree(), b.tree());
    assert!(s3.read("Documents/dir/child.txt").is_none());
    assert!(s3.read("Documents/dir2/child.txt").is_none());
}

/// A relative link inside the folder travels through a drive synced by name as a link: the
/// drive holds its target as a small file, the folder record says it is a link.
#[cfg(unix)]
#[test]
fn a_relative_link_travels_through_a_drive_synced_by_name_as_a_link() {
    let s3 = FakeS3::new();
    let drive = NamesHashes(s3_drive(&s3));
    let a = Device::new("dev-a-link");
    let b = Device::new("dev-b-link");
    a.write("notes/report.md", b"the report");
    std::os::unix::fs::symlink("notes/report.md", a.folder.path().join("latest.md")).unwrap();
    a.sync(&drive);
    b.sync(&drive);
    let path = b.folder.path().join("latest.md");
    assert!(fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_link(&path).unwrap(), Path::new("notes/report.md"));
    let again = b.sync(&drive);
    assert_eq!((again.files_up, again.files_down), (0, 0), "{}", again.summary());
}

/// AZ-S5, AZ-S15. The generation of a drive synced by name is this device's own commit counter
/// (the memo beside the base), so it says nothing of the drive: a memo that is lost starts it at 0
/// again, and the old rule (an index older than the last one seen is a rollback) refused the
/// drive on every poll for ever with a base that remembered the high number.
#[test]
fn az_s5_a_drive_synced_by_name_whose_memo_is_lost_is_no_rollback() {
    let s3 = FakeS3::new();
    let drive = s3_drive(&s3);
    let a = Device::new("dev-s5-memo");
    a.write("one.md", b"1");
    a.sync(&*drive);
    a.write("two.md", b"2");
    a.sync(&*drive);
    fs::remove_file(a.state.path().join("index.named.json")).expect("the memo is beside the base");
    let report = a.sync(&*drive);
    assert!(report.refused.is_none(), "{}", report.summary());
    assert!(
        !report
            .problems
            .iter()
            .any(|p| p.code == Code::RemoteRollback),
        "{:?}",
        report.problems
    );
    a.write("three.md", b"3");
    let sent = a.sync(&*drive);
    assert_eq!(sent.files_up, 1, "{}", sent.summary());
    assert_eq!(s3.read("Documents/three.md").as_deref(), Some(&b"3"[..]));
}

/// AZ-S5, RF45. An announced restore of a drive synced by name is taken as the restore it is,
/// whatever its generation says: what the copy lacks is sent again from here, and a file edited
/// after the copy is kept (here and on the drive) as a conflict copy, never overwritten.
#[test]
fn az_s5_a_drive_synced_by_name_takes_an_announced_restore_and_keeps_what_the_copy_lacks() {
    let s3 = FakeS3::new();
    let drive = NamesHashes(s3_drive(&s3));
    let a = Device::new("dev-s5-restore");
    a.write("kept.md", b"in the copy");
    a.sync(&drive);
    // The off-site copy: every object of the drive as it is now.
    let copy: Vec<(String, Vec<u8>)> = s3
        .keys()
        .into_iter()
        .filter_map(|key| s3.read(&key).map(|bytes| (key, bytes)))
        .collect();
    a.write("lag.md", b"committed after the copy");
    a.write("kept.md", b"edited after the copy");
    a.sync(&drive);
    // The drive is restored from the copy.
    for key in s3.keys() {
        s3.remove(&key);
    }
    for (key, bytes) in copy {
        s3.write(&key, bytes);
    }
    let mut opts = a.opts();
    opts.restore = Some(crate::sync::RestoreNotice {
        epoch: 1,
        ..crate::sync::RestoreNotice::default()
    });
    let report = sync_to(
        Target::Named(&drive),
        &a.root(),
        &a.index(),
        &opts,
        &RunHooks::default(),
    )
    .unwrap();
    assert!(report.refused.is_none(), "{}", report.summary());
    assert!(
        report
            .problems
            .iter()
            .any(|p| p.code == Code::ServerRestored),
        "{:?}",
        report.problems
    );
    assert_eq!(
        s3.read("Documents/lag.md").as_deref(),
        Some(&b"committed after the copy"[..]),
        "sent again"
    );
    let on_drive = |bytes: &[u8]| {
        s3.keys()
            .iter()
            .any(|k| s3.read(k).as_deref() == Some(bytes))
    };
    assert!(
        on_drive(b"edited after the copy"),
        "the edit is on the drive again: {:?}",
        s3.keys()
    );
    assert!(
        a.tree()
            .values()
            .any(|bytes| bytes == b"edited after the copy"),
        "the edit is still here: {:?}",
        a.tree().keys().collect::<Vec<_>>()
    );
    assert_eq!(report.deleted_here, 0, "{}", report.summary());
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

/// A drive whose conditional write lands but says nothing of the new version's tag, and whose
/// object another device replaces right after it - before any question about the tag.
struct SilentWrite {
    inner: Arc<dyn Drive>,
    s3: Arc<FakeS3>,
    /// The tag of the other device's version, once it wrote.
    rival: std::sync::Mutex<Option<String>>,
}

impl Drive for SilentWrite {
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
        self.inner.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.inner.put_if(key, bytes, condition)?;
        let tag = self.s3.write(key, b"the other device's commit".to_vec());
        *self.rival.lock().unwrap() = Some(tag.trim_matches('"').to_string());
        Ok(None)
    }
}

/// AZ-S18 (RED not run: the old code asked the drive for the tag after the write, got the other
/// device's, and handed it on as the new version's - the `assert_ne!` fails with the two equal).
#[test]
fn az_s18_a_write_that_names_no_tag_never_takes_another_devices_tag_as_its_own() {
    let s3 = FakeS3::new();
    let drive = Arc::new(SilentWrite {
        inner: s3_drive(&s3),
        s3: s3.clone(),
        rival: std::sync::Mutex::new(None),
    });
    let store = DriveStore::new(drive.clone());
    let ours = store
        .put_if("index.json", b"mine", None)
        .unwrap()
        .expect("the write landed");
    let rival = drive
        .rival
        .lock()
        .unwrap()
        .clone()
        .expect("another device wrote right after");
    assert_ne!(ours, rival, "the other device's tag is not ours");
    // Asked with the tag it handed out, the drive's whole object comes - the other device's
    // commit, never our cached copy served as "not modified".
    match store.get_unless("index.json", Some(&ours)).unwrap() {
        Conditional::Found { body, .. } => assert_eq!(body, b"the other device's commit"),
        other => panic!("a whole read was expected: {other:?}"),
    }
}

/// A drive that answers the conditional write of a folder's record without the new version's tag
/// and with another device's record already in its place (the other writes of this drive are
/// the inner drive's).
struct SilentRecord {
    inner: Arc<dyn Drive>,
    s3: Arc<FakeS3>,
}

impl Drive for SilentRecord {
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
        self.inner.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let tag = self.inner.put_if(key, bytes, condition)?;
        if key.ends_with("record.json") {
            self.s3.write(key, b"{}".to_vec());
            return Ok(None);
        }
        Ok(tag)
    }
}

/// AZ-S18, the second compare-and-swap: the folder's record of a drive synced by name (RED not
/// run: the old code asked the drive for the record's tag after the write, got the other
/// device's, and kept it with OUR record in the memo - so the next read answered with our copy as
/// the drive's, and the next conditional write matched and overwrote theirs).
#[test]
fn az_s18_a_folder_record_written_without_a_tag_is_not_kept_under_another_devices_tag() {
    let s3 = FakeS3::new();
    let drive = SilentRecord {
        inner: s3_drive(&s3),
        s3: s3.clone(),
    };
    let a = Device::new("dev-a");
    a.write("a.txt", b"alpha");
    a.sync(&drive);
    let memo: serde_json::Value =
        serde_json::from_slice(&fs::read(a.state.path().join("index.named.json")).unwrap())
            .unwrap();
    assert!(
        memo.get("record").is_none(),
        "no record is kept under a tag the write did not name: {memo}"
    );
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

/// X-DELTA-AZDRIVE-KEYS-43: the folder's record was read with a plain `get` after a HEAD that had
/// just said how big it is. It is read within that size now: a storage that streams on is refused
/// on its way in.
#[test]
fn x_delta_azdrive_keys_43_the_folders_record_is_read_within_the_size_its_head_said() {
    use super::bounded_reads::Reads;
    const RECORD: &str = "Documents/.azlin/sync/record.json";
    let reads = Arc::new(Reads::new());
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("notes/a.md", b"alpha");
    let sent = a.sync(&*reads);
    assert_eq!(sent.files_up, 1, "{}", sent.summary());
    let size = reads.head(RECORD).expect("the record is there").size;

    // device b's first run reads the record a wrote
    b.sync(&*reads);
    let limits = reads.limits_of(RECORD);
    assert!(
        limits.contains(&size),
        "the record (a HEAD said {size} bytes) is read within its size: {limits:?}"
    );
    assert_eq!(
        reads.plain_gets_of(RECORD),
        0,
        "never by a plain GET that takes whatever comes"
    );
    assert_eq!(b.read("notes/a.md").as_deref(), Some(&b"alpha"[..]));
}

// ==== The read-back of a drive synced by name (X-DELTA-AZDRIVE-SYNC-4) ====

/// What happens to the read of one key of a [`ReadBackProbe`].
enum Fate {
    /// Nothing: the drive answers as it is.
    Fine,
    /// The read fails with this error.
    Fails(DriveError),
    /// The bytes read have a flipped bit under the tag the listing carried: a disk that rots.
    Rots,
    /// A disk that rots, and another device saves a newer file after the damage was found and
    /// before the heal writes: the heal meets a changed tag.
    RotsThenRaced(Vec<u8>),
    /// Another device saves a newer file after the check listed the drive and before it reads
    /// the file.
    Replaced(Vec<u8>),
}

/// A drive for the read-back: the read of one key can fail, rot or be raced, and every write the
/// read-back makes to it is recorded - with its condition or without.
struct ReadBackProbe {
    inner: Arc<dyn Drive>,
    s3: Arc<FakeS3>,
    key: Mutex<String>,
    fate: Mutex<Fate>,
    plain: Mutex<Vec<String>>,
    conditional: Mutex<Vec<(String, Precondition)>>,
}

impl ReadBackProbe {
    fn new(s3: &Arc<FakeS3>) -> ReadBackProbe {
        ReadBackProbe {
            inner: s3_drive(s3),
            s3: s3.clone(),
            key: Mutex::new(String::new()),
            fate: Mutex::new(Fate::Fine),
            plain: Mutex::new(Vec::new()),
            conditional: Mutex::new(Vec::new()),
        }
    }

    /// From now on `key` (the drive's full key) meets `fate`, and the writes made so far are
    /// forgotten.
    fn arm(&self, key: &str, fate: Fate) {
        *self.key.lock().unwrap() = key.to_string();
        *self.fate.lock().unwrap() = fate;
        self.plain.lock().unwrap().clear();
        self.conditional.lock().unwrap().clear();
    }

    /// Every write since [`ReadBackProbe::arm`], as text.
    fn writes(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .plain
            .lock()
            .unwrap()
            .iter()
            .map(|key| format!("unconditional {key}"))
            .collect();
        out.extend(
            self.conditional
                .lock()
                .unwrap()
                .iter()
                .map(|(key, condition)| format!("conditional {key} {condition:?}")),
        );
        out
    }

    fn is_armed(&self, key: &str) -> bool {
        *self.key.lock().unwrap() == key
    }
}

impl Drive for ReadBackProbe {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        if !self.is_armed(key) {
            return self.inner.get(key);
        }
        let mut fate = self.fate.lock().unwrap();
        match &*fate {
            Fate::Fine => self.inner.get(key),
            Fate::Fails(e) => Err(e.clone()),
            Fate::Rots | Fate::RotsThenRaced(_) => {
                let mut bytes = self.inner.get(key)?;
                if let Some(first) = bytes.first_mut() {
                    *first ^= 1;
                }
                Ok(bytes)
            }
            Fate::Replaced(newer) => {
                self.s3.write(key, newer.clone());
                let newer = newer.clone();
                *fate = Fate::Fine;
                Ok(newer)
            }
        }
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.plain.lock().unwrap().push(key.to_string());
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
        self.conditional
            .lock()
            .unwrap()
            .push((key.to_string(), condition.clone()));
        // The race: the other device saves right before this write reaches the drive.
        let raced = match &*self.fate.lock().unwrap() {
            Fate::RotsThenRaced(newer) if self.is_armed(key) => Some(newer.clone()),
            _ => None,
        };
        if let Some(newer) = raced {
            self.s3.write(key, newer);
            *self.fate.lock().unwrap() = Fate::Fine;
        }
        self.inner.put_if(key, bytes, condition)
    }
}

/// A device that synced `plan.md` and `other.md` to a probe drive.
fn synced_pair(name: &'static str) -> (Arc<FakeS3>, ReadBackProbe, Device) {
    let s3 = FakeS3::new();
    let probe = ReadBackProbe::new(&s3);
    let a = Device::new(name);
    a.write("plan.md", b"plan one");
    a.write("other.md", b"other");
    let sent = a.sync(&probe);
    assert_eq!(sent.files_up, 2, "{}", sent.summary());
    (s3, probe, a)
}

fn read_back_of(probe: &ReadBackProbe, a: &Device) -> VerifyReport {
    verify(Target::Named(probe), &a.root(), &a.index(), &a.opts()).unwrap()
}

fn code_of(report: &VerifyReport, key: &str) -> Vec<Code> {
    report
        .problems
        .iter()
        .filter(|p| p.key == key)
        .map(|p| p.code)
        .collect()
}

/// X-DELTA-AZDRIVE-SYNC-4, the scenario of the finding. The check lists the drive, device B saves
/// `plan.md` meanwhile, and the check reads B's bytes: they are not the version it listed. The old
/// read-back took that for damage, found its own copy to be the listed version and wrote it over
/// B's file with an unconditional put - B's edit was gone from the drive, and came down over B's
/// own copy at its next pass. RED not run: the old code makes one unconditional write of
/// `Documents/plan.md` and the drive holds `plan one` afterwards.
#[test]
fn x_delta_azdrive_sync_4_a_file_another_device_saved_during_the_check_is_not_overwritten() {
    let (s3, probe, a) = synced_pair("dev-sync4-race");
    probe.arm(
        "Documents/plan.md",
        Fate::Replaced(b"plan two, saved on b".to_vec()),
    );
    let report = read_back_of(&probe, &a);
    assert_eq!(
        s3.read("Documents/plan.md").as_deref(),
        Some(&b"plan two, saved on b"[..]),
        "B's newer file is still the drive's: {report:?}"
    );
    assert_eq!(
        probe.writes(),
        Vec::<String>::new(),
        "the check wrote nothing"
    );
    assert!(report.mismatches.is_empty(), "{report:?}");
    assert!(report.healed.is_empty(), "{report:?}");
    assert_eq!(report.changed, vec![String::from("plan.md")], "{report:?}");
    assert_eq!(code_of(&report, "plan.md"), vec![Code::ChangedDuringSync]);
    // The other file was read whole.
    assert_eq!(report.checked, 2);
}

/// X-DELTA-AZDRIVE-SYNC-4: a read that fails says nothing about the copy. RED not run: the old
/// code listed the file as a `hash-mismatch`, found its own copy to be the listed version and
/// wrote it to the drive again.
#[test]
fn x_delta_azdrive_sync_4_a_read_that_fails_is_not_damage_and_writes_nothing() {
    let (_s3, probe, a) = synced_pair("dev-sync4-fails");
    probe.arm(
        "Documents/plan.md",
        Fate::Fails(DriveError::Transport(String::from(
            "connection reset by peer",
        ))),
    );
    let report = read_back_of(&probe, &a);
    assert_eq!(report.unread, vec![String::from("plan.md")], "{report:?}");
    assert!(report.mismatches.is_empty(), "{report:?}");
    assert!(report.healed.is_empty(), "{report:?}");
    assert_eq!(probe.writes(), Vec::<String>::new());
    assert_eq!(code_of(&report, "plan.md"), vec![Code::Unchecked]);
    assert_eq!(a.read("plan.md").as_deref(), Some(&b"plan one"[..]));
}

/// X-DELTA-AZDRIVE-SYNC-4: a file that is gone when the check reads it was deleted by another
/// device after the listing; the heal would have put it back.
#[test]
fn x_delta_azdrive_sync_4_a_file_deleted_during_the_check_is_not_put_back() {
    let (_s3, probe, a) = synced_pair("dev-sync4-gone");
    probe.arm(
        "Documents/plan.md",
        Fate::Fails(DriveError::NotFound {
            key: String::from("Documents/plan.md"),
        }),
    );
    let report = read_back_of(&probe, &a);
    assert_eq!(report.changed, vec![String::from("plan.md")], "{report:?}");
    assert!(
        report.mismatches.is_empty() && report.healed.is_empty(),
        "{report:?}"
    );
    assert_eq!(probe.writes(), Vec::<String>::new());
}

/// X-DELTA-AZDRIVE-SYNC-4: an object that does not authenticate (a rotated key, a damaged
/// ciphertext, a change by someone without the key) is listed as damaged, and nothing is written
/// over it: the check cannot tell which of the three it met.
#[test]
fn x_delta_azdrive_sync_4_a_copy_that_does_not_authenticate_is_listed_and_left_alone() {
    let (_s3, probe, a) = synced_pair("dev-sync4-corrupt");
    probe.arm(
        "Documents/plan.md",
        Fate::Fails(DriveError::Corrupt {
            key: String::from("Documents/plan.md"),
            reason: String::from("the object does not authenticate"),
        }),
    );
    let report = read_back_of(&probe, &a);
    assert_eq!(report.unread, vec![String::from("plan.md")], "{report:?}");
    assert!(report.healed.is_empty(), "{report:?}");
    assert_eq!(probe.writes(), Vec::<String>::new());
    assert_eq!(code_of(&report, "plan.md"), vec![Code::HashMismatch]);
}

/// X-DELTA-AZDRIVE-SYNC-4, the one case that IS damage: the bytes were read whole, they are not
/// the listed version's, and the drive still names the tag the listing carried. The heal is ONE
/// write, conditional on that tag.
#[test]
fn x_delta_azdrive_sync_4_a_damaged_object_is_healed_by_a_write_conditional_on_its_tag() {
    let (_s3, probe, a) = synced_pair("dev-sync4-rot");
    let tag = probe
        .head("Documents/plan.md")
        .unwrap()
        .etag
        .expect("the drive names a tag")
        .trim_matches('"')
        .to_string();
    probe.arm("Documents/plan.md", Fate::Rots);
    let report = read_back_of(&probe, &a);
    assert_eq!(
        report.mismatches,
        vec![String::from("plan.md")],
        "{report:?}"
    );
    assert_eq!(report.healed, vec![String::from("plan.md")], "{report:?}");
    assert_eq!(
        probe.writes(),
        vec![format!(
            "conditional Documents/plan.md {:?}",
            Precondition::Matches(tag)
        )],
        "one write, conditional on the version the listing carried"
    );
    assert_eq!(code_of(&report, "plan.md"), vec![Code::HashMismatch]);
}

/// X-DELTA-AZDRIVE-SYNC-4: the heal is not sent from a copy that is not the version. This device
/// edited `plan.md` since it synced; the drive's copy is damaged; nothing is written.
#[test]
fn x_delta_azdrive_sync_4_a_damaged_object_is_not_healed_from_a_copy_that_is_another_version() {
    let (_s3, probe, a) = synced_pair("dev-sync4-edited");
    a.write("plan.md", b"plan, edited here");
    probe.arm("Documents/plan.md", Fate::Rots);
    let report = read_back_of(&probe, &a);
    assert_eq!(
        report.mismatches,
        vec![String::from("plan.md")],
        "{report:?}"
    );
    assert!(report.healed.is_empty(), "{report:?}");
    assert_eq!(probe.writes(), Vec::<String>::new());
    assert_eq!(
        a.read("plan.md").as_deref(),
        Some(&b"plan, edited here"[..])
    );
}

/// X-DELTA-AZDRIVE-SYNC-4: another device saves a newer file between the damage verdict and the
/// heal's write. The write carries the old tag, the drive refuses it, and the newer file stays.
/// RED not run: the old heal was `put_from` (unconditional) and the drive held `plan one` after.
#[test]
fn x_delta_azdrive_sync_4_a_heal_that_meets_a_newer_file_leaves_it() {
    let (s3, probe, a) = synced_pair("dev-sync4-late");
    probe.arm(
        "Documents/plan.md",
        Fate::RotsThenRaced(b"plan two, saved on b".to_vec()),
    );
    let report = read_back_of(&probe, &a);
    assert_eq!(
        s3.read("Documents/plan.md").as_deref(),
        Some(&b"plan two, saved on b"[..]),
        "{report:?}"
    );
    assert_eq!(
        report.mismatches,
        vec![String::from("plan.md")],
        "{report:?}"
    );
    assert!(report.healed.is_empty(), "{report:?}");
}
