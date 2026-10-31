//! The shipped sync made safe: nothing written through a link, a missing or replaced folder
//! never read as "every file deleted", deletes into the trash, small folders behind the breaker,
//! downloads renamed in whole with their permission bits capped, the disk's reserve kept, an
//! app's save never taken for a delete, a file still being written left for later, the drive's
//! bad entries listed and never acted on, the folder's bookkeeping unreachable from the drive.

use std::{fs, path::Path, time::Duration};

use azul_storage::testing::TempDir;

use super::{
    fake_s3::S3Bucket,
    sync::{index_of, Device, PREFIX},
};
use crate::sync::{
    local::hash_bytes,
    problems::Code,
    remote::{blob_key, index_key},
    root, sync_to, RemoteFile, RemoteIndex, RunHooks, SyncReport, Target,
};

/// Whether the run listed `code` (for `key`, when given).
fn lists(report: &SyncReport, code: Code, key: Option<&str>) -> bool {
    report
        .problems
        .iter()
        .any(|p| p.code == code && key.is_none_or(|k| p.key == k))
}

/// Puts a file straight into the drive's index, as another device (or a hostile one) would.
fn put_remote(store: &S3Bucket, key: &str, bytes: &[u8], exec: bool) {
    let index_key = index_key(PREFIX);
    let mut index = store
        .read(&index_key)
        .map_or_else(RemoteIndex::empty, |b| RemoteIndex::parse(&b).unwrap());
    let hash = hash_bytes(bytes);
    store.write(&blob_key(PREFIX, &hash), bytes.to_vec());
    index.generation += 1;
    index.files.insert(
        key.to_string(),
        RemoteFile {
            hash,
            size: bytes.len() as u64,
            mtime: 1_700_000_000,
            mtime_ns: 0,
            gen: index.generation,
            device: String::from("someone"),
            exec,
            link: None,
        },
    );
    store.write(&index_key, index.to_bytes());
}

/// The files of the folder's trash, by content.
fn trashed(folder: &Path) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut stack = vec![folder.join(".azlin").join("sync").join("trash")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                out.push(fs::read(entry.path()).unwrap());
            }
        }
    }
    out
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[cfg(unix)]
#[test]
fn a_file_from_the_drive_under_a_linked_folder_here_is_never_written_through_the_link() {
    let store = S3Bucket::new();
    let a = Device::new("link-a");
    let b = Device::new("link-b");
    let outside = TempDir::new("azcloud-link-outside");
    a.write("inbox/readme.txt", b"a's inbox");
    a.sync(&store);
    // b's inbox is a link to a folder outside (as ~/Library/LaunchAgents would be).
    std::os::unix::fs::symlink(outside.path(), b.path().join("inbox")).unwrap();
    b.sync(&store);
    a.write("inbox/evil.plist", b"<plist>run me at login</plist>");
    a.sync(&store);
    let report = b.sync(&store);
    assert!(
        names(outside.path()).is_empty(),
        "{:?}",
        names(outside.path())
    );
    assert!(
        lists(&report, Code::ThroughLink, Some("inbox/evil.plist")),
        "{:?}",
        report.problems
    );
    assert!(fs::symlink_metadata(b.path().join("inbox"))
        .unwrap()
        .file_type()
        .is_symlink());
    // The link hides what the drive has under it: nothing there is a delete.
    assert!(index_of(&store).files.contains_key("inbox/readme.txt"));
}

#[cfg(unix)]
#[test]
fn a_synced_folder_replaced_by_a_link_keeps_its_files_on_the_drive() {
    let store = S3Bucket::new();
    let a = Device::new("relink-a");
    let elsewhere = TempDir::new("azcloud-relink-elsewhere");
    a.write("docs/a.txt", b"a");
    a.write("docs/b.txt", b"b");
    a.write("other.txt", b"other");
    a.sync(&store);
    fs::rename(a.path().join("docs"), elsewhere.path().join("docs-old")).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), a.path().join("docs")).unwrap();
    let report = a.sync(&store);
    assert_eq!(report.deleted_there, 0, "{}", report.summary());
    let index = index_of(&store);
    assert!(index.files.contains_key("docs/a.txt") && index.files.contains_key("docs/b.txt"));
}

