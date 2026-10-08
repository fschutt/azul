//! The sync loop against an S3 service in memory, through the kit's bucket: two "devices"
//! (two folders, two local indexes) sharing one bucket.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use azul_storage::testing::TempDir;

use super::fake_s3::S3Bucket;
use crate::{
    sync::{
        azlin_roots, collect_garbage,
        local::{hash_bytes, path_of},
        remote::{blob_key, index_key},
        sync_folder, LocalRoot, RemoteFile, RemoteIndex, Rules, SyncOptions, SyncReport,
        DATA_PREFIX, HOME_PREFIX,
    },
    CloudResult,
};

const PREFIX: &str = "e2e/sync/";

/// One device: its folder and its state folder.
struct Device {
    name: &'static str,
    folder: TempDir,
    state: TempDir,
    data_tree: bool,
    json_merge: Vec<String>,
    rules: Rules,
}

impl Device {
    fn new(name: &'static str) -> Device {
        Device {
            name,
            folder: TempDir::new(&format!("azcloud-sync-{name}")),
            state: TempDir::new(&format!("azcloud-state-{name}")),
            data_tree: false,
            json_merge: Vec::new(),
            rules: Rules::base(),
        }
    }

    fn root(&self) -> LocalRoot {
        LocalRoot {
            path: self.folder.path().to_path_buf(),
            data_tree: self.data_tree,
            json_merge: self.json_merge.clone(),
            rules: self.rules.clone(),
        }
    }

    fn index(&self) -> PathBuf {
        self.state.path().join("sync").join("index.json")
    }

    fn opts(&self) -> SyncOptions {
        SyncOptions::new(PREFIX, "d-test", self.name).unwrap()
    }

    fn sync(&self, store: &S3Bucket) -> SyncReport {
        sync_folder(store, &self.root(), &self.index(), &self.opts()).unwrap()
    }

