//! A banned Azlin drive in AzDrive (ban contract v1): the banner with the hours left to migrate,
//! the actions that write refused with the reason, the sync's uploads paused, "Copy everything
//! to this computer" and, past the end, the closed drive's one message. No window.

use azcloud_kit::{azlin_contract::DriveAccess, Ban};
use azul_storage::{testing::TempDir, Drive, LocalDrive};

use crate::{
    access::refusal,
    actions::Action,
    ban::{banner_text, closed, closed_text, copy_prefix, refused_words, sync_text},
    fileops::{plan_transfer, run_transfer, ConflictChoice, SourceItem, TransferKind},
};

/// 2026-10-12T10:00:00Z.
const UNTIL: u64 = 1_791_799_200;

fn ban() -> Ban {
    Ban {
        reason: String::from("spam distribution"),
        until: Some(UNTIL),
        closed: false,
    }
}

/// The same ban once the token server refused the drive (403 `drive_banned`).
fn closed_ban() -> Ban {
    Ban {
        closed: true,
        ..ban()
    }
}

#[test]
fn the_banner_says_why_and_the_hours_left_to_migrate_the_files() {
    assert_eq!(
        banner_text("spam distribution", Some(36)),
        "Due to spam distribution, your account has been banned, but you have 36 hours to \
         migrate your files."
    );
    assert_eq!(
        banner_text("spam distribution", Some(1)),
        "Due to spam distribution, your account has been banned, but you have 1 hour to \
         migrate your files."
    );
    let none = banner_text("spam distribution", None);
    assert!(
        none.starts_with("Due to spam distribution, your account has been banned"),
        "{none}"
    );
    assert_eq!(
        ban().hours_left(UNTIL - 36 * 3_600),
        36,
        "the hours count down"
    );
}

#[test]
fn past_its_end_the_drive_says_when_it_was_closed_and_why_and_nothing_else() {
    assert_eq!(
        closed_text(Some(UNTIL), "spam distribution"),
        "This drive was closed on 2026-10-12 because spam distribution."
    );
    assert_eq!(
        closed_text(None, "spam distribution"),
        "This drive was closed because spam distribution."
    );
}

#[test]
fn a_banned_drive_takes_no_uploads_new_folders_or_links_and_says_why() {
    crate::l10n::in_english();
    for action in [
        Action::Upload,
        Action::NewFolder,
        Action::NewTextDocument,
        Action::Paste,
        Action::Rename,
        Action::Delete,
        Action::Share,
    ] {
        assert!(
            refusal(&action, DriveAccess::Banned, false).is_some(),
            "{action:?} runs"
        );
    }
    for action in [
        Action::Download,
        Action::Copy,
        Action::CopyPath,
        Action::SelectAll,
    ] {
        assert_eq!(
            refusal(&action, DriveAccess::Banned, false),
            None,
            "{action:?}: reads go on"
        );
    }
    // What it says: why it is banned...
    let why = refused_words(&ban());
    assert!(
        why.contains("banned") && why.contains("spam distribution"),
        "{why}"
    );
    // ...and once the token server closed it, nothing of it is left to open.
    assert!(refusal(&Action::Download, DriveAccess::Closed, false).is_some());
    let shut = refused_words(&closed_ban());
    assert!(
        shut.starts_with("This drive was closed on 2026-10-12"),
        "{shut}"
    );
}

#[test]
fn the_sync_pauses_its_uploads_while_the_drive_is_banned() {
    crate::l10n::in_english();
    let paused = azul_appkit::l10n::t_phrase(&sync_text(&ban()));
    assert!(paused.starts_with("Uploads paused"), "{paused}");
    assert!(paused.contains("banned"), "{paused}");
    let shut = azul_appkit::l10n::t_phrase(&sync_text(&closed_ban()));
    assert!(shut.starts_with("This drive was closed"), "{shut}");
}

/// AZ-P19: a clock set ahead (a dead battery, a time-zone tool) moved a banned drive past the
/// ban's end and "closed" it: its folder hidden, Copy everything withdrawn, while the token server
/// still let its owner read it. AzDrive closes a drive when the SERVER refuses it ([`Ban::closed`]),
/// on any clock.
#[test]
fn az_p19_a_clock_past_the_end_does_not_close_a_banned_drive_only_the_server_does() {
    crate::l10n::in_english();
    // Whatever the clock says of the ban's end (400 days past it, here): AzDrive waits for the
    // server, and `closed` takes no clock.
    assert!(!closed(&ban()), "AzDrive waits for the server");
    let said = refused_words(&ban());
    assert!(
        said.contains("banned") && !said.contains("closed"),
        "{said}"
    );
    let paused = azul_appkit::l10n::t_phrase(&sync_text(&ban()));
    assert!(paused.starts_with("Uploads paused"), "{paused}");

    // The server's word closes it, whatever the clock says: one that is a year behind included.
    assert!(closed(&closed_ban()));
    let shut = refused_words(&closed_ban());
    assert!(shut.starts_with("This drive was closed"), "{shut}");
}