#[test]
fn a_folder_that_is_gone_changes_nothing_anywhere() {
    let store = S3Bucket::new();
    let a = Device::new("gone-a");
    let b = Device::new("gone-b");
    for (key, text) in [
        ("vm/disk.img", "the only copy"),
        ("vm/notes.txt", "notes"),
        ("vm/keys.txt", "keys"),
    ] {
        a.write(key, text.as_bytes());
    }
    a.sync(&store);
    b.sync(&store);
    fs::remove_dir_all(a.path()).unwrap();
    let report = a.sync(&store);
    assert!(report.refused.is_some(), "{}", report.summary());
    assert!(
        lists(&report, Code::RootMissing, None),
        "{:?}",
        report.problems
    );
    assert!(!a.path().exists(), "a missing folder is never made again");
    assert_eq!(index_of(&store).files.len(), 3);
    b.sync(&store);
    assert_eq!(b.tree().len(), 3);
    assert!(trashed(b.path()).is_empty());
}

/// A folder this app may no longer read (its permission taken away, macOS privacy) is said
/// as unreadable, not as another folder, and nothing changes on either side.
#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_read_is_said_so_and_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let store = S3Bucket::new();
    let a = Device::new("closed-a");
    let b = Device::new("closed-b");
    for key in ["one.txt", "two.txt", "three.txt"] {
        a.write(key, key.as_bytes());
    }
    a.sync(&store);
    b.sync(&store);
    fs::set_permissions(a.path(), fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(a.path()).is_ok() {
        // Root reads every folder: nothing to play here.
        fs::set_permissions(a.path(), fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let report = a.sync(&store);
    fs::set_permissions(a.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(report.refused.is_some(), "{}", report.summary());
    assert!(lists(&report, Code::Unreadable, None), "{:?}", report.problems);
    assert_eq!(index_of(&store).files.len(), 3, "nothing deleted on the drive");
    b.sync(&store);
    assert_eq!(b.tree().len(), 3);
    assert!(trashed(b.path()).is_empty());
    let again = a.sync(&store);
    assert!(again.refused.is_none(), "{}", again.summary());
    assert_eq!(a.tree().len(), 3);
}

#[test]
fn a_folder_without_its_marker_is_not_the_paired_one_and_changes_nothing() {
    // An unplugged disk leaves its mount point: empty, or holding a stale copy written to the
    // internal disk meanwhile. Either way it lacks the marker the pairing wrote.
    let store = S3Bucket::new();
    let a = Device::new("mount-a");
    for i in 0..12 {
        a.write(&format!("docs/f{i}.txt"), format!("doc {i}").as_bytes());
    }
    a.sync(&store);
    let marker = root::read_marker(a.path()).expect("the pairing wrote its marker");
    assert_eq!(marker.id.len(), 32);
    for i in 0..12 {
        a.remove(&format!("docs/f{i}.txt"));
    }
    fs::remove_dir_all(a.path().join(".azlin")).unwrap();
    a.write("docs/f0.txt", b"stale shadow copy");
    let report = a.sync(&store);
    assert!(
        lists(&report, Code::RootReplaced, None),
        "{:?}",
        report.problems
    );
    let index = index_of(&store);
    assert_eq!(index.files.len(), 12, "nothing deleted on the drive");
    let f0 = &index.files["docs/f0.txt"];
    assert_eq!(
        f0.hash,
        hash_bytes(b"doc 0"),
        "the stale copy never went up"
    );
}

#[test]
fn deleting_every_file_of_a_small_folder_waits_for_the_user() {
    let store = S3Bucket::new();
    let a = Device::new("small-a");
    let b = Device::new("small-b");
    for name in ["2025.pdf", "2024.pdf", "2023.pdf", "receipts.zip"] {
        a.write(&format!("tax/{name}"), name.as_bytes());
    }
    a.sync(&store);
    b.sync(&store);
    a.remove_folder("tax");
    let hooks = RunHooks {
        hold_mass_delete: true,
        ..RunHooks::default()
    };
    let report = sync_to(
        Target::Index(&store),
        &a.root(),
        &a.index(),
        &a.opts(),
        &hooks,
    )
    .unwrap();
    assert!(report.mass_delete.is_some(), "{}", report.summary());
    assert!(lists(&report, Code::MassDeleteHeld, None));
    assert_eq!(
        index_of(&store).files.len(),
        4,
        "nothing deleted on the drive"
    );
    // The command line's run stops with the question.
    assert!(a.sync_with(&store, &a.opts()).is_err());
    b.sync(&store);
    assert_eq!(b.tree().len(), 4);
}

#[test]
fn deleting_one_subfolder_of_many_goes_through_into_the_other_devices_trash() {
    let store = S3Bucket::new();
    let a = Device::new("sub-a");
    let b = Device::new("sub-b");
    for folder in ["alpha", "beta", "gamma"] {
        for i in 0..5 {
            a.write(
                &format!("work/{folder}/{i}.txt"),
                format!("{folder} {i}").as_bytes(),
            );
        }
    }
    a.sync(&store);
    b.sync(&store);
    a.remove_folder("work/beta");
    let sent = a.sync(&store);
    assert_eq!(sent.deleted_there, 5, "{}", sent.summary());
    let got = b.sync(&store);
    assert_eq!(got.deleted_here, 5, "{}", got.summary());
    assert_eq!(got.trashed.len(), 5);
    assert_eq!(b.tree().len(), 10);
    let trash = trashed(b.path());
    assert!(trash.contains(&b"beta 0".to_vec()) && trash.contains(&b"beta 4".to_vec()));
}

#[test]
fn a_delete_from_the_drive_moves_the_file_into_the_trash_here() {
    let store = S3Bucket::new();
    let a = Device::new("trash-a");
    let b = Device::new("trash-b");
    a.write("contract.pdf", b"signed contract");
    a.write("other.txt", b"other");
    a.sync(&store);
    b.sync(&store);
    a.remove("contract.pdf");
    a.sync(&store);
    let report = b.sync(&store);
    assert_eq!(report.deleted_here, 1, "{}", report.summary());
    assert!(b.read("contract.pdf").is_none());
    assert_eq!(trashed(b.path()), vec![b"signed contract".to_vec()]);
    let (key, to) = &report.trashed[0];
    assert_eq!(key, "contract.pdf");
    assert!(
        to.starts_with(".azlin/sync/trash/") && to.ends_with("/contract.pdf"),
        "{to}"
    );
    // The trash is the folder's bookkeeping: never synced.
    assert!(!index_of(&store).files.keys().any(|k| k.contains("trash")));
}

#[test]
fn a_download_is_renamed_in_whole_and_leaves_no_temporary_file() {
    let store = S3Bucket::new();
    let a = Device::new("whole-a");
    let b = Device::new("whole-b");
    a.write("big.bin", &vec![3u8; 9 * 1024 * 1024]);
    a.write("small.txt", b"v1");
    a.sync(&store);
    b.sync(&store);
    a.write("small.txt", b"v2");
    a.write("big.bin", &vec![4u8; 9 * 1024 * 1024]);
    a.sync(&store);
    b.sync(&store);
    assert_eq!(b.read("small.txt").as_deref(), Some(&b"v2"[..]));
    assert_eq!(b.read("big.bin").unwrap()[0], 4);
    assert_eq!(names(b.path()), vec![".azlin", "big.bin", "small.txt"]);
    let spool = b.path().join(".azlin").join("sync").join("tmp");
    assert!(
        names(&spool).is_empty(),
        "the spool is empty: {:?}",
        names(&spool)
    );
}

#[cfg(unix)]
#[test]
fn a_download_takes_the_executable_bit_and_never_any_other_permission_from_the_drive() {
    use std::os::unix::fs::PermissionsExt;
    let store = S3Bucket::new();
    let b = Device::new("mode-b");
    put_remote(&store, "tools/helper", b"#!/bin/sh\nid\n", true);
    put_remote(&store, "notes.txt", b"plain", false);
    b.sync(&store);
    let mode = |key: &str| {
        fs::metadata(b.path().join(key))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777
    };
    assert_eq!(mode("tools/helper"), 0o755);
    assert_eq!(mode("notes.txt"), 0o644);
}

#[cfg(unix)]
#[test]
fn an_executable_file_here_syncs_its_bit_as_part_of_the_file() {
    use std::os::unix::fs::PermissionsExt;
    let store = S3Bucket::new();
    let a = Device::new("exec-a");
    let b = Device::new("exec-b");
    a.write("run.sh", b"#!/bin/sh\necho hi\n");
    a.sync(&store);
    b.sync(&store);
    fs::set_permissions(a.path().join("run.sh"), fs::Permissions::from_mode(0o4755)).unwrap();
    let sent = a.sync(&store);
    assert_eq!(sent.files_up, 1, "{}", sent.summary());
    assert!(index_of(&store).files["run.sh"].exec);
    b.sync(&store);
    let mode = fs::metadata(b.path().join("run.sh"))
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(mode, 0o755, "the setuid bit never travels");
}

#[test]
fn a_download_that_would_eat_the_disks_reserve_is_not_started() {
    let store = S3Bucket::new();
    let a = Device::new("reserve-a");
    let b = Device::new("reserve-b");
    a.write("big.bin", &vec![1u8; 300_000]);
    a.write("small.txt", b"v1");
    a.sync(&store);
    let mut opts = b.opts();
    opts.disk_reserve = u64::MAX / 4;
    let report = b.sync_with(&store, &opts).unwrap();
    if azul_storage::beneath::free_space(b.path()).is_some() {
        assert!(
            lists(&report, Code::DiskReserve, Some("big.bin")),
            "{:?}",
            report.problems
        );
        assert!(b.read("big.bin").is_none());
    }
    let report = b.sync(&store);
    assert!(b.read("big.bin").is_some(), "{}", report.summary());
}

#[test]
fn an_app_saving_a_file_is_no_delete() {
    // Word's save: the new version is written aside, the old one renamed to a temporary name,
    // then the new one renamed in. A pass in the middle sees the file gone.
    let store = S3Bucket::new();
    let a = Device::new("save-a");
    let b = Device::new("save-b");
    a.write("Pra\u{308}sentation.docx", b"draft");
    a.sync(&store);
    b.sync(&store);
    a.write("~WRD0001.tmp", b"final");
    fs::rename(
        a.path().join("Pra\u{308}sentation.docx"),
        a.path().join("~WRL0002.tmp"),
    )
    .unwrap();
    let report = a.sync(&store);
    assert!(
        lists(&report, Code::SaveInProgress, None),
        "{:?}",
        report.problems
    );
    assert_eq!(report.deleted_there, 0);
    b.sync(&store);
    assert_eq!(
        b.read("Pra\u{308}sentation.docx").as_deref(),
        Some(&b"draft"[..])
    );
    fs::rename(
        a.path().join("~WRD0001.tmp"),
        a.path().join("Pra\u{308}sentation.docx"),
    )
    .unwrap();
    fs::remove_file(a.path().join("~WRL0002.tmp")).unwrap();
    a.sync(&store);
    b.sync(&store);
    assert_eq!(
        b.read("Pra\u{308}sentation.docx").as_deref(),
        Some(&b"final"[..])
    );
    assert!(trashed(b.path()).is_empty());
    assert_eq!(index_of(&store).files.len(), 1);
}

#[test]
fn a_file_still_being_written_waits_and_is_never_a_delete() {
    let store = S3Bucket::new();
    let a = Device::new("settle-a");
    a.write("export.mov", b"frame 1");
    a.sync(&store);
    a.write("export.mov", b"frame 1, frame 2");
    a.write("fresh.mov", b"just started");
    let mut opts = a.opts();
    opts.file_settle = Duration::from_secs(3600);
    let report = a.sync_with(&store, &opts).unwrap();
    assert!(
        lists(&report, Code::Settling, Some("export.mov")),
        "{:?}",
        report.problems
    );
    assert!(lists(&report, Code::Settling, Some("fresh.mov")));
    let index = index_of(&store);
    assert_eq!(
        index.files["export.mov"].hash,
        hash_bytes(b"frame 1"),
        "not sent half"
    );
    assert!(!index.files.contains_key("fresh.mov"));
    assert_eq!(report.deleted_there, 0);
    let report = a.sync(&store);
    assert_eq!(report.files_up, 2, "{}", report.summary());
}

#[test]
fn a_drive_entry_no_folder_can_hold_is_listed_kept_on_the_drive_and_never_acted_on() {
    let store = S3Bucket::new();
    let b = Device::new("names-b");
    put_remote(&store, "fine.txt", b"fine", false);
    // A hostile writer's entries: traversal, the folder's own bookkeeping, a control character.
    for key in ["../../escape.txt", ".azlin/sync/root-id", "bell\u{7}.txt"] {
        let index_key = index_key(PREFIX);
        let mut index = RemoteIndex::parse(&store.read(&index_key).unwrap()).unwrap();
        let mut entry = index.files["fine.txt"].clone();
        entry.gen += 1;
        index.files.insert(key.to_string(), entry);
        store.write(&index_key, index.to_bytes());
    }
    let report = b.sync(&store);
    assert_eq!(
        b.read("fine.txt").as_deref(),
        Some(&b"fine"[..]),
        "{}",
        report.summary()
    );
    for key in ["../../escape.txt", ".azlin/sync/root-id", "bell\u{7}.txt"] {
        assert!(
            lists(&report, Code::BadRemoteName, Some(key)),
            "{key}: {:?}",
            report.problems
        );
    }
    let marker = root::read_marker(b.path()).expect("the folder's own marker");
    assert_eq!(marker.id.len(), 32, "the marker was never overwritten");
    assert!(b
        .path()
        .parent()
        .map_or(true, |p| !p.join("escape.txt").exists()));
    // b writes the index again: the entries stay as they are.
    b.write("mine.txt", b"mine");
    b.sync(&store);
    let index = index_of(&store);
    assert!(index.files.contains_key("../../escape.txt"));
    assert!(index.files.contains_key("mine.txt"));
}

#[test]
fn a_drive_entry_that_names_no_content_is_never_a_delete_here() {
    let store = S3Bucket::new();
    let a = Device::new("damaged-a");
    let b = Device::new("damaged-b");
    a.write("x.txt", b"x");
    a.write("y.txt", b"y");
    a.sync(&store);
    b.sync(&store);
    let key = index_key(PREFIX);
    let mut index = RemoteIndex::parse(&store.read(&key).unwrap()).unwrap();
    index.files.get_mut("x.txt").unwrap().hash = String::from("not a hash");
    index.generation += 1;
    store.write(&key, index.to_bytes());
    let report = b.sync(&store);
    assert!(
        lists(&report, Code::RemoteEntryUnreadable, Some("x.txt")),
        "{:?}",
        report.problems
    );
    assert_eq!(b.read("x.txt").as_deref(), Some(&b"x"[..]));
    assert!(trashed(b.path()).is_empty());
}

/// An entry the index's own fields cannot be read from is kept aside when the index is read;
/// the next pass that gets "not modified" from the drive works from its cached copy, which
/// must keep it aside too - never a delete here.
#[test]
fn an_unreadable_drive_entry_stays_unknown_in_the_next_pass_too() {
    let store = S3Bucket::new();
    let a = Device::new("unreadable-a");
    let b = Device::new("unreadable-b");
    a.write("x.txt", b"x");
    a.write("y.txt", b"y");
    a.sync(&store);
    b.sync(&store);
    let key = index_key(PREFIX);
    let mut whole: serde_json::Value =
        serde_json::from_slice(&store.read(&key).unwrap()).unwrap();
    whole["files"]["x.txt"] = serde_json::json!({ "hash": "bad tag", "size": -1 });
    whole["generation"] = serde_json::json!(whole["generation"].as_u64().unwrap() + 1);
    store.write(&key, serde_json::to_vec_pretty(&whole).unwrap());
    for pass in 0..3 {
        let report = b.sync(&store);
        assert!(
            lists(&report, Code::RemoteEntryUnreadable, Some("x.txt")),
            "pass {pass}: {:?}",
            report.problems
        );
        assert_eq!(b.read("x.txt").as_deref(), Some(&b"x"[..]), "pass {pass}");
        assert!(trashed(b.path()).is_empty(), "pass {pass}");
    }
}

#[cfg(unix)]
#[test]
fn a_fifo_in_the_folder_is_listed_and_never_opened() {
    let store = S3Bucket::new();
    let a = Device::new("fifo-a");
    a.write("a.txt", b"a");
    let path = std::ffi::CString::new(
        a.path()
            .join("pipe")
            .as_os_str()
            .to_string_lossy()
            .as_bytes(),
    )
    .unwrap();
    // SAFETY: a valid C string.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    // Opening the FIFO to hash it would wait for a writer for ever.
    let report = a.sync(&store);
    assert!(
        lists(&report, Code::SpecialFile, Some("pipe")),
        "{:?}",
        report.problems
    );
    assert!(index_of(&store).files.contains_key("a.txt"));
    assert!(!index_of(&store).files.contains_key("pipe"));
}

#[test]
fn the_volume_is_probed_once_and_the_probe_leaves_nothing_behind() {
    let dir = TempDir::new("azcloud-probe");
    let volume = root::probe_volume(dir.path());
    if cfg!(target_os = "macos") {
        // APFS as macOS formats it ignores case and normalization.
        assert!(
            volume.case_insensitive && volume.norm_insensitive,
            "{volume:?}"
        );
    }
    assert!(names(&dir.path().join(".azlin").join("sync")).is_empty());
}

/// A system without permission bits (Windows) shows every file as not executable: its volume
/// keeps none, so an edit made there sends the bit the version had on the drive, never a
/// cleared one, and a download keeps the drive's bit in the base.
#[cfg(not(unix))]
#[test]
fn a_system_without_permission_bits_keeps_the_drives_executable_bit() {
    let dir = TempDir::new("azcloud-probe-exec");
    assert!(root::probe_volume(dir.path()).no_exec_bits);
}

#[test]
fn the_token_servers_restore_announcement_reaches_the_sync_and_the_account_record() {
    // V3: the token server answers the drive's newest verified restore announcement in every
    // refresh; the sync takes it as its restore notice.
    let bundle = |restore: &str| {
        crate::bundle::DriveBundle::parse(&format!(
            r#"{{"drive": {{"id": "d_42", "name": "Docs",
                  "location": {{"kind": "s3", "endpoint": "http://127.0.0.1:19000",
                                "region": "us-east-1", "bucket": "d-42", "path_style": true,
                                "auth": {{"type": "azlin", "drive_id": "d_42"}}}}}},
                "credentials": {{"access_key_id": "AK", "secret_access_key": "SK"}},
                "drive_token": "dt", "restore": {restore}}}"#
        ))
        .unwrap()
    };
    let announced = bundle(
        r#"{"epoch": 3, "kind": "offsite", "as_of": null, "prefix": "",
            "announced_at": "2026-10-24T10:00:00Z", "generation": 1, "sig": "c2ln"}"#,
    );
    assert_eq!(
        announced.restore,
        Some(crate::sync::RestoreNotice {
            epoch: 3,
            prefix: String::new()
        })
    );
    assert_eq!(bundle("null").restore, None);
    let grant = crate::account::grant_of(&announced, "http://127.0.0.1:18081", "owner", 1).unwrap();
    assert_eq!(grant.record.restore, announced.restore);
}