    fn sync_with(&self, store: &S3Bucket, opts: &SyncOptions) -> CloudResult<SyncReport> {
        sync_folder(store, &self.root(), &self.index(), opts)
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let path = path_of(self.folder.path(), key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn read(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(path_of(self.folder.path(), key)).ok()
    }

    fn remove(&self, key: &str) {
        fs::remove_file(path_of(self.folder.path(), key)).unwrap();
    }

    /// Every file but the folder's own bookkeeping.
    fn tree(&self) -> BTreeMap<String, Vec<u8>> {
        let mut out = BTreeMap::new();
        walk(self.folder.path(), "", &mut out);
        out
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let key = format!("{prefix}{name}");
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

fn index_of(store: &S3Bucket) -> RemoteIndex {
    RemoteIndex::parse(&store.read(&index_key(PREFIX)).expect("an index")).unwrap()
}

#[test]
fn the_second_sync_of_an_unchanged_folder_uploads_nothing_and_writes_no_index() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("notes/a.md", b"alpha");
    a.write("notes/sub/b.md", b"beta");
    a.write("calc/history.jsonl", b"1+1=2\n");
    let first = a.sync(&store);
    assert_eq!(first.files_up, 3);
    assert_eq!(first.blobs_up, 3);
    assert!(first.index_written);
    assert_eq!(first.generation, 1);

    store.clear_log();
    let second = a.sync(&store);
    assert_eq!(second.files_up, 0, "{}", second.summary());
    assert_eq!(second.blobs_up, 0);
    assert_eq!(second.bytes_up, 0);
    assert!(!second.index_written);
    assert_eq!(second.unchanged, 3);
    assert_eq!(store.count("PUT"), 0, "{:?}", store.log.lock().unwrap());
    assert_eq!(store.count("GET"), 1, "one conditional GET of the index");
}

#[test]
fn two_devices_converge_on_the_same_files() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("notes/a.md", b"alpha");
    a.write("notes/old.md", b"old");
    a.write("big/blob.bin", &vec![7u8; 9 * 1024 * 1024]);
    a.sync(&store);
    let got = b.sync(&store);
    assert_eq!(got.files_down, 3, "{}", got.summary());
    assert_eq!(b.tree(), a.tree());

    b.write("notes/a.md", b"alpha, edited on b");
    b.write("notes/new.md", b"new on b");
    b.remove("notes/old.md");
    let sent = b.sync(&store);
    assert_eq!(sent.files_up, 2);
    assert_eq!(sent.deleted_there, 1);
    let got = a.sync(&store);
    assert_eq!(got.files_down, 2);
    assert_eq!(got.deleted_here, 1);
    assert_eq!(a.tree(), b.tree());
    assert!(a.read("notes/old.md").is_none());
    let index = index_of(&store);
    assert!(index.deleted.contains_key("notes/old.md"), "a tombstone");
    // Nothing more to do on either side.
    assert!(!a.sync(&store).index_written);
    assert!(!b.sync(&store).index_written);
}

#[test]
fn a_lost_race_rereads_the_index_merges_again_and_retries() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("x.md", b"x1");
    a.sync(&store);
    a.write("x.md", b"x2");
    // Device b commits y.md between a's read of the index and a's write.
    store.before_next_cas(|s| {
        let key = index_key(PREFIX);
        let mut index = RemoteIndex::parse(&s.read(&key).unwrap()).unwrap();
        let blob = b"from b".to_vec();
        let hash = hash_bytes(&blob);
        s.write(&blob_key(PREFIX, &hash), blob);
        index.generation += 1;
        index.files.insert(
            String::from("y.md"),
            RemoteFile {
                hash,
                size: 6,
                mtime: 0,
                gen: index.generation,
                device: String::from("dev-b"),
            },
        );
        s.write(&key, index.to_bytes());
    });
    let report = a.sync(&store);
    assert_eq!(report.cas_retries, 1, "{}", report.summary());
    assert!(report.index_written);
    assert_eq!(
        report.blobs_up, 1,
        "x2's blob travelled once, before the lost race"
    );
    assert_eq!(a.read("y.md").as_deref(), Some(&b"from b"[..]));
    let index = index_of(&store);
    assert_eq!(index.generation, 3);
    assert_eq!(index.files["x.md"].hash, hash_bytes(b"x2"));
    assert!(index.files.contains_key("y.md"), "b's commit survived");
}

#[test]
fn an_edit_on_both_devices_keeps_both_versions_on_both() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("notes/a.md", b"base");
    a.sync(&store);
    b.sync(&store);
    a.write("notes/a.md", b"edited on a");
    b.write("notes/a.md", b"edited on b!");
    a.sync(&store);
    let report = b.sync(&store);
    assert_eq!(report.conflicts.len(), 1, "{}", report.summary());
    let copy = report.conflicts[0]
        .split(" -> ")
        .nth(1)
        .unwrap()
        .to_string();
    assert!(copy.starts_with("notes/a (conflict dev-b "), "{copy}");
    assert!(copy.ends_with(").md"), "{copy}");
    assert_eq!(
        b.read("notes/a.md").as_deref(),
        Some(&b"edited on a"[..]),
        "a committed first"
    );
    assert_eq!(b.read(&copy).as_deref(), Some(&b"edited on b!"[..]));
    a.sync(&store);
    assert_eq!(a.tree(), b.tree());
    assert_eq!(a.tree().len(), 2);
}

#[test]
fn a_delete_travels_and_an_edit_beats_a_delete() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("x.md", b"x");
    a.write("y.md", b"y");
    a.sync(&store);
    b.sync(&store);
    a.remove("x.md");
    a.sync(&store);
    assert_eq!(b.sync(&store).deleted_here, 1);
    assert!(b.read("x.md").is_none());

    b.remove("y.md");
    a.write("y.md", b"y, edited on a");
    b.sync(&store);
    assert!(!index_of(&store).files.contains_key("y.md"));
    let report = a.sync(&store);
    assert_eq!(
        report.files_up,
        1,
        "the edit comes back: {}",
        report.summary()
    );
    b.sync(&store);
    assert_eq!(b.read("y.md").as_deref(), Some(&b"y, edited on a"[..]));
}

#[test]
fn an_emptied_folder_does_not_delete_the_drive() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    for i in 0..20 {
        a.write(&format!("f{i}.txt"), format!("{i}").as_bytes());
    }
    a.sync(&store);
    for i in 0..20 {
        a.remove(&format!("f{i}.txt"));
    }
    let err = a.sync_with(&store, &a.opts()).unwrap_err().to_string();
    assert!(err.contains("--allow-mass-delete"), "{err}");
    assert_eq!(index_of(&store).files.len(), 20, "nothing was deleted");
    let mut opts = a.opts();
    opts.allow_mass_delete = true;
    let report = a.sync_with(&store, &opts).unwrap();
    assert_eq!(report.deleted_there, 20);
}

