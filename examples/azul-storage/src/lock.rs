//! Locks every process of this user shares (ARCH28 RF47; moved down from azcloud-kit so a document
//! of this crate can take one): a folder (an app's is beside its drives file,
//! `<config dir>/azul-storage/locks`) with one file per lock, held with the OS's own file lock
//! (`flock` on Unix, `LockFileEx` on Windows - `std::fs::File::try_lock`), which the OS drops
//! when its process ends: a crash leaves no stale lock to wait out. Threads of one process take
//! turns as well (a lock is held once per path in the process too), whatever the file system
//! makes of two locks of one process.
//!
//! What they guard: an Azlin drive's session while its drive token is spent and the next one
//! stored (two windows, two apps never spend one token twice: the token server would revoke
//! the device), and the list of unfinished checkouts while it is changed. A lock file is never
//! removed - a waiter that opened it before a removal would hold a lock of a file nobody else
//! locks any more.

use std::{
    collections::BTreeSet,
    fmt,
    fs::{self, File, TryLockError},
    path::{Path, PathBuf},
    sync::{Condvar, Mutex, PoisonError},
    time::{Duration, Instant},
};

use crate::key::System;

/// Why a lock was not taken: held by someone for longer than the caller waits, or its file could
/// not be made or locked. The sentence names the lock's file, never a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockError {
    kind: LockFailure,
    text: String,
}

/// What kind of failure a [`LockError`] is: a caller decides on this, never on the sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockFailure {
    /// Held by someone else for longer than the caller waits (waiting longer may help).
    Busy,
    /// The file system, or the system, has no file locks (a network share, a FUSE mount):
    /// `flock` answered "not supported" or "no locks available".
    Unsupported,
    /// The lock's folder or file could not be made, or the lock failed for another reason.
    Failed,
}

impl LockError {
    fn new(kind: LockFailure, text: impl Into<String>) -> LockError {
        LockError {
            kind,
            text: text.into(),
        }
    }

    /// What kind of failure it is.
    #[must_use]
    pub fn kind(&self) -> LockFailure {
        self.kind
    }

    /// Whether the lock was held by someone else (waiting longer may help) and not broken.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.kind == LockFailure::Busy
    }

    /// Whether the file system has no file locks at all.
    #[must_use]
    pub fn is_unsupported(&self) -> bool {
        self.kind == LockFailure::Unsupported
    }
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Whether `e`, the answer of a file lock, says that there are no file locks here: the file system
/// (a network share, a FUSE mount) or the system does not do them. Not a lock somebody holds, and
/// not a file that cannot be had.
#[must_use]
pub(crate) fn lock_unsupported(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(unix)]
    if let Some(code) = e.raw_os_error() {
        // Compared with `||`: on some systems two of the names are one number.
        return code == libc::ENOLCK
            || code == libc::ENOTSUP
            || code == libc::EOPNOTSUPP
            || code == libc::ENOSYS;
    }
    false
}

/// The lock files this process went on without, because their file system has no file locks: the
/// threads of this process took turns, another program was not kept out.
static UNLOCKED: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// The lock files that were taken among the threads of this process only, on a file system with no
/// file locks ([`LockDir::lock_best_effort`]).
#[must_use]
pub fn unlocked_files() -> Vec<PathBuf> {
    UNLOCKED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .cloned()
        .collect()
}

/// Notes (and says once, on stderr) that `path` could not be locked at the system's level.
fn note_unlocked(path: &Path, why: &LockError) {
    let first = UNLOCKED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(path.to_path_buf());
    if first {
        eprintln!(
            "[azul-storage] {why}: this file system has no file locks, so two programs of this user              changing the same document at once are not kept apart (the threads of this one are)"
        );
    }
}

impl std::error::Error for LockError {}

/// How long a waiter sleeps between two tries of another process's lock.
const RETRY: Duration = Duration::from_millis(20);
/// The longest readable part of a lock file's name.
const NAME_LEN: usize = 64;

