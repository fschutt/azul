//! The git side of a folder (`git_repo`) on real repositories in temporary folders, made by the
//! git program with a configuration of its own (no user's, no system's): the branch chip's
//! facts, the branches of the quick switcher, the check before a switch, the files watched.

use std::{path::Path, process::Command, time::Duration};

use azul_storage::testing::TempDir;

use crate::git_repo::{
    branches, chip_text, filter_branches, paths_overlap, plan_switch, probe, stamp, AheadBehind,
    Branch, Changes, Head, Limits, SwitchPlan, Unknown,
};

/// `git <args>` in `dir`, isolated from the user's and the system's configuration; its output.
fn git(dir: &Path, args: &[&str]) -> String {
    git_at(dir, args, None)
}

/// [`git`] with the commit dates set to `date` (ISO 8601), so commits have a known order.
fn git_at(dir: &Path, args: &[&str], date: Option<&str>) -> String {
    let mut command = Command::new("git");
    command
        .args([
            "-c",
            "user.name=AzDrive Test",
            "-c",
            "user.email=test@azdrive.invalid",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.autocrlf=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", std::env::temp_dir().join("azdrive-git-tests-none.gitconfig"))
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    if let Some(date) = date {
        command.env("GIT_AUTHOR_DATE", date).env("GIT_COMMITTER_DATE", date);
    }
    let out = command.output().expect("the git program runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Writes `text` to `rel` under `root`, its folders made.
fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("a folder");
    }
    std::fs::write(path, text).expect("a file");
}

/// A repository in `dir` with one commit of `a.txt` and `docs/b.txt` on `main`.
fn repo_with_a_commit(dir: &Path) {
    git(dir, &["init", "-q"]);
    write(dir, "a.txt", "one\n");
    write(dir, "docs/b.txt", "two\n");
    git(dir, &["add", "."]);
    git_at(dir, &["commit", "-q", "-m", "first"], Some("2026-01-01T10:00:00Z"));
}

fn same_folder(a: &Path, b: &Path) -> bool {
    a.canonicalize().ok() == b.canonicalize().ok()
}

fn probed(dir: &Path) -> crate::git_repo::RepoInfo {
    probe(dir, &Limits::default())
        .expect("the repository reads")
        .expect("a repository holds the folder")
}

#[test]
fn a_folder_outside_any_repository_shows_no_branch() {
    let tmp = TempDir::new("git-none");
    let folder = tmp.path().join("plain");
    std::fs::create_dir_all(&folder).expect("a folder");
    assert!(probe(&folder, &Limits::default()).expect("no error").is_none());
}

#[test]
fn a_folder_deep_in_a_repository_names_its_branch_and_a_clean_tree() {
    let tmp = TempDir::new("git-deep");
    repo_with_a_commit(tmp.path());
    let info = probed(&tmp.path().join("docs"));
    assert_eq!(info.head, Head::Branch(String::from("main")));
    assert_eq!(info.changes, Changes::Clean);
    assert!(!info.bare());
    assert!(!info.linked_worktree);
    let root = info.work_dir.clone().expect("a work tree");
    assert!(same_folder(&root, tmp.path()), "{}", root.display());
    assert_eq!(info.upstream, None);
    assert_eq!(info.ahead_behind, None);
}

#[test]
fn an_edited_file_a_staged_file_and_a_new_file_make_the_tree_dirty() {
    let tmp = TempDir::new("git-dirty");
    repo_with_a_commit(tmp.path());
    write(tmp.path(), "a.txt", "one, edited\n");
    write(tmp.path(), "docs/b.txt", "two, staged\n");
    git(tmp.path(), &["add", "docs/b.txt"]);
    write(tmp.path(), "new.txt", "new\n");
    let info = probed(tmp.path());
    assert_eq!(
        info.changes,
        Changes::Dirty {
            changed: 2,
            untracked: 1
        }
    );
    assert!(chip_text(&info).dirty, "the chip shows the dot");
}

#[test]
fn a_detached_head_shows_the_short_commit_id() {
    let tmp = TempDir::new("git-detached");
    repo_with_a_commit(tmp.path());
    git(tmp.path(), &["checkout", "-q", "--detach"]);
    let short = git(tmp.path(), &["rev-parse", "--short=7", "HEAD"]);
    let info = probed(tmp.path());
    assert_eq!(info.head, Head::Detached(short.clone()));
    let chip = chip_text(&info);
    assert_eq!(chip.name, short);
    assert!(chip.detached);
}

#[test]
fn a_new_repository_without_commits_names_its_unborn_branch() {
    let tmp = TempDir::new("git-unborn");
    git(tmp.path(), &["init", "-q"]);
    let info = probed(tmp.path());
    assert_eq!(info.head, Head::Unborn(String::from("main")));
    assert_eq!(info.head.branch(), Some("main"));
}

#[test]
fn a_linked_worktree_shows_its_own_branch() {
    let tmp = TempDir::new("git-worktree");
    let main = tmp.path().join("main");
    std::fs::create_dir_all(&main).expect("a folder");
    repo_with_a_commit(&main);
    git(&main, &["worktree", "add", "-q", "-b", "feature", "../linked"]);
    let linked = tmp.path().join("linked");
    assert!(linked.join(".git").is_file(), "a linked worktree's .git is a file");
    let info = probed(&linked.join("docs"));
    assert_eq!(info.head, Head::Branch(String::from("feature")));
    assert!(info.linked_worktree);
    assert!(same_folder(info.work_dir.as_deref().expect("a work tree"), &linked));
    // The main work tree keeps its branch.
    assert_eq!(probed(&main).head, Head::Branch(String::from("main")));
}

#[test]
fn a_bare_repository_shows_its_branch_and_has_no_work_tree() {
    let tmp = TempDir::new("git-bare");
    let source = tmp.path().join("source");
    std::fs::create_dir_all(&source).expect("a folder");
    repo_with_a_commit(&source);
    git(tmp.path(), &["clone", "-q", "--bare", "source", "store.git"]);
    let info = probed(&tmp.path().join("store.git"));
    assert!(info.bare());
    assert_eq!(info.head, Head::Branch(String::from("main")));
    assert_eq!(info.changes, Changes::NoWorkTree);
}

#[test]
fn a_repository_past_the_size_cap_is_not_scanned_for_changes() {
    let tmp = TempDir::new("git-big");
    repo_with_a_commit(tmp.path());
    write(tmp.path(), "a.txt", "edited\n");
    let limits = Limits {
        max_index_entries: 1,
        ..Limits::default()
    };
    let info = probe(tmp.path(), &limits).expect("reads").expect("a repository");
    assert_eq!(info.changes, Changes::Unknown(Unknown::TooBig));
    assert_eq!(info.head, Head::Branch(String::from("main")), "the branch still shows");
    let chip = chip_text(&info);
    assert!(!chip.dirty, "no dot for what was not looked at");
    assert!(chip.unknown);
}

#[test]
fn a_scan_past_its_deadline_says_it_could_not_tell() {
    let tmp = TempDir::new("git-deadline");
    repo_with_a_commit(tmp.path());
    let limits = Limits {
        deadline: Duration::ZERO,
        ..Limits::default()
    };
    let info = probe(tmp.path(), &limits).expect("reads").expect("a repository");
    assert_eq!(info.changes, Changes::Unknown(Unknown::TimedOut));
}

#[test]
fn ahead_and_behind_count_the_commits_against_the_upstream() {
    let tmp = TempDir::new("git-upstream");
    let origin = tmp.path().join("origin");
    std::fs::create_dir_all(&origin).expect("a folder");
    repo_with_a_commit(&origin);
    git(tmp.path(), &["clone", "-q", "origin", "clone"]);
    let clone = tmp.path().join("clone");
    let info = probed(&clone);
    assert_eq!(info.upstream.as_deref(), Some("origin/main"));
    assert_eq!(
        info.ahead_behind,
        Some(AheadBehind {
            ahead: 0,
            behind: 0,
            capped: false
        })
    );
    write(&clone, "c.txt", "mine\n");
    git(&clone, &["add", "c.txt"]);
    git(&clone, &["commit", "-q", "-m", "mine"]);
    write(&origin, "d.txt", "theirs\n");
    git(&origin, &["add", "d.txt"]);
    git(&origin, &["commit", "-q", "-m", "theirs 1"]);
    write(&origin, "d.txt", "theirs again\n");
    git(&origin, &["commit", "-q", "-am", "theirs 2"]);
    git(&clone, &["fetch", "-q"]);
    let info = probed(&clone);
    assert_eq!(
        info.ahead_behind,
        Some(AheadBehind {
            ahead: 1,
            behind: 2,
            capped: false
        })
    );
    assert_eq!(chip_text(&info).counts, "\u{2191}1 \u{2193}2");
    // A cap of one commit says "at least".
    let capped = probe(
        &clone,
        &Limits {
            walk_cap: 1,
            ..Limits::default()
        },
    )
    .expect("reads")
    .expect("a repository");
    assert_eq!(
        capped.ahead_behind,
        Some(AheadBehind {
            ahead: 1,
            behind: 1,
            capped: true
        })
    );
    assert_eq!(chip_text(&capped).counts, "\u{2191}1+ \u{2193}1+");
}

#[test]
fn the_branches_list_the_current_first_then_the_most_recent() {
    let tmp = TempDir::new("git-branches");
    repo_with_a_commit(tmp.path());
    for (name, date) in [
        ("old-work", "2026-02-01T10:00:00Z"),
        ("newest", "2026-04-01T10:00:00Z"),
        ("middle", "2026-03-01T10:00:00Z"),
    ] {
        git(tmp.path(), &["switch", "-q", "-c", name, "main"]);
        write(tmp.path(), &format!("{name}.txt"), name);
        git(tmp.path(), &["add", "."]);
        git_at(tmp.path(), &["commit", "-q", "-m", name], Some(date));
    }
    git(tmp.path(), &["switch", "-q", "main"]);
    let names: Vec<String> = branches(tmp.path())
        .expect("the branches read")
        .into_iter()
        .map(|b| b.name)
        .collect();
    assert_eq!(names, ["main", "newest", "middle", "old-work"]);
    let list = branches(tmp.path()).expect("the branches read");
    assert!(list[0].current);
    assert!(list[1..].iter().all(|b| !b.current));
}

#[test]
fn an_unborn_branch_is_listed_as_the_current_one() {
    let tmp = TempDir::new("git-branches-unborn");
    git(tmp.path(), &["init", "-q"]);
    let list = branches(tmp.path()).expect("the branches read");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "main");
    assert!(list[0].current);
}

fn branch(name: &str, tip_time: i64, current: bool) -> Branch {
    Branch {
        name: name.to_string(),
        tip_time,
        current,
    }
}

#[test]
fn typing_keeps_the_branches_that_hold_every_term_in_their_order() {
    let list = vec![
        branch("main", 50, true),
        branch("feature/login-form", 40, false),
        branch("fix/login", 30, false),
        branch("Feature/Settings", 20, false),
    ];
    let names = |query: &str| -> Vec<String> {
        filter_branches(&list, query)
            .into_iter()
            .map(|b| b.name.clone())
            .collect()
    };
    assert_eq!(names(""), ["main", "feature/login-form", "fix/login", "Feature/Settings"]);
    assert_eq!(names("login"), ["feature/login-form", "fix/login"]);
    assert_eq!(names("feat login"), ["feature/login-form"]);
    assert_eq!(names("FEATURE"), ["feature/login-form", "Feature/Settings"]);
    assert!(names("nothing-like-it").is_empty());
}

#[test]
fn a_switch_from_a_clean_tree_is_plain_and_to_the_current_branch_is_nothing() {
    let tmp = TempDir::new("git-plan-clean");
    repo_with_a_commit(tmp.path());
    git(tmp.path(), &["branch", "other"]);
    let limits = Limits::default();
    assert_eq!(plan_switch(tmp.path(), "other", &limits), Ok(SwitchPlan::Clean));
    assert_eq!(plan_switch(tmp.path(), "main", &limits), Ok(SwitchPlan::Current));
    assert_eq!(plan_switch(tmp.path(), "no-such", &limits), Ok(SwitchPlan::NoSuchBranch));
}

#[test]
fn changes_to_files_the_branch_leaves_alone_are_carried() {
    let tmp = TempDir::new("git-plan-carry");
    repo_with_a_commit(tmp.path());
    git(tmp.path(), &["switch", "-q", "-c", "other"]);
    write(tmp.path(), "docs/b.txt", "two on other\n");
    git(tmp.path(), &["commit", "-q", "-am", "other changes b"]);
    git(tmp.path(), &["switch", "-q", "main"]);
    // a.txt is the same on both branches; new.txt is in neither.
    write(tmp.path(), "a.txt", "one, edited\n");
    write(tmp.path(), "new.txt", "new\n");
    assert_eq!(
        plan_switch(tmp.path(), "other", &Limits::default()),
        Ok(SwitchPlan::Carry {
            changed: 1,
            untracked: 1
        })
    );
}

#[test]
fn a_change_to_a_file_the_branch_changes_blocks_the_switch() {
    let tmp = TempDir::new("git-plan-blocked");
    repo_with_a_commit(tmp.path());
    git(tmp.path(), &["switch", "-q", "-c", "other"]);
    write(tmp.path(), "a.txt", "one on other\n");
    git(tmp.path(), &["commit", "-q", "-am", "other changes a"]);
    git(tmp.path(), &["switch", "-q", "main"]);
    write(tmp.path(), "a.txt", "one, edited here\n");
    assert_eq!(
        plan_switch(tmp.path(), "other", &Limits::default()),
        Ok(SwitchPlan::Blocked {
            files: vec![String::from("a.txt")],
            more: 0
        })
    );
}

#[test]
fn a_new_file_the_branch_would_overwrite_blocks_the_switch() {
    let tmp = TempDir::new("git-plan-untracked");
    repo_with_a_commit(tmp.path());
    git(tmp.path(), &["switch", "-q", "-c", "other"]);
    write(tmp.path(), "notes/plan.txt", "on other\n");
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-q", "-m", "other adds notes"]);
    git(tmp.path(), &["switch", "-q", "main"]);
    // An untracked folder (one collapsed entry) holding the file the branch brings.
    write(tmp.path(), "notes/plan.txt", "mine, untracked\n");
    match plan_switch(tmp.path(), "other", &Limits::default()) {
        Ok(SwitchPlan::Blocked { files, .. }) => {
            assert!(files.iter().any(|f| f.starts_with("notes")), "{files:?}");
        }
        other => panic!("blocked expected, got {other:?}"),
    }
}

#[test]
fn a_path_overlaps_itself_its_folders_and_what_they_hold() {
    assert!(paths_overlap("a.txt", "a.txt"));
    assert!(paths_overlap("notes", "notes/plan.txt"));
    assert!(paths_overlap("notes/", "notes/plan.txt"));
    assert!(paths_overlap("notes/plan.txt", "notes"));
    assert!(!paths_overlap("notes", "notes2/plan.txt"));
    assert!(!paths_overlap("a.txt", "b.txt"));
    assert!(!paths_overlap("docs/a", "docs/ab"));
}

#[test]
fn the_watched_files_change_when_the_branch_or_the_index_moves() {
    let tmp = TempDir::new("git-stamps");
    repo_with_a_commit(tmp.path());
    git(tmp.path(), &["branch", "other"]);
    let info = probed(tmp.path());
    let files = info.watched_files();
    assert!(files.iter().any(|f| f.ends_with("HEAD")), "{files:?}");
    assert!(files.iter().any(|f| f.ends_with("index")), "{files:?}");
    let before = stamp(&files);
    assert_eq!(before, stamp(&files), "nothing moved");
    // HEAD's file is rewritten with another length: the stamp sees it even within one second.
    git(tmp.path(), &["switch", "-q", "other"]);
    assert_ne!(before, stamp(&files));
}

#[test]
fn the_chip_reads_the_branch_and_says_nothing_of_an_upstream_it_does_not_have() {
    let tmp = TempDir::new("git-chip");
    repo_with_a_commit(tmp.path());
    let chip = chip_text(&probed(tmp.path()));
    assert_eq!(chip.name, "main");
    assert!(!chip.detached);
    assert!(!chip.dirty);
    assert!(!chip.unknown);
    assert_eq!(chip.counts, "");
}
