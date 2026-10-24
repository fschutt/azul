//! The sync's writes into a synced folder outside a pass - a file put back from the trash, a
//! step a crash cut off finished, a move into the folder - reach every name from the folder's
//! root, never through a link below it. (Links need Unix to be made here.)

use std::fs;

use azul_storage::testing::TempDir;

use crate::sync::{
    journal::{Intent, Journal},
    local::hash_bytes,
    movein::move_in,
    oplog::{Op, OpKind, OpLog},
    trash::untrash,
};

fn is_empty(dir: &std::path::Path) -> bool {
    fs::read_dir(dir).unwrap().next().is_none()
}

#[test]
fn a_file_put_back_from_the_trash_never_goes_through_a_link() {
    let root = TempDir::new("azcloud-writes-untrash");
    let state = TempDir::new("azcloud-writes-untrash-state");
    let outside = TempDir::new("azcloud-writes-untrash-outside");
    let trashed = ".azlin/sync/trash/20261024-110000-bbbbbb/photos/a.jpg";
    fs::create_dir_all(
        root.path()
            .join(".azlin/sync/trash/20261024-110000-bbbbbb/photos"),
    )
    .unwrap();
    fs::write(root.path().join(trashed), b"a").unwrap();
    OpLog::new(state.path())
        .record(&Op {
            t: 1.0,
            op: OpKind::Trash,
            key: String::from("photos/a.jpg"),
            to: Some(trashed.to_string()),
            hash: None,
        })
        .unwrap();
    // The folder the file came from is a link to a folder of the user's now.
    std::os::unix::fs::symlink(outside.path(), root.path().join("photos")).unwrap();
    assert!(untrash(root.path(), state.path(), "photos").is_err());
    assert!(is_empty(outside.path()), "nothing landed through the link");
    assert_eq!(
        fs::read(root.path().join(trashed)).unwrap(),
        b"a",
        "it stays in the trash"
    );
}

#[test]
fn a_move_into_the_folder_never_lands_through_a_link() {
    let root = TempDir::new("azcloud-writes-movein");
    let state = TempDir::new("azcloud-writes-movein-state");
    let outside = TempDir::new("azcloud-writes-movein-outside");
    let source = TempDir::new("azcloud-writes-movein-source");
    fs::write(source.path().join("report.pdf"), b"report").unwrap();
    fs::create_dir_all(source.path().join("docs")).unwrap();
    fs::write(source.path().join("docs").join("a.txt"), b"a").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("inbox")).unwrap();
    let file = source.path().join("report.pdf");
    assert!(move_in(root.path(), state.path(), &file, "inbox/report.pdf").is_err());
    let folder = source.path().join("docs");
    assert!(move_in(root.path(), state.path(), &folder, "inbox/docs").is_err());
    assert!(is_empty(outside.path()), "nothing landed through the link");
    assert_eq!(fs::read(&file).unwrap(), b"report", "the source stays");
    assert_eq!(fs::read(folder.join("a.txt")).unwrap(), b"a");
}

#[test]
fn a_file_moved_in_keeps_its_executable_bit() {
    use std::os::unix::fs::PermissionsExt;

    let root = TempDir::new("azcloud-writes-movein-exec");
    let state = TempDir::new("azcloud-writes-movein-exec-state");
    let source = TempDir::new("azcloud-writes-movein-exec-source");
    let script = source.path().join("run.sh");
    fs::write(&script, b"#!/bin/sh\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let moved = move_in(root.path(), state.path(), &script, "bin/run.sh").unwrap();
    assert_eq!(moved, ["bin/run.sh"]);
    let mode = fs::metadata(root.path().join("bin").join("run.sh"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o100, 0o100, "{mode:o}");
}

#[test]
fn recovery_finishes_no_conflict_or_case_rename_through_a_link() {
    let root = TempDir::new("azcloud-writes-journal");
    let state = TempDir::new("azcloud-writes-journal-state");
    let outside = TempDir::new("azcloud-writes-journal-outside");
    fs::write(outside.path().join("k.txt"), b"mine").unwrap();
    fs::write(outside.path().join(".azsync-case-1"), b"report").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("docs")).unwrap();
    let journal = Journal::new(state.path());
    journal
        .begin(Intent::Conflict {
            key: String::from("docs/k.txt"),
            copy: String::from("docs/k (conflict laptop 2026-10-24).txt"),
            hash: hash_bytes(b"mine"),
        })
        .unwrap();
    journal
        .begin(Intent::CaseRename {
            from: String::from("docs/Report.txt"),
            via: String::from("docs/.azsync-case-1"),
            to: String::from("docs/report.txt"),
        })
        .unwrap();
    let recovery = journal.recover(root.path()).unwrap();
    let mut names: Vec<String> = fs::read_dir(outside.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, [".azsync-case-1", "k.txt"], "{recovery:?}");
    assert!(recovery.finished.is_empty(), "{recovery:?}");
}
