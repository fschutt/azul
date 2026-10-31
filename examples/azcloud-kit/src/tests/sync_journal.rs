//! The journal: a crash in the middle of a step is finished or rolled back by the next pass.

use std::fs;

use azul_storage::testing::TempDir;

use crate::sync::{
    journal::{is_download_temp, Intent, Journal},
    local::hash_bytes,
};

#[test]
fn a_step_marked_done_is_no_open_step_and_one_cut_off_by_a_crash_is() {
    let state = TempDir::new("journal-state");
    let journal = Journal::new(state.path());
    let a = journal
        .begin(Intent::Download {
            key: String::from("a.txt"),
            hash: hash_bytes(b"a"),
        })
        .unwrap();
    let b = journal
        .begin(Intent::Trash {
            key: String::from("b.txt"),
            to: String::from(".azlin/sync/trash/2026-10-24/b.txt"),
        })
        .unwrap();
    journal.done(a).unwrap();
    // A new process (after the crash) reads the same journal.
    let again = Journal::new(state.path());
    let open = again.open_steps();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].0, b);
    assert_eq!(open[0].1.key(), "b.txt");
    // Its tickets do not reuse an id the journal holds.
    let c = again
        .begin(Intent::Download {
            key: String::from("c.txt"),
            hash: hash_bytes(b"c"),
        })
        .unwrap();
    assert!(c.0 > b.0);
}

#[test]
fn a_half_download_leaves_no_temp_file_after_recovery_and_the_old_version_whole() {
    let root = TempDir::new("journal-root");
    let state = TempDir::new("journal-state");
    fs::create_dir_all(root.path().join("docs")).unwrap();
    fs::write(root.path().join("docs/a.txt"), b"old").unwrap();
    let temp = root.path().join("docs/.a.txt.azul-storage-77-0.tmp");
    fs::write(&temp, b"half of the new").unwrap();
    fs::write(root.path().join("docs/.other.azul-storage-1-0.tmp"), b"not ours").unwrap();
    let journal = Journal::new(state.path());
    journal
        .begin(Intent::Download {
            key: String::from("docs/a.txt"),
            hash: hash_bytes(b"new"),
        })
        .unwrap();
    let recovery = Journal::new(state.path()).recover(root.path()).unwrap();
    assert!(!temp.exists(), "{recovery:?}");
    assert_eq!(fs::read(root.path().join("docs/a.txt")).unwrap(), b"old");
    assert!(
        root.path().join("docs/.other.azul-storage-1-0.tmp").exists(),
        "only the step's own temp files go"
    );
    assert!(Journal::new(state.path()).open_steps().is_empty(), "recovery empties the journal");
}

#[test]
fn a_conflict_cut_off_before_its_rename_is_finished_so_no_second_copy_is_made() {
    let root = TempDir::new("journal-root");
    let state = TempDir::new("journal-state");
    fs::write(root.path().join("n.md"), b"mine").unwrap();
    let journal = Journal::new(state.path());
    journal
        .begin(Intent::Conflict {
            key: String::from("n.md"),
            copy: String::from("n (conflict a 2026-10-24).md"),
            hash: hash_bytes(b"mine"),
        })
        .unwrap();
    let recovery = journal.recover(root.path()).unwrap();
    assert_eq!(recovery.finished.len(), 1, "{recovery:?}");
    assert_eq!(
        fs::read(root.path().join("n (conflict a 2026-10-24).md")).unwrap(),
        b"mine"
    );
    assert!(!root.path().join("n.md").exists(), "the drive's version comes down next pass");
}

#[test]
fn a_conflict_whose_file_changed_since_is_left_for_the_next_pass() {
    let root = TempDir::new("journal-root");
    let state = TempDir::new("journal-state");
    fs::write(root.path().join("n.md"), b"edited again").unwrap();
    let journal = Journal::new(state.path());
    journal
        .begin(Intent::Conflict {
            key: String::from("n.md"),
            copy: String::from("n (conflict a 2026-10-24).md"),
            hash: hash_bytes(b"mine"),
        })
        .unwrap();
    let recovery = journal.recover(root.path()).unwrap();
    assert!(recovery.finished.is_empty());
    assert_eq!(fs::read(root.path().join("n.md")).unwrap(), b"edited again");
    assert!(!root.path().join("n (conflict a 2026-10-24).md").exists());
}

#[cfg(unix)]
#[test]
fn recovery_never_acts_through_a_folder_that_became_a_link() {
    let root = TempDir::new("journal-root");
    let outside = TempDir::new("journal-outside");
    let state = TempDir::new("journal-state");
    fs::write(outside.path().join(".a.txt.azul-storage-1-0.tmp"), b"outside").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("docs")).unwrap();
    let journal = Journal::new(state.path());
    journal
        .begin(Intent::Download {
            key: String::from("docs/a.txt"),
            hash: hash_bytes(b"x"),
        })
        .unwrap();
    let recovery = journal.recover(root.path()).unwrap();
    assert!(outside.path().join(".a.txt.azul-storage-1-0.tmp").exists());
    assert_eq!(recovery.left.len(), 1, "{recovery:?}");
}

#[test]
fn a_case_rename_cut_off_half_way_is_finished() {
    let root = TempDir::new("journal-root");
    let state = TempDir::new("journal-state");
    fs::write(root.path().join(".azsync-case-1"), b"report").unwrap();
    let journal = Journal::new(state.path());
    journal
        .begin(Intent::CaseRename {
            from: String::from("Report.txt"),
            via: String::from(".azsync-case-1"),
            to: String::from("report.txt"),
        })
        .unwrap();
    journal.recover(root.path()).unwrap();
    assert_eq!(fs::read(root.path().join("report.txt")).unwrap(), b"report");
}

#[test]
fn the_temp_names_are_azul_storages() {
    assert!(is_download_temp(".a.txt.azul-storage-12-3.tmp", "a.txt"));
    assert!(!is_download_temp(".a.txt.azul-storage-12-3.tmp", "b.txt"));
    assert!(!is_download_temp("a.txt", "a.txt"));
}
