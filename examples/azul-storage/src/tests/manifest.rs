//! The data tree's manifest (`<root>/.azlin/cache`): the `LocalDrive` keeps
//! it on every put / delete / rename / copy, `.azlin/` is never content, and
//! `diff` answers what changed since the manifest - the later S3 / database
//! sync only diffs.

use std::time::{Duration, SystemTime};

use super::TempDir;
use crate::{
    manifest::{changes, diff, is_reserved_key, Changes, Manifest, ManifestEntry, Record},
    sigv4::sha256_hex,
    Drive, DriveError, ListRequest, LocalDrive,
};

fn keys(m: &Manifest) -> Vec<&str> {
    m.iter().map(|(k, _)| k).collect()
}

fn entry<'a>(m: &'a Manifest, key: &str) -> &'a ManifestEntry {
    m.get(key)
        .unwrap_or_else(|| panic!("{key} is in the manifest: {:?}", keys(m)))
}

/// Moves a file's modification time an hour back, as an edit in another
/// program on another day would leave it.
fn age(path: &std::path::Path) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(3600))
        .unwrap();
}

#[test]
fn a_put_records_the_key_its_size_time_and_content_hash() {
    let tmp = TempDir::new("manifest-put");
    let drive = LocalDrive::new(tmp.path());
    drive.put("notes/inbox/a.md", b"hello").unwrap();

    let manifest = drive.manifest().unwrap();
    let e = entry(&manifest, "notes/inbox/a.md");
    assert_eq!(e.size, 5);
    assert_eq!(
        e.hash,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        "the SHA-256 of the bytes, lowercase hex (the hash SigV4 sends for a PUT)"
    );
    assert_eq!(e.hash, sha256_hex(b"hello"));
    assert_eq!(e.modified, drive.head("notes/inbox/a.md").unwrap().modified);

    // A second put replaces the entry.
    drive.put("notes/inbox/a.md", b"hello, world").unwrap();
    let manifest = drive.manifest().unwrap();
    assert_eq!(entry(&manifest, "notes/inbox/a.md").size, 12);
    assert_eq!(
        entry(&manifest, "notes/inbox/a.md").hash,
        sha256_hex(b"hello, world")
    );
    assert_eq!(manifest.len(), 1);
}

#[test]
fn the_manifest_lives_in_dot_azlin_cache_and_is_never_user_content() {
    let tmp = TempDir::new("manifest-hidden");
    let drive = LocalDrive::new(tmp.path());
    drive.put("readme.txt", b"x").unwrap();
    drive.put("mail/inbox/1.eml", b"y").unwrap();

    assert!(tmp.path().join(".azlin").join("cache").is_file());
    assert_eq!(drive.manifest_path(), tmp.path().join(".azlin").join("cache"));

    let root = drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(root.folders, vec!["mail/".to_string()], "no .azlin/ folder");
    let all = drive.list(&ListRequest::recursive("")).unwrap();
    let listed: Vec<&str> = all.objects.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(listed, vec!["mail/inbox/1.eml", "readme.txt"]);
    let inside = drive.list(&ListRequest::folder(".azlin/")).unwrap();
    assert!(inside.objects.is_empty() && inside.folders.is_empty());

    let invalid = |r: Result<(), DriveError>| matches!(r, Err(DriveError::InvalidKey { .. }));
    assert!(invalid(drive.get(".azlin/cache").map(|_| ())));
    assert!(invalid(drive.head(".azlin/cache").map(|_| ())));
    assert!(invalid(drive.put(".azlin/cache", b"overwritten")));
    assert!(invalid(drive.delete(".azlin/cache")));
    assert!(invalid(drive.delete_folder(".azlin/")));
    assert!(invalid(drive.rename("readme.txt", ".azlin/readme.txt")));
    assert!(invalid(drive.copy("readme.txt", ".azlin/x")));
    assert!(tmp.path().join(".azlin").join("cache").is_file());
    assert!(drive.local_path(".azlin/cache").is_none());

    assert!(is_reserved_key(".azlin"));
    assert!(is_reserved_key(".azlin/"));
    assert!(is_reserved_key(".azlin/cache"));
    assert!(!is_reserved_key(".azlinx/cache"));
    assert!(!is_reserved_key("notes/.azlin/cache"), "only at the root");
}