#[test]
fn a_file_with_a_time_in_the_future_is_no_write_in_progress() {
    // A file from a computer whose clock runs ahead: waiting for it to be quiet would wait for
    // ever.
    let store = S3Bucket::new();
    let a = Device::new("future-a");
    a.write("ahead.txt", b"from a clock that runs ahead");
    let file = fs::File::options()
        .write(true)
        .open(a.path().join("ahead.txt"))
        .unwrap();
    file.set_modified(std::time::SystemTime::now() + Duration::from_secs(86_400))
        .unwrap();
    drop(file);
    let mut opts = a.opts();
    opts.file_settle = Duration::from_secs(3600);
    let report = a.sync_with(&store, &opts).unwrap();
    assert!(
        !lists(&report, Code::Settling, None),
        "{:?}",
        report.problems
    );
    assert!(index_of(&store).files.contains_key("ahead.txt"));
}

#[test]
fn a_data_root_whose_bookkeeping_is_the_azlin_folder_never_syncs_its_marker_or_trash() {
    let data = TempDir::new("azcloud-nested-data");
    let roots = crate::sync::azlin_roots(data.path(), &data.path().join(".azlin"));
    let home = &roots[1].0;
    assert!(home.rules.excluded("sync/root-id").is_some());
    assert!(home
        .rules
        .excluded("sync/trash/20261024-101500-a1b2c3/x.txt")
        .is_some());
}