#[test]
fn a_dry_run_plans_without_writing_anything() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("a.md", b"a");
    let mut opts = a.opts();
    opts.dry_run = true;
    let report = a.sync_with(&store, &opts).unwrap();
    assert_eq!(report.planned, vec![String::from("up a.md")]);
    assert_eq!(store.count("PUT"), 0);
    assert!(!a.index().exists());
}

#[test]
fn temporary_lock_and_bookkeeping_files_never_travel() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("a.md", b"a");
    a.write("a.md~", b"backup");
    a.write(".DS_Store", b"finder");
    a.write(".azlin/cache", b"manifest");
    a.write("db/x.sqlite-wal", b"wal");
    a.write(".azcloudignore", b"private/\n");
    a.write("private/diary.md", b"secret");
    let mut b_root = a.root().with_ignore_file();
    b_root.path = a.folder.path().to_path_buf();
    let report = sync_folder(&store, &b_root, &a.index(), &a.opts()).unwrap();
    let index = index_of(&store);
    let keys: Vec<&str> = index.files.keys().map(String::as_str).collect();
    assert_eq!(keys, vec![".azcloudignore", "a.md"], "{}", report.summary());
    assert!(report.excluded >= 5, "{report:?}");
}

#[test]
fn a_damaged_blob_is_refused_and_its_file_left_alone() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("a.md", b"good");
    a.sync(&store);
    store.write(&blob_key(PREFIX, &hash_bytes(b"good")), b"evil".to_vec());
    let report = b.sync(&store);
    assert_eq!(report.files_down, 0);
    assert!(report.errors[0].contains("damaged"), "{:?}", report.errors);
    assert!(b.read("a.md").is_none());
    // Repaired, the next run takes it.
    store.write(&blob_key(PREFIX, &hash_bytes(b"good")), b"good".to_vec());
    assert_eq!(b.sync(&store).files_down, 1);
}

#[test]
fn a_download_into_the_data_tree_keeps_its_manifest_right() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let mut b = Device::new("dev-b");
    b.data_tree = true;
    b.rules = Rules::azlin_data();
    a.write("contacts/ab12.vcf", b"BEGIN:VCARD\nEND:VCARD\n");
    a.sync(&store);
    b.sync(&store);
    let manifest = fs::read_to_string(b.folder.path().join(".azlin").join("cache")).unwrap();
    assert!(manifest.contains("contacts/ab12.vcf"), "{manifest}");
    assert!(
        !index_of(&store)
            .files
            .keys()
            .any(|k| k.starts_with(".azlin")),
        "the manifest never travels"
    );
}

#[test]
fn the_shared_config_travels_without_its_endpoints_and_merges_key_by_key() {
    let store = S3Bucket::new();
    let mut a = Device::new("dev-a");
    let mut b = Device::new("dev-b");
    for d in [&mut a, &mut b] {
        d.json_merge = vec![String::from("config.json")];
        d.rules = Rules::azlin_home();
    }
    a.write(
        "config.json",
        br#"{"currentTheme": "flat", "mode": "dark", "endpoints": {"profile": "local", "token": "http://127.0.0.1:8081"}}"#,
    );
    b.write(
        "config.json",
        br#"{"endpoints": {"profile": "production"}}"#,
    );
    a.sync(&store);
    let report = b.sync(&store);
    assert_eq!(
        report.merged,
        vec![String::from("config.json")],
        "{}",
        report.summary()
    );
    let on_b: serde_json::Value = serde_json::from_slice(&b.read("config.json").unwrap()).unwrap();
    assert_eq!(on_b["currentTheme"], "flat", "a's look reached b");
    assert_eq!(on_b["mode"], "dark");
    assert_eq!(
        on_b["endpoints"]["profile"], "production",
        "b kept its own endpoints"
    );
    for key in store.keys() {
        let text = String::from_utf8_lossy(&store.read(&key).unwrap()).into_owned();
        assert!(
            !text.contains("127.0.0.1:8081"),
            "{key} holds a's endpoints: {text}"
        );
        assert!(!text.contains("endpoints"), "{key}: {text}");
    }

    // a changes the mode, b the theme, at the same time: both changes stay.
    a.write(
        "config.json",
        br#"{"currentTheme": "flat", "mode": "light", "endpoints": {"profile": "local"}}"#,
    );
    let mut on_b = on_b;
    on_b["currentTheme"] = serde_json::json!("flora");
    b.write("config.json", &serde_json::to_vec(&on_b).unwrap());
    a.sync(&store);
    b.sync(&store);
    a.sync(&store);
    for d in [&a, &b] {
        let v: serde_json::Value = serde_json::from_slice(&d.read("config.json").unwrap()).unwrap();
        assert_eq!(v["currentTheme"], "flora", "{}: {v}", d.name);
        assert_eq!(v["mode"], "light", "{}: {v}", d.name);
    }
    let on_a: serde_json::Value = serde_json::from_slice(&a.read("config.json").unwrap()).unwrap();
    assert_eq!(on_a["endpoints"]["profile"], "local");
}

