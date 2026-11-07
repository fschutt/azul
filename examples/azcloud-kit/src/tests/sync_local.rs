//! This device's side of a synced folder: the scan, the racy files, the shared config without
//! its endpoints, the checks before a download overwrites or a delete removes a file.

use std::{collections::BTreeMap, fs, path::Path};

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
