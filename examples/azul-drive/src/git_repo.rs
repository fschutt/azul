//! The git side of a folder on this computer, read through gix (gitoxide, pure Rust): which
//! repository holds it, the branch it is on (or the commit of a detached HEAD), whether its work
//! tree has uncommitted changes, how far the branch is ahead of and behind its upstream, the
//! local branches for the quick switcher, and whether a switch to another branch would keep the
//! changes or lose them. No azul types and no git program: `ui_git` runs these on a worker
//! thread and the address row shows the answer.
//!
//! Nothing here writes to a repository. gix's status could refresh the index's file times; its
//! changes are never written back, so a look never wakes a watcher of `.git`.
//!
//! A huge repository stays cheap ([`Limits`]): past `max_index_entries` tracked files the
//! change scan is skipped, a scan that runs past `deadline` is stopped (gix checks an interrupt
//! flag a watchdog sets), and the ahead / behind walks stop after `walk_cap` commits. What could
//! not be told is said ([`Changes::Unknown`]), never guessed.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, SystemTime},
};

/// How much a look at a repository may cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Tracked files (index entries) past which the change scan is not run.
    pub max_index_entries: usize,
    /// How long the change scan may run; zero stops it before it starts (the tests).
    pub deadline: Duration,
    /// The changed paths kept for the check before a switch; past it the check cannot tell.
    pub max_paths: usize,
    /// The commits each ahead / behind walk counts at most.
    pub walk_cap: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_index_entries: 150_000,
            deadline: Duration::from_secs(3),
            max_paths: 20_000,
            walk_cap: 1_000,
        }
    }
}

/// How often a quick repository is looked at again where no cheap watcher of its work tree
/// exists (`ui_git`: every system but macOS).
pub const POLL_EVERY: Duration = Duration::from_secs(10);
/// A look that took this long or longer is not repeated by that poll: the repository is looked
/// at on the events alone (another folder, F5, a command, a ref or index write).
pub const POLL_MAX_LOOK: Duration = Duration::from_millis(500);

/// Whether a repository whose last look took `took` and ended `since` ago is due for another
/// look by the poll.
#[must_use]
pub fn poll_due(took: Duration, since: Duration) -> bool {
    took < POLL_MAX_LOOK && since >= POLL_EVERY
}

/// The hex digits a detached HEAD's commit is shown with (git's default short id).
pub const SHORT_ID: usize = 7;

/// What HEAD names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    /// A branch with commits (its short name: `main`, `feature/login`).
    Branch(String),
    /// A branch without a commit yet (a new repository).
    Unborn(String),
    /// A commit and no branch: its short id.
    Detached(String),
}

impl Head {
    /// The branch, unless HEAD is detached.
    #[must_use]
    pub fn branch(&self) -> Option<&str> {
        match self {
            Head::Branch(name) | Head::Unborn(name) => Some(name),
            Head::Detached(_) => None,
        }
    }
}

/// Why the uncommitted changes are not known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unknown {
    /// More tracked files than [`Limits::max_index_entries`]: the scan was not run.
    TooBig,
    /// The scan ran past [`Limits::deadline`] and was stopped.
    TimedOut,
    /// gix could not read the index or the work tree; its words.
    Failed(String),
}

/// The uncommitted changes of a work tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Changes {
    /// A bare repository: there is no work tree to change.
    NoWorkTree,
    /// Nothing uncommitted.
    Clean,
    /// `changed` tracked files differ from the last commit (staged or not) and `untracked`
    /// files or folders are new to git (a new folder counts once).
    Dirty { changed: usize, untracked: usize },
    /// Not looked at, or the look did not finish.
    Unknown(Unknown),
}

/// How far the branch is from its upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AheadBehind {
    /// Commits of the branch the upstream does not have.
    pub ahead: usize,
    /// Commits of the upstream the branch does not have.
    pub behind: usize,
    /// A walk stopped at [`Limits::walk_cap`]: the counts are "at least".
    pub capped: bool,
}

/// What the address row's branch chip says about the repository holding a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub head: Head,
    /// The work tree's root; `None` for a bare repository.
    pub work_dir: Option<PathBuf>,
    /// The repository's own folder (`.git`, a linked worktree's `.git/worktrees/<name>`).
    pub git_dir: PathBuf,
    /// The folder the worktrees share (refs, objects); `git_dir` for an ordinary repository.
    pub common_dir: PathBuf,
    /// A linked worktree (`git worktree add`): its `.git` is a file naming its git folder.
    pub linked_worktree: bool,
    pub changes: Changes,
    /// The upstream's short name (`origin/main`), when the branch has one.
    pub upstream: Option<String>,
    /// Ahead / behind the upstream; `None` without one (or when its commit is missing).
    pub ahead_behind: Option<AheadBehind>,
}

