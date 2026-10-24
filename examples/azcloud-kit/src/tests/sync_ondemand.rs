//! What an app adds to the sync loop ([`RunHooks`]): files kept in the cloud only (the
//! auto-download policy, "Free up space", opening one), conflicts that wait for the user (D52),
//! each file told as it moves, a run that can be stopped. Two "devices" share one bucket in
//! memory, as in `sync.rs`.

use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::{atomic::AtomicBool, Mutex},
};

use azul_storage::testing::TempDir;

use super::fake_s3::S3Bucket;
use crate::sync::{
    evict_file, fetch_file,
    local::{hash_bytes, path_of},
    remote::index_key,
    sync_to, LocalIndex, LocalRoot, RemoteIndex, RunHooks, SyncEvent, SyncOptions, SyncReport,
    Target,
};

const PREFIX: &str = "app/sync/";

struct Device {
    name: &'static str,
    folder: TempDir,
    state: TempDir,
}

impl Device {
    fn new(name: &'static str) -> Device {
        Device {
            name,
            folder: TempDir::new(&format!("azcloud-ondemand-{name}")),
            state: TempDir::new(&format!("azcloud-ondemand-state-{name}")),
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

    fn run(&self, store: &S3Bucket, hooks: &RunHooks<'_>) -> SyncReport {
        sync_to(Target::Index(store), &self.root(), &self.index(), &self.opts(), hooks).unwrap()
    }

    fn sync(&self, store: &S3Bucket) -> SyncReport {
        self.run(store, &RunHooks::default())
    }

    fn write(&self, key: &str, bytes: &[u8]) {
        let path = path_of(self.folder.path(), key);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn read(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(path_of(self.folder.path(), key)).ok()
    }

    fn base(&self) -> LocalIndex {
        LocalIndex::load(&self.index()).unwrap().expect("a local index")
    }
}

fn index_of(store: &S3Bucket) -> RemoteIndex {
    RemoteIndex::parse(&store.read(&index_key(PREFIX)).expect("an index")).unwrap()
}

/// Files under 10 bytes come down, bigger ones stay in the cloud.
fn small_only(_key: &str, size: u64) -> bool {
    size < 10
}

#[test]
fn a_file_the_policy_leaves_in_the_cloud_is_not_written_here_and_stays_on_the_drive() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("small.txt", b"tiny");
    a.write("docs/big.bin", b"a file of thirty-one bytes here");
    a.sync(&store);

    let hooks = RunHooks {
        fetch: Some(&small_only),
        ..RunHooks::default()
    };
    let got = b.run(&store, &hooks);
    assert_eq!(got.files_down, 1, "{}", got.summary());
    assert_eq!(b.read("small.txt").as_deref(), Some(&b"tiny"[..]));
    assert!(b.read("docs/big.bin").is_none(), "cloud only: no bytes here");
    assert_eq!(got.cloud_only, vec![String::from("docs/big.bin")]);
    let base = b.base();
    assert!(base.files["docs/big.bin"].cloud_only);
    assert_eq!(base.files["docs/big.bin"].hash, hash_bytes(b"a file of thirty-one bytes here"));
    let remote = got.remote.as_ref().expect("the drive's files after the run");
    assert!(remote.files.contains_key("docs/big.bin"));

    // The next runs - with or without the policy - neither download nor delete it.
    let again = b.sync(&store);
    assert_eq!(again.files_down, 0, "{}", again.summary());
    assert_eq!(again.deleted_there, 0);
    assert!(!again.index_written);
    assert_eq!(again.cloud_only, vec![String::from("docs/big.bin")]);
    assert!(index_of(&store).files.contains_key("docs/big.bin"));
    assert!(b.read("docs/big.bin").is_none());
}

#[test]
fn a_cloud_only_file_deleted_on_the_drive_is_forgotten_here() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("big.bin", b"thirty bytes of something big!");
    a.sync(&store);
    let hooks = RunHooks {
        fetch: Some(&small_only),
        ..RunHooks::default()
    };
    b.run(&store, &hooks);
    fs::remove_file(path_of(a.folder.path(), "big.bin")).unwrap();
    assert_eq!(a.sync(&store).deleted_there, 1);

    let got = b.run(&store, &hooks);
    assert_eq!(got.deleted_here, 0, "nothing here to delete: {}", got.summary());
    assert!(got.cloud_only.is_empty());
    assert!(!b.base().files.contains_key("big.bin"), "the base forgets it");
}

#[test]
fn a_cloud_only_file_changed_on_the_drive_stays_in_the_cloud_at_the_new_version() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("big.bin", b"thirty bytes of something big!");
    a.sync(&store);
    let hooks = RunHooks {
        fetch: Some(&small_only),
        ..RunHooks::default()
    };
    b.run(&store, &hooks);
    a.write("big.bin", b"thirty-one bytes, edited on a!!");
    a.sync(&store);

    let got = b.run(&store, &hooks);
    assert_eq!(got.files_down, 0, "{}", got.summary());
    assert!(b.read("big.bin").is_none());
    let base = b.base();
    assert!(base.files["big.bin"].cloud_only);
    assert_eq!(base.files["big.bin"].hash, hash_bytes(b"thirty-one bytes, edited on a!!"));
}

