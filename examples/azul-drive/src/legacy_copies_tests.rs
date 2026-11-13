//! The copies an older AzDrive handed out that stay on this computer, and what the start says about them
//! (X-DELTA-SYNC9-11): the notice names the folders, and says it once per change.

use std::path::PathBuf;

use azul_storage::{scratch::LegacyCopies, testing::TempDir};

use crate::{kept_copies_in, tell_kept_copies, KeptCopies};

/// The notice names every folder that keeps copies, with the files in it: the temp folder's `AzDrive-open` and the `open`
/// folder of each pairing - not only the temp folder, which is where the old notice sent the reader for copies that sit in
/// a pairing's state folder. A pairing whose `open` folder holds no file has none to name (and the folder goes), and a
/// pairing with no such folder none either. RED on the old count: a number and no folder.
#[test]
fn x_delta_sync9_11_the_folders_that_keep_copies_are_named_with_the_files_they_hold() {
    let dir = TempDir::new("legacy-kept");
    let sync = dir.path().join("sync");
    let with = sync.join("pairing-a");
    std::fs::create_dir_all(with.join("open").join("docs")).unwrap();
    std::fs::write(
        with.join("open").join("docs").join("report.docx"),
        b"edited",
    )
    .unwrap();
    std::fs::write(with.join("open").join("b.txt"), b"x").unwrap();
    let without = sync.join("pairing-b");
    std::fs::create_dir_all(without.join("open").join("folder-only")).unwrap();
    let missing = sync.join("pairing-c");
    let temp = dir.path().join("AzDrive-open");

    let kept = kept_copies_in(
        &temp,
        &LegacyCopies::Kept { files: 3 },
        &[with.clone(), without.clone(), missing],
    );
    assert_eq!(
        kept,
        vec![
            KeptCopies {
                folder: temp.clone(),
                files: 3
            },
            KeptCopies {
                folder: with.join("open"),
                files: 2
            },
        ]
    );
    assert!(!without.join("open").exists(), "a folder with no file goes");
    assert!(with.join("open").join("b.txt").exists(), "the copies stay");

    // The temp folder dealt with (removed, absent, or kept with no file that could be counted): nothing to name.
    for temp_state in [
        LegacyCopies::Absent,
        LegacyCopies::Removed,
        LegacyCopies::Kept { files: 0 },
    ] {
        assert!(
            kept_copies_in(&temp, &temp_state, &[]).is_empty(),
            "{temp_state:?}"
        );
    }
}

/// The notice is said once per change, not at every start for as long as the folders stay: what was told last is a record in
/// the cache folder. The same copies are not told again; a count that changed, a folder that went and came back are new
/// notices; nothing kept clears the record; a record that cannot be read is none, and no cache folder is no record. RED on the
/// old start: it said the same sentence at every start.
#[test]
fn x_delta_sync9_11_the_notice_is_said_once_per_change_and_not_at_every_start() {
    let dir = TempDir::new("legacy-told");
    let record = dir.path().join("legacy-copies-told");
    let copies = |files| {
        vec![KeptCopies {
            folder: PathBuf::from("/state/open"),
            files,
        }]
    };

    assert!(
        tell_kept_copies(Some(record.as_path()), &copies(2)),
        "the first start tells it"
    );
    assert!(
        !tell_kept_copies(Some(record.as_path()), &copies(2)),
        "the same copies at the next start: not again"
    );
    assert!(
        tell_kept_copies(Some(record.as_path()), &copies(1)),
        "a count that changed is a new notice"
    );
    assert!(
        !tell_kept_copies(Some(record.as_path()), &[]),
        "nothing kept, nothing told"
    );
    assert!(
        tell_kept_copies(Some(record.as_path()), &copies(1)),
        "and when copies are kept again, they are told again"
    );

    // A record that cannot be read (a folder where the file should be) is no record: told.
    std::fs::remove_file(&record).unwrap();
    std::fs::create_dir(&record).unwrap();
    assert!(tell_kept_copies(Some(record.as_path()), &copies(1)));
    // No cache folder, no record.
    assert!(tell_kept_copies(None, &copies(1)));
    assert!(!tell_kept_copies(None, &[]));
}
