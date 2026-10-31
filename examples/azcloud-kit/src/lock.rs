//! Locks every process of this user shares: a folder (an app's is beside its drives file,
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
    fs::{self, File, TryLockError},
    path::{Path, PathBuf},
    sync::{Condvar, Mutex, PoisonError},
    time::{Duration, Instant},
};

use crate::error::{fail, CloudResult, Context};

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
    pub fn lock(&self, name: &str, wait: Duration) -> CloudResult<HeldLock> {
        let path = self.file_of(name);
        let deadline = Instant::now() + wait;
        let Some(slot) = ProcessSlot::take(&path, deadline) else {
            fail!(
                "the lock {} is held by another task of this app for more than {} s",
                path.display(),
                wait.as_secs()
            );
        };
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("the lock folder {}", self.dir.display()))?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("the lock {}", path.display()))?;
        loop {
            match file.try_lock() {
                Ok(()) => {
                    return Ok(HeldLock {
                        _file: file,
                        _slot: slot,
                    })
                }
                Err(TryLockError::WouldBlock) => {
                    if Instant::now() >= deadline {
                        fail!(
                            "the lock {} is held by another program for more than {} s",
                            path.display(),
                            wait.as_secs()
                        );
                    }
                    std::thread::sleep(RETRY);
                }
                Err(TryLockError::Error(e)) => {
                    return Err(e).with_context(|| format!("the lock {}", path.display()));
                }
            }
        }
    }
}

/// A lock held: dropped, it is released - its file lock first, then the waiters of this
/// process.
#[derive(Debug)]
pub struct HeldLock {
    _file: File,
    _slot: ProcessSlot,
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
