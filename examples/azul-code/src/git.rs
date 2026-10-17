//! The branch a folder is checked out at, for the status bar (VSCode's
//! "main" at its left): read from the repository's `HEAD` - the `.git`
//! folder, or the git dir a worktree's `.git` file points at - in the folder
//! or the nearest folder above it. No git process; plain Rust, read on the
//! drive thread with the folder's first listing.

use std::path::{Path, PathBuf};

/// How many folders above the workspace are looked at for a repository.
const MAX_LEVELS: usize = 64;

/// The git dir of a checkout at `folder`: its `.git` folder, or the folder a
/// `.git` FILE names (`gitdir: ...`, a worktree's or a submodule's). `None`
/// when `folder` is no checkout's root.
#[must_use]
pub fn git_dir(folder: &Path) -> Option<PathBuf> {
    let dot_git = folder.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let target = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
    if target.is_empty() {
        return None;
    }
    let path = Path::new(target);
    Some(if path.is_absolute() {
        path.to_path_buf()
    } else {
        folder.join(path)
    })
}

/// What `HEAD` says: the branch (`ref: refs/heads/main` is `main`), or the
/// first seven hex digits of a detached commit. `None` for anything else.
#[must_use]
pub fn branch_of_head(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref:") {
        let reference = reference.trim();
        let name = reference.strip_prefix("refs/heads/").unwrap_or(reference);
        return (!name.is_empty()).then(|| name.to_string());
    }
    let commit = head.len() >= 7 && head.chars().all(|c| c.is_ascii_hexdigit());
    commit.then(|| head[..7].to_string())
}

/// The branch of the repository `folder` is in (its own `.git`, else the
/// nearest folder above it with one); `None` outside a repository.
#[must_use]
pub fn branch(folder: &Path) -> Option<String> {
    let mut at = Some(folder);
    for _ in 0..MAX_LEVELS {
        let dir = at?;
        if let Some(git) = git_dir(dir) {
            let head = std::fs::read_to_string(git.join("HEAD")).ok()?;
            return branch_of_head(&head);
        }
        at = dir.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "azcode-git-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("a temporary folder");
        dir
    }

    #[test]
    fn head_names_a_branch_or_a_detached_commit() {
        assert_eq!(branch_of_head("ref: refs/heads/main\n").as_deref(), Some("main"));
        assert_eq!(
            branch_of_head("ref: refs/heads/fix/input-bugs\n").as_deref(),
            Some("fix/input-bugs")
        );
        assert_eq!(
            branch_of_head("86c0e821e0b3a5c1d2e3f4a5b6c7d8e9f0a1b2c3\n").as_deref(),
            Some("86c0e82")
        );
        assert_eq!(branch_of_head(""), None);
        assert_eq!(branch_of_head("not a head"), None);
    }

    #[test]
    fn the_branch_is_read_from_the_folder_its_parents_and_a_worktrees_git_file() {
        let repo = temp_dir("repo");
        std::fs::create_dir_all(repo.join(".git")).expect(".git");
        std::fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
        assert_eq!(branch(&repo).as_deref(), Some("main"));
        let inside = repo.join("src").join("deep");
        std::fs::create_dir_all(&inside).expect("a folder inside");
        assert_eq!(branch(&inside).as_deref(), Some("main"), "a folder inside the repository");

        let worktree = temp_dir("worktree");
        let gitdir = repo.join(".git").join("worktrees").join("wt");
        std::fs::create_dir_all(&gitdir).expect("the worktree's git dir");
        std::fs::write(gitdir.join("HEAD"), "ref: refs/heads/feature\n").expect("HEAD");
        std::fs::write(worktree.join(".git"), format!("gitdir: {}\n", gitdir.display())).expect(".git file");
        assert_eq!(branch(&worktree).as_deref(), Some("feature"));

        let plain = temp_dir("plain");
        assert_eq!(branch(&plain), None, "no repository");
        for dir in [repo, worktree, plain] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
