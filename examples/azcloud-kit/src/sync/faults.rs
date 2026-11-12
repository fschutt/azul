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
//! - **A look at every folder the scan lists** ([`set_listing_hook`]): a test hears each listing
//!   as it happens, to count what the scan holds open at that moment.
//! - **A disk that gives no identity to folders** ([`set_blank_folder_ids`]): the scan's folders
//!   come back with no inode and no device, as FAT, exFAT and a network share give them, so a
//!   folder renamed here is no rename the scan can tell by identity.
//! - **A file that cannot be put in place** ([`fail_replace`]): the steps that write, trash or
//!   rename a file here (a download, a delete into the trash, a conflict's rename) meet the error
//!   the system gives for a lock, a sharing violation or a permission, for the files whose key
//!   matches a pattern and for a number of passes - on every host, where `chmod` plays it on Unix
//!   only.
//!
//! Faults are kept per synced folder, the pairing's folder exactly as its setup names it, so a
//! process that syncs several folders (AzDrive's E2E devices) faults each on its own. Nothing is
//! armed in a normal run: every call is one lookup in an empty map.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
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
    // Right before the conflict's rename of this side's file to the copy's name, after the look at
    // the name: a harness puts a file there to play the window.
    "conflict-before-rename",
    "conflict-renamed",
    "before-base-save",
    // "Free up space" (`SyncSession::free_up`, and the size cap after a pass): after the base
    // says the file is kept in the cloud only and before the file leaves, and once it has left.
    "before-evict-unlink",
    "after-evict-unlink",
    // The read-back's replace of a damaged copy: written down in the journal, nothing moved yet.
    "before-replace",
];

/// The payload a crashed pass unwinds with. Whoever runs passes on a thread catches it
/// (`std::panic::catch_unwind`) and treats the pass as a process that died there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crashed {
    pub point: String,
}

/// What a test hears of each folder a scan lists: told the folder's key.
pub type ListingHook = Arc<dyn Fn(&str) + Send + Sync>;

/// A [`ListingHook`] kept per folder.
struct Listing(ListingHook);

impl fmt::Debug for Listing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Listing(..)")
    }
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
    /// Told the key of every folder right before a scan lists it.
    listing: Option<Listing>,
    /// Key patterns whose files cannot be put in place for the next scans' passes: the error
    /// number and how many are left.
    replace_errors: BTreeMap<String, (i32, u32)>,
    /// The patterns that fail in the pass running now (taken at its scan).
    replace_now: BTreeMap<String, i32>,
    /// The scan keeps no inode and no device of the folders it finds.
    blank_folder_ids: bool,
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
            && self.listing.is_none()
            && self.replace_errors.is_empty()
            && self.replace_now.is_empty()
            && !self.blank_folder_ids
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
    free_override(root).unwrap_or(real)
}

