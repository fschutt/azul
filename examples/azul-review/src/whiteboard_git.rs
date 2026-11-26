//! What the repository says about the code a board points at, read from the FILES of `.git` - git is never run (it
//! can be missing, it can ask questions, it is slow, and all that is wanted is a few words):
//!
//! - the **hash** of HEAD (a branch's file or `packed-refs`, or the hash itself when HEAD is detached), and the branch;
//! - the **remote's address** from the config, as it is (without a password or token in it) and as a web address
//!   (`https://github.com/owner/repo`), and which host it is, for the form of a permalink;
//! - the **name** that is configured (`user.name`, in the repository's config, then the global one) - never the email.
//!
//! `.git` may be a folder, or a file that points to the real one (a worktree, a submodule); a linked worktree has its
//! own HEAD and shares the refs and the config. Anything odd - a file that is too large, a hash that is not hex, a ref
//! that leaves `refs/` - is nothing: a value, not an error. Every read is bounded.
//!
//! A [`permalink`] is the address of some lines of a file at a revision, in the form of the host (GitHub, GitLab,
//! Bitbucket, or a template for another one). Pure std: no window, no azul, no process.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// The largest file of `.git` that is read, bytes: a config of a big repository is a few kilobytes.
const MAX_FILE: u64 = 256 * 1024;
/// The longest a configured name is, characters.
const MAX_NAME: usize = 200;
/// How many folders up are looked at for a `.git`.
const MAX_UP: usize = 64;

/// The host of a repository, for the form of a link to a line of its code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Host {
    GitHub,
    GitLab,
    Bitbucket,
    /// Gitea and Forgejo (Codeberg and the many installations of them).
    Gitea,
    /// A host of no known form: the GitHub form, or the one template the settings hold ([`permalink`]).
    #[default]
    Other,
}

impl Host {
    /// The name a document writes for the host.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
            Self::Bitbucket => "bitbucket",
            Self::Gitea => "gitea",
            Self::Other => "other",
        }
    }

    /// The host a document names; anything else is [`Host::Other`].
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "github" => Self::GitHub,
            "gitlab" => Self::GitLab,
            "bitbucket" => Self::Bitbucket,
            "gitea" => Self::Gitea,
            _ => Self::Other,
        }
    }
}

/// What is known of a repository.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepoFacts {
    /// The full hash of HEAD.
    pub rev: Option<String>,
    /// The branch HEAD is on (none when it is detached).
    pub branch: Option<String>,
    /// The remote's address as the config says it, without a password or token.
    pub remote_url: Option<String>,
    /// The remote as a web address: `https://github.com/owner/repo`.
    pub web_url: Option<String>,
    pub host: Host,
    /// The configured name of the person (`user.name`).
    pub author: Option<String>,
    /// Where the folder the facts were read for lies inside the repository, with `/` and without one at either end:
    /// empty when it is the repository's own folder. A file of the folder is at `prefix/file` in the repository, which is
    /// the path a link to it names.
    pub prefix: String,
    /// Whether the working tree has changes that are not committed: known only when [`dirty`] was asked.
    pub dirty: Option<bool>,
}

/// A file as text, if it is there, is not larger than `max` bytes and is text.
fn small(path: &Path, max: u64) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// Whether `text` is a hash: 40 hex digits (SHA-1) or 64 (SHA-256), lower case.
#[must_use]
pub fn is_hash(text: &str) -> bool {
    (text.len() == 40 || text.len() == 64) && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether `name` is a ref this reader follows: `refs/...` with plain segments (letters, digits, `-_./`), none empty
/// or leading to a parent.
fn is_ref(name: &str) -> bool {
    name.starts_with("refs/")
        && name.len() < 256
        && name.split('/').all(|s| !s.is_empty() && s != "." && s != ".." && !s.ends_with(".lock"))
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.'))
}

/// The folder with the working tree's `.git`, looking from `start` up.
fn find_root(start: &Path) -> Option<PathBuf> {
    let from = if start.is_file() { start.parent()? } else { start };
    from.ancestors().take(MAX_UP).find(|p| fs::symlink_metadata(p.join(".git")).is_ok()).map(Path::to_path_buf)
}

/// The real git folder of the working tree at `root`: `.git` itself, or where a `.git` file points.
fn git_dir(root: &Path) -> Option<PathBuf> {
    let dot = root.join(".git");
    if fs::metadata(&dot).ok()?.is_dir() {
        return Some(dot);
    }
    let text = small(&dot, 4096)?;
    let target = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    if target.is_empty() {
        return None;
    }
    let path = Path::new(target);
    let dir = if path.is_absolute() { path.to_path_buf() } else { root.join(path) };
    dir.is_dir().then_some(dir)
}

/// The folder a linked worktree shares its refs and config with (its own, for any other).
fn common_dir(git: &Path) -> PathBuf {
    small(&git.join("commondir"), 4096)
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .map(|t| {
            let p = Path::new(&t);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                git.join(p)
            }
        })
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| git.to_path_buf())
}