#[test]
fn a_downloads_time_is_the_writers_to_the_nanosecond() {
    let store = S3Bucket::new();
    let a = Device::new("mtime-a");
    let b = Device::new("mtime-b");
    a.write("photo-2003.jpg", b"an old photo");
    let taken = std::time::UNIX_EPOCH + Duration::from_nanos(1_049_760_000_123_456_789);
    fs::File::options()
        .write(true)
        .open(a.path().join("photo-2003.jpg"))
        .unwrap()
        .set_modified(taken)
        .unwrap();
    a.sync(&store);
    b.sync(&store);
    let got = fs::metadata(b.path().join("photo-2003.jpg"))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(got, taken);
}

#[test]
fn a_folder_renamed_on_one_device_is_renamed_on_the_other_not_trashed_and_fetched_again() {
    let store = S3Bucket::new();
    let a = Device::new("rename-a");
    let b = Device::new("rename-b");
    for i in 0..30 {
        a.write(&format!("old/page{i}.txt"), format!("page {i}").as_bytes());
    }
    a.write("readme.txt", b"stays");
    a.sync(&store);
    b.sync(&store);
    a.rename_folder("old", "new");
    let sent = a.sync(&store);
    assert!(sent.mass_delete.is_none(), "{}", sent.summary());
    let got = b.sync(&store);
    assert!(got.mass_delete.is_none(), "{}", got.summary());
    // One rename of the folder, its 30 files with it.
    assert_eq!(got.moved, vec![String::from("old/ -> new/")], "{}", got.summary());
    assert_eq!(got.files_down, 0, "nothing fetched again");
    assert!(trashed(b.path()).is_empty());
    assert_eq!(b.tree(), a.tree());
}

