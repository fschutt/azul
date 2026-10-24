//! The folder sync's fault seam: what a test harness changes about a pass from outside.
//!
//! - **Points.** The engine names places in a pass ([`POINTS`]) and tells them to a session's
//!   points hook (`SyncSession::with_points`); an app that wants its passes faulted hands it a
//!   hook that calls [`point`] with the pairing's folder. A harness arms a point to CRASH the
//!   pass there (the pass thread unwinds with a [`Crashed`] payload: nothing after the point
//!   runs, no error handler sees it, only destructors) or to PAUSE it until [`release`] (a
//!   laptop lid closed mid-pass). Each is one-shot: the pass after it runs through. (The
//!   `azsync` command line kills and stops itself at points on its own, from its
//!   environment; a wrong clock is `SyncSession::with_clock`.)
//! - **The disk** a pass writes to: [`free_bytes`] is what the free-space check sees, and
//!   [`check_write`] fails with "no space left" while the disk is played full.
//! - **A folder that cannot be listed** for a number of scans ([`set_readdir_error`]): the
//!   scan meets the error the system would give (an I/O error of a failing disk or a network
//!   share, a folder swapped for a file, a permission), which `chmod` alone cannot play.
//!
//! Faults are kept per synced folder, the pairing's folder exactly as its setup names it, so a
//! process that syncs several folders (AzDrive's E2E devices) faults each on its own. Nothing is
//! armed in a normal run: every call is one lookup in an empty map.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

/// The points the engine names, in the order a pass meets them.
pub const POINTS: &[&str] = &[
    "start",
    "after-scan",
    "before-upload",
    "mid-read",
    "after-blob",
    "after-commit",
    // Inside a big download's body, its first part in the spool (before the file is written).
    "mid-download",
    "download-start",
    "before-rename",
    "after-rename",
    "before-trash",
    "conflict-start",
    "conflict-renamed",
    "before-base-save",
];

/// The payload a crashed pass unwinds with. Whoever runs passes on a thread catches it
/// (`std::panic::catch_unwind`) and treats the pass as a process that died there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crashed {
    pub point: String,
}

/// One folder's faults.
#[derive(Debug, Default)]
struct Faults {
    /// The point that crashes the next pass to reach it.
    crash_at: Option<String>,
    /// The point the next pass to reach it waits at.
    pause_at: Option<String>,
    /// Where a pass waits now (a harness waits for it to get there).
    paused_at: Option<String>,
    /// Set by [`release`]: the waiting pass goes on.
    released: bool,
    /// Passes crashed here so far, and the point of the last crash not yet taken: a crash
    /// on a worker thread of the pass reaches its caller as a scoped thread's panic, without
    /// the [`Crashed`] payload ([`take_crashed`]).
    crashes: u64,
    last_crash: Option<String>,
    /// What the free-space check sees instead of the disk's answer.
    free_bytes: Option<u64>,
    /// Writes in the folder (and the pairing's state) fail with "no space left".
    disk_full: bool,
    /// Folders (keys, `""` the top) whose listing fails with an OS error for the next scans:
    /// the error number and how many scans are left.
    readdir_errors: BTreeMap<String, (i32, u32)>,
    /// The folders whose listing fails in the scan running now (taken at its start).
    readdir_now: BTreeMap<String, i32>,
}

impl Faults {
    fn is_empty(&self) -> bool {
        self.crash_at.is_none()
            && self.pause_at.is_none()
            && self.paused_at.is_none()
            && self.crashes == 0
            && self.last_crash.is_none()
            && self.free_bytes.is_none()
            && !self.disk_full
            && self.readdir_errors.is_empty()
            && self.readdir_now.is_empty()
    }
}

/// Every folder's faults.
static FAULTS: Mutex<BTreeMap<PathBuf, Faults>> = Mutex::new(BTreeMap::new());
/// Wakes a paused pass at [`release`], and a harness waiting in [`wait_paused`].
static PAUSES: Condvar = Condvar::new();

fn faults() -> MutexGuard<'static, BTreeMap<PathBuf, Faults>> {
    FAULTS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn with<T>(root: &Path, f: impl FnOnce(&mut Faults) -> T) -> T {
    let mut all = faults();
    let out = f(all.entry(root.to_path_buf()).or_default());
    if all.get(root).is_some_and(Faults::is_empty) {
        all.remove(root);
    }
    out
}

/// Crashes the next pass of `root` that reaches `point` (one-shot).
pub fn arm_crash(root: &Path, point: &str) {
    with(root, |f| f.crash_at = Some(point.to_string()));
}

/// Makes the next pass of `root` that reaches `point` wait there until [`release`] (one-shot).
pub fn arm_pause(root: &Path, point: &str) {
    with(root, |f| {
        f.pause_at = Some(point.to_string());
        f.released = false;
    });
}