/// The lock files this process holds, and the waiters of each.
static HELD: (Mutex<BTreeSet<PathBuf>>, Condvar) = (Mutex::new(BTreeSet::new()), Condvar::new());

/// The folder of the locks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockDir {
    dir: PathBuf,
}

impl LockDir {
    /// The locks in `dir` (made with the first lock taken).
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> LockDir {
        LockDir { dir: dir.into() }
    }

    /// The folder.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// The file of the lock `name` (a keyring entry's name: `azul-storage/s3/d_k3f9`): its
    /// letters, digits, `-` and `_` (the rest `_`, at most 64), a hash of the whole name (two
    /// names never share a file), `.lock` - always in the folder.
    #[must_use]
    pub fn file_of(&self, name: &str) -> PathBuf {
        let mut readable: String = name
            .chars()
            .take(NAME_LEN)
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if readable.is_empty() {
            readable.push_str("lock");
        }
        let hash = blake3::hash(name.as_bytes()).to_hex();
        self.dir
            .join(format!("{readable}.{}.lock", &hash.as_str()[..16]))
    }

    /// Takes the lock `name`, waiting up to `wait` for its holder - in this process or another.
    ///
    /// # Errors
    ///
    /// When it is still held after `wait`, or its file cannot be made or locked.
    pub fn lock(&self, name: &str, wait: Duration) -> Result<HeldLock, LockError> {
        self.lock_using(name, wait, &File::try_lock)
    }

    /// [`LockDir::lock`] for a document, where a lock that cannot be had at all must not stop the
    /// write: on a file system with no file locks ([`LockFailure::Unsupported`]) the threads of
    /// this process still take turns, the lock is noted ([`unlocked_files`]) and said once, and the
    /// writer goes on - another program changing the same document at once is not kept out there.
    /// A lock the keyring's session or a token spend depends on is [`LockDir::lock`]: it fails
    /// closed.
    ///
    /// # Errors
    ///
    /// As [`LockDir::lock`], except an unsupported lock.
    pub fn lock_best_effort(&self, name: &str, wait: Duration) -> Result<HeldLock, LockError> {
        self.best_effort_using(name, wait, &File::try_lock)
    }

    /// [`LockDir::lock_best_effort`] with the file lock a test replaces.
    pub(crate) fn best_effort_using(
        &self,
        name: &str,
        wait: Duration,
        try_lock: &dyn Fn(&File) -> Result<(), TryLockError>,
    ) -> Result<HeldLock, LockError> {
        match self.lock_using(name, wait, try_lock) {
            Err(e) if e.is_unsupported() => {
                let path = self.file_of(name);
                note_unlocked(&path, &e);
                match ProcessSlot::take(&path, Instant::now() + wait) {
                    Some(slot) => Ok(HeldLock {
                        _file: None,
                        _slot: slot,
                    }),
                    None => Err(LockError::new(
                        LockFailure::Busy,
                        format!(
                            "the lock {} is held by another task of this app for more than {} s",
                            path.display(),
                            wait.as_secs()
                        ),
                    )),
                }
            }
            other => other,
        }
    }