impl RepoInfo {
    /// A bare repository (no work tree).
    #[must_use]
    pub fn bare(&self) -> bool {
        self.work_dir.is_none()
    }

    /// The files whose change means the chip is out of date: HEAD and the index of this work
    /// tree, the packed refs, the branch's ref and its upstream's, the last fetch. Edits in the
    /// work tree itself are a watcher's (`ui_git`).
    #[must_use]
    pub fn watched_files(&self) -> Vec<PathBuf> {
        let mut files = vec![
            self.git_dir.join("HEAD"),
            self.git_dir.join("index"),
            self.common_dir.join("packed-refs"),
            self.common_dir.join("FETCH_HEAD"),
        ];
        if let Some(branch) = self.head.branch() {
            files.push(self.common_dir.join("refs").join("heads").join(branch));
        }
        if let Some(upstream) = &self.upstream {
            files.push(self.common_dir.join("refs").join("remotes").join(upstream));
        }
        files
    }
}

/// The modification time and length of each of `files` (`None`: not there): two stamps differ
/// when one of the files was written, created or removed. The length catches a rewrite within
/// the file system's time resolution (HEAD going from `main` to `other`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp(Vec<Option<(SystemTime, u64)>>);

/// [`Stamp`] of `files` now.
#[must_use]
pub fn stamp(files: &[PathBuf]) -> Stamp {
    Stamp(
        files
            .iter()
            .map(|file| {
                let meta = std::fs::metadata(file).ok()?;
                Some((meta.modified().ok()?, meta.len()))
            })
            .collect(),
    )
}

/// Whether `dir` itself looks like a git folder: what a bare repository is.
fn is_git_dir(dir: &Path) -> bool {
    dir.join("HEAD").is_file() && dir.join("objects").is_dir() && dir.join("refs").is_dir()
}

/// The nearest of `dir` and its parents a repository holds: one with a `.git` (a folder, or the
/// file of a linked worktree or a submodule), or a git folder itself (a bare repository). Cheap
/// (a few `stat`s per level): most folders are in no repository, and gix is not asked about
/// them at all.
#[must_use]
pub fn holder_of(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|level| level.join(".git").exists() || is_git_dir(level))
        .map(Path::to_path_buf)
}

/// The repository holding `dir`, as the chip shows it; `None` when no repository holds it.
///
/// # Errors
/// A repository is there but gix cannot read it (a damaged HEAD, an unknown format): its words.
pub fn probe(dir: &Path, limits: &Limits) -> Result<Option<RepoInfo>, String> {
    if holder_of(dir).is_none() {
        return Ok(None);
    }
    let repo = gix::discover(dir).map_err(|e| e.to_string())?;
    let head = head_of(&repo)?;
    let work_dir = repo.workdir().map(Path::to_path_buf);
    let changes = if work_dir.is_none() {
        Changes::NoWorkTree
    } else {
        match scan(&repo, limits) {
            Ok(found) if found.is_empty() => Changes::Clean,
            Ok(found) => Changes::Dirty {
                changed: found.changed,
                untracked: found.untracked,
            },
            Err(why) => Changes::Unknown(why),
        }
    };
    let (upstream, ahead_behind) = match &head {
        Head::Branch(_) => upstream_of(&repo, limits.walk_cap),
        Head::Unborn(_) | Head::Detached(_) => (None, None),
    };
    Ok(Some(RepoInfo {
        head,
        work_dir,
        git_dir: repo.git_dir().to_path_buf(),
        common_dir: repo.common_dir().to_path_buf(),
        linked_worktree: matches!(repo.kind(), gix::repository::Kind::LinkedWorkTree),
        changes,
        upstream,
        ahead_behind,
    }))
}

/// HEAD of `repo` as the chip names it.
fn head_of(repo: &gix::Repository) -> Result<Head, String> {
    let head = repo.head().map_err(|e| e.to_string())?;
    Ok(match &head.kind {
        gix::head::Kind::Symbolic(reference) => Head::Branch(reference.name.shorten().to_string()),
        gix::head::Kind::Unborn(name) => Head::Unborn(name.shorten().to_string()),
        gix::head::Kind::Detached { target, peeled } => {
            Head::Detached(short_id((*peeled).unwrap_or(*target)))
        }
    })
}

/// A commit's short id (`a1b2c3d`).
fn short_id(id: gix::ObjectId) -> String {
    id.to_hex_with_len(SHORT_ID).to_string()
}