/// The free space the harness set for `root` ([`set_free_bytes`]), when it did: the disk's own
/// answer is not asked then.
#[must_use]
pub fn free_override(root: &Path) -> Option<u64> {
    faults().get(root).and_then(|f| f.free_bytes)
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
    f.replace_now.clear();
    for (pattern, (errno, left)) in &mut f.replace_errors {
        f.replace_now.insert(pattern.clone(), *errno);
        *left -= 1;
    }
    f.replace_errors.retain(|_, (_, left)| *left > 0);
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

/// Plays `root`'s disk as one that gives no identity to its folders (`true`) or not: the folders the
/// scan finds come back with inode 0 and device 0, on a disk that has them too. (The files keep
/// theirs: a file's identity is checked again when it is read, and a disk that gave none there would
/// hold every upload for ever.)
pub fn set_blank_folder_ids(root: &Path, blank: bool) {
    with(root, |f| f.blank_folder_ids = blank);
}

/// Whether the scan of `root` keeps no identity of the folders ([`set_blank_folder_ids`]).
#[must_use]
pub fn blank_folder_ids(root: &Path) -> bool {
    let all = faults();
    !all.is_empty() && all.get(root).is_some_and(|f| f.blank_folder_ids)
}

/// Makes the steps of `root`'s passes that put a file in place - a download written, a delete moved
/// into the trash, a conflict's rename - fail with the OS error `errno` for every key that matches
/// `key_glob` (`*` any run of characters, `?` one), in the next `passes` passes (counted at their
/// scans). Nothing is changed by a step that fails: the file is as it was.
pub fn fail_replace(root: &Path, key_glob: &str, errno: i32, passes: u32) {
    with(root, |f| {
        if passes == 0 {
            f.replace_errors.remove(key_glob);
        } else {
            f.replace_errors
                .insert(key_glob.to_string(), (errno, passes));
        }
    });
}

/// Whether the step that puts the file of `key` in place may go ahead: the error a pattern of
/// [`fail_replace`] gives while its passes last. A pass calls it before it writes, trashes or renames
/// a file of the folder.
///
/// # Errors
///
/// The OS error a test harness played for the file.
pub fn check_replace(root: &Path, key: &str) -> std::io::Result<()> {
    let all = faults();
    if all.is_empty() {
        return Ok(());
    }
    match all.get(root).and_then(|f| {
        f.replace_now
            .iter()
            .find(|(pattern, _)| glob_matches(pattern, key))
    }) {
        Some((_, errno)) => Err(std::io::Error::from_raw_os_error(*errno)),
        None => Ok(()),
    }
}

/// Whether `text` matches `pattern`, `*` standing for any run of characters (a `/` too) and `?` for
/// one.
fn glob_matches(pattern: &str, text: &str) -> bool {
    let (pattern, text): (Vec<char>, Vec<char>) =
        (pattern.chars().collect(), text.chars().collect());
    let (mut p, mut t) = (0, 0);
    // Where the last `*` was, and the text it covers so far.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, t));
            p += 1;
        } else if let Some((at, covered)) = star {
            p = at + 1;
            t = covered + 1;
            star = Some((at, covered + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// Has `hook` told the key of every folder (`""` the top) the scans of `root` list, right before
/// each is listed and on the scan's own thread; `None` takes it away.
pub fn set_listing_hook(root: &Path, hook: Option<ListingHook>) {
    with(root, |f| f.listing = hook.map(Listing));
}

/// A scan of `root` is about to list `folder` (its key, `""` the top): tells the hook a test set.
pub fn listing(root: &Path, folder: &str) {
    let hook = {
        let all = faults();
        if all.is_empty() {
            return;
        }
        all.get(root)
            .and_then(|f| f.listing.as_ref())
            .map(|l| Arc::clone(&l.0))
    };
    // Called without the lock: the hook may ask the seam for more.
    if let Some(hook) = hook {
        hook(folder);
    }
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
    fn a_replace_fails_for_the_files_a_pattern_names_and_for_the_passes_it_was_set_for() {
        let r = root("replace");
        fail_replace(&r, "f2/*", 13, 2);
        // Not armed until a pass starts: the pass that is running now is another.
        assert!(check_replace(&r, "f2/a.txt").is_ok());
        scan_started(&r);
        let e = check_replace(&r, "f2/a.txt").expect_err("the first pass fails it");
        assert_eq!(e.raw_os_error(), Some(13));
        assert!(
            check_replace(&r, "f1/a.txt").is_ok(),
            "another folder is not named"
        );
        scan_started(&r);
        assert!(
            check_replace(&r, "f2/deep/b.txt").is_err(),
            "the second pass too"
        );
        scan_started(&r);
        assert!(check_replace(&r, "f2/a.txt").is_ok(), "the third is free");
        clear(&r);
    }

    #[test]
    fn a_glob_takes_a_star_for_any_run_and_a_question_mark_for_one_character() {
        assert!(glob_matches("f?/*.txt", "f2/a/b.txt"));
        assert!(glob_matches("*", ""));
        assert!(glob_matches("a*b*c", "aXXbYYc"));
        assert!(!glob_matches("a*b*c", "aXXbYY"));
        assert!(!glob_matches("f?", "f"));
        assert!(glob_matches("exact", "exact"));
        assert!(!glob_matches("exact", "exactly"));
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
