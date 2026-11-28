//! `git status --porcelain=v1 -z` read: one change per side that changed, renames with where they came from,
//! untracked and conflicted entries, what is left out; and a change's path as a workspace key.

use super::*;

fn change(kind: ChangeKind, staged: bool, path: &str) -> Change {
    Change { kind, staged, path: path.to_string(), from: None }
}

#[test]
fn a_file_changed_in_the_work_tree_or_staged_is_one_change() {
    let out = " M src/main.rs\0A  src/new.rs\0 D old.txt\0";
    assert_eq!(
        parse_status(out),
        vec![
            change(ChangeKind::Modified, false, "src/main.rs"),
            change(ChangeKind::Added, true, "src/new.rs"),
            change(ChangeKind::Deleted, false, "old.txt"),
        ]
    );
}

#[test]
fn a_file_changed_in_the_index_and_again_in_the_work_tree_is_two_changes_staged_first() {
    assert_eq!(
        parse_status("MM both.rs\0"),
        vec![change(ChangeKind::Modified, true, "both.rs"), change(ChangeKind::Modified, false, "both.rs")]
    );
}

#[test]
fn a_rename_names_the_path_it_came_from() {
    let out = "R  new name.rs\0old name.rs\0 M other.rs\0";
    let found = parse_status(out);
    assert_eq!(found.len(), 2, "the old path is part of the rename's entry, not an entry of its own: {found:?}");
    assert_eq!(found[0].kind, ChangeKind::Renamed);
    assert!(found[0].staged);
    assert_eq!(found[0].path, "new name.rs");
    assert_eq!(found[0].from.as_deref(), Some("old name.rs"));
    assert_eq!(found[1], change(ChangeKind::Modified, false, "other.rs"));
}

#[test]
fn untracked_files_are_unstaged_changes_and_ignored_ones_are_not_listed() {
    let out = "?? notes.txt\0!! target/\0?? a dir/\0";
    assert_eq!(
        parse_status(out),
        vec![change(ChangeKind::Untracked, false, "notes.txt"), change(ChangeKind::Untracked, false, "a dir/")]
    );
}

#[test]
fn an_unmerged_file_is_one_conflict_whichever_side_says_so() {
    for code in ["UU", "AA", "DD", "AU", "UD", "UA", "DU"] {
        let out = format!("{code} fight.rs\0");
        assert_eq!(parse_status(&out), vec![change(ChangeKind::Conflicted, false, "fight.rs")], "{code}");
    }
}

#[test]
fn what_cannot_be_read_is_left_out() {
    assert!(parse_status("").is_empty());
    assert!(parse_status("\0\0").is_empty());
    assert!(parse_status("M\0").is_empty(), "an entry too short");
    assert!(parse_status("MXfile.rs\0").is_empty(), "no space after the two codes");
    assert!(parse_status("M \0").is_empty(), "no path");
    assert_eq!(parse_status("T  link\0"), vec![change(ChangeKind::Modified, true, "link")], "a type change is a modification");
}

#[test]
fn a_path_with_unicode_or_spaces_is_kept_as_it_is() {
    // -z never quotes
    assert_eq!(parse_status(" M süß € file.rs\0"), vec![change(ChangeKind::Modified, false, "süß € file.rs")]);
}

#[test]
fn the_letters_and_the_names_of_the_kinds_are_each_their_own() {
    let kinds = [
        ChangeKind::Modified,
        ChangeKind::Added,
        ChangeKind::Deleted,
        ChangeKind::Renamed,
        ChangeKind::Copied,
        ChangeKind::Untracked,
        ChangeKind::Conflicted,
    ];
    let mut letters: Vec<&str> = kinds.iter().map(|k| k.letter()).collect();
    letters.sort_unstable();
    letters.dedup();
    assert_eq!(letters.len(), kinds.len());
    let mut names: Vec<&str> = kinds.iter().map(|k| k.name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), kinds.len(), "each kind names its own words");
}

#[test]
fn a_changes_path_is_a_workspace_key_when_it_lies_in_the_workspaces_folder() {
    assert_eq!(workspace_key("src/main.rs", ""), Some("src/main.rs"), "the workspace is the repository's root");
    assert_eq!(workspace_key("examples/app/src/lib.rs", "examples/app/"), Some("src/lib.rs"));
    assert_eq!(workspace_key("README.md", "examples/app/"), None, "outside the workspace's folder");
    assert_eq!(workspace_key("examples/app/", "examples/app/"), None, "the folder itself is no file of it");
}

#[test]
fn a_folder_that_is_no_repository_and_a_missing_git_are_told_apart() {
    // a temporary folder that is no repository: `git` (when this machine has it) says so; without `git` the view is unavailable
    let dir = std::env::temp_dir().join(format!("azcode-nogit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    match status(&dir) {
        Status::NoRepository | Status::Unavailable(_) => {}
        other => panic!("a folder outside any repository gave {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