    /// [`LockDir::lock`] with the file lock a test replaces.
    pub(crate) fn lock_using(
        &self,
        name: &str,
        wait: Duration,
        try_lock: &dyn Fn(&File) -> Result<(), TryLockError>,
    ) -> Result<HeldLock, LockError> {
        let path = self.file_of(name);
        let deadline = Instant::now() + wait;
        let Some(slot) = ProcessSlot::take(&path, deadline) else {
            return Err(LockError::new(
                LockFailure::Busy,
                format!(
                    "the lock {} is held by another task of this app for more than {} s",
                    path.display(),
                    wait.as_secs()
                ),
            ));
        };
        fs::create_dir_all(&self.dir).map_err(|e| {
            LockError::new(
                LockFailure::Failed,
                format!("the lock folder {}: {e}", self.dir.display()),
            )
        })?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| {
                LockError::new(
                    LockFailure::Failed,
                    format!("the lock {}: {e}", path.display()),
                )
            })?;
        loop {
            match try_lock(&file) {
                Ok(()) => {
                    return Ok(HeldLock {
                        _file: Some(file),
                        _slot: slot,
                    })
                }
                Err(TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        return Err(LockError::new(
                            LockFailure::Busy,
                            format!(
                                "the lock {} is held by another program for more than {} s",
                                path.display(),
                                wait.as_secs()
                            ),
                        ));
                    }
                    std::thread::sleep(RETRY);
                }
                Err(TryLockError::Error(e)) => {
                    let kind = if lock_unsupported(&e) {
                        LockFailure::Unsupported
                    } else {
                        LockFailure::Failed
                    };
                    return Err(LockError::new(
                        kind,
                        format!("the lock {}: {e}", path.display()),
                    ));
                }
            }
        }
    }
}

/// A lock held: dropped, it is released - its file lock first, then the waiters of this
/// process.
#[derive(Debug)]
pub struct HeldLock {
    _file: Option<File>,
    _slot: ProcessSlot,
}

/// Takes the lock `name` among the threads of this process only, waiting up to `wait`: for what a
/// store with no folder of locks changes in several steps (a keyring entry read, decided and
/// written back). Another program of the user is not kept out; a folder of locks
/// ([`LockDir::lock`]) does that too.
///
/// # Errors
///
/// When it is still held after `wait`.
pub fn in_process(name: &str, wait: Duration) -> Result<HeldLock, LockError> {
    // A path that is no file: the in-process names live beside the lock files in one set.
    let path = PathBuf::from(format!("in-process:{name}"));
    match ProcessSlot::take(&path, Instant::now() + wait) {
        Some(slot) => Ok(HeldLock {
            _file: None,
            _slot: slot,
        }),
        None => Err(LockError::new(
            LockFailure::Busy,
            format!(
                "the lock {name} is held by another task of this app for more than {} s",
                wait.as_secs()
            ),
        )),
    }
}

/// The user's configuration folder, as the OS names it (this crate has no azul to ask).
#[must_use]
pub fn user_config_dir() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    match System::here() {
        System::Windows => var("APPDATA"),
        System::Macos => var("HOME").map(|home| home.join("Library").join("Application Support")),
        System::Linux => {
            var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|home| home.join(".config")))
        }
    }
}

/// The folder where the locks of the keyring entries of every namespace are, beside each other:
/// `<config dir>/azlin/locks`.
#[must_use]
pub fn keyring_lock_root(config_dir: &Path) -> PathBuf {
    config_dir.join("azlin").join("locks")
}

/// The folder name of the keyring namespace `namespace` among the locks: its letters, digits, `.`,
/// `-` and `_`; `app` for an app's own (empty) namespace.
#[must_use]
pub fn namespace_folder(namespace: &str) -> String {
    let name: String = namespace
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name.chars().all(|c| c == '.') {
        String::from("app")
    } else {
        name
    }
}

/// One lock file held by this process: the next thread of the process waits for it to drop.
#[derive(Debug)]
struct ProcessSlot {
    path: PathBuf,
}

impl ProcessSlot {
    /// The slot of `path`, once no other thread of this process holds it; `None` at `deadline`.
    fn take(path: &Path, deadline: Instant) -> Option<ProcessSlot> {
        let (held, freed) = &HELD;
        let mut set = held.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if !set.contains(path) {
                set.insert(path.to_path_buf());
                return Some(ProcessSlot {
                    path: path.to_path_buf(),
                });
            }
            let now = Instant::now();
            if now >= deadline {
                return None;
            }
            set = freed
                .wait_timeout(set, deadline - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

impl Drop for ProcessSlot {
    fn drop(&mut self) {
        let (held, freed) = &HELD;
        held.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.path);
        freed.notify_all();
    }
}
