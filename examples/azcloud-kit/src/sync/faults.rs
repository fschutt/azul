//! The folder sync's fault seam: what a test harness changes about a pass from outside.
//!
//! - **Points.** A pass calls [`point`] at named places (`after-scan`, `after-blob`,
//!   `after-commit`, `before-rename`, `conflict-start`, `before-trash`, `before-base-save`). A
//!   harness arms one to CRASH the pass there (the pass thread unwinds with a [`Crashed`]
//!   payload: nothing after the point runs, no error handler sees it, only destructors), to
//!   ABORT the process (the command line's `kill -9` stand-in, armed from
//!   `$AZSYNC_CRASH_AT` by [`arm_from_env`]) or to PAUSE it until [`release`] (a laptop
//!   lid closed mid-pass). Each is one-shot: the pass after it runs through.
//! - **The clock** a pass reads ([`now_unix`]): the real one plus a per-folder offset, so a
//!   device whose clock is wrong can be played without changing the machine's.
//! - **The disk** a pass writes to: [`free_bytes`] is what the free-space check sees, and
//!   [`check_write`] fails with "no space left" while the disk is played full.
//!
//! Faults are kept per synced folder, the pairing's folder exactly as its setup names it, so a
//! process that syncs several folders (AzDrive's E2E devices) faults each on its own. Nothing is
//! armed in a normal run: every call is one lookup in an empty map.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// The points a pass calls [`point`] at, in the order a pass meets them.
pub const POINTS: &[&str] = &[
    "after-scan",
    "after-blob",
    "after-commit",
    "conflict-start",
    "before-trash",
    "before-rename",
    "before-base-save",
];

/// The payload a crashed pass unwinds with. Whoever runs passes on a thread catches it
/// (`std::panic::catch_unwind`) and treats the pass as a process that died there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crashed {
    pub point: String,
}

/// What an armed point does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The pass thread unwinds with [`Crashed`].
    Crash,
    /// The process ends at once (`std::process::abort`), as a `kill -9` would end it.
    Abort,
}

/// One folder's faults.
#[derive(Debug, Default)]
struct Faults {
    /// The point that crashes or aborts the next pass to reach it.
    crash_at: Option<(String, Action)>,
    /// The point the next pass to reach it waits at.
    pause_at: Option<String>,
    /// Where a pass waits now (a harness waits for it to get there).
    paused_at: Option<String>,
    /// Set by [`release`]: the waiting pass goes on.
    released: bool,
    /// Passes crashed here so far.
    crashes: u64,
    /// Added to the real clock, in milliseconds.
    clock_offset_ms: i64,
    /// What the free-space check sees instead of the disk's answer.
    free_bytes: Option<u64>,
    /// Writes in the folder (and the pairing's state) fail with "no space left".
    disk_full: bool,
}

impl Faults {
    fn is_empty(&self) -> bool {
        self.crash_at.is_none()
            && self.pause_at.is_none()
            && self.paused_at.is_none()
            && self.crashes == 0
            && self.clock_offset_ms == 0
            && self.free_bytes.is_none()
            && !self.disk_full
    }
}

/// Every folder's faults; the key `""` holds the ones armed for every folder.
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
    with(root, |f| f.crash_at = Some((point.to_string(), Action::Crash)));
}

/// Makes the next pass of `root` that reaches `point` wait there until [`release`] (one-shot).
pub fn arm_pause(root: &Path, point: &str) {
    with(root, |f| {
        f.pause_at = Some(point.to_string());
        f.released = false;
    });
}

/// `$AZSYNC_CRASH_AT=<point>` ends this process at that point of its first pass, whatever folder
/// it syncs: what a harness's `kill -9` at a named step needs from a command line.
pub fn arm_from_env() {
    if let Some(point) = std::env::var("AZSYNC_CRASH_AT").ok().filter(|p| !p.trim().is_empty()) {
        with(Path::new(""), |f| {
            f.crash_at = Some((point.trim().to_string(), Action::Abort));
        });
    }
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

/// The pass of `root` reached `point`: crashes, aborts or waits there when a harness armed it,
/// else returns at once.
///
/// # Panics
///
/// Unwinds with [`Crashed`] when a crash is armed at `point` (that is the crash).
pub fn point(root: &Path, point: &str) {
    let mut all = faults();
    if all.is_empty() {
        return;
    }
    // A fault armed for this folder, else one armed for every folder.
    let key = [root, Path::new("")]
        .into_iter()
        .find(|k| all.get(*k).is_some_and(|f| armed_here(f, point)))
        .map(Path::to_path_buf);
    let Some(key) = key else {
        return;
    };
    let Some(f) = all.get_mut(&key) else {
        return;
    };
    if let Some((at, action)) = f.crash_at.clone() {
        if at == point {
            f.crash_at = None;
            f.crashes += 1;
            drop(all);
            match action {
                Action::Abort => std::process::abort(),
                // resume_unwind: no panic hook runs, so a crash prints nothing and is not
                // reported as a bug.
                Action::Crash => std::panic::resume_unwind(Box::new(Crashed {
                    point: point.to_string(),
                })),
            }
        }
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

fn armed_here(f: &Faults, point: &str) -> bool {
    f.crash_at.as_ref().is_some_and(|(at, _)| at == point) || f.pause_at.as_deref() == Some(point)
}

/// The crashed pass's point, when `payload` (a caught unwind) is a [`Crashed`].
#[must_use]
pub fn crashed_at(payload: &(dyn std::any::Any + Send)) -> Option<String> {
    payload.downcast_ref::<Crashed>().map(|c| c.point.clone())
}

/// Plays `root`'s clock `offset_secs` away from the real one (negative: behind).
pub fn set_clock_offset(root: &Path, offset_secs: f64) {
    with(root, |f| f.clock_offset_ms = (offset_secs * 1000.0).round() as i64);
}

/// Moves `root`'s clock by `secs` more (a jump, as after a sleep or an NTP correction).
pub fn jump_clock(root: &Path, secs: f64) {
    with(root, |f| {
        f.clock_offset_ms = f.clock_offset_ms.saturating_add((secs * 1000.0).round() as i64);
    });
}

/// The time as a pass of `root` reads it.
#[must_use]
pub fn now(root: &Path) -> SystemTime {
    let offset = faults().get(root).map_or(0, |f| f.clock_offset_ms);
    let real = SystemTime::now();
    let delta = Duration::from_millis(offset.unsigned_abs());
    if offset >= 0 {
        real + delta
    } else {
        real.checked_sub(delta).unwrap_or(UNIX_EPOCH)
    }
}

/// [`now`] in seconds since 1970.
#[must_use]
pub fn now_unix(root: &Path) -> i64 {
    now(root)
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
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
    fn the_clock_offset_and_its_jumps_move_the_folders_time_only() {
        let r = root("clock");
        set_clock_offset(&r, -3600.0);
        jump_clock(&r, 600.0);
        let behind = now_unix(&root("clock-other")) - now_unix(&r);
        assert!((2999..=3001).contains(&behind), "3000 s behind, got {behind}");
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