#[test]
fn delete_rename_copy_and_folder_operations_keep_the_manifest_in_step() {
    let tmp = TempDir::new("manifest-ops");
    let drive = LocalDrive::new(tmp.path());
    drive.put("docs/a.txt", b"aaa").unwrap();
    drive.put("docs/b.txt", b"bb").unwrap();
    drive.put("docs/sub/c.txt", b"c").unwrap();
    drive.put("old/x.txt", b"xx").unwrap();
    drive.put("old/deep/y.txt", b"yyy").unwrap();

    drive.copy("docs/a.txt", "docs/a copy.txt").unwrap();
    drive.rename("docs/b.txt", "docs/b2.txt").unwrap();
    drive.delete("docs/a.txt").unwrap();
    drive.rename("old/", "new/").unwrap();
    drive.delete_folder("docs/sub/").unwrap();

    let m = drive.manifest().unwrap();
    assert_eq!(
        keys(&m),
        vec![
            "docs/a copy.txt",
            "docs/b2.txt",
            "new/deep/y.txt",
            "new/x.txt"
        ]
    );
    assert_eq!(entry(&m, "docs/a copy.txt").hash, sha256_hex(b"aaa"));
    assert_eq!(entry(&m, "docs/b2.txt").hash, sha256_hex(b"bb"));
    assert_eq!(entry(&m, "new/deep/y.txt").size, 3);
    assert_eq!(entry(&m, "new/x.txt").hash, sha256_hex(b"xx"));

    // Every one of those went through the drive: nothing changed since.
    assert!(diff(&m, &drive).unwrap().is_empty());
}

#[test]
fn diff_reports_what_changed_outside_the_drive() {
    let tmp = TempDir::new("manifest-diff");
    let drive = LocalDrive::new(tmp.path());
    drive.put("keep.txt", b"same").unwrap();
    drive.put("grow.txt", b"short").unwrap();
    drive.put("edit.txt", b"abcd").unwrap();
    drive.put("touch.txt", b"same bytes").unwrap();
    drive.put("gone.txt", b"bye").unwrap();
    let manifest = drive.manifest().unwrap();

    // Another program (Finder, an editor, a sync client) changes the tree.
    let file = |name: &str| tmp.path().join(name);
    std::fs::write(file("grow.txt"), b"much longer now").unwrap();
    std::fs::write(file("edit.txt"), b"wxyz").unwrap(); // same size, new bytes
    age(&file("edit.txt"));
    std::fs::write(file("touch.txt"), b"same bytes").unwrap(); // rewritten, unchanged
    age(&file("touch.txt"));
    std::fs::remove_file(file("gone.txt")).unwrap();
    std::fs::create_dir_all(file("new")).unwrap();
    std::fs::write(file("new").join("born.txt"), b"hi").unwrap();

    let found = diff(&manifest, &drive).unwrap();
    assert_eq!(
        found,
        Changes {
            added: vec!["new/born.txt".to_string()],
            modified: vec!["edit.txt".to_string(), "grow.txt".to_string()],
            deleted: vec!["gone.txt".to_string()],
        },
        "a file rewritten with the same bytes is not a change"
    );
    assert!(!found.is_empty());

    // Any drive will do: the sync diffs against the bucket the same way.
    let as_dyn: &dyn Drive = &drive;
    assert_eq!(diff(&manifest, as_dyn).unwrap(), found);
}

#[test]
fn refresh_records_what_changed_outside_so_the_next_diff_is_empty() {
    let tmp = TempDir::new("manifest-refresh");
    let drive = LocalDrive::new(tmp.path());
    drive.put("a.txt", b"one").unwrap();
    drive.put("b.txt", b"two").unwrap();
    std::fs::write(tmp.path().join("a.txt"), b"one, edited").unwrap();
    std::fs::remove_file(tmp.path().join("b.txt")).unwrap();
    std::fs::write(tmp.path().join("c.txt"), b"three").unwrap();

    let found = drive.refresh_manifest().unwrap();
    assert_eq!(found.added, vec!["c.txt".to_string()]);
    assert_eq!(found.modified, vec!["a.txt".to_string()]);
    assert_eq!(found.deleted, vec!["b.txt".to_string()]);

    let m = drive.manifest().unwrap();
    assert_eq!(keys(&m), vec!["a.txt", "c.txt"]);
    assert_eq!(entry(&m, "a.txt").hash, sha256_hex(b"one, edited"));
    assert_eq!(entry(&m, "c.txt").hash, sha256_hex(b"three"));
    assert!(diff(&m, &drive).unwrap().is_empty());
}

#[test]
fn a_tree_written_before_the_manifest_existed_diffs_as_all_added() {
    let tmp = TempDir::new("manifest-legacy");
    std::fs::create_dir_all(tmp.path().join("show")).unwrap();
    std::fs::write(tmp.path().join("show").join("deck.json"), b"{}").unwrap();
    let drive = LocalDrive::new(tmp.path());
    let m = drive.manifest().unwrap();
    assert!(m.is_empty(), "no manifest yet reads as an empty one");
    assert_eq!(
        diff(&m, &drive).unwrap().added,
        vec!["show/deck.json".to_string()]
    );
}

#[test]
fn manifest_to_manifest_changes_compare_size_and_hash_without_io() {
    let e = |size: u64, hash: &str, modified: u64| ManifestEntry {
        size,
        modified: Some(modified),
        hash: hash.to_string(),
    };
    let mut synced = Manifest::default();
    synced.insert("a", e(1, "aa", 10));
    synced.insert("b", e(2, "bb", 10));
    synced.insert("c", e(3, "cc", 10));
    let mut now = synced.clone();
    now.insert("a", e(1, "aa", 99)); // touched: same bytes
    now.insert("b", e(2, "b2", 10)); // new bytes
    now.remove("c");
    now.insert("d", e(4, "dd", 10));

    assert_eq!(
        changes(&synced, &now),
        Changes {
            added: vec!["d".to_string()],
            modified: vec!["b".to_string()],
            deleted: vec!["c".to_string()],
        }
    );
    assert!(changes(&now, &now).is_empty());
}