/// What a change scan found.
#[derive(Debug, Default)]
struct Found {
    /// Tracked files that differ from the last commit, in the index or in the work tree.
    changed: usize,
    /// Files and folders git does not track (not ignored).
    untracked: usize,
    /// Their paths (relative to the work tree, `/`-separated), up to [`Limits::max_paths`].
    tracked_paths: BTreeSet<String>,
    untracked_paths: BTreeSet<String>,
    /// More paths changed than were kept.
    truncated: bool,
}

impl Found {
    fn is_empty(&self) -> bool {
        self.changed == 0 && self.untracked == 0
    }
}

/// Sets `flag` when `after` has passed, unless dropped first: the deadline of a change scan.
/// Dropping it hangs up its channel, which ends the watchdog thread at once.
struct Deadline {
    _hang_up: Option<mpsc::Sender<()>>,
}

impl Deadline {
    fn start(after: Duration, flag: Arc<AtomicBool>) -> Deadline {
        if after.is_zero() {
            flag.store(true, Ordering::Release);
            return Deadline { _hang_up: None };
        }
        let (hang_up, wait) = mpsc::channel::<()>();
        let spawned = std::thread::Builder::new()
            .name(String::from("git-scan-deadline"))
            .spawn(move || {
                if let Err(mpsc::RecvTimeoutError::Timeout) = wait.recv_timeout(after) {
                    flag.store(true, Ordering::Release);
                }
            });
        // Without a watchdog the scan has no deadline; the size cap still holds.
        Deadline {
            _hang_up: spawned.ok().map(|_| hang_up),
        }
    }
}

/// The uncommitted changes of `repo`'s work tree within `limits`: the index against HEAD's tree
/// (staged) and the work tree against the index (edited, deleted, new), the ignore rules
/// applied, new folders collapsed to one entry as `git status` shows them.
fn scan(repo: &gix::Repository, limits: &Limits) -> Result<Found, Unknown> {
    let entries = repo
        .index_or_empty()
        .map_err(|e| Unknown::Failed(e.to_string()))?
        .entries()
        .len();
    if entries > limits.max_index_entries {
        return Err(Unknown::TooBig);
    }
    let interrupt = Arc::new(AtomicBool::new(false));
    let _deadline = Deadline::start(limits.deadline, interrupt.clone());
    if interrupt.load(Ordering::Acquire) {
        return Err(Unknown::TimedOut);
    }
    let iter = repo
        .status(gix::progress::Discard)
        .map_err(|e| Unknown::Failed(e.to_string()))?
        .untracked_files(gix::status::UntrackedFiles::Collapsed)
        .tree_index_track_renames(gix::status::tree_index::TrackRenames::Disabled)
        .should_interrupt_owned(interrupt.clone())
        .into_iter(Vec::<gix::bstr::BString>::new());
    let iter = match iter {
        Ok(iter) => iter,
        Err(_) if interrupt.load(Ordering::Acquire) => return Err(Unknown::TimedOut),
        Err(e) => return Err(Unknown::Failed(e.to_string())),
    };
    let mut found = Found::default();
    let mut tracked_seen: BTreeSet<String> = BTreeSet::new();
    for item in iter {
        let item = match item {
            Ok(item) => item,
            Err(_) if interrupt.load(Ordering::Acquire) => return Err(Unknown::TimedOut),
            Err(e) => return Err(Unknown::Failed(e.to_string())),
        };
        let untracked = match &item {
            gix::status::Item::IndexWorktree(change) => match change.summary() {
                // A stat-only difference (a file touched, its content the same): no change.
                None => continue,
                Some(gix::status::index_worktree::iter::Summary::Added) => true,
                Some(_) => false,
            },
            gix::status::Item::TreeIndex(_) => false,
        };
        let path = item.location().to_string();
        if untracked {
            found.untracked += 1;
            keep_path(&mut found.untracked_paths, &mut found.truncated, path, limits);
        } else if tracked_seen.insert(path.clone()) {
            // A file both staged and edited again is one changed file.
            found.changed += 1;
            keep_path(&mut found.tracked_paths, &mut found.truncated, path, limits);
        }
    }
    if interrupt.load(Ordering::Acquire) {
        return Err(Unknown::TimedOut);
    }
    Ok(found)
}

fn keep_path(paths: &mut BTreeSet<String>, truncated: &mut bool, path: String, limits: &Limits) {
    if paths.len() < limits.max_paths {
        paths.insert(path);
    } else {
        *truncated = true;
    }
}

