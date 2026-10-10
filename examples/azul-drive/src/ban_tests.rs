//! A banned Azlin drive in AzDrive (ban contract v1): the banner with the hours left to migrate,
//! the actions that write refused with the reason, the sync's uploads paused, "Copy everything
//! to this computer" and, past the end, the closed drive's one message. No window.

use azcloud_kit::Ban;
use azul_storage::{testing::TempDir, Drive, LocalDrive};

use crate::{
    actions::Action,
    ban::{banner_text, closed_text, copy_prefix, refusal, sync_text},
    fileops::{plan_transfer, SourceItem, TransferKind},
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
    assert!(none.starts_with("Due to spam distribution, your account has been banned"), "{none}");
    assert_eq!(ban().hours_left(UNTIL - 36 * 3_600), 36, "the hours count down");
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
    let now = UNTIL - 10 * 3_600;
    for action in [
        Action::Upload,
        Action::NewFolder,
        Action::NewTextDocument,
        Action::Paste,
        Action::Rename,
        Action::Delete,
        Action::Share,
    ] {
        let why = refusal(&action, &ban(), now).unwrap_or_else(|| panic!("{action:?} runs"));
        assert!(why.contains("banned") && why.contains("spam distribution"), "{why}");
    }
    for action in [Action::Download, Action::Copy, Action::CopyPath, Action::SelectAll] {
        assert_eq!(refusal(&action, &ban(), now), None, "{action:?}: reads go on");
    }
    // Past the end nothing of it is left to open.
    let closed = refusal(&Action::Download, &ban(), UNTIL).expect("closed");
    assert!(closed.starts_with("This drive was closed on 2026-10-12"), "{closed}");
}

#[test]
fn the_sync_pauses_its_uploads_while_the_drive_is_banned() {
    crate::l10n::in_english();
    let paused = azul_appkit::l10n::t_phrase(&sync_text(&ban(), UNTIL - 60));
    assert!(paused.starts_with("Uploads paused"), "{paused}");
    assert!(paused.contains("banned"), "{paused}");
    let closed = azul_appkit::l10n::t_phrase(&sync_text(&ban(), UNTIL + 60));
    assert!(closed.starts_with("This drive was closed"), "{closed}");
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
        false,
        TransferKind::Download,
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
    assert_eq!(copy_prefix("a/b: c"), "a-b- c/", "no folder of the name breaks the path");
    assert_eq!(copy_prefix("  "), "Azlin drive/");
}