/// The hash a ref names: its file (in the worktree's folder, then the shared one), else `packed-refs`.
fn resolve(name: &str, git: &Path, common: &Path) -> Option<String> {
    if !is_ref(name) {
        return None;
    }
    for dir in [git, common] {
        if let Some(text) = small(&dir.join(name), 256) {
            let hash = text.trim();
            if is_hash(hash) {
                return Some(hash.to_string());
            }
        }
    }
    for dir in [common, git] {
        let Some(packed) = small(&dir.join("packed-refs"), MAX_FILE * 16) else {
            continue;
        };
        for line in packed.lines() {
            if line.starts_with('#') || line.starts_with('^') {
                continue;
            }
            if let Some((hash, r)) = line.split_once(' ') {
                if r.trim() == name && is_hash(hash) {
                    return Some(hash.to_string());
                }
            }
        }
    }
    None
}

/// HEAD: the hash and the branch.
fn head(git: &Path, common: &Path) -> (Option<String>, Option<String>) {
    let Some(text) = small(&git.join("HEAD"), 1024) else {
        return (None, None);
    };
    let text = text.trim();
    if let Some(name) = text.strip_prefix("ref:") {
        let name = name.trim();
        if !is_ref(name) {
            return (None, None);
        }
        let branch = name.strip_prefix("refs/heads/").map(str::to_string);
        return (resolve(name, git, common), branch);
    }
    (is_hash(text).then(|| text.to_string()), None)
}

/// A value of a git config: the `key` of `[section]` or `[section "sub"]`, in the order they come in.
fn config_values(text: &str, section: &str, key: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current: Option<(String, String)> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            let Some(header) = header.split(']').next() else {
                current = None;
                continue;
            };
            let (name, sub) = match header.split_once(char::is_whitespace) {
                Some((n, s)) => (n.trim().to_ascii_lowercase(), s.trim().trim_matches('"').to_string()),
                None => (header.trim().to_ascii_lowercase(), String::new()),
            };
            current = Some((name, sub));
            continue;
        }
        let Some((name, sub)) = &current else {
            continue;
        };
        if name != section {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim().eq_ignore_ascii_case(key) {
            out.push((sub.clone(), value_of(v)));
        }
    }
    out
}

/// A config value: quotes taken off, a comment after it cut, the ends trimmed.
fn value_of(raw: &str) -> String {
    let v = raw.trim();
    if let Some(rest) = v.strip_prefix('"') {
        return rest.split('"').next().unwrap_or_default().to_string();
    }
    let cut = [" #", "\t#", " ;", "\t;"].iter().filter_map(|c| v.find(c)).min().unwrap_or(v.len());
    v[..cut].trim().to_string()
}

/// A name that may be kept: not empty, not long, one line.
fn usable_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && name.chars().count() <= MAX_NAME && !name.chars().any(char::is_control)).then(|| name.to_string())
}

/// The remote's address with the secrets of its authority taken off: `https://user:token@host/x` is `https://host/x`
/// (an `ssh` address keeps its user, which is no secret).
#[must_use]
pub fn public_remote(url: &str) -> String {
    let url = url.trim();
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    if !matches!(scheme, "http" | "https") {
        return url.to_string();
    }
    let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
    let host = authority.rsplit('@').next().unwrap_or(authority);
    if path.is_empty() {
        format!("{scheme}://{host}")
    } else {
        format!("{scheme}://{host}/{path}")
    }
}

