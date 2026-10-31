//! This device's side of a synced folder: the scan, the racy files, the shared config without
//! its endpoints, the checks before a download overwrites or a delete removes a file.

use std::{collections::BTreeMap, fs, path::Path};

use azul_storage::testing::TempDir;
use serde_json::Value;

use crate::sync::{
    local::{
        self, hash_bytes, index_path, json_blob, json_with_local_keys, mtime_ns, path_of,
        prune_empty_parents, scan, still_as_scanned, BaseEntry, RACY_NS,
    },
    rules::Rules,
};

fn write(root: &Path, key: &str, bytes: &[u8]) {
    let path = path_of(root, key);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn a_scan_finds_the_files_the_rules_let_through_and_never_follows_a_link() {
    let dir = TempDir::new("azcloud-scan");
    let root = dir.path();
    write(root, "notes/a.md", b"a");
    write(root, "notes/sub/b.md", b"bb");
    write(root, ".azlin/cache", b"manifest");
    write(root, "notes/.a.md.azul-storage-1-0.tmp", b"half");
    write(root, "music/cache/c.jpg", b"jpg");
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc", root.join("notes/etc")).unwrap();
    let scan = scan(
        root,
        &Rules::azlin_data(),
        &BTreeMap::new(),
        0,
        &[],
        u64::MAX,
    )
    .unwrap();
    let keys: Vec<&str> = scan.files.keys().map(String::as_str).collect();
    assert_eq!(keys, vec!["notes/a.md", "notes/sub/b.md"]);
    assert_eq!(scan.files["notes/sub/b.md"].hash, hash_bytes(b"bb"));
    assert_eq!(scan.hashed, 2);
    assert_eq!(
        scan.excluded, 3,
        ".azlin/, the temporary file, music/cache/"
    );
    #[cfg(unix)]
    assert!(scan
        .skipped
        .iter()
        .any(|(k, why)| k == "notes/etc" && why.contains("link")));
    // A missing folder is no empty folder: an unplugged disk must never read as "every file
    // deleted" (the run checks the folder's marker before it scans).
    let missing = local::scan(
        &root.join("nope"),
        &Rules::base(),
        &BTreeMap::new(),
        0,
        &[],
        1,
    );
    assert!(missing.is_err(), "a missing folder is an error, not an empty scan");
}

#[test]
fn an_unchanged_file_keeps_its_base_hash_unless_its_time_is_racy() {
    let dir = TempDir::new("azcloud-racy");
    let root = dir.path();
    write(root, "a.txt", b"one");
    let meta = fs::metadata(root.join("a.txt")).unwrap();
    let entry = BaseEntry {
        hash: String::from("cached"),
        size: 3,
        mtime_ns: mtime_ns(&meta),
        ..BaseEntry::default()
    };
    let base: BTreeMap<String, BaseEntry> = [(String::from("a.txt"), entry.clone())].into();
    let long_after = entry.mtime_ns + 10 * RACY_NS;
    let s = scan(root, &Rules::none(), &base, long_after, &[], u64::MAX).unwrap();
    assert_eq!(
        s.files["a.txt"].hash, "cached",
        "size and time say unchanged"
    );
    assert_eq!(s.hashed, 0);
    let s = scan(
        root,
        &Rules::none(),
        &base,
        entry.mtime_ns + 1,
        &[],
        u64::MAX,
    )
    .unwrap();
    assert_eq!(
        s.files["a.txt"].hash,
        hash_bytes(b"one"),
        "racy: read again"
    );
    let s = scan(root, &Rules::none(), &base, long_after, &[], 2).unwrap();
    assert!(s.files.is_empty());
    assert!(s.skipped[0].1.contains("--max-file-mb"));
}

#[test]
fn the_shared_config_syncs_without_its_endpoints_and_gets_this_computers_back() {
    let ours =
        br#"{"mode": "dark", "endpoints": {"profile": "local"}, "currentTheme": "flat"}"#;
    let blob = json_blob(ours).unwrap();
    let text = String::from_utf8(blob.clone()).unwrap();
    assert!(!text.contains("endpoints"), "{text}");
    assert!(
        text.find("currentTheme") < text.find("mode"),
        "keys in order: {text}"
    );
    assert_eq!(
        json_blob(br#"{"currentTheme":"flat","mode":"dark","endpoints":{"token":"x"}}"#)
            .unwrap(),
        blob,
        "another computer's endpoints make no other blob"
    );
    // A blob from the bucket that carries endpoints never brings them here.
    let evil = br#"{"mode": "light", "endpoints": {"token": "https://evil.test"}}"#;
    let written = json_with_local_keys(evil, Some(&ours[..])).unwrap();
    let written: Value = serde_json::from_slice(&written).unwrap();
    assert_eq!(written["mode"], "light");
    assert_eq!(written["endpoints"]["profile"], "local");
    assert!(written["endpoints"]["token"].is_null());
    let fresh: Value =
        serde_json::from_slice(&json_with_local_keys(evil, None).unwrap()).unwrap();
    assert!(fresh.get("endpoints").is_none());
    assert_eq!(json_blob(b"not json"), None);
    assert_eq!(json_blob(b"[1, 2]"), None);
}

#[test]
fn a_json_merge_file_that_is_no_object_is_skipped_not_uploaded_raw() {
    let dir = TempDir::new("azcloud-json-skip");
    write(dir.path(), "config.json", br#"{"endpoints": {"token": "#);
    let s = scan(
        dir.path(),
        &Rules::none(),
        &BTreeMap::new(),
        0,
        &[String::from("config.json")],
        u64::MAX,
    )
    .unwrap();
    assert!(s.files.is_empty());
    assert!(s.skipped[0].1.contains("JSON object"), "{:?}", s.skipped);
}

#[test]
fn a_change_after_the_scan_is_noticed_before_a_download_overwrites_it() {
    let dir = TempDir::new("azcloud-still");
    let root = dir.path();
    write(root, "a.txt", b"one");
    let s = scan(root, &Rules::none(), &BTreeMap::new(), 0, &[], u64::MAX).unwrap();
    assert!(still_as_scanned(root, "a.txt", s.files.get("a.txt")));
    assert!(still_as_scanned(root, "b.txt", None));
    write(root, "a.txt", b"one, edited");
    assert!(!still_as_scanned(root, "a.txt", s.files.get("a.txt")));
    write(root, "b.txt", b"new");
    assert!(!still_as_scanned(root, "b.txt", None));
}

#[test]
fn a_delete_prunes_the_folders_it_left_empty_and_nothing_else() {
    let dir = TempDir::new("azcloud-prune");
    let root = dir.path();
    write(root, "a/b/c/d.txt", b"x");
    write(root, "a/keep.txt", b"y");
    fs::remove_file(root.join("a/b/c/d.txt")).unwrap();
    prune_empty_parents(root, "a/b/c/d.txt");
    assert!(!root.join("a/b").exists());
    assert!(root.join("a/keep.txt").exists());
    assert!(root.exists());
}

#[test]
fn the_local_index_lives_in_the_state_folder_one_file_per_folder_bucket_and_prefix() {
    let sync = Path::new("/state/sync");
    let a = index_path(sync, "d-1", "azlin/data/", Path::new("/data"));
    assert!(a.starts_with(sync));
    assert_ne!(
        a,
        index_path(sync, "d-1", "azlin/config/", Path::new("/data"))
    );
    assert_ne!(
        a,
        index_path(sync, "d-2", "azlin/data/", Path::new("/data"))
    );
    assert_eq!(
        a,
        index_path(sync, "d-1", "azlin/data/", Path::new("/data"))
    );
}

#[test]
fn a_file_another_app_holds_locked_waits_and_is_never_taken_for_deleted() {
    use crate::sync::problems::Code;
    let dir = TempDir::new("azcloud-scan-locked");
    let root = dir.path();
    write(root, "open-in-word.docx", b"v2 saved while open");
    write(root, "other.txt", b"goes on");
    // Another app's exclusive lock: a handle of its own on the file.
    let held = fs::File::options()
        .read(true)
        .write(true)
        .open(root.join("open-in-word.docx"))
        .unwrap();
    held.lock().unwrap();
    let locked = scan(root, &Rules::base(), &BTreeMap::new(), 0, &[], u64::MAX).unwrap();
    assert!(!locked.files.contains_key("open-in-word.docx"), "not read mid-write");
    assert!(locked.unknown.contains("open-in-word.docx"), "unknown: never a delete");
    assert!(locked
        .problems
        .iter()
        .any(|p| p.key == "open-in-word.docx" && p.code == Code::Locked));
    assert!(locked.files.contains_key("other.txt"), "the rest goes on");
    held.unlock().unwrap();
    drop(held);
    let free = scan(root, &Rules::base(), &BTreeMap::new(), 0, &[], u64::MAX).unwrap();
    assert_eq!(
        free.files["open-in-word.docx"].hash,
        hash_bytes(b"v2 saved while open")
    );
}

#[test]
fn a_base_a_newer_version_wrote_is_never_read_as_this_ones() {
    use crate::sync::local::{LocalIndex, LOCAL_VERSION};
    let dir = TempDir::new("azcloud-newer-base");
    let path = dir.path().join("index.json");
    let mut index = LocalIndex::new(dir.path(), "bucket", "");
    index.save(&path).unwrap();
    assert!(LocalIndex::load(&path).unwrap().is_some());
    index.version = LOCAL_VERSION + 1;
    index.save(&path).unwrap();
    let err = LocalIndex::load(&path).unwrap_err().to_string();
    assert!(err.contains("newer version"), "{err}");
}

/// A base an earlier version wrote (state format 1, before the base kept folders) is read as
/// this version's: its files as they were, its folders not known yet (the next pass records
/// them), and its next save is in the current format.
#[test]
fn a_base_of_state_format_one_is_read_and_saved_in_the_current_format() {
    use crate::sync::local::{LocalIndex, LOCAL_VERSION};
    let dir = TempDir::new("azcloud-v1-base");
    let path = dir.path().join("index.json");
    let snapshot = serde_json::json!({
        "format": "azcloud.sync-local",
        "version": 1,
        "root": "/home/user/AzDrive",
        "bucket": "d-test",
        "prefix": "",
        "generation": 4,
        "scanned_at_ns": 1_761_000_000_000_000_000_u64,
        "remote_etag": "\"v4\"",
        "files": {
            "notes/a.md": {
                "hash": hash_bytes(b"alpha"),
                "size": 5,
                "mtime_ns": 1_760_000_000_000_000_000_u64
            },
            "run.sh": {
                "hash": hash_bytes(b"#!/bin/sh"),
                "size": 9,
                "mtime_ns": 1_760_000_000_000_000_001_u64,
                "exec": true
            }
        },
        "root_id": "a1b2c3"
    });
    fs::write(&path, serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();
    let index = LocalIndex::load(&path).unwrap().expect("the base");
    assert_ne!(LOCAL_VERSION, 1, "the snapshot is of an older format than this version's");
    assert_eq!(index.version, LOCAL_VERSION);
    assert_eq!((index.generation, index.files.len()), (4, 2));
    assert!(index.files["run.sh"].exec);
    assert_eq!(index.root_id.as_deref(), Some("a1b2c3"));
    index.save(&path).unwrap();
    let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["version"], Value::from(LOCAL_VERSION));
    assert_eq!(LocalIndex::load(&path).unwrap(), Some(index));
}

/// P2: an ignore rule typed in NFC takes a folder a Mac named in NFD, and the other way round.
#[test]
fn an_ignore_rule_matches_names_in_either_normalization() {
    let mut rules = Rules::base();
    rules.add("caf\u{e9}-drafts/", "ignored");
    rules.add("Pra\u{308}sentation-*.key", "ignored");
    assert!(rules.excluded("cafe\u{301}-drafts/draft.txt").is_some());
    assert!(rules.excluded("caf\u{e9}-drafts/draft.txt").is_some());
    assert!(rules.excluded_dir("cafe\u{301}-drafts").is_some());
    assert!(rules.excluded("Pr\u{e4}sentation-2.key").is_some());
    assert!(rules.excluded("keep.txt").is_none());
}