#[test]
fn a_stale_copy_on_a_new_device_of_a_file_deleted_elsewhere_is_deleted() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("old.md", b"old");
    a.write("keep.md", b"keep");
    a.sync(&store);
    a.remove("old.md");
    a.sync(&store);
    // A device restored from a backup that still holds old.md, never synced.
    let c = Device::new("dev-c");
    c.write("old.md", b"old");
    let report = c.sync(&store);
    assert_eq!(report.deleted_here, 1, "{}", report.summary());
    assert!(c.read("old.md").is_none());
    assert_eq!(c.read("keep.md").as_deref(), Some(&b"keep"[..]));
}

#[test]
fn an_index_that_went_back_deletes_nothing_on_either_side() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("a.md", b"a");
    a.write("b.md", b"b");
    a.sync(&store);
    a.write("c.md", b"c");
    a.sync(&store);
    // Someone deleted the index; a's base is newer than what is there now.
    store.write(&index_key(PREFIX), RemoteIndex::empty().to_bytes());
    let report = a.sync(&store);
    assert!(
        report.notes.iter().any(|n| n.contains("deletes nothing")),
        "{report:?}"
    );
    assert_eq!(report.deleted_here, 0);
    assert_eq!(a.tree().len(), 3);
    assert_eq!(
        index_of(&store).files.len(),
        3,
        "everything is back on the drive"
    );
}

#[test]
fn garbage_collection_deletes_only_old_blobs_no_index_names() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("a.md", b"one");
    a.sync(&store);
    a.write("a.md", b"two");
    a.sync(&store);
    let report = collect_garbage(&store, PREFIX, 3600, false).unwrap();
    assert_eq!(report.deleted, 0, "young blobs stay: {report:?}");
    assert_eq!(report.kept_young, 1);
    let report = collect_garbage(&store, PREFIX, -1, false).unwrap();
    assert_eq!(report.deleted, 1, "{report:?}");
    assert!(store.read(&blob_key(PREFIX, &hash_bytes(b"one"))).is_none());
    assert!(store.read(&blob_key(PREFIX, &hash_bytes(b"two"))).is_some());
}

#[test]
fn the_azlin_tree_is_the_data_root_and_the_dot_azlin_folder_each_once() {
    let roots = azlin_roots(
        Path::new("/home/a/.azlin/data"),
        Path::new("/home/a/.azlin"),
    );
    assert_eq!(roots[0].1, DATA_PREFIX);
    assert_eq!(roots[1].1, HOME_PREFIX);
    assert!(roots[0].0.data_tree);
    assert_eq!(roots[1].0.json_merge, vec![String::from("config.json")]);
    assert!(
        roots[1].0.rules.excluded("data/notes/a.md").is_some(),
        "the .azlin folder leaves the data root inside it to its own sync"
    );
    let apart = azlin_roots(
        Path::new("/home/a/Library/Application Support/Azlin"),
        Path::new("/home/a/.azlin"),
    );
    assert!(apart[1].0.rules.excluded("data/notes/a.md").is_none());
}