/// The point a pass of `root` waits at now.
#[must_use]
pub fn paused_at(root: &Path) -> Option<String> {
    faults().get(root).and_then(|f| f.paused_at.clone())
}

/// Waits up to `timeout` for a pass of `root` to wait at a point; the point.
#[must_use]
pub fn wait_paused(root: &Path, timeout: Duration) -> Option<String> {
    let all = faults();
    let (all, _) = PAUSES
        .wait_timeout_while(all, timeout, |all| {
            all.get(root).is_none_or(|f| f.paused_at.is_none())
        })
        .unwrap_or_else(PoisonError::into_inner);
    all.get(root).and_then(|f| f.paused_at.clone())
}

/// Lets the pass of `root` that waits at a point go on (and disarms a pause not reached yet).
pub fn release(root: &Path) {
    with(root, |f| {
        f.pause_at = None;
        f.released = true;
    });
    PAUSES.notify_all();
}

/// How many passes of `root` crashed at a point.
#[must_use]
pub fn crashes(root: &Path) -> u64 {
    faults().get(root).map_or(0, |f| f.crashes)
}

/// The pass of `root` reached `point`: crashes or waits there when a harness armed it, else
/// returns at once.
///
/// # Panics
///
/// Unwinds with [`Crashed`] when a crash is armed at `point` (that is the crash).
pub fn point(root: &Path, point: &str) {
    let mut all = faults();
    if all.is_empty() {
        return;
    }
    let key = root.to_path_buf();
    let Some(f) = all.get_mut(&key) else {
        return;
    };
    if f.crash_at.as_deref() == Some(point) {
        f.crash_at = None;
        f.crashes += 1;
        f.last_crash = Some(point.to_string());
        drop(all);
        // resume_unwind: no panic hook runs, so a crash prints nothing and is not reported as
        // a bug.
        std::panic::resume_unwind(Box::new(Crashed {
            point: point.to_string(),
        }));
    }
    if f.pause_at.as_deref() == Some(point) {
        f.pause_at = None;
        f.paused_at = Some(point.to_string());
        f.released = false;
        PAUSES.notify_all();
        let all = PAUSES
            .wait_while(all, |all| !all.get(&key).is_none_or(|f| f.released))
            .unwrap_or_else(PoisonError::into_inner);
        let mut all = all;
        if let Some(f) = all.get_mut(&key) {
            f.paused_at = None;
            f.released = false;
        }
        if all.get(&key).is_some_and(Faults::is_empty) {
            all.remove(&key);
        }
    }
}

/// The point the last crash of `root` happened at, once (a caught unwind that is no
/// [`Crashed`] - a worker thread's crash, re-raised by its scope - is still known as one).
#[must_use]
pub fn take_crashed(root: &Path) -> Option<String> {
    with(root, |f| f.last_crash.take())
}

/// The crashed pass's point, when `payload` (a caught unwind) is a [`Crashed`].
#[must_use]
pub fn crashed_at(payload: &(dyn std::any::Any + Send)) -> Option<String> {
    payload.downcast_ref::<Crashed>().map(|c| c.point.clone())
}

/// The point a pass of `root` crashed at, when the caught unwind `payload` of one of its
/// threads is that crash: a [`Crashed`] itself, or the panic of a thread scope one of whose
/// threads crashed (the seam noted the point, which is left for [`take_crashed`]).
#[must_use]
pub fn crash_of(root: &Path, payload: &(dyn std::any::Any + Send)) -> Option<String> {
    crashed_at(payload).or_else(|| faults().get(root).and_then(|f| f.last_crash.clone()))
}

/// Makes the free-space check of `root` see `bytes` (`None`: the disk's own answer).
pub fn set_free_bytes(root: &Path, bytes: Option<u64>) {
    with(root, |f| f.free_bytes = bytes);
}

/// The free space a pass of `root` sees, given the disk's own answer `real`.
#[must_use]
pub fn free_bytes(root: &Path, real: u64) -> u64 {
    faults()
        .get(root)
        .and_then(|f| f.free_bytes)
        .unwrap_or(real)
}

/// Plays `root`'s disk full (`true`) or not.
pub fn set_disk_full(root: &Path, full: bool) {
    with(root, |f| f.disk_full = full);
}

/// Whether a write for `root` may go ahead: "no space left on device" while its disk is played
/// full. A pass calls it before it writes a file in the folder or its state.
///
/// # Errors
///
/// The OS's "no space left" error while the disk is played full.
pub fn check_write(root: &Path) -> std::io::Result<()> {
    if faults().get(root).is_some_and(|f| f.disk_full) {
        // ENOSPC on Unix, ERROR_DISK_FULL on Windows: the error a real full disk gives.
        #[cfg(windows)]
        let code = 112;
        #[cfg(not(windows))]
        let code = 28;
        return Err(std::io::Error::from_raw_os_error(code));
    }
    Ok(())
}