/// The upstream of HEAD's branch (its short name) and how far the branch is from it.
fn upstream_of(repo: &gix::Repository, cap: usize) -> (Option<String>, Option<AheadBehind>) {
    let Ok(Some(branch)) = repo.head_name() else {
        return (None, None);
    };
    let Some(Ok(upstream)) =
        repo.branch_remote_tracking_ref_name(branch.as_ref(), gix::remote::Direction::Fetch)
    else {
        return (None, None);
    };
    let short = upstream.shorten().to_string();
    let full = upstream.as_bstr().to_string();
    let theirs = repo
        .try_find_reference(full.as_str())
        .ok()
        .flatten()
        .and_then(|mut reference| reference.peel_to_id().ok())
        .map(|id| id.detach());
    let ours = repo.head_id().ok().map(|id| id.detach());
    let (Some(ours), Some(theirs)) = (ours, theirs) else {
        return (Some(short), None);
    };
    let ahead = count_between(repo, ours, theirs, cap);
    let behind = count_between(repo, theirs, ours, cap);
    let counts = match (ahead, behind) {
        (Some((ahead, a_capped)), Some((behind, b_capped))) => Some(AheadBehind {
            ahead,
            behind,
            capped: a_capped || b_capped,
        }),
        _ => None,
    };
    (Some(short), counts)
}

/// The commits reachable from `from` but not from `without` (`git rev-list --count
/// without..from`), at most `cap`: the count, and whether the cap stopped it.
fn count_between(
    repo: &gix::Repository,
    from: gix::ObjectId,
    without: gix::ObjectId,
    cap: usize,
) -> Option<(usize, bool)> {
    if from == without {
        return Some((0, false));
    }
    let walk = repo.rev_walk([from]).with_hidden([without]).all().ok()?;
    let mut count = 0;
    for info in walk {
        info.ok()?;
        count += 1;
        if count >= cap {
            return Some((cap, true));
        }
    }
    Some((count, false))
}

/// A local branch for the quick switcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// Its short name (`feature/login`).
    pub name: String,
    /// Its newest commit's time (seconds since 1970; 0 without a commit).
    pub tip_time: i64,
    /// HEAD is on it.
    pub current: bool,
}

/// The local branches of the repository holding `dir`: the current one first, then the newest
/// commit first (`git branch --sort=-committerdate`), then by name. A new repository's unborn
/// branch is listed as the current one.
///
/// # Errors
/// No repository holds `dir`, or gix cannot read its refs.
pub fn branches(dir: &Path) -> Result<Vec<Branch>, String> {
    let repo = gix::discover(dir).map_err(|e| e.to_string())?;
    let current = head_of(&repo)?.branch().map(str::to_string);
    let platform = repo.references().map_err(|e| e.to_string())?;
    let mut list = Vec::new();
    for reference in platform.local_branches().map_err(|e| e.to_string())? {
        // A ref that cannot be read is left out, not the whole list.
        let Ok(mut reference) = reference else {
            continue;
        };
        let name = reference.name().shorten().to_string();
        let tip_time = reference
            .peel_to_commit()
            .ok()
            .and_then(|commit| commit.time().ok())
            .map_or(0, |time| time.seconds);
        list.push(Branch {
            current: current.as_deref() == Some(name.as_str()),
            name,
            tip_time,
        });
    }
    if let Some(current) = current {
        if !list.iter().any(|b| b.name == current) {
            list.push(Branch {
                name: current,
                tip_time: 0,
                current: true,
            });
        }
    }
    sort_branches(&mut list);
    Ok(list)
}

/// The switcher's order: the current branch, then the newest tip, then by name.
pub fn sort_branches(list: &mut [Branch]) {
    list.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then(b.tip_time.cmp(&a.tip_time))
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// The branches whose name holds every term typed (case and accents folded, as the palette
/// matches), in their order.
#[must_use]
pub fn filter_branches<'a>(list: &'a [Branch], query: &str) -> Vec<&'a Branch> {
    list.iter()
        .filter(|b| crate::rank::name_matches(&b.name, query))
        .collect()
}

/// What a switch to another branch would do with the work tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchPlan {
    /// HEAD is on that branch already.
    Current,
    /// No such local branch.
    NoSuchBranch,
    /// A bare repository: there is no work tree to switch.
    NoWorkTree,
    /// Nothing uncommitted: a plain switch.
    Clean,
    /// Uncommitted changes in files the branch leaves as they are: git switches and they come
    /// along (`changed` tracked files, `untracked` new ones).
    Carry { changed: usize, untracked: usize },
    /// Uncommitted changes in files the branch has differently (or new files it would
    /// overwrite): git refuses. The first files, and how many more.
    Blocked { files: Vec<String>, more: usize },
    /// The changes could not be checked (size cap, deadline, too many paths, an error).
    Unknown(Unknown),
}