#[test]
fn the_record_format_round_trips_keys_with_spaces_percent_signs_and_unicode() {
    let key = "notes/My Notes/50% \u{fc}ber.md";
    let put = Record::Put {
        key: key.to_string(),
        entry: ManifestEntry {
            size: 42,
            modified: Some(1_700_000_000),
            hash: sha256_hex(b"x"),
        },
    };
    let line = put.line();
    assert!(line.starts_with("+ 42 1700000000 "), "{line}");
    assert_eq!(line.split(' ').count(), 5, "the key's spaces are escaped: {line}");
    assert!(!line.contains('\n'));
    assert_eq!(Record::parse_line(&line), Some(put));

    let unknown_time = Record::Put {
        key: "a".to_string(),
        entry: ManifestEntry {
            size: 0,
            modified: None,
            hash: sha256_hex(b""),
        },
    };
    assert_eq!(Record::parse_line(&unknown_time.line()), Some(unknown_time));

    for record in [
        Record::Delete {
            key: key.to_string(),
        },
        Record::Delete {
            key: "notes/My Notes/".to_string(),
        },
        Record::Rename {
            from: key.to_string(),
            to: "notes/b c.md".to_string(),
        },
    ] {
        assert_eq!(Record::parse_line(&record.line()), Some(record.clone()));
    }
    assert_eq!(Record::parse_line("? something new"), None);
    assert_eq!(Record::parse_line("+ not-a-number 1 abc key"), None);
}

#[test]
fn a_torn_last_line_and_unknown_records_are_skipped_when_reading() {
    let hash = sha256_hex(b"x");
    let text = format!(
        "azlin-cache 1 0000000000000000\n\
         + 1 5 {hash} a.txt\n\
         + 1 5 {hash} b.txt\n\
         ? a record of a later version\n\
         - b.txt\n\
         > a.txt c.txt\n\
         + 1 5 {hash} torn.t"
    );
    let m = Manifest::parse(&text);
    assert_eq!(keys(&m), vec!["c.txt"], "the last line has no newline: a crash cut it");
}

#[test]
fn a_snapshot_reads_back_as_the_same_manifest() {
    let mut m = Manifest::default();
    for i in 0..20u64 {
        m.insert(
            &format!("dir {i}/f\u{e9}{i}.txt"),
            ManifestEntry {
                size: i,
                modified: if i % 2 == 0 { Some(i * 1000) } else { None },
                hash: sha256_hex(&i.to_le_bytes()),
            },
        );
    }
    let text = m.to_text();
    assert!(text.starts_with("azlin-cache 1 "), "{text}");
    assert!(text.ends_with('\n'));
    assert_eq!(Manifest::parse(&text), m);
}

#[test]
fn a_long_log_is_compacted_to_one_line_per_object() {
    let tmp = TempDir::new("manifest-compact");
    let drive = LocalDrive::new(tmp.path());
    for i in 0..400u32 {
        drive
            .put("hot/autosave.json", format!("{{\"rev\":{i}}}").as_bytes())
            .unwrap();
    }
    drive.put("cold.txt", b"c").unwrap();
    let text = std::fs::read_to_string(drive.manifest_path()).unwrap();
    let lines = text.lines().count();
    assert!(
        lines < 200,
        "400 saves of one file must not leave 400 lines behind ({lines} lines)"
    );
    let m = drive.manifest().unwrap();
    assert_eq!(keys(&m), vec!["cold.txt", "hot/autosave.json"]);
    assert_eq!(
        entry(&m, "hot/autosave.json").hash,
        sha256_hex(b"{\"rev\":399}")
    );
}

#[test]
fn a_drive_without_a_manifest_writes_no_azlin_folder() {
    let tmp = TempDir::new("manifest-none");
    let drive = LocalDrive::without_manifest(tmp.path());
    drive.put("Downloads/report.pdf", b"%PDF").unwrap();
    drive.copy("Downloads/report.pdf", "Downloads/copy.pdf").unwrap();
    drive.delete("Downloads/copy.pdf").unwrap();
    assert!(!tmp.path().join(".azlin").exists());
    assert!(drive.manifest().unwrap().is_empty());
}

#[test]
fn a_manifest_that_cannot_be_written_never_fails_the_write() {
    let tmp = TempDir::new("manifest-blocked");
    // A FILE where the manifest's folder should be: the manifest cannot be
    // kept, the data write still succeeds.
    std::fs::write(tmp.path().join(".azlin"), b"in the way").unwrap();
    let drive = LocalDrive::new(tmp.path());
    drive.put("a.txt", b"data").unwrap();
    assert_eq!(drive.get("a.txt").unwrap(), b"data");
    drive.delete("a.txt").unwrap();
}