/// The web address of a remote (`https://host/owner/repo`) and which host it is; `None` for a path, a `file:` address or
/// anything that is not an address of a repository on a host.
#[must_use]
pub fn web_address(remote: &str) -> Option<(String, Host)> {
    let remote = remote.trim();
    if remote.is_empty() || remote.contains('\\') || remote.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let (scheme, authority, path) = if let Some((scheme, rest)) = remote.split_once("://") {
        if !matches!(scheme, "http" | "https" | "ssh" | "git") {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next().unwrap_or(authority);
        (if scheme == "http" { "http" } else { "https" }, host.split(':').next().unwrap_or(host).to_string(), path.to_string())
    } else {
        // `user@host:owner/repo`
        let (head, path) = remote.split_once(':')?;
        if head.contains('/') {
            return None;
        }
        let host = head.rsplit('@').next().unwrap_or(head);
        ("https", host.to_string(), path.to_string())
    };
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path).trim_end_matches('/');
    let host_ok = authority.len() >= 2
        && authority.contains('.')
        && authority.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    let segments: Vec<&str> = path.split('/').collect();
    if !host_ok || segments.len() < 2 || segments.iter().any(|s| s.is_empty() || *s == "." || *s == "..") {
        return None;
    }
    let host = match authority.to_ascii_lowercase().as_str() {
        "github.com" => Host::GitHub,
        "gitlab.com" => Host::GitLab,
        "bitbucket.org" => Host::Bitbucket,
        "codeberg.org" => Host::Gitea,
        h if h.starts_with("gitlab.") => Host::GitLab,
        h if h.starts_with("gitea.") || h.starts_with("forgejo.") || h.starts_with("git.gitea.") => Host::Gitea,
        _ => Host::Other,
    };
    Some((format!("{scheme}://{authority}/{path}"), host))
}

/// The remotes in the output of `git remote -v` (`name<TAB>url (fetch)` and `(push)` lines): each remote once, in the
/// order they come, with its fetch address (the push one when there is no fetch line). The address is as git gives it,
/// with `insteadOf` rewrites applied, and without a password or token ([`public_remote`]).
#[must_use]
pub fn parse_remote_v(output: &str) -> Vec<(String, String)> {
    let mut remotes: Vec<(String, String, bool)> = Vec::new();
    for line in output.lines().take(1_000) {
        let Some((name, rest)) = line.trim_end_matches('\r').split_once('\t') else {
            continue;
        };
        let rest = rest.trim();
        let (url, fetch) = if let Some(u) = rest.strip_suffix("(fetch)") {
            (u.trim(), true)
        } else if let Some(u) = rest.strip_suffix("(push)") {
            (u.trim(), false)
        } else {
            (rest, true)
        };
        if name.is_empty() || url.is_empty() || name.len() > 100 || url.len() > 2_000 {
            continue;
        }
        match remotes.iter_mut().find(|(n, _, _)| n == name) {
            Some(existing) => {
                if fetch && !existing.2 {
                    existing.1 = public_remote(url);
                    existing.2 = true;
                }
            }
            None => remotes.push((name.to_string(), public_remote(url), fetch)),
        }
    }
    remotes.into_iter().map(|(n, u, _)| (n, u)).collect()
}

/// The remote a board's links are made from: `origin`, else the first one; `prefer` (the person's choice in the export
/// dialog) first when it is among them.
#[must_use]
pub fn choose_remote<'a>(remotes: &'a [(String, String)], prefer: Option<&str>) -> Option<&'a (String, String)> {
    prefer
        .and_then(|p| remotes.iter().find(|(n, _)| n == p))
        .or_else(|| remotes.iter().find(|(n, _)| n == "origin"))
        .or_else(|| remotes.first())
}

/// The repository's facts for the code at `start` (a folder, or a file in a working tree); `home` is where the global
/// config is looked for (`.gitconfig`). `None` when there is no repository.
#[must_use]
pub fn read(start: &Path, home: Option<&Path>) -> Option<RepoFacts> {
    let root = find_root(start)?;
    let git = git_dir(&root)?;
    let common = common_dir(&git);
    let (rev, branch) = head(&git, &common);
    let config = small(&common.join("config"), MAX_FILE).unwrap_or_default();
    let urls = config_values(&config, "remote", "url");
    let remote = urls.iter().find(|(name, _)| name == "origin").or_else(|| urls.first()).map(|(_, u)| public_remote(u));
    let (web_url, host) = remote.as_deref().and_then(web_address).map_or((None, Host::Other), |(w, h)| (Some(w), h));
    let mut names = config_values(&config, "user", "name");
    if names.is_empty() {
        for file in [".gitconfig", ".config/git/config"] {
            if let Some(h) = home {
                names = small(&h.join(file), MAX_FILE).map(|t| config_values(&t, "user", "name")).unwrap_or_default();
                if !names.is_empty() {
                    break;
                }
            }
        }
    }
    let author = names.last().and_then(|(_, n)| usable_name(n));
    let prefix = prefix_of(start, &root);
    Some(RepoFacts { rev, branch, remote_url: remote.filter(|r| !r.is_empty()), web_url, host, author, prefix, dirty: None })
}