#[test]
fn a_conflict_waits_for_the_user_when_conflicts_are_held() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("note.md", b"base");
    a.sync(&store);
    b.sync(&store);
    a.write("note.md", b"edited on a");
    b.write("note.md", b"edited on b!");
    a.sync(&store);

    let hooks = RunHooks {
        hold_conflicts: true,
        ..RunHooks::default()
    };
    let got = b.run(&store, &hooks);
    assert!(got.conflicts.is_empty(), "no copy made: {}", got.summary());
    assert_eq!(got.held.len(), 1);
    let held = &got.held[0];
    assert_eq!(held.key, "note.md");
    assert_eq!(held.here, hash_bytes(b"edited on b!"));
    assert_eq!(held.there, hash_bytes(b"edited on a"));
    assert_eq!(held.there_size, 11);
    assert_eq!(held.there_device, "dev-a");
    assert_eq!(b.read("note.md").as_deref(), Some(&b"edited on b!"[..]), "mine stays here");
    assert_eq!(index_of(&store).files["note.md"].hash, hash_bytes(b"edited on a"));
    assert_eq!(b.base().files["note.md"].hash, hash_bytes(b"base"), "the base did not move");
    assert_eq!(fs::read_dir(b.folder.path()).unwrap().count(), 1, "no conflict copy");

    // "Keep both": the next run makes the kit's conflict copy.
    let both: BTreeSet<String> = [String::from("note.md")].into_iter().collect();
    let hooks = RunHooks {
        hold_conflicts: true,
        keep_both: Some(&both),
        ..RunHooks::default()
    };
    let got = b.run(&store, &hooks);
    assert!(got.held.is_empty());
    assert_eq!(got.conflicts.len(), 1, "{}", got.summary());
    assert_eq!(b.read("note.md").as_deref(), Some(&b"edited on a"[..]));
}

#[test]
fn a_run_tells_each_file_as_it_moves() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    let b = Device::new("dev-b");
    a.write("one.txt", b"1");
    a.write("two.txt", b"22");
    let events: Mutex<Vec<SyncEvent>> = Mutex::new(Vec::new());
    let hear = |event: SyncEvent| events.lock().unwrap().push(event);
    let hooks = RunHooks {
        progress: Some(&hear),
        ..RunHooks::default()
    };
    a.run(&store, &hooks);
    let heard = std::mem::take(&mut *events.lock().unwrap());
    assert_eq!(
        heard.first(),
        Some(&SyncEvent::Planned {
            up_files: 2,
            up_bytes: 3,
            down_files: 0,
            down_bytes: 0,
        })
    );
    for key in ["one.txt", "two.txt"] {
        assert!(heard.iter().any(
            |e| matches!(e, SyncEvent::Started { key: k, up: true, .. } if k == key)
        ));
        assert!(heard.iter().any(|e| matches!(
            e,
            SyncEvent::Finished { key: k, up: true, error: None, .. } if k == key
        )));
    }

    b.run(&store, &hooks);
    let heard = std::mem::take(&mut *events.lock().unwrap());
    assert!(matches!(
        heard.first(),
        Some(SyncEvent::Planned { down_files: 2, down_bytes: 3, .. })
    ));
    assert_eq!(
        heard
            .iter()
            .filter(|e| matches!(e, SyncEvent::Finished { up: false, error: None, .. }))
            .count(),
        2
    );
}

#[test]
fn a_cancelled_run_changes_nothing_and_the_next_run_does_it_all() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("one.txt", b"1");
    a.write("two.txt", b"22");
    let stop = AtomicBool::new(true);
    let hooks = RunHooks {
        cancel: Some(&stop),
        ..RunHooks::default()
    };
    let got = a.run(&store, &hooks);
    assert!(got.cancelled);
    assert!(!got.index_written);
    assert!(store.read(&index_key(PREFIX)).is_none(), "no index on the drive");
    assert!(LocalIndex::load(&a.index()).unwrap().is_none(), "no base written");

    let got = a.sync(&store);
    assert!(!got.cancelled);
    assert_eq!(got.files_up, 2, "{}", got.summary());
}

#[test]
fn a_freed_file_stays_on_the_drive_and_opening_it_brings_it_back() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("docs/report.txt", b"the report");
    a.write("keep.txt", b"keep");
    a.sync(&store);

    evict_file(&a.root(), &a.index(), "docs/report.txt").unwrap();
    assert!(a.read("docs/report.txt").is_none(), "its bytes are gone from here");
    assert!(a.folder.path().join("docs").is_dir(), "its folder stays");
    assert!(a.base().files["docs/report.txt"].cloud_only);
    let got = a.sync(&store);
    assert_eq!(got.deleted_there, 0, "a freed file is no delete: {}", got.summary());
    assert!(!got.index_written);
    assert_eq!(got.cloud_only, vec![String::from("docs/report.txt")]);

    let path = fetch_file(
        Target::Index(&store),
        &a.root(),
        &a.index(),
        &a.opts(),
        "docs/report.txt",
    )
    .unwrap();
    assert_eq!(path, path_of(a.folder.path(), "docs/report.txt"));
    assert_eq!(fs::read(&path).unwrap(), b"the report");
    assert!(!a.base().files["docs/report.txt"].cloud_only);
    let got = a.sync(&store);
    assert_eq!(got.files_up, 0, "{}", got.summary());
    assert!(got.cloud_only.is_empty());
}

#[test]
fn a_file_changed_since_its_last_sync_is_not_freed() {
    let store = S3Bucket::new();
    let a = Device::new("dev-a");
    a.write("draft.txt", b"first");
    a.sync(&store);
    a.write("draft.txt", b"second, not synced yet");
    let refused = evict_file(&a.root(), &a.index(), "draft.txt").unwrap_err();
    assert!(refused.to_string().contains("not synced yet"), "{refused}");
    assert_eq!(a.read("draft.txt").as_deref(), Some(&b"second, not synced yet"[..]));
    a.write("new.txt", b"never synced");
    assert!(evict_file(&a.root(), &a.index(), "new.txt").is_err());
}
