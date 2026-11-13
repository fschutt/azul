//! This device's side of a synced folder: the scan, the racy files, the shared config without
//! its endpoints, the checks before a download overwrites or a delete removes a file.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use azul_storage::testing::TempDir;
use serde_json::Value;

use crate::sync::{
    local::{
        self, hash_bytes, index_path, json_blob, json_with_local_keys, mtime_ns, open_checked,
        link_detail, other_spelling, path_of, prune_empty_parents, read_failed, same_entry, scan,
        still_as_scanned, BaseEntry, Skip, RACY_NS,
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

/// What the base keeps of `a.txt` under `root` as a scan left it: size, time, status-change time
/// and the file's identity.
fn base_entry_of(root: &Path, name: &str, hash: &str) -> BaseEntry {
    let st = azul_storage::beneath::Root::open(root)
        .unwrap()
        .top()
        .unwrap()
        .lstat(name)
        .unwrap();
    BaseEntry {
        hash: hash.to_string(),
        size: st.size,
        mtime_ns: st.mtime_ns,
        ctime_ns: st.ctime_ns,
        ino: st.ino,
        dev: st.dev,
        ..BaseEntry::default()
    }
}

#[test]
fn an_unchanged_file_keeps_its_base_hash_unless_its_time_is_racy() {
    let dir = TempDir::new("azcloud-racy");
    let root = dir.path();
    write(root, "a.txt", b"one");
    let meta = fs::metadata(root.join("a.txt")).unwrap();
    // The base as a scan wrote it, with the file's identity; where the system gives none (an
    // inode of 0) the hash is not taken on a guess - see the X8 tests below.
    let entry = BaseEntry {
        mtime_ns: mtime_ns(&meta),
        ..base_entry_of(root, "a.txt", "cached")
    };
    let known = entry.id().is_known();
    let base: BTreeMap<String, BaseEntry> = [(String::from("a.txt"), entry.clone())].into();
    let long_after = entry.mtime_ns + 10 * RACY_NS;
    let s = scan(root, &Rules::none(), &base, long_after, &[], u64::MAX).unwrap();
    if known {
        assert_eq!(
            s.files["a.txt"].hash, "cached",
            "size, time and identity say unchanged"
        );
        assert_eq!(s.hashed, 0);
    }
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
fn x_ar5a_8_a_base_entry_without_an_inode_is_read_again_not_taken_for_unchanged() {
    // An inode of 0 (a base an older version wrote, a system without inodes) used to match any
    // file: size and time alone kept the cached hash, and `0 == 0` made every file the same one.
    let dir = TempDir::new("azcloud-x8-scan");
    let root = dir.path();
    write(root, "a.txt", b"one");
    let long_after = u64::MAX / 2;
    let known = base_entry_of(root, "a.txt", "stale");
    let unknown = BaseEntry {
        ino: 0,
        dev: 0,
        ..known.clone()
    };
    let mut base: BTreeMap<String, BaseEntry> = BTreeMap::new();
    base.insert(String::from("a.txt"), unknown);
    let s = scan(root, &Rules::none(), &base, long_after, &[], u64::MAX).unwrap();
    assert_eq!(
        s.files["a.txt"].hash,
        hash_bytes(b"one"),
        "no identity: the file is read, the base's hash is not trusted"
    );
    assert_eq!(s.hashed, 1);
    // The same entry with its identity is trusted, and another inode is not the same file.
    #[cfg(unix)]
    {
        base.insert(String::from("a.txt"), known.clone());
        let s = scan(root, &Rules::none(), &base, long_after, &[], u64::MAX).unwrap();
        assert_eq!(s.files["a.txt"].hash, "stale");
        assert_eq!(s.hashed, 0);
        base.insert(
            String::from("a.txt"),
            BaseEntry {
                ino: known.ino + 1,
                ..known
            },
        );
        let s = scan(root, &Rules::none(), &base, long_after, &[], u64::MAX).unwrap();
        assert_eq!(s.files["a.txt"].hash, hash_bytes(b"one"));
        assert_eq!(s.hashed, 1);
    }
}

#[test]
fn x_ar5a_8_a_scanned_file_without_an_inode_is_never_still_there() {
    use crate::sync::local::{Content, LocalFile};
    let dir = TempDir::new("azcloud-x8-matches");
    let root = dir.path();
    write(root, "a.txt", b"one");
    let st = azul_storage::beneath::Root::open(root)
        .unwrap()
        .top()
        .unwrap()
        .lstat("a.txt")
        .unwrap();
    let file = |ino: u64, dev: u64| LocalFile {
        hash: hash_bytes(b"one"),
        size: st.size,
        mtime_ns: st.mtime_ns,
        blob_size: st.size,
        content: Content::Raw,
        exec: false,
        ctime_ns: st.ctime_ns,
        ino,
        dev,
        link: None,
    };
    assert!(
        !file(0, 0).matches(&st),
        "what cannot be told is not the file that was scanned: nothing is replaced on a guess"
    );
    #[cfg(unix)]
    {
        assert!(file(st.ino, st.dev).matches(&st));
        assert!(!file(st.ino + 1, st.dev).matches(&st), "another inode");
        assert!(!file(st.ino, st.dev + 1).matches(&st), "another device");
        let grown = LocalFile {
            size: st.size + 1,
            ..file(st.ino, st.dev)
        };
        assert!(!grown.matches(&st));
    }
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

/// AZ-CN13 / RF54: what leaves the computer is an allow-list of the two keys of the look. The
/// `oauth` section (client secrets, endpoints a sign-in sends its code to), the pins, the update
/// policy and any section a later build adds stay on the machine, and a blob from the bucket that
/// carries one of them never brings it here (whoever can write the bucket would set
/// `oauth.google.token_url` and receive the next sign-in's code and client secret).
#[test]
fn az_cn13_only_the_look_is_synced_and_every_other_section_stays_on_this_computer() {
    let ours = br#"{"currentTheme": "flat", "mode": "dark",
        "oauth": {"google": {"client_id": "id-1", "client_secret": "s3cret"}},
        "trust": {"pinned_roots": ["~/ca.pem"]}, "updates": {"cohort": 7},
        "locale": "de", "a-later-section": {"x": 1}}"#;
    let blob = json_blob(ours).unwrap();
    let text = String::from_utf8(blob.clone()).unwrap();
    for local in [
        "oauth",
        "s3cret",
        "trust",
        "updates",
        "locale",
        "a-later-section",
    ] {
        assert!(!text.contains(local), "{local} left the computer: {text}");
    }
    assert!(
        text.contains("currentTheme") && text.contains("mode"),
        "{text}"
    );
    // A blob from the bucket brings only the look; this computer's own sections are put back.
    let theirs = br#"{"currentTheme": "flora", "mode": "light",
        "oauth": {"google": {"token_url": "https://evil.test/token"}},
        "locale": "fr", "a-later-section": "theirs"}"#;
    let written = json_with_local_keys(theirs, Some(&ours[..])).unwrap();
    let written: Value = serde_json::from_slice(&written).unwrap();
    assert_eq!(written["currentTheme"], "flora");
    assert_eq!(written["mode"], "light");
    assert_eq!(written["oauth"]["google"]["client_secret"], "s3cret");
    assert!(
        written["oauth"]["google"]["token_url"].is_null(),
        "{written}"
    );
    assert_eq!(written["locale"], "de");
    assert_eq!(written["a-later-section"]["x"], 1);
    assert_eq!(written["trust"]["pinned_roots"][0], "~/ca.pem");
    // On a computer with no config yet, only the look arrives.
    let fresh: Value =
        serde_json::from_slice(&json_with_local_keys(theirs, None).unwrap()).unwrap();
    assert_eq!(fresh.as_object().unwrap().len(), 2, "{fresh}");
    // Another computer's other sections make no other blob.
    assert_eq!(
        json_blob(br#"{"mode": "dark", "currentTheme": "flat", "oauth": 1}"#).unwrap(),
        json_blob(br#"{"currentTheme": "flat", "mode": "dark", "locale": "x"}"#).unwrap()
    );
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

/// ARCH29 C6c, sync/local.rs. `stat_now` and `prune_empty_parents` walk their key as an `OnDisk` one:
/// they find what is there and make nothing. A name a drive could send that the full rows refuse - a
/// repository's folder in any spelling, its NTFS short name, the engine's own folder - is only looked
/// at: no folder appears for it, however it is spelled (the walk that made a drive-renamed `.git` on
/// the victim's disk, FILES-1, took any name and a flag). Green on the old code too: it pins what the
/// type now holds.
#[test]
fn c6c_a_look_at_a_name_the_full_rows_refuse_makes_no_folder_and_a_prune_makes_none() {
    let dir = TempDir::new("azcloud-c6c-local");
    let root = dir.path();
    for key in [
        ".GIT/hooks/post-commit",
        "GIT~1/config",
        "proj/.hg./x",
        ".svn/x",
        ".azlin/sync/x",
        ".git::$INDEX_ALLOCATION/x",
    ] {
        assert!(
            !matches!(local::stat_now(root, key), Ok(Some(_))),
            "{key:?}"
        );
        prune_empty_parents(root, key);
    }
    assert_eq!(
        fs::read_dir(root).unwrap().count(),
        0,
        "a folder was made for a look"
    );
    // What is there is still found.
    write(root, "proj/a.txt", b"a");
    assert!(local::stat_now(root, "proj/a.txt").unwrap().is_some());
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
    assert!(
        !locked.files.contains_key("open-in-word.docx"),
        "not read mid-write"
    );
    assert!(
        locked.unknown.contains_key("open-in-word.docx"),
        "unknown: never a delete"
    );
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

/// AZ-W6: which entry of a folder is "the same file" is the disk's identity of it. Windows had none
/// (every file read 0 and 0), so its twin of this question compared the size and the time, and two
/// files copied with their times kept were one.
#[test]
fn az_w6_two_entries_are_the_same_file_by_identity_and_by_size_and_time_only_without_one() {
    use azul_storage::beneath::{Kind, Stat};
    let stat = |dev: u64, ino: u64, size: u64, mtime_ns: u64| Stat {
        kind: Kind::File,
        size,
        mtime_ns,
        ctime_ns: 0,
        ino,
        dev,
        mode: 0o644,
        nlink: 1,
    };
    let a = stat(7, 100, 5, 1_000);
    // Another file of the same size and time: two files, by their identities.
    assert!(!same_entry(&a, &stat(7, 101, 5, 1_000)));
    assert!(!same_entry(&a, &stat(8, 100, 5, 1_000)), "another volume");
    assert!(same_entry(&a, &stat(7, 100, 5, 1_000)));
    // The same file after a write: still the same file (the identity says so, not the size).
    assert!(same_entry(&a, &stat(7, 100, 9, 2_000)));
    // A disk that gives no identity (ino 0): the size and the time are what is left.
    let none = stat(0, 0, 5, 1_000);
    assert!(same_entry(&none, &stat(0, 0, 5, 1_000)));
    assert!(!same_entry(&none, &stat(0, 0, 6, 1_000)));
    assert!(!same_entry(&none, &stat(0, 0, 5, 1_001)));
    // One side known, one not: no identity to compare, the fallback applies the same way.
    assert!(same_entry(&a, &stat(0, 0, 5, 1_000)));
}

/// AZ-W6, through the function production calls: on a disk that folds case the other spelling of a
/// name is the entry that IS the file, not the first entry of its size and time.
#[test]
fn az_w6_the_other_spelling_of_a_name_is_the_entry_that_is_the_file_not_one_that_looks_like_it() {
    let dir = TempDir::new("azcloud-other-spelling");
    let root = dir.path();
    // `Aaa.txt` comes before `Notes.txt` in a listing and is a copy of it, time kept.
    write(root, "Notes.txt", b"same");
    write(root, "Aaa.txt", b"same");
    let when = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    for name in ["Notes.txt", "Aaa.txt"] {
        fs::File::options()
            .write(true)
            .open(root.join(name))
            .unwrap()
            .set_modified(when)
            .unwrap();
    }
    // Given: a disk that folds case (macOS and Windows by default); on one that does not there is
    // no other spelling to find.
    if fs::symlink_metadata(root.join("NOTES.TXT")).is_err() {
        assert_eq!(other_spelling(root, "NOTES.TXT"), None);
        return;
    }
    assert_eq!(
        other_spelling(root, "NOTES.TXT").as_deref(),
        Some("Notes.txt")
    );
    // The spelling it has is not "another": nothing to say.
    assert_eq!(other_spelling(root, "Notes.txt"), None);
}

/// AZ-W8: the scan opens a file to hash it and asks whether another app holds it. On Windows the
/// probe was a shared lock over the whole file kept until the hash was done - and Windows' locks are
/// mandatory, so the app that writes the file (a log, a database, an editor saving in place) failed
/// with a lock violation for as long as the scan read it. The probe asks and takes nothing there.
#[test]
fn az_w8_the_scans_open_of_a_file_never_stops_the_app_that_writes_it() {
    use std::io::Write;
    let dir = TempDir::new("azcloud-probe-lock");
    write(dir.path(), "log.txt", b"line 1\n");
    let tree = azul_storage::beneath::Root::open(dir.path()).unwrap();
    let top = tree.top().unwrap();
    let st = top.lstat("log.txt").unwrap();
    // Through the function the scan calls: the file open as the scan has it while it reads.
    let (reader, _) = open_checked(&top, "log.txt", &st)
        .ok()
        .flatten()
        .expect("a file nobody holds opens");
    let mut writer = fs::OpenOptions::new()
        .append(true)
        .open(dir.path().join("log.txt"))
        .unwrap();
    writer
        .write_all(b"line 2\n")
        .expect("the app that writes the file is not stopped by the scan's read");
    drop(reader);
    assert_eq!(
        fs::read(dir.path().join("log.txt")).unwrap(),
        b"line 1\nline 2\n"
    );
}

/// AZ-W8: a read that fails because another app locked the range is "busy" - the file is skipped
/// for this pass and never taken for deleted - not "cannot be read"; the Windows numbers for it
/// (32, 33) are other errors elsewhere.
#[test]
fn az_w8_a_read_refused_by_a_lock_is_busy_and_any_other_failure_is_unreadable() {
    let busy = |code| {
        matches!(
            read_failed(&std::io::Error::from_raw_os_error(code)),
            Skip::Locked
        )
    };
    assert_eq!(busy(33), cfg!(windows), "ERROR_LOCK_VIOLATION");
    assert_eq!(busy(32), cfg!(windows), "ERROR_SHARING_VIOLATION");
    assert!(!busy(5));
    assert!(matches!(
        read_failed(&std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        Skip::Unreadable(_)
    ));
}

/// AZ-W11: a replace that another app held up is "locked, the next pass tries again", not a broken
/// disk. Windows' sharing and lock violations (32, 33) have no `ErrorKind` of their own, so they
/// were told `IoError`; the numbers are other errors on other systems (EPIPE, EDOM).
#[test]
fn az_w11_a_step_another_app_held_up_is_locked_and_a_broken_one_is_an_io_error() {
    use crate::sync::{problem_of, problems::Code};
    use azul_storage::beneath::BeneathError;
    let io = |code| BeneathError::Io(std::io::Error::from_raw_os_error(code));
    for code in [32, 33] {
        let wanted = if cfg!(windows) {
            Code::Locked
        } else {
            Code::IoError
        };
        assert_eq!(problem_of("a.txt", &io(code)).code, wanted, "{code}");
    }
    let denied = BeneathError::Io(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
    assert_eq!(problem_of("a.txt", &denied).code, Code::Locked);
    let other = BeneathError::Io(std::io::Error::from(std::io::ErrorKind::InvalidData));
    assert_eq!(problem_of("a.txt", &other).code, Code::IoError);
}

/// AZ-W15: a reparse point that is no link - a OneDrive placeholder - was told as "a link to \"\"".
/// It is told for what it is, and a link keeps its target in the sentence.
#[test]
fn az_w15_a_placeholder_is_not_told_as_a_link_to_nowhere() {
    assert_eq!(
        link_detail("../other"),
        "a link to \"../other\" is never followed"
    );
    let placeholder = link_detail("");
    assert!(placeholder.contains("cloud placeholder"), "{placeholder}");
    assert!(
        placeholder.contains("not taken for deleted"),
        "{placeholder}"
    );
    assert!(!placeholder.contains("link to"), "{placeholder}");
}

/// X-DELTA-AZDRIVE-FILES-32 (b): a link whose target reaches the bookkeeping was refused only for the
/// exact first name `.azlin`, and a link to a repository's folder not at all - `.GIT/hooks` is
/// `.git/hooks` on APFS and NTFS. The names a link reaches are held to what a drive's keys are.
#[test]
fn x_delta_azdrive_files_32_a_link_into_a_protected_name_is_not_made_in_any_spelling() {
    use crate::sync::local::link_stays_inside;
    // The bookkeeping is the folder's first name, in any spelling.
    for target in [
        ".azlin/index.json",
        ".AZLIN/index.json",
        ".Azlin",
        "sub/../.AZLIN/x",
    ] {
        assert!(!link_stays_inside("l", target), "{target:?}");
    }
    for target in ["../.azlin/x", "../.AZLIN/x"] {
        assert!(!link_stays_inside("links/l", target), "{target:?}");
    }
    // A version-control name at any depth.
    for target in [
        ".git/hooks",
        ".GIT/hooks/post-commit",
        "sub/.git",
        "sub/GIT~1/config",
        "sub/.hg/hgrc",
        ".svn",
    ] {
        assert!(!link_stays_inside("links/l", target), "{target:?}");
    }
    // Ordinary names are made, and so is one that only looks like a protected one, or a user's
    // own `.azlin` below the root.
    for target in [
        "a.txt",
        "../docs/a.txt",
        "sub/.gitignore",
        "gits/x",
        "x/.azlin/y",
    ] {
        assert!(link_stays_inside("links/l", target), "{target:?}");
    }
    assert!(link_stays_inside("l", "x/.azlin/y"));
}

/// Lowers this process's soft limit of open files to `soft` (the hard limit stays): the scan of a
/// folder with more sibling folders than that is what the next test plays.
#[cfg(unix)]
fn lower_open_files_to(soft: libc::rlim_t) {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: getrlimit and setrlimit only read and write the struct they are given.
    unsafe {
        assert_eq!(libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit), 0);
        limit.rlim_cur = soft.min(limit.rlim_max);
        assert_eq!(libc::setrlimit(libc::RLIMIT_NOFILE, &limit), 0);
    }
}

/// X-DELTA-EDGE-2 (E15a, T9). The scan opened a directory descriptor for every sub-folder of the
/// folder it listed and kept all of them on its stack until each was popped, and the client never
/// raised the limit: a folder with a few hundred sub-folders (macOS' soft limit is 256) failed the
/// `openat` of the rest with EMFILE - those folders were listed Unreadable, never uploaded, never
/// taken for deleted, a problem line every pass. 600 sibling folders with one file each, and three
/// more levels of 40 folders, under a limit of 256 descriptors: every file is found, and at the
/// listing of each folder the process holds at most 64 descriptors (the scan opens a sub-folder when
/// its turn comes). The limit is the process's, and the other tests of this binary open files
/// beside this one, so the scan runs in a child process (this test again, told by its environment).
///
/// X-DELTA-INTEGRITY3-18: a libtest binary that is given a filter that matches no test runs none and
/// exits 0, so the parent that read only the child's status stayed green on a renamed or moved test
/// and the scan under 256 descriptors was never played. The parent now reads what the child SAID:
/// it ran exactly one test, and that one passed. The child's name is not a second spelling of this
/// function's: it is the path of an item declared inside it (`here`), less the crate's name and the
/// item's own.
#[cfg(unix)]
#[test]
fn a_folder_with_more_sibling_folders_than_the_fd_limit_allows_is_scanned_whole() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use crate::sync::{faults, problems::Code};
    fn here() {}
    const CHILD: &str = "AZCLOUD_KIT_FD_LIMIT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        // `<crate>::<module path>::<this function>::here`: libtest names the test by the middle.
        let full = std::any::type_name_of_val(&here);
        let name = full
            .strip_suffix("::here")
            .and_then(|test| test.split_once("::"))
            .map_or(full, |(_krate, test)| test);
        let me = std::env::current_exe().expect("the test binary");
        let out = std::process::Command::new(me)
            .args([name, "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .expect("the child process starts");
        let said = String::from_utf8_lossy(&out.stdout);
        let told = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "the scan under 256 descriptors failed:\n{said}\n{told}"
        );
        // A binary that finds no test of that name exits 0 as well: the child ran this one test.
        assert!(
            said.lines().any(|line| line == "running 1 test")
                && said.contains("test result: ok. 1 passed; 0 failed"),
            "the child process ran no test called {name:?} (it must say `running 1 test` and \
             `1 passed`), so the scan under 256 descriptors was not played:\n{said}\n{told}"
        );
        return;
    }
    let dir = TempDir::new("azcloud-fd-limit");
    let root = dir.path();
    let mut made = 0;
    for i in 0..600 {
        write(root, &format!("wide/f{i:03}/a.txt"), b"a");
        made += 1;
    }
    // Three levels, 40 folders at each, one file in each: wide below a depth, not only at the top.
    let mut at = String::from("levels");
    for level in 0..3 {
        for i in 0..40 {
            write(
                root,
                &format!("{at}/s{i:02}/b.txt"),
                format!("{level}-{i}").as_bytes(),
            );
            made += 1;
        }
        at = format!("{at}/s00");
    }
    lower_open_files_to(256);
    // The descriptors the process holds as each folder is listed (/dev/fd names one for each).
    let peak = Arc::new(AtomicUsize::new(0));
    let heard = Arc::clone(&peak);
    faults::set_listing_hook(
        root,
        Some(Arc::new(move |_folder: &str| {
            let open = fs::read_dir("/dev/fd").map_or(0, Iterator::count);
            heard.fetch_max(open, Ordering::SeqCst);
        })),
    );
    let scan = scan(root, &Rules::none(), &BTreeMap::new(), 0, &[], u64::MAX).unwrap();
    faults::clear(root);
    assert_eq!(
        scan.files.len(),
        made,
        "every file is found; unreadable: {:?}",
        scan.problems
            .iter()
            .filter(|p| p.code == Code::Unreadable)
            .take(3)
            .collect::<Vec<_>>()
    );
    assert!(
        !scan.problems.iter().any(|p| p.code == Code::Unreadable),
        "no folder is unreadable: {:?}",
        scan.problems.iter().take(3).collect::<Vec<_>>()
    );
    assert!(
        scan.unknown.is_empty(),
        "{:?}",
        scan.unknown.keys().take(3).collect::<Vec<_>>()
    );
    let peak = peak.load(Ordering::SeqCst);
    assert!(
        peak > 0,
        "the listing hook heard the scan and /dev/fd answered"
    );
    assert!(
        peak <= 64,
        "{peak} descriptors were open at once: the scan held one for every sibling folder"
    );
}

/// The bytes a scan holds in its keys and sentences: what a tree with long names costs in memory.
fn held_bytes(scan: &local::Scan) -> usize {
    let text = |s: &String| s.len();
    scan.files.keys().map(text).sum::<usize>()
        + scan.dirs.iter().map(text).sum::<usize>()
        + scan.folder_ids.keys().map(text).sum::<usize>()
        + scan
            .links
            .iter()
            .map(|(k, t)| k.len() + t.len())
            .sum::<usize>()
        + scan.mounts.iter().map(text).sum::<usize>()
        + scan.unknown.keys().map(text).sum::<usize>()
        + scan.temps.keys().map(text).sum::<usize>()
        + scan
            .spelled
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>()
        + scan
            .skipped
            .iter()
            .map(|(k, why)| k.len() + why.len())
            .sum::<usize>()
        + scan
            .problems
            .iter()
            .map(|p| p.key.len() + p.detail.len())
            .sum::<usize>()
}

/// Makes the chain `levels` deep of folders called `name` under `top` with `openat` (the path of
/// the deep ones is past what a path-based call takes), calling `each(level, folder)` on the way
/// down.
#[cfg(unix)]
fn chain(
    top: &Path,
    name: &str,
    levels: usize,
    each: impl FnMut(usize, &azul_storage::beneath::Dir),
) {
    azul_storage::testing::make_folder_chain(top, name, levels, each);
}

/// Removes a tree `std::fs::remove_dir_all` would meet with a recursion of its own, and a test that
/// made one waits for.
#[cfg(unix)]
fn remove_tree(path: &Path) {
    let removed = std::process::Command::new("rm")
        .arg("-rf")
        .arg(path)
        .status();
    assert!(removed.is_ok_and(|s| s.success()), "the tree is removed");
}

/// X-DELTA-EDGE-2 and -3 (E15a, T10, the depth half). The scan walks with an explicit stack, so a deep
/// tree overflows no stack - but it opened a descriptor for every folder of a level, kept the key of
/// every folder (a hundred megabytes of keys for ten thousand levels) and went on down a folder no
/// file of which can sync, its key being past 4,096 bytes. A chain of 500 levels of eight-letter
/// folders (the key of the folder at level n is 9n - 1 bytes: 456 is the first past a key; the
/// brief's 10,000 levels of one letter took minutes to make and longer to remove on APFS, where every
/// operation costs the depth, and the walk never goes below the first folder past a key), a file in
/// each of the first 100 and one at the bottom, scanned on a thread with a stack of 128 KiB (a
/// recursion of a frame a level overflows it within a few hundred levels): Ok within ten seconds, the
/// 100 files are found, the one folder past the longest key is told once and nothing below it is
/// walked, and what the scan holds in keys and sentences is megabytes.
#[cfg(unix)]
#[test]
fn a_tree_ten_thousand_levels_deep_is_walked_without_recursion_and_the_refused_names_are_summarised(
) {
    use crate::sync::problems::Code;
    let dir = TempDir::new("azcloud-deep");
    let root = dir.path().to_path_buf();
    let folder = "d".repeat(8);
    chain(&root, &folder, 500, |level, at| {
        if level <= 100 || level == 500 {
            at.create_new(if level == 500 { "bottom" } else { "f" }, 0o644)
                .unwrap();
        }
    });
    let started = std::time::Instant::now();
    let scanned = {
        let root = root.clone();
        std::thread::Builder::new()
            .stack_size(128 << 10)
            .spawn(move || scan(&root, &Rules::none(), &BTreeMap::new(), 0, &[], u64::MAX))
            .unwrap()
            .join()
            .expect("the scan overflowed a stack of 128 KiB: it recurses")
    };
    let took = started.elapsed();
    remove_tree(&root);
    let scan = scanned.expect("the scan of a deep tree is Ok");
    assert!(took < std::time::Duration::from_secs(10), "{took:?}");
    assert_eq!(
        scan.files.len(),
        100,
        "{:?}",
        scan.files.keys().map(String::len).max()
    );
    assert!(scan.files.keys().all(|k| k.len() <= 4_096));
    assert!(
        scan.dirs.iter().all(|k| k.len() <= 4_096),
        "no folder past a key is kept"
    );
    assert_eq!(scan.dirs.len(), 455, "the folders a key can be");
    let past_a_key = vec![folder.as_str(); 456].join("/");
    assert_eq!(past_a_key.len(), 4_103);
    // A path of 260 characters is told to be too long for Windows: a note on a file, not a refusal.
    assert!(
        scan.problems
            .iter()
            .all(|p| matches!(p.code, Code::BadName | Code::Portability)),
        "{:?}",
        scan.problems
            .iter()
            .map(|p| p.code)
            .collect::<BTreeSet<_>>()
    );
    let told: Vec<_> = scan
        .problems
        .iter()
        .filter(|p| p.code == Code::BadName)
        .collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].key, past_a_key);
    assert!(
        !scan.files.keys().any(|k| k.ends_with("bottom")),
        "nothing below the folder past a key was walked"
    );
    assert!(
        held_bytes(&scan) < 16 << 20,
        "{} bytes of keys and sentences",
        held_bytes(&scan)
    );
}

/// X-DELTA-EDGE-3 (E15a, T10, the width half). A folder whose files have names no key can be - a path
/// past 4,096 bytes - was told with one `skipped` line and one problem for each: 5,000 of them were
/// 10,000 lines of 4 KB in the scan (the brief's 20,000 are 170 MB), in the report and in
/// `problems.json`. A chain of 40 folders of a hundred letters (a key of 4,039 bytes) with 5,000
/// empty files of a hundred letters in the last: every file is refused, the first thousand are listed
/// and one row says there are 4,000 more, the scan holds megabytes, and the folders above are all
/// found.
#[cfg(unix)]
#[test]
fn five_thousand_names_that_cannot_be_keys_are_listed_a_thousand_and_summarised() {
    use crate::sync::problems::{Code, LISTED_PER_CODE};
    let dir = TempDir::new("azcloud-wide-refused");
    let root = dir.path().to_path_buf();
    let folder = "d".repeat(100);
    chain(&root, &folder, 40, |level, at| {
        if level == 40 {
            for i in 0..5_000 {
                at.create_new(&format!("{i:05}{}", "n".repeat(95)), 0o644)
                    .unwrap();
            }
        }
    });
    let scanned = scan(&root, &Rules::none(), &BTreeMap::new(), 0, &[], u64::MAX);
    remove_tree(&root);
    let scan = scanned.unwrap();
    assert!(
        scan.files.is_empty(),
        "every name is past a key: {:?}",
        scan.files.keys().map(String::len).max()
    );
    assert_eq!(scan.dirs.len(), 40, "the folders are all found");
    let listed = scan
        .problems
        .iter()
        .filter(|p| p.code == Code::BadName && p.more == 0)
        .count();
    assert_eq!(listed, LISTED_PER_CODE);
    let summary: Vec<_> = scan.problems.iter().filter(|p| p.more > 0).collect();
    assert_eq!(summary.len(), 1, "{summary:?}");
    assert_eq!((summary[0].code, summary[0].more), (Code::BadName, 4_000));
    assert_eq!(scan.problems.len(), LISTED_PER_CODE + 1);
    assert_eq!(
        scan.skipped.len(),
        LISTED_PER_CODE,
        "the sentences are capped with the problems"
    );
    // The problem that is listed first is a name, the one that stands for the rest names none.
    assert!(scan
        .problems
        .iter()
        .filter(|p| p.more == 0)
        .all(|p| p.key.len() > 4_096));
    assert!(
        held_bytes(&scan) < 16 << 20,
        "{} bytes of keys and sentences",
        held_bytes(&scan)
    );
}
