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

/// ARCH29 C6c, sync/journal.rs. Recovery walks the names of a journalled step as `OnDisk` keys: to what
/// is there, never making a folder. A step whose name the full rows refuse - a conflict copy or a case
/// rename's target in a protected spelling (a journal is this device's own, but a line can be cut,
/// damaged or written by an older build) - finds no folder, is left as it is and says so; no folder
/// of a protected name appears and the file stays where it was.
#[test]
fn c6c_recovery_of_a_step_with_a_protected_name_makes_no_folder_and_moves_nothing() {
    let root = TempDir::new("journal-c6c-root");
    let state = TempDir::new("journal-c6c-state");
    fs::write(root.path().join("n.md"), b"mine").unwrap();
    fs::write(root.path().join(".azsync-case-1"), b"case").unwrap();
    let journal = Journal::new(state.path());
    journal
        .begin(Intent::Conflict {
            key: String::from("n.md"),
            copy: String::from(".GIT/hooks/n (conflict a 2026-10-24).md"),
            hash: hash_bytes(b"mine"),
        })
        .unwrap();
    journal
        .begin(Intent::CaseRename {
            from: String::from("Case.txt"),
            via: String::from(".azsync-case-1"),
            to: String::from("GIT~1/case.txt"),
        })
        .unwrap();
    let recovery = journal.recover(root.path()).unwrap();
    assert!(recovery.finished.is_empty(), "{recovery:?}");
    for made in [".GIT", "GIT~1"] {
        assert!(!root.path().join(made).exists(), "{made} was made");
    }
    assert_eq!(fs::read(root.path().join("n.md")).unwrap(), b"mine");
    assert_eq!(
        fs::read(root.path().join(".azsync-case-1")).unwrap(),
        b"case"
    );
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

/// The disk after the first of a replace's two renames, where the volume makes no hard links: the
/// old file is in the trash slot, the name is EMPTY, and the whole new version waits beside it.
/// Returns the journal's intent for it.
fn cut_between_the_two_renames(root: &std::path::Path, old: &[u8], new: &[u8]) -> Intent {
    let slot = ".azlin/sync/trash/20261111-101500-a1b2c3/docs/a.txt";
    let staged = "docs/.a.txt.azul-storage-77-1.tmp";
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::create_dir_all(root.join(slot).parent().unwrap()).unwrap();
    fs::write(root.join(slot), old).unwrap();
    fs::write(root.join(staged), new).unwrap();
    Intent::Replace {
        key: String::from("docs/a.txt"),
        staged: String::from(staged),
        slot: String::from(slot),
    }
}

/// X-DELTA-SYNC2-6. `replace_keeping` is two renames on a volume without hard links, and a process
/// cut between them left the name empty - which the next pass reads as the user's delete. The
/// replace is in the journal now, and recovery finishes it: the whole new version takes the name,
/// the old file stays in the trash, nothing is left beside.
#[test]
fn x_delta_sync2_6_a_replace_cut_between_its_renames_gets_the_new_version_in_its_name() {
    let root = TempDir::new("journal-replace-root");
    let state = TempDir::new("journal-replace-state");
    let intent = cut_between_the_two_renames(root.path(), b"rotted", b"the new version");
    let journal = Journal::new(state.path());
    journal.begin(intent.clone()).unwrap();
    // A new process: the journal is read from the disk.
    let recovery = Journal::new(state.path()).recover(root.path()).unwrap();
    assert_eq!(recovery.finished.len(), 1, "{recovery:?}");
    assert!(
        recovery.finished[0].starts_with("docs/a.txt"),
        "{recovery:?}"
    );
    assert_eq!(
        fs::read(root.path().join("docs/a.txt")).unwrap(),
        b"the new version",
        "the name holds the new version"
    );
    let Intent::Replace { slot, .. } = &intent else {
        unreachable!()
    };
    assert_eq!(
        fs::read(root.path().join(slot)).unwrap(),
        b"rotted",
        "the old file stays in the trash"
    );
    let left: Vec<String> = fs::read_dir(root.path().join("docs"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, vec![String::from("a.txt")], "no staged file is left");
}

/// X-DELTA-SYNC2-6: the new version is gone too (a spool swept, a disk that lost it): the old file
/// comes back from the trash, so the name is never empty for a pass.
#[test]
fn x_delta_sync2_6_a_replace_cut_with_its_new_version_gone_gets_the_old_file_back() {
    let root = TempDir::new("journal-replace-back-root");
    let state = TempDir::new("journal-replace-back-state");
    let intent = cut_between_the_two_renames(root.path(), b"rotted", b"the new version");
    let Intent::Replace { staged, .. } = &intent else {
        unreachable!()
    };
    fs::remove_file(root.path().join(staged)).unwrap();
    Journal::new(state.path()).begin(intent).unwrap();
    let recovery = Journal::new(state.path()).recover(root.path()).unwrap();
    assert_eq!(recovery.rolled_back.len(), 1, "{recovery:?}");
    assert_eq!(fs::read(root.path().join("docs/a.txt")).unwrap(), b"rotted");
}

/// X-DELTA-SYNC2-6: a name that is held is nothing to finish - the old file is still there (the
/// replace was cut before it moved anything) or somebody has made a new file since. Recovery puts
/// nothing over it and takes nothing away.
#[test]
fn x_delta_sync2_6_a_replace_whose_name_is_held_is_left_as_it_is() {
    let root = TempDir::new("journal-replace-held-root");
    let state = TempDir::new("journal-replace-held-state");
    let intent = cut_between_the_two_renames(root.path(), b"rotted", b"the new version");
    // Cut before the first rename: the old file is at its name (and in the slot's second name).
    fs::write(root.path().join("docs/a.txt"), b"saved by the user since").unwrap();
    Journal::new(state.path()).begin(intent.clone()).unwrap();
    let recovery = Journal::new(state.path()).recover(root.path()).unwrap();
    assert!(
        recovery.finished.is_empty() && recovery.rolled_back.is_empty(),
        "{recovery:?}"
    );
    assert_eq!(
        fs::read(root.path().join("docs/a.txt")).unwrap(),
        b"saved by the user since"
    );
    let Intent::Replace { staged, slot, .. } = &intent else {
        unreachable!()
    };
    assert_eq!(
        fs::read(root.path().join(staged)).unwrap(),
        b"the new version",
        "the staged file is not touched"
    );
    assert_eq!(fs::read(root.path().join(slot)).unwrap(), b"rotted");
}

/// X-DELTA-SYNC2-6: neither the new version nor the old file is where the journal says: nothing is
/// invented, and the step is reported as left.
#[test]
fn x_delta_sync2_6_a_replace_that_finds_neither_file_says_so() {
    let root = TempDir::new("journal-replace-none-root");
    let state = TempDir::new("journal-replace-none-state");
    let intent = cut_between_the_two_renames(root.path(), b"rotted", b"the new version");
    let Intent::Replace { staged, slot, .. } = &intent else {
        unreachable!()
    };
    fs::remove_file(root.path().join(staged)).unwrap();
    fs::remove_file(root.path().join(slot)).unwrap();
    Journal::new(state.path()).begin(intent).unwrap();
    let recovery = Journal::new(state.path()).recover(root.path()).unwrap();
    assert_eq!(recovery.left.len(), 1, "{recovery:?}");
    assert!(!root.path().join("docs/a.txt").exists());
}