/// Files a blocked switch names; the rest are counted.
const BLOCKED_SHOWN: usize = 8;

/// What switching the repository holding `dir` to the local branch `target` would do with its
/// uncommitted changes, by git's rule: a switch keeps local changes when none of their files
/// differs between the two commits, and refuses otherwise (and when a new file would be
/// overwritten). The check is on paths only: where git could still carry a change (the edit
/// equals the branch's version), this says Blocked - it never promises a switch git refuses.
///
/// # Errors
/// No repository holds `dir`, or gix cannot read it.
pub fn plan_switch(dir: &Path, target: &str, limits: &Limits) -> Result<SwitchPlan, String> {
    let repo = gix::discover(dir).map_err(|e| e.to_string())?;
    if head_of(&repo)?.branch() == Some(target) {
        return Ok(SwitchPlan::Current);
    }
    let full = format!("refs/heads/{target}");
    let Some(mut reference) = repo
        .try_find_reference(full.as_str())
        .map_err(|e| e.to_string())?
    else {
        return Ok(SwitchPlan::NoSuchBranch);
    };
    if repo.workdir().is_none() {
        return Ok(SwitchPlan::NoWorkTree);
    }
    let found = match scan(&repo, limits) {
        Ok(found) => found,
        Err(why) => return Ok(SwitchPlan::Unknown(why)),
    };
    if found.is_empty() {
        return Ok(SwitchPlan::Clean);
    }
    if found.truncated {
        return Ok(SwitchPlan::Unknown(Unknown::TooBig));
    }
    let theirs = reference.peel_to_tree().map_err(|e| e.to_string())?;
    // A branch without commits has the empty tree.
    let ours = repo.head_tree().ok();
    let differ = repo
        .diff_tree_to_tree(ours.as_ref(), &theirs, gix::diff::Options::default())
        .map_err(|e| e.to_string())?;
    let mut differing: BTreeSet<String> = BTreeSet::new();
    for change in &differ {
        differing.insert(change.location().to_string());
        differing.insert(change.source_location().to_string());
    }
    let blocked: Vec<String> = found
        .tracked_paths
        .iter()
        .chain(found.untracked_paths.iter())
        .filter(|path| differing.iter().any(|other| paths_overlap(path, other)))
        .cloned()
        .collect();
    if blocked.is_empty() {
        return Ok(SwitchPlan::Carry {
            changed: found.changed,
            untracked: found.untracked,
        });
    }
    let more = blocked.len().saturating_sub(BLOCKED_SHOWN);
    Ok(SwitchPlan::Blocked {
        files: blocked.into_iter().take(BLOCKED_SHOWN).collect(),
        more,
    })
}

/// Whether two repository paths (`/`-separated) name the same file or one lies in the other (a
/// new folder collapsed to its name holds every file under it).
#[must_use]
pub fn paths_overlap(a: &str, b: &str) -> bool {
    let a = a.trim_end_matches('/');
    let b = b.trim_end_matches('/');
    let under = |inner: &str, outer: &str| {
        inner.len() > outer.len()
            && inner.starts_with(outer)
            && inner.as_bytes()[outer.len()] == b'/'
    };
    a == b || under(a, b) || under(b, a)
}

/// The chip's words for a repository: the branch (or the short commit), whether to show the
/// dirty dot, whether the changes are unknown, and the counts against the upstream (`↑2 ↓1`,
/// `↑1000+` at the cap; empty without an upstream or when nothing is apart).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipText {
    pub name: String,
    pub detached: bool,
    pub dirty: bool,
    pub unknown: bool,
    pub counts: String,
}

/// [`ChipText`] of `info`.
#[must_use]
pub fn chip_text(info: &RepoInfo) -> ChipText {
    let (name, detached) = match &info.head {
        Head::Branch(name) | Head::Unborn(name) => (name.clone(), false),
        Head::Detached(id) => (id.clone(), true),
    };
    let counts = match info.ahead_behind {
        Some(ab) if ab.ahead > 0 || ab.behind > 0 => {
            let plus = if ab.capped { "+" } else { "" };
            let mut parts = Vec::new();
            if ab.ahead > 0 {
                parts.push(format!("\u{2191}{}{plus}", ab.ahead));
            }
            if ab.behind > 0 {
                parts.push(format!("\u{2193}{}{plus}", ab.behind));
            }
            parts.join(" ")
        }
        _ => String::new(),
    };
    ChipText {
        name,
        detached,
        dirty: matches!(info.changes, Changes::Dirty { .. }),
        unknown: matches!(info.changes, Changes::Unknown(_)),
        counts,
    }
}