#[test]
fn a_case_only_rename_travels_as_a_rename() {
    let store = S3Bucket::new();
    let a = Device::new("case-a");
    let b = Device::new("case-b");
    a.write("notes.txt", b"the notes");
    a.sync(&store);
    b.sync(&store);
    let tmp = a.path().join("notes.tmp-rename");
    fs::rename(a.path().join("notes.txt"), &tmp).unwrap();
    fs::rename(&tmp, a.path().join("Notes.txt")).unwrap();
    a.sync(&store);
    let index = index_of(&store);
    assert!(index.files.contains_key("Notes.txt") && !index.files.contains_key("notes.txt"));
    b.sync(&store);
    assert_eq!(names(b.path()), vec![".azlin", "Notes.txt"]);
    assert!(trashed(b.path()).is_empty());
    assert!(!a.sync(&store).index_written, "the next pass is quiet");
}

#[test]
fn a_base_that_cannot_be_read_pairs_afresh_and_deletes_nothing() {
    let store = S3Bucket::new();
    let a = Device::new("lost-a");
    let b = Device::new("lost-b");
    for (key, text) in [("x.txt", "x"), ("y.txt", "y"), ("z.txt", "z")] {
        a.write(key, text.as_bytes());
    }
    a.sync(&store);
    b.sync(&store);
    b.remove("z.txt");
    b.sync(&store);
    fs::write(a.index(), b"{\"generation\": 3, \"files\": {\"half\": ").unwrap();
    let report = a.sync(&store);
    assert!(
        lists(&report, Code::StateReset, None),
        "{:?}",
        report.problems
    );
    assert_eq!(report.deleted_here, 0);
    assert_eq!(a.tree().len(), 3, "z.txt, deleted elsewhere, is kept here");
    assert!(lists(&report, Code::DeletedElsewhereKept, Some("z.txt")));
    assert_eq!(index_of(&store).files.len(), 2, "and not sent again");
}

