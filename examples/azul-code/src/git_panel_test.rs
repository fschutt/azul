//! Which changes a click opens, and what the letters look like.

use super::*;

fn change(kind: ChangeKind, path: &str) -> Change {
    Change { kind, staged: false, path: path.to_string(), from: None }
}

#[test]
fn a_click_opens_a_changed_file_of_the_workspaces_folder() {
    assert_eq!(openable_key(&change(ChangeKind::Modified, "src/main.rs"), "").as_deref(), Some("src/main.rs"));
    assert_eq!(
        openable_key(&change(ChangeKind::Added, "examples/app/src/lib.rs"), "examples/app/").as_deref(),
        Some("src/lib.rs"),
        "the key is the path less the folder's place in the repository"
    );
    assert_eq!(openable_key(&change(ChangeKind::Untracked, "notes.txt"), "").as_deref(), Some("notes.txt"));
}

#[test]
fn a_deleted_file_a_folder_and_a_file_outside_the_workspace_open_nothing() {
    assert_eq!(openable_key(&change(ChangeKind::Deleted, "gone.rs"), ""), None, "there is no file to open");
    assert_eq!(openable_key(&change(ChangeKind::Untracked, "newdir/"), ""), None, "an untracked folder is listed as dir/");
    assert_eq!(openable_key(&change(ChangeKind::Modified, "README.md"), "examples/app/"), None, "outside the folder");
}

#[test]
fn every_kind_of_change_has_a_colour_and_what_differs_in_meaning_differs_in_colour() {
    let kinds = [
        ChangeKind::Modified,
        ChangeKind::Added,
        ChangeKind::Deleted,
        ChangeKind::Renamed,
        ChangeKind::Copied,
        ChangeKind::Untracked,
        ChangeKind::Conflicted,
    ];
    for k in kinds {
        assert!(letter_colour(k).starts_with('#'), "{k:?}");
    }
    assert_ne!(letter_colour(ChangeKind::Modified), letter_colour(ChangeKind::Deleted));
    assert_ne!(letter_colour(ChangeKind::Added), letter_colour(ChangeKind::Deleted));
    assert_ne!(letter_colour(ChangeKind::Conflicted), letter_colour(ChangeKind::Modified));
}
