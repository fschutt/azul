//! The base as a state database (feature `state-db`): round trips, row-wise saves, the import
//! of an older JSON base, owner-only files, never inside the synced folder.

use std::fs;

use azul_storage::testing::TempDir;

use super::fake_s3::S3Bucket;
use crate::sync::{
    base::{BaseDb, BASE_FORMAT},
    local::hash_bytes,
    sync_folder, BaseEntry, LocalIndex, LocalRoot, SyncOptions,
};

fn entry(bytes: &[u8]) -> BaseEntry {
    serde_json::from_value(serde_json::json!({
        "hash": hash_bytes(bytes),
        "size": bytes.len(),
        "mtime_ns": 1_000_000_000u64,
    }))
    .unwrap()
}

fn index(root: &std::path::Path) -> LocalIndex {
    let mut index = LocalIndex::new(root, "d-test", "e2e/");
    index.generation = 7;
    index.files.insert(String::from("a.txt"), entry(b"a"));
    index.files.insert(String::from("dir/b.txt"), entry(b"b"));
    index
}

#[test]
fn a_base_saved_to_a_db_path_reads_back_whole() {
    let root = TempDir::new("base-root");
    let state = TempDir::new("base-state");
    let path = state.path().join("index.db");
    let saved = index(root.path());
    saved.save(&path).unwrap();
    assert_eq!(LocalIndex::load(&path).unwrap(), Some(saved));
    assert!(!state.path().join("index.json").exists(), "no JSON base beside it");
}

#[test]
fn a_save_drops_the_files_gone_and_keeps_the_rest() {
    let root = TempDir::new("base-root");
    let state = TempDir::new("base-state");
    let path = state.path().join("index.db");
    let mut base = index(root.path());
    base.save(&path).unwrap();
    base.files.remove("a.txt");
    base.files.insert(String::from("c.txt"), entry(b"c"));
    base.generation = 8;
    base.save(&path).unwrap();
    let back = LocalIndex::load(&path).unwrap().unwrap();
    assert_eq!(back.generation, 8);
    assert_eq!(back.files.keys().cloned().collect::<Vec<_>>(), vec!["c.txt", "dir/b.txt"]);
}

#[test]
fn an_older_json_base_is_imported_once_and_kept_aside() {
    let root = TempDir::new("base-root");
    let state = TempDir::new("base-state");
    let json = state.path().join("index.json");
    let old = index(root.path());
    old.save(&json).unwrap();
    let db = state.path().join("index.db");
    assert_eq!(LocalIndex::load(&db).unwrap(), Some(old.clone()));
    assert!(!json.exists());
    assert!(state.path().join("index.json.migrated").exists());
    assert_eq!(LocalIndex::load(&db).unwrap(), Some(old), "the second load reads the database");
}

/// The state database keeps the base's format version too: a base a newer version wrote is
/// never taken for this one's (D5: the pass pairs afresh and deletes nothing), and one of an
/// older format is read as the current one.
#[test]
fn a_base_database_reads_the_formats_it_knows_and_refuses_a_newer_one() {
    use crate::sync::local::LOCAL_VERSION;
    let root = TempDir::new("base-root");
    let state = TempDir::new("base-state");
    let path = state.path().join("index.db");
    let mut old = index(root.path());
    old.version = 1;
    old.save(&path).unwrap();
    assert_ne!(LOCAL_VERSION, 1, "format 1 is older than this version's");
    let read = LocalIndex::load(&path).unwrap().unwrap();
    assert_eq!(read.version, LOCAL_VERSION);
    assert_eq!(read.files, old.files);
    let newer_path = state.path().join("newer.db");
    let mut newer = index(root.path());
    newer.version = LOCAL_VERSION + 1;
    newer.save(&newer_path).unwrap();
    let err = LocalIndex::load(&newer_path).unwrap_err().to_string();
    assert!(err.contains("newer version"), "{err}");
}

#[cfg(unix)]
#[test]
fn the_base_and_its_side_files_are_readable_by_their_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new("base-root");
    let state = TempDir::new("base-state");
    let dir = state.path().join("pairing");
    let path = dir.join("index.db");
    index(root.path()).save(&path).unwrap();
    assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o077, 0);
    for entry in fs::read_dir(&dir).unwrap().flatten() {
        let mode = entry.metadata().unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{:?} is {mode:o}", entry.path());
    }
}

#[test]
fn a_base_inside_the_synced_folder_is_refused() {
    let root = TempDir::new("base-root");
    let inside = root.path().join(".azlin").join("index.db");
    assert!(BaseDb::open(&inside, Some(root.path())).is_err());
    assert!(index(root.path()).save(&inside).is_err());
}

#[test]
fn a_database_without_a_pairing_record_is_an_error_never_an_empty_base() {
    let state = TempDir::new("base-state");
    let path = state.path().join("index.db");
    let db = BaseDb::open(&path, None).unwrap();
    assert_eq!(db.load().unwrap(), None, "a new database: never synced");
    drop(db);
    // A base whose pairing record is lost: the files alone say nothing about the pairing.
    let root = TempDir::new("base-root");
    index(root.path()).save(&path).unwrap();
    let db = BaseDb::open(&path, None).unwrap();
    assert!(db.load().unwrap().is_some());
    assert_eq!(BASE_FORMAT, "azcloud.sync-base 1");
}