/// Where `start` (a folder, or a file of one) lies inside the repository at `root`: the folder's path below the root,
/// with `/`; empty at the root (or for anything odd).
fn prefix_of(start: &Path, root: &Path) -> String {
    let from = if start.is_file() { start.parent().unwrap_or(start) } else { start };
    let Ok(below) = from.strip_prefix(root) else {
        return String::new();
    };
    below
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// A path of the repository as it goes into a link: the characters that mean something in an address are written as
/// `%XX`, the slashes stay. `None` for an empty path, an absolute one, or one that goes to a parent.
fn link_path(path: &str) -> Option<String> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return None;
    }
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    Some(out)
}

/// The address of lines `start` to `end` (from 1) of `path` (relative to the repository's root) at revision `rev`, in the
/// form of `host`; for a host of no known form the `template` ({web}, {rev}, {path}, {start}, {end}), without one the
/// GitHub form. `None` for a revision that is no hash, a path that is none, a web address that is none.
#[must_use]
pub fn permalink(web_url: &str, host: Host, rev: &str, path: &str, start: u32, end: u32, template: Option<&str>) -> Option<String> {
    if !is_hash(rev) || !(web_url.starts_with("https://") || web_url.starts_with("http://")) {
        return None;
    }
    let path = link_path(path)?;
    let (a, b) = if start <= end { (start.max(1), end.max(1)) } else { (end.max(1), start.max(1)) };
    let web = web_url.trim_end_matches('/');
    Some(match host {
        Host::GitHub => {
            let lines = if a == b { format!("L{a}") } else { format!("L{a}-L{b}") };
            format!("{web}/blob/{rev}/{path}#{lines}")
        }
        Host::GitLab => {
            let lines = if a == b { format!("L{a}") } else { format!("L{a}-{b}") };
            format!("{web}/-/blob/{rev}/{path}#{lines}")
        }
        Host::Bitbucket => {
            let lines = if a == b { format!("lines-{a}") } else { format!("lines-{a}:{b}") };
            format!("{web}/src/{rev}/{path}#{lines}")
        }
        Host::Gitea => {
            let lines = if a == b { format!("L{a}") } else { format!("L{a}-L{b}") };
            format!("{web}/src/commit/{rev}/{path}#{lines}")
        }
        // A host of no known form: the template of the settings, and without one the GitHub form (most hosts that
        // are none of the others copy it).
        Host::Other => match template {
            Some(t) => t
                .replace("{web}", web)
                .replace("{rev}", rev)
                .replace("{path}", &path)
                .replace("{start}", &a.to_string())
                .replace("{end}", &b.to_string()),
            None => {
                let lines = if a == b { format!("L{a}") } else { format!("L{a}-L{b}") };
                format!("{web}/blob/{rev}/{path}#{lines}")
            }
        },
    })
}

// ---- `git remote -v`, once, when a board is exported

/// How much of the output of git is read, bytes: `git remote -v` is a few lines.
const MAX_OUTPUT: u64 = 64 * 1024;