#[test]
fn a_file_bigger_than_this_device_takes_stays_on_the_drive_and_is_listed() {
    let store = S3Bucket::new();
    let a = Device::new("large-a");
    let b = Device::new("large-b");
    a.write("backup.tar", &vec![9u8; 4096]);
    a.write("small.txt", b"small");
    a.sync(&store);
    let mut opts = b.opts();
    opts.max_file_bytes = 1024;
    let report = b.sync_with(&store, &opts).unwrap();
    assert!(
        lists(&report, Code::TooLarge, Some("backup.tar")),
        "{:?}",
        report.problems
    );
    assert!(b.read("backup.tar").is_none());
    assert_eq!(b.read("small.txt").as_deref(), Some(&b"small"[..]));
    assert!(
        index_of(&store).files.contains_key("backup.tar"),
        "never a delete"
    );
}

#[test]
fn a_file_made_again_on_a_paired_folder_goes_up_and_a_kept_one_waits_for_the_users_choice() {
    let store = S3Bucket::new();
    let a = Device::new("again-a");
    a.write("note.txt", b"the note");
    a.write("other.txt", b"other");
    a.sync(&store);
    a.remove("note.txt");
    a.sync(&store);
    // Restored from the system's trash, the same bytes: the user's, it goes up.
    a.write("note.txt", b"the note");
    let report = a.sync(&store);
    assert!(
        !lists(&report, Code::DeletedElsewhereKept, None),
        "{:?}",
        report.problems
    );
    assert!(index_of(&store).files.contains_key("note.txt"));
    assert!(!index_of(&store).deleted.contains_key("note.txt"));
    // A device paired afresh keeps its copy of a file deleted meanwhile until the user says.
    a.remove("note.txt");
    a.sync(&store);
    let c = Device::new("again-c");
    c.write("note.txt", b"the note");
    c.sync(&store);
    let report = c.sync(&store);
    assert!(lists(&report, Code::DeletedElsewhereKept, Some("note.txt")));
    assert!(
        !index_of(&store).files.contains_key("note.txt"),
        "not sent on the second pass"
    );
}
