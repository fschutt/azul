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

/// X-DELTA-SYNC4-22, the recovery of a conflict copy. It looks at the copy's name ("still free"), then
/// renames this side's file to it with a replacing rename: a file that took the name in between was
/// overwritten. The rename never replaces now, and the seam (`recover_with`) lets the test put the
/// file there at the moment. RED on the old code: the file that took the name holds this side's bytes.
#[test]
fn x_delta_sync4_22_the_recovery_of_a_conflict_never_replaces_a_file_that_took_the_copys_name() {
    let root = TempDir::new("journal-s22-conflict");
    let state = TempDir::new("journal-s22-conflict-state");
    fs::write(root.path().join("plan.txt"), b"this side's plan").unwrap();
    Journal::new(state.path())
        .begin(Intent::Conflict {
            key: String::from("plan.txt"),
            copy: String::from("plan (B).txt"),
            hash: hash_bytes(b"this side's plan"),
        })
        .unwrap();
    let folder = root.path().to_path_buf();
    let recovery = Journal::new(state.path())
        .recover_with(root.path(), &|name| {
            if name == "recover-conflict-before-rename" {
                fs::write(folder.join("plan (B).txt"), b"another file took the name").unwrap();
            }
        })
        .unwrap();
    assert_eq!(
        fs::read(root.path().join("plan (B).txt")).unwrap(),
        b"another file took the name",
        "{recovery:?}"
    );
    assert_eq!(
        fs::read(root.path().join("plan.txt")).unwrap(),
        b"this side's plan",
        "this side's file stays where it was"
    );
    assert!(recovery.finished.is_empty(), "{recovery:?}");
}

/// X-DELTA-SYNC4-22, the recovery of a case rename: the same window, the same answer.
#[test]
fn x_delta_sync4_22_the_recovery_of_a_case_rename_never_replaces_a_file_that_took_the_name() {
    let root = TempDir::new("journal-s22-case");
    let state = TempDir::new("journal-s22-case-state");
    fs::write(root.path().join(".azsync-case-1"), b"report").unwrap();
    Journal::new(state.path())
        .begin(Intent::CaseRename {
            from: String::from("Report.txt"),
            via: String::from(".azsync-case-1"),
            to: String::from("report.txt"),
        })
        .unwrap();
    let folder = root.path().to_path_buf();
    let recovery = Journal::new(state.path())
        .recover_with(root.path(), &|name| {
            if name == "recover-case-before-rename" {
                fs::write(folder.join("report.txt"), b"another file took the name").unwrap();
            }
        })
        .unwrap();
    assert_eq!(
        fs::read(root.path().join("report.txt")).unwrap(),
        b"another file took the name",
        "{recovery:?}"
    );
    assert_eq!(
        fs::read(root.path().join(".azsync-case-1")).unwrap(),
        b"report",
        "the half-renamed file stays under its temporary name"
    );
    assert!(recovery.finished.is_empty(), "{recovery:?}");
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

/// X-DELTA-SYNC6-5. "Free up space" takes the file aside under `.azul-freeing-<pid>-<n>` before it
/// unlinks it. A kill between the two left the file's bytes under a name the rules hide: never
/// freed, never swept, and the space the user asked to free stayed used. The step is in the journal
/// now (`Intent::Evict`): recovery puts the file back under its name, where the pass reads it as
/// present and unchanged and makes the base entry whole again. The cut is built by hand, as the
/// other recoveries' are.
#[test]
fn x_delta_sync6_5_a_free_up_space_cut_between_its_rename_and_its_unlink_gets_the_file_back() {
    let root = TempDir::new("journal-root");
    let state = TempDir::new("journal-state");
    fs::create_dir_all(root.path().join("docs")).unwrap();
    fs::write(
        root.path().join("docs/.azul-freeing-4711-3"),
        b"the freed file",
    )
    .unwrap();
    Journal::new(state.path())
        .begin(Intent::Evict {
            key: String::from("docs/report.pdf"),
            aside: String::from("docs/.azul-freeing-4711-3"),
        })
        .unwrap();
    let recovery = Journal::new(state.path()).recover(root.path()).unwrap();
    assert_eq!(
        fs::read(root.path().join("docs/report.pdf")).unwrap(),
        b"the freed file",
        "{recovery:?}"
    );
    assert!(!root.path().join("docs/.azul-freeing-4711-3").exists());
    assert_eq!(recovery.rolled_back.len(), 1, "{recovery:?}");
}

/// X-DELTA-SYNC6-5. A newer file holds the name when recovery looks (the user saved one meanwhile):
/// it is never overwritten; the freed file is kept beside it under a name that says so.
#[test]
fn x_delta_sync6_5_a_free_up_space_cut_whose_name_is_held_again_keeps_the_file_beside() {
    let root = TempDir::new("journal-root");
    let state = TempDir::new("journal-state");
    fs::write(root.path().join("report.pdf"), b"a newer file").unwrap();
    fs::write(root.path().join(".azul-freeing-4711-3"), b"the freed file").unwrap();
    Journal::new(state.path())
        .begin(Intent::Evict {
            key: String::from("report.pdf"),
            aside: String::from(".azul-freeing-4711-3"),
        })
        .unwrap();
    let recovery = Journal::new(state.path()).recover(root.path()).unwrap();
    assert_eq!(
        fs::read(root.path().join("report.pdf")).unwrap(),
        b"a newer file"
    );
    assert_eq!(
        fs::read(root.path().join("report.pdf (kept)")).unwrap(),
        b"the freed file",
        "{recovery:?}"
    );
    assert!(!root.path().join(".azul-freeing-4711-3").exists());
}

/// X-DELTA-SYNC6-5. The cut came before the rename (or the unlink finished): nothing is under the
/// aside name, and recovery changes nothing - least of all makes a file appear.
#[test]
fn x_delta_sync6_5_a_free_up_space_cut_with_nothing_taken_aside_changes_nothing() {
    let root = TempDir::new("journal-root");
    let state = TempDir::new("journal-state");
    fs::write(root.path().join("report.pdf"), b"still here").unwrap();
    Journal::new(state.path())
        .begin(Intent::Evict {
            key: String::from("report.pdf"),
            aside: String::from(".azul-freeing-4711-3"),
        })
        .unwrap();
    let recovery = Journal::new(state.path()).recover(root.path()).unwrap();
    assert_eq!(
        fs::read(root.path().join("report.pdf")).unwrap(),
        b"still here"
    );
    assert!(
        recovery.rolled_back.is_empty() && recovery.finished.is_empty(),
        "{recovery:?}"
    );
}