/// Whether `path` is a program a person can run (a file with an execute bit; on Windows a file).
fn is_program(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Where git is, found by looking at files only - a stub is never run to find out: the first `git` in `dirs` that is a
/// program. On a Mac `/usr/bin/git` is a stub that opens a dialog offering to install the command line tools when they are
/// not there, so it counts only when they are (`tools_present`).
#[must_use]
pub fn find_git(dirs: &[PathBuf], tools_present: bool) -> Option<PathBuf> {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    dirs.iter().map(|d| d.join(name)).find(|p| is_program(p) && (tools_present || p != Path::new("/usr/bin/git")))
}

/// Whether the command line tools (or Xcode) are installed, which is what makes `/usr/bin/git` a git (macOS; elsewhere
/// there is no stub).
#[must_use]
pub fn tools_present() -> bool {
    !cfg!(target_os = "macos")
        || ["/Library/Developer/CommandLineTools/usr/bin/git", "/Applications/Xcode.app/Contents/Developer/usr/bin/git"]
            .iter()
            .any(|p| is_program(Path::new(p)))
}

/// Runs `program remote -v` in the working tree at `root` ([`run_git`]).
#[must_use]
pub fn run_remote_v(program: &Path, root: &Path, timeout: Duration) -> Option<String> {
    run_git(program, root, &["remote", "-v"], timeout)
}

/// Runs `program args` in the working tree at `root` and gives what it printed, waiting `timeout` at most (the
/// process is killed then). Nothing is asked of the person (`GIT_TERMINAL_PROMPT=0`), no optional lock is taken, no
/// pager, no input, no network is needed; what it says on its error stream is dropped. `None` for a program that
/// could not be run, did not end in time, or failed.
#[must_use]
pub fn run_git(program: &Path, root: &Path, args: &[&str], timeout: Duration) -> Option<String> {
    let mut child = Command::new(program)
        .arg("--no-pager")
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // What it prints is read while it runs (a program that prints more than a pipe holds would wait for ever for a
    // reader): the first [`MAX_OUTPUT`] bytes are kept, the rest is thrown away as it comes.
    let mut stdout = child.stdout.take()?;
    let (sent, received) = std::sync::mpsc::channel();
    let _reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _read = (&mut stdout).take(MAX_OUTPUT).read_to_end(&mut bytes);
        let _sent = sent.send(String::from_utf8_lossy(&bytes).into_owned());
        let _drained = std::io::copy(&mut stdout, &mut std::io::sink());
    });
    let began = Instant::now();
    let ended = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if began.elapsed() < timeout => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _killed = child.kill();
                let _reaped = child.wait();
                break false;
            }
        }
    };
    if !ended {
        // (Whatever still holds the pipe - a child of the program that was killed - ends the reader when it ends; it is not waited for.)
        return None;
    }
    received.recv_timeout(Duration::from_secs(1)).ok()
}

/// The remotes of the repository at `start`, as git says them now (with `insteadOf` rewrites), by running `git remote -v`
/// once; when git is not there, or does not answer, the remotes of the config files ([`config_remotes`]). Call it off the UI thread.
#[must_use]
pub fn remotes(start: &Path, timeout: Duration) -> Vec<(String, String)> {
    let Some(root) = find_root(start) else {
        return Vec::new();
    };
    if let Some(text) = git_program().and_then(|g| run_remote_v(&g, &root, timeout)) {
        let found = parse_remote_v(&text);
        if !found.is_empty() {
            return found;
        }
    }
    config_remotes(start)
}

/// The remotes the repository's config files name, in order, each with its address (without a password or token): what is
/// known when git cannot be asked.
#[must_use]
pub fn config_remotes(start: &Path) -> Vec<(String, String)> {
    let Some(root) = find_root(start) else {
        return Vec::new();
    };
    let Some(git) = git_dir(&root) else {
        return Vec::new();
    };
    let config = small(&common_dir(&git).join("config"), MAX_FILE).unwrap_or_default();
    let mut out: Vec<(String, String)> = Vec::new();
    for (name, url) in config_values(&config, "remote", "url") {
        if !name.is_empty() && !url.is_empty() && !out.iter().any(|(n, _)| *n == name) {
            out.push((name, public_remote(&url)));
        }
    }
    out
}

/// The git program: the first one on the `PATH` that is a program ([`find_git`]).
fn git_program() -> Option<PathBuf> {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .and_then(|dirs| find_git(&dirs, tools_present()))
}

/// Whether the output of `git status --porcelain` names a change: any line at all.
#[must_use]
pub fn parse_dirty(output: &str) -> bool {
    output.lines().any(|l| !l.trim().is_empty())
}

/// Whether the working tree at `start` has changes that are not committed (files git tracks; new files do not count),
/// by running `git status` once; `None` when there is no repository, git is not there, or it does not answer in time.
/// Call it off the UI thread.
#[must_use]
pub fn dirty(start: &Path, timeout: Duration) -> Option<bool> {
    let root = find_root(start)?;
    let text = run_git(&git_program()?, &root, &["status", "--porcelain=v1", "--untracked-files=no"], timeout)?;
    Some(parse_dirty(&text))
}

#[cfg(test)]
#[path = "whiteboard_git_test.rs"]
mod whiteboard_git_test;
