//! Small files synced to an encrypted drive that packs (feature `encryption`): the sync sends
//! them in batches, the drive packs each batch and commits its index once per pack, a packed
//! file comes back with one ranged GET over HTTPS, and five thousand files survive a sync, a
//! read back on another device, half of them deleted, a repack and the garbage collection.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use azul_storage::{
    crypto::{DriveKey, ObjectId},
    encrypted::{EncryptedDrive, MemoryIndex},
    meta::{open_encrypted_drive, MemoryBucket},
    ops::list_all,
    packing::{PackOptions, RETIRED_GRACE_SECS},
    testing::TempDir,
    time::now_unix,
    BatchPut, Drive, ListRequest,
};

use super::{fake_s3::FakeS3, sync_named::s3_drive};
use crate::sync::{
    local::path_of, sync_to, LocalRoot, RunHooks, SyncOptions, SyncReport, Target,
};

const PREFIX: &str = "Documents/";

/// A folder on a device and its sync state.
struct Device {
    name: &'static str,
    folder: TempDir,
    state: TempDir,
}

impl Device {
    fn new(name: &'static str) -> Device {
        Device {
            name,
            folder: TempDir::new(&format!("azcloud-pack-{name}")),
            state: TempDir::new(&format!("azcloud-pack-state-{name}")),
        }
    }

