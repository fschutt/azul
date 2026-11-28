//! The branch a folder is checked out at, for the status bar (VSCode's
//! "main" at its left): read from the repository's `HEAD` - the `.git`
//! folder, or the git dir a worktree's `.git` file points at - in the folder
//! or the nearest folder above it, no git process, plain Rust, read on the
//! drive thread with the folder's first listing. And the changes of the
//! source control view ([`status`]): what `git status` says, from the `git`
//! of the machine when there is one (a phone has none: the view says so).

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

// ==== The changes: `git status` ====

/// The most changes the source control view lists; the rest are counted.
pub const MAX_CHANGES: usize = 500;

/// What happened to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    /// Both sides changed it and the merge has not been resolved.
    Conflicted,
}

impl ChangeKind {
    /// The letter VS Code puts at the end of the row (`M`, `A`, `D`, `R`, `C`, `U`, `!`).
    #[must_use]
    pub const fn letter(self) -> &'static str {
        match self {
            ChangeKind::Modified => "M",
            ChangeKind::Added => "A",
            ChangeKind::Deleted => "D",
            ChangeKind::Renamed => "R",
            ChangeKind::Copied => "C",
            ChangeKind::Untracked => "U",
            ChangeKind::Conflicted => "!",
        }
    }

    /// The key of the words that name it (`azcode-change-modified`), for the row's accessible name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            ChangeKind::Modified => "azcode-change-modified",
            ChangeKind::Added => "azcode-change-added",
            ChangeKind::Deleted => "azcode-change-deleted",
            ChangeKind::Renamed => "azcode-change-renamed",
            ChangeKind::Copied => "azcode-change-copied",
            ChangeKind::Untracked => "azcode-change-untracked",
            ChangeKind::Conflicted => "azcode-change-conflicted",
        }
    }
}

/// One changed file: what happened, whether it is in the index (staged), where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub kind: ChangeKind,
    /// In the index: the change is staged for the next commit.
    pub staged: bool,
    /// The path as git says it, relative to the repository's root.
    pub path: String,
    /// Where a renamed or copied file came from.
    pub from: Option<String>,
}

/// The changes `git status --porcelain=v1 -z` lists in `out`: one entry per side that changed (a file
/// changed in the index and again in the work tree is two), the staged ones first within a file;
/// ignored files and entries too short to read are left out.
#[must_use]
pub fn parse_status(out: &str) -> Vec<Change> {
    let mut changes = Vec::new();
    let mut fields = out.split('\0');
    while let Some(entry) = fields.next() {
        let mut chars = entry.chars();
        let (Some(x), Some(y), Some(' ')) = (chars.next(), chars.next(), chars.next()) else {
            continue;
        };
        let path = chars.as_str().to_string();
        if path.is_empty() || (x == '!' && y == '!') {
            continue;
        }
        // A rename or a copy is followed by the path it came from.
        let from = if matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C') {
            fields.next().filter(|f| !f.is_empty()).map(str::to_string)
        } else {
            None
        };
        if x == '?' && y == '?' {
            changes.push(Change { kind: ChangeKind::Untracked, staged: false, path, from: None });
            continue;
        }
        let conflicted = matches!((x, y), ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D'));
        if conflicted {
            changes.push(Change { kind: ChangeKind::Conflicted, staged: false, path, from: None });
            continue;
        }
        for (code, staged) in [(x, true), (y, false)] {
            let kind = match code {
                'M' | 'T' => ChangeKind::Modified,
                'A' => ChangeKind::Added,
                'D' => ChangeKind::Deleted,
                'R' => ChangeKind::Renamed,
                'C' => ChangeKind::Copied,
                _ => continue,
            };
            changes.push(Change { kind, staged, path: path.clone(), from: from.clone() });
        }
    }
    changes
}

/// What asking git about a folder came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// There is no `git` to ask (a phone has none), or it could not be run: why.
    Unavailable(String),
    /// The folder is in no repository.
    NoRepository,
    /// The repository's state: the branch, the changes (at most [`MAX_CHANGES`]; how many more
    /// there were), and the folder's place in the repository (`""` at the root, else `src/app/`):
    /// the prefix a change's path has before it is the workspace's key.
    Repository {
        branch: Option<String>,
        changes: Vec<Change>,
        more: usize,
        prefix: String,
    },
}

/// Runs `git` in `folder` with `args`, its output when it succeeded; `Err(None)` when there is no
/// `git` to run, `Err(Some(words))` when it ran and failed.
fn run_git(folder: &Path, args: &[&str]) -> Result<String, Option<String>> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(folder)
        .args(args)
        // A status that writes nothing: no index refresh lock under an editor that asks often.
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { None } else { Some(e.to_string()) })?;
    if !output.status.success() {
        let why = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(Some(why));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The state of the repository `folder` is in, from the `git` on this machine.
#[must_use]
pub fn status(folder: &Path) -> Status {
    let prefix = match run_git(folder, &["rev-parse", "--show-prefix"]) {
        Ok(p) => p.trim().to_string(),
        Err(None) => return Status::Unavailable("git".to_string()),
        // `git` runs and says the folder is no repository (the words differ by version).
        Err(Some(_)) => return Status::NoRepository,
    };
    let listing = match run_git(folder, &["status", "--porcelain=v1", "-z"]) {
        Ok(out) => out,
        Err(None) => return Status::Unavailable("git".to_string()),
        Err(Some(why)) => return Status::Unavailable(why),
    };
    let mut changes = parse_status(&listing);
    let more = changes.len().saturating_sub(MAX_CHANGES);
    changes.truncate(MAX_CHANGES);
    Status::Repository { branch: branch(folder), changes, more, prefix }
}

/// A change's path as a workspace key: its path less the folder's place in the repository; `None`
/// when the change lies outside the workspace's folder.
#[must_use]
pub fn workspace_key<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    path.strip_prefix(prefix).filter(|rest| !rest.is_empty())
}

#[cfg(test)]
#[path = "git_test.rs"]
mod tests;

#[cfg(test)]
#[path = "git_status_test.rs"]
mod status;