#[test]
fn passes_with_a_database_base_send_nothing_twice() {
    let store = S3Bucket::new();
    let folder = TempDir::new("base-folder");
    let state = TempDir::new("base-state");
    fs::write(folder.path().join("a.txt"), b"alpha").unwrap();
    fs::write(folder.path().join("b.txt"), b"beta").unwrap();
    let root = LocalRoot::folder(folder.path());
    let opts = SyncOptions::new("e2e/base/", "d-test", "dev-a").unwrap();
    let path = state.path().join("index.db");
    let first = sync_folder(&store, &root, &path, &opts).unwrap();
    assert_eq!(first.files_up, 2);
    let second = sync_folder(&store, &root, &path, &opts).unwrap();
    assert_eq!(second.files_up, 0, "{}", second.summary());
    assert!(!second.index_written);
}

/// A base database that cannot be read (a crash mid-write, a bad sector) is no reason to stop
/// syncing (D5): the pass says so and pairs afresh, deleting nothing, and the next pass works
/// from a new base - the damaged one is kept aside, never written into.
#[test]
fn a_damaged_base_database_is_set_aside_and_the_folder_pairs_afresh() {
    let store = S3Bucket::new();
    let folder = TempDir::new("base-folder");
    let state = TempDir::new("base-state");
    fs::write(folder.path().join("a.txt"), b"alpha").unwrap();
    fs::write(folder.path().join("b.txt"), b"beta").unwrap();
    let root = LocalRoot::folder(folder.path());
    let opts = SyncOptions::new("e2e/base/", "d-test", "dev-a").unwrap();
    let path = state.path().join("index.db");
    sync_folder(&store, &root, &path, &opts).unwrap();
    fs::write(&path, br#"{"generation": 3, "files": {"half": "#).unwrap();

    let after = sync_folder(&store, &root, &path, &opts).unwrap();
    assert!(
        after.problems.iter().any(|p| p.code == crate::sync::problems::Code::StateReset),
        "{:?}",
        after.problems
    );
    assert_eq!((after.deleted_here, after.deleted_there), (0, 0), "{}", after.summary());
    assert!(folder.path().join("a.txt").exists() && folder.path().join("b.txt").exists());
    let kept = state.path().join("index.db.unreadable");
    assert!(kept.exists(), "the damaged base is kept aside");

    let next = sync_folder(&store, &root, &path, &opts).unwrap();
    assert!(next.problems.is_empty(), "{:?}", next.problems);
    assert_eq!(next.files_up, 0, "{}", next.summary());
}

/// A synced folder whose sync state is gone (deleted, a lost cache) holds the marker of its
/// pairing: the pass says so, matches both sides by content and deletes nothing; a folder
/// never synced says nothing.
#[test]
fn a_synced_folder_whose_state_is_gone_says_so_and_deletes_nothing() {
    let store = S3Bucket::new();
    let folder = TempDir::new("base-folder");
    let state = TempDir::new("base-state");
    fs::write(folder.path().join("a.txt"), b"alpha").unwrap();
    let root = LocalRoot::folder(folder.path());
    let opts = SyncOptions::new("e2e/lost/", "d-test", "dev-a").unwrap();
    let path = state.path().join("index.json");
    let first = sync_folder(&store, &root, &path, &opts).unwrap();
    assert!(first.problems.is_empty(), "a new pairing: {:?}", first.problems);
    fs::remove_file(&path).unwrap();

    let after = sync_folder(&store, &root, &path, &opts).unwrap();
    assert!(
        after.problems.iter().any(|p| p.code == crate::sync::problems::Code::StateReset),
        "{:?}",
        after.problems
    );
    assert_eq!((after.files_up, after.deleted_here, after.deleted_there), (0, 0, 0), "{}", after.summary());
    let next = sync_folder(&store, &root, &path, &opts).unwrap();
    assert!(next.problems.is_empty(), "{:?}", next.problems);
}

/// Everything a pass keeps in the device's state folder names the user's files (the base, the
/// drive's index it caches): readable by its owner only, whichever base it keeps.
#[cfg(unix)]
#[test]
fn every_file_a_pass_keeps_in_the_state_folder_is_its_owners_only() {
    use std::os::unix::fs::PermissionsExt;
    for base in ["index.db", "index.json"] {
        let store = S3Bucket::new();
        let folder = TempDir::new("base-folder");
        let state = TempDir::new("base-state");
        fs::write(folder.path().join("secret-plans.txt"), b"alpha").unwrap();
        let root = LocalRoot::folder(folder.path());
        let opts = SyncOptions::new("e2e/base/", "d-test", "dev-a").unwrap();
        let path = state.path().join(base);
        sync_folder(&store, &root, &path, &opts).unwrap();
        sync_folder(&store, &root, &path, &opts).unwrap();
        let mut seen = Vec::new();
        for entry in fs::read_dir(state.path()).unwrap() {
            let entry = entry.unwrap();
            let mode = entry.metadata().unwrap().permissions().mode();
            seen.push(entry.file_name());
            assert_eq!(mode & 0o077, 0, "{base}: {:?} is {:o}", entry.file_name(), mode & 0o777);
        }
        assert!(seen.len() >= 2, "{base}: the base and the cached index: {seen:?}");
    }
}