#[test]
fn copy_everything_plans_the_whole_drive_into_a_folder_named_after_it() {
    let tmp = TempDir::new("ban-copy");
    let drive = LocalDrive::new(tmp.path().join("drive"));
    drive.put("docs/a.txt", b"alpha").unwrap();
    drive.put("docs/sub/b.txt", b"beta").unwrap();
    drive.put("readme.txt", b"hello").unwrap();
    drive.create_folder("empty/").unwrap();
    let here = LocalDrive::without_manifest(tmp.path().join("here"));
    let prefix = copy_prefix("Photos 2026");
    assert_eq!(prefix, "Photos 2026/");
    let everything = SourceItem {
        key: String::new(),
        is_folder: true,
        size: None,
    };
    let plan = plan_transfer(
        &drive,
        &[everything],
        &here,
        &prefix,
        TransferKind::Download,
        "Copy",
    )
    .unwrap();
    let mut targets: Vec<&str> = plan.files.iter().map(|f| f.target_key.as_str()).collect();
    targets.sort_unstable();
    assert_eq!(
        targets,
        vec![
            "Photos 2026/docs/a.txt",
            "Photos 2026/docs/sub/b.txt",
            "Photos 2026/readme.txt"
        ]
    );
    assert!(
        plan.folders.contains(&String::from("Photos 2026/empty/")),
        "{:?}",
        plan.folders
    );
    assert_eq!(
        copy_prefix("a/b: c"),
        "a-b- c/",
        "no folder of the name breaks the path"
    );
    assert_eq!(copy_prefix("  "), "Azlin drive/");
}

/// AZ-U20: Copy everything ran again after a failure (or a day later) with every conflict "keep
/// both": every file was copied a second time as `name (2)` - the disk doubled, nothing resumed.
/// Run again, it skips the files that are there with the size of the drive's file and keeps both
/// of the ones that differ.
#[test]
fn az_u20_copy_everything_run_again_resumes_instead_of_copying_everything_twice() {
    use std::sync::atomic::AtomicBool;
    let tmp = TempDir::new("ban-copy-again");
    let drive = LocalDrive::new(tmp.path().join("drive"));
    drive.put("docs/a.txt", b"alpha").unwrap();
    drive.put("docs/b.txt", b"beta").unwrap();
    drive.put("readme.txt", b"hello").unwrap();
    let here = LocalDrive::without_manifest(tmp.path().join("here"));
    let everything = SourceItem {
        key: String::new(),
        is_folder: true,
        size: None,
    };
    let go = |expect_conflicts: usize| {
        let mut plan = plan_transfer(
            &drive,
            &[everything.clone()],
            &here,
            "Photos/",
            TransferKind::Download,
            "Copy",
        )
        .unwrap();
        assert_eq!(plan.conflicts().len(), expect_conflicts);
        for i in plan.conflicts() {
            plan.choose(i, ConflictChoice::Resume);
        }
        assert!(plan.unresolved().is_none());
        run_transfer(
            &plan,
            &drive,
            &here,
            TransferKind::Download,
            &AtomicBool::new(false),
            &mut |_| {},
        )
    };
    let first = go(0);
    assert!(first.failed.is_empty(), "{:?}", first.failed);
    assert_eq!(first.done, 3);
    // A failure left half of it: here b.txt is cut short (a different size).
    std::fs::write(
        tmp.path()
            .join("here")
            .join("Photos")
            .join("docs")
            .join("b.txt"),
        b"be",
    )
    .unwrap();
    let second = go(3);
    assert!(second.failed.is_empty(), "{:?}", second.failed);
    assert_eq!(
        second.skipped, 2,
        "a.txt and readme.txt are there: {second:?}"
    );
    assert_eq!(second.done, 1, "only the file that differs is copied");
    assert_eq!(here.get("Photos/docs/a.txt").unwrap(), b"alpha");
    assert!(
        here.get("Photos/docs/a (2).txt").is_err(),
        "no second copy of a file that is there"
    );
    assert_eq!(
        here.get("Photos/docs/b.txt").unwrap(),
        b"be",
        "the file that differs is not replaced"
    );
    assert_eq!(
        here.get("Photos/docs/b (2).txt").unwrap(),
        b"beta",
        "both are kept"
    );
}