/// The OS error number `name` stands for (`EIO`, `ENOTDIR`, `EACCES`, or a number), as this
/// system numbers it.
#[must_use]
pub fn errno_of(name: &str) -> Option<i32> {
    // Windows: ERROR_CRC for a failing disk's I/O error, ERROR_DIRECTORY, ERROR_ACCESS_DENIED.
    #[cfg(windows)]
    let known = [("EIO", 23), ("ENOTDIR", 267), ("EACCES", 5)];
    #[cfg(not(windows))]
    let known = [("EIO", 5), ("ENOTDIR", 20), ("EACCES", 13)];
    known
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, code)| *code)
        .or_else(|| name.parse().ok())
}

/// Makes the listing of the folder `folder` (its key in the pairing, `""` the top) fail with
/// the OS error `errno` in the next `scans` scans of `root`.
pub fn set_readdir_error(root: &Path, folder: &str, errno: i32, scans: u32) {
    let folder = folder.trim_matches('/').to_string();
    with(root, |f| {
        if scans == 0 {
            f.readdir_errors.remove(&folder);
        } else {
            f.readdir_errors.insert(folder, (errno, scans));
        }
    });
}

/// A scan of `root` starts: the folders whose listing fails in it are taken (one scan less
/// left for each).
pub fn scan_started(root: &Path) {
    let mut all = faults();
    if all.is_empty() {
        return;
    }
    let Some(f) = all.get_mut(root) else {
        return;
    };
    f.readdir_now.clear();
    for (folder, (errno, left)) in &mut f.readdir_errors {
        f.readdir_now.insert(folder.clone(), *errno);
        *left -= 1;
    }
    f.readdir_errors.retain(|_, (_, left)| *left > 0);
    if f.is_empty() {
        all.remove(root);
    }
}

/// The error the listing of `folder` meets in the scan of `root` running now, if one is played.
#[must_use]
pub fn readdir_error(root: &Path, folder: &str) -> Option<std::io::Error> {
    let all = faults();
    if all.is_empty() {
        return None;
    }
    all.get(root)
        .and_then(|f| f.readdir_now.get(folder.trim_matches('/')))
        .map(|errno| std::io::Error::from_raw_os_error(*errno))
}

/// Every fault of `root` gone (a crash count too).
pub fn clear(root: &Path) {
    faults().remove(root);
    PAUSES.notify_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(name: &str) -> PathBuf {
        PathBuf::from(format!("/faults-test/{name}"))
    }

    #[test]
    fn an_unarmed_point_returns_at_once() {
        point(&root("calm"), "after-blob");
        assert_eq!(crashes(&root("calm")), 0);
    }

    #[test]
    fn an_armed_crash_unwinds_the_pass_at_its_point_once_and_counts_it() {
        let r = root("crash");
        arm_crash(&r, "after-commit");
        point(&r, "after-blob");
        let caught = std::panic::catch_unwind(|| point(&r, "after-commit"));
        let payload = caught.expect_err("the armed point unwinds");
        assert_eq!(crashed_at(payload.as_ref()).as_deref(), Some("after-commit"));
        assert_eq!(crashes(&r), 1);
        // One-shot: the next pass runs through.
        point(&r, "after-commit");
        clear(&r);
    }

    #[test]
    fn a_crash_armed_for_one_folder_leaves_the_others_alone() {
        arm_crash(&root("one"), "after-scan");
        point(&root("other"), "after-scan");
        assert!(std::panic::catch_unwind(|| point(&root("one"), "after-scan")).is_err());
        clear(&root("one"));
    }

    #[test]
    fn a_paused_pass_waits_at_its_point_until_released() {
        let r = root("pause");
        arm_pause(&r, "before-rename");
        let pass = {
            let r = r.clone();
            std::thread::spawn(move || {
                point(&r, "before-rename");
                "went on"
            })
        };
        assert_eq!(
            wait_paused(&r, Duration::from_secs(10)).as_deref(),
            Some("before-rename")
        );
        assert!(!pass.is_finished(), "the pass waits while paused");
        release(&r);
        assert_eq!(pass.join().expect("the pass ends"), "went on");
        assert_eq!(paused_at(&r), None);
        clear(&r);
    }

    #[test]
    fn a_full_disk_refuses_writes_with_no_space_left_and_the_free_space_is_what_was_set() {
        let r = root("disk");
        set_disk_full(&r, true);
        set_free_bytes(&r, Some(1024));
        let e = check_write(&r).expect_err("a full disk refuses");
        assert_eq!(e.raw_os_error(), Some(if cfg!(windows) { 112 } else { 28 }));
        assert_eq!(free_bytes(&r, 1 << 40), 1024);
        set_disk_full(&r, false);
        set_free_bytes(&r, None);
        assert!(check_write(&r).is_ok());
        assert_eq!(free_bytes(&r, 77), 77);
        clear(&r);
    }
}