    fn sync(&self, drive: &dyn Drive) -> SyncReport {
        let mut opts = SyncOptions::new(PREFIX, "d-pack", self.name).unwrap();
        // Half the folder goes on purpose: no mass-delete stop, no burst pause.
        opts.allow_mass_delete = true;
        opts.allow_burst = true;
        let root = LocalRoot::folder(self.folder.path());
        let index = self.state.path().join("index.json");
        sync_to(Target::Named(drive), &root, &index, &opts, &RunHooks::default()).unwrap()
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let path = path_of(self.folder.path(), key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn remove(&self, key: &str) {
        fs::remove_file(path_of(self.folder.path(), key)).unwrap();
    }

    fn tree(&self) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        walk(self.folder.path(), "", &mut out);
        out
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // The sync's own bookkeeping and the system's temporary files are no files of the user.
        if name.starts_with('.') {
            continue;
        }
        let key = format!("{prefix}{name}");
        if entry.file_type().unwrap().is_dir() {
            walk(&entry.path(), &format!("{key}/"), out);
        } else {
            out.insert(key, fs::read(entry.path()).unwrap());
        }
    }
}

/// File `i` of the test: notes that compress and photos that do not, 100 bytes to 12 KB, in 37
/// folders (an odd count: every folder holds even and odd files alike). A photo's bytes come
/// from its number, so a test asks for the same file twice.
fn file(i: usize) -> (String, Vec<u8>) {
    let size = 100 + (i * 7_919) % 12_000;
    if i % 3 == 0 {
        let mut bytes = Vec::with_capacity(size + 32);
        let mut block = *blake3::hash(&(i as u64).to_le_bytes()).as_bytes();
        while bytes.len() < size {
            bytes.extend_from_slice(&block);
            block = *blake3::hash(&block).as_bytes();
        }
        bytes.truncate(size);
        (format!("photos/{:02}/{i:05}.jpg", i % 37), bytes)
    } else {
        let line = format!("Note {i}: call the plumber, then the bank, then mother. ");
        let bytes = line.into_bytes().into_iter().cycle().take(size).collect();
        (format!("notes/{:02}/{i:05}.txt", i % 37), bytes)
    }
}

/// The bucket's data objects (packs and files with objects of their own).
fn data_objects(bucket: &MemoryBucket) -> Vec<String> {
    bucket
        .objects()
        .into_iter()
        .map(|(key, _)| key)
        .filter(|key| ObjectId::from_bucket_key(key).is_some())
        .collect()
}

/// The bucket object that holds the folder record's current version (a pack of its own, or an
/// object when it is large).
fn record_object(drive: &dyn Drive) -> String {
    drive
        .metadata(&format!("{PREFIX}.azlin/sync/record.json"))
        .unwrap()
        .into_iter()
        .find(|(name, _)| name == azul_storage::OBJECT_KEY_METADATA)
        .map(|(_, object)| object)
        .expect("the record's object")
}

/// An encrypted drive that packs, its index the bucket's metadata repository, over `bucket`.
fn packing_drive(
    bucket: &Arc<MemoryBucket>,
    key: &DriveKey,
    device: &str,
    cache: &Path,
    options: PackOptions,
) -> EncryptedDrive<Arc<MemoryBucket>> {
    let index_dir: PathBuf = cache.join("index");
    open_encrypted_drive(Arc::clone(bucket), key.clone(), device, device, Some(index_dir), false)
        .unwrap()
        .with_packing(options.with_journal(cache.join("packs")))
}

#[test]
fn a_sync_of_small_files_packs_them_and_commits_the_drive_index_once_per_pack() {
    let bucket = Arc::new(MemoryBucket::new());
    let key = DriveKey::generate().unwrap();
    let cache = TempDir::new("azcloud-pack-cache");
    let drive = packing_drive(&bucket, &key, "laptop", cache.path(), PackOptions::default());
    let a = Device::new("dev-a");
    for i in 0..300 {
        let (key, bytes) = file(i);
        a.write(&key, &bytes);
    }
    let logs_before = list_all(&*bucket, ".azlin/meta/log/").unwrap().len();
    let sent = a.sync(&drive);
    assert_eq!(sent.files_up, 300, "{}", sent.summary());
    // The folder's record is a small file of the drive too, written after the files.
    let record = record_object(&drive);
    let all = data_objects(&bucket);
    let packs: Vec<&String> = all.iter().filter(|key| **key != record).collect();
    assert!(packs.len() <= 2, "300 small files in {} objects", packs.len());
    let logs = list_all(&*bucket, ".azlin/meta/log/").unwrap().len() - logs_before;
    assert!(logs <= all.len(), "{logs} index commits for {} packs", all.len());
    for i in 0..300 {
        let (key, bytes) = file(i);
        assert_eq!(drive.get(&format!("{PREFIX}{key}")).unwrap(), bytes, "{key}");
    }
}

#[test]
fn a_packed_file_comes_back_over_https_with_one_ranged_get() {
    let s3 = FakeS3::new();
    let drive = EncryptedDrive::new(
        s3_drive(&s3),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    )
    .with_packing(PackOptions::default());
    let files: Vec<(String, Vec<u8>)> = (0..20)
        .map(|i| (format!("{i}.bin"), vec![b'a' + i as u8; 10_000]))
        .collect();
    let items: Vec<BatchPut<'_>> = files.iter().map(|(k, b)| BatchPut::new(k, b)).collect();
    for result in drive.put_many(&items) {
        result.unwrap();
    }
    assert_eq!(s3.count("PUT"), 1, "twenty files, one pack");
    s3.clear_log();
    assert_eq!(drive.get("7.bin").unwrap(), files[7].1);
    assert_eq!(s3.count("GET"), 1, "one GET");
    assert_eq!(s3.count("HEAD"), 0);
}

#[test]
fn five_thousand_small_files_sync_read_back_half_deleted_repacked_and_all_verified() {
    let bucket = Arc::new(MemoryBucket::new());
    let key = DriveKey::generate().unwrap();
    let mut options = PackOptions::default();
    // Half the files go: their packs are then half live, and this round repacks them.
    options.repack_below_percent = 75;
    let laptop_cache = TempDir::new("azcloud-pack-laptop");
    let phone_cache = TempDir::new("azcloud-pack-phone");
    let laptop = packing_drive(&bucket, &key, "laptop", laptop_cache.path(), options.clone());
    let phone = packing_drive(&bucket, &key, "phone", phone_cache.path(), options);
    let a = Device::new("dev-laptop");
    let b = Device::new("dev-phone");
    const FILES: usize = 5_000;
    for i in 0..FILES {
        let (key, bytes) = file(i);
        a.write(&key, &bytes);
    }

    // Synced up: packs, not five thousand objects.
    let sent = a.sync(&laptop);
    assert_eq!(sent.files_up, FILES, "{}", sent.summary());
    // The files' packs; the folder record's object is no pack (a later run writes a new one
    // and the drive's history keeps the old).
    let record = record_object(&laptop);
    let packs: Vec<String> = data_objects(&bucket)
        .into_iter()
        .filter(|key| *key != record)
        .collect();
    assert!(packs.len() < FILES / 100, "{FILES} small files in {} objects", packs.len());

    // Read back on another device: every file, byte for byte.
    let got = b.sync(&phone);
    assert_eq!(got.files_down, FILES, "{}", got.summary());
    assert_eq!(b.tree(), a.tree());

    // Half deleted.
    for i in (0..FILES).step_by(2) {
        a.remove(&file(i).0);
    }
    let deleted = a.sync(&laptop);
    assert_eq!(deleted.deleted_there, FILES / 2, "{}", deleted.summary());

    // Repacked: the old packs are retired, the live files are in new ones.
    let now = now_unix();
    let report = laptop.repack(now).unwrap();
    assert!(report.packs_read >= 1, "{report:?}");
    assert_eq!(report.files_moved, FILES / 2, "{report:?}");
    assert!(report.bytes_after < report.bytes_before, "{report:?}");
    let collected = laptop.collect_retired(now + RETIRED_GRACE_SECS + 1).unwrap();
    assert!(collected >= report.packs_read, "{collected} collected: {report:?}");
    let left = data_objects(&bucket);
    for pack in &packs {
        assert!(!left.contains(pack), "{pack} was repacked and collected");
    }

    // Every remaining file verified, on both devices.
    for i in (1..FILES).step_by(2) {
        let (key, bytes) = file(i);
        assert_eq!(laptop.get(&format!("{PREFIX}{key}")).unwrap(), bytes, "{key}");
    }
    // The phone asks the drive's index again now (it would within its poll time anyway).
    phone.list(&ListRequest::recursive(PREFIX).refreshed()).unwrap();
    let after = b.sync(&phone);
    assert_eq!(after.deleted_here, FILES / 2, "{}", after.summary());
    assert_eq!(after.files_down, 0, "a repack changes no file: {}", after.summary());
    assert_eq!(b.tree(), a.tree());
    assert_eq!(b.tree().len(), FILES / 2);
    for i in (1..FILES).step_by(2) {
        let (key, bytes) = file(i);
        assert_eq!(phone.get(&format!("{PREFIX}{key}")).unwrap(), bytes, "{key}");
    }
}
