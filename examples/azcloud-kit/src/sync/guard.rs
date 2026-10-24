//! The burst guard (D42): a run whose changes look like a burst - more than [`BURST_CHANGES`]
//! rewrites or deletes within [`BURST_WINDOW_SECS`] - or like ransomware - [`ENTROPY_REWRITES`]
//! files that were low-entropy rewritten into high-entropy bytes - pauses the folder's uploads and
//! asks the user.
//!
//! While a folder is paused nothing goes up from it, and what the drive changed still comes
//! here. The [`Pause`] stays in the folder's local index until a run with
//! [`SyncOptions::allow_burst`](super::SyncOptions::allow_burst) ("these changes are mine"),
//! after which the window starts afresh, or until nothing is left to send (the files were
//! restored).
//!
//! What counts is what this device does to files the drive already had: rewrites, deletes,
//! conflicts. New files are no burst (a first sync, a folder of new photos). The mass-delete
//! guard still stops a plan that would empty most of a folder at once.
//!
//! A file's entropy is that of its first [`SAMPLE_BYTES`] (ransomware that encrypts only the
//! start of each file shows there too); a shorter file than [`MIN_SAMPLE_BYTES`] is not judged.
//! The guard remembers which files were low-entropy when last synced; a folder synced before the
//! guard learns that on its next run.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};

use serde::{Deserialize, Serialize};

use super::{
    local::{self, BaseEntry, Scan},
    merge::Action,
};

/// More rewrites and deletes than this within [`BURST_WINDOW_SECS`] pause uploads.
pub const BURST_CHANGES: usize = 200;
/// The window of [`BURST_CHANGES`] and [`ENTROPY_REWRITES`] (seconds).
pub const BURST_WINDOW_SECS: i64 = 300;
/// This many low-entropy files rewritten into high-entropy bytes within the window pause
/// uploads.
pub const ENTROPY_REWRITES: usize = 10;
/// Bits per byte above which a sample looks encrypted or compressed (random bytes: about 7.8 in
/// 1 KiB, 7.95 in 4 KiB).
pub const HIGH_ENTROPY: f64 = 7.5;
/// Bits per byte below which a sample is text-like (prose, code, CSV: about 4 to 5.5).
pub const LOW_ENTROPY: f64 = 6.0;
/// The bytes of a file the guard reads: its start.
pub const SAMPLE_BYTES: usize = 4096;
/// Files shorter than this are not judged (too few bytes to look random).
pub const MIN_SAMPLE_BYTES: usize = 1024;
/// The files a pause names.
const PAUSE_FILES: usize = 20;

/// Why uploads paused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    /// More than [`BURST_CHANGES`] rewrites and deletes within five minutes.
    Burst,
    /// [`ENTROPY_REWRITES`] or more files turned from text-like into random-looking bytes.
    Encryption,
}

/// A folder's uploads, paused until the user answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pause {
    pub reason: PauseReason,
    /// When it paused (Unix seconds).
    pub since: i64,
    /// The rewrites and deletes within the window ([`PauseReason::Burst`]), or the files that
    /// turned high-entropy ([`PauseReason::Encryption`]).
    pub changes: usize,
    /// Some of them (at most 20, sorted), to show.
    pub files: Vec<String>,
}

impl Pause {
    /// One line for people (the command line; AzDrive words its own question).
    #[must_use]
    pub fn describe(&self) -> String {
        match self.reason {
            PauseReason::Burst => format!(
                "{} files were changed or deleted within five minutes",
                self.changes
            ),
            PauseReason::Encryption => format!(
                "{} files were rewritten into what looks like encrypted data (ransomware?)",
                self.changes
            ),
        }
    }
}

/// The guard's memory, in the folder's local index.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Guard {
    /// This device's runs within the window: when (Unix seconds), their rewrites and deletes,
    /// and how many of those turned high-entropy.
    pub recent: Vec<(i64, usize, usize)>,
    /// The files that were low-entropy when last synced.
    pub low_entropy: BTreeSet<String>,
    /// Whether every file of the folder was judged once.
    pub classified: bool,
    /// Uploads paused, until the user answers.
    pub paused: Option<Pause>,
}

/// Whether a run that planned `actions` against `base` (the folder at `root`) may send them, at
/// `now` (Unix seconds); `allow` is the user's "these changes are mine".
///
/// The run is recorded in `guard`: without a pause its rewrites and deletes count towards the
/// window of the next runs; a pause is kept in `guard.paused` and the run sends nothing. A pause
/// the user has not answered is returned again (with this run's numbers when they trip the guard
/// too) until a run is allowed or has nothing left to send.
pub fn check_burst(
    guard: &mut Guard,
    actions: &[Action],
    base: &BTreeMap<String, BaseEntry>,
    root: &Path,
    now: i64,
    allow: bool,
) -> Option<Pause> {
    guard
        .recent
        .retain(|(when, _, _)| now.saturating_sub(*when) < BURST_WINDOW_SECS);
    if allow || !actions.iter().any(sends) {
        if allow {
            guard.recent.clear();
        }
        guard.paused = None;
        return None;
    }
    let mut changed: Vec<String> = Vec::new();
    let mut encrypted: Vec<String> = Vec::new();
    for action in actions {
        let key = action.key();
        match action {
            Action::Upload { .. } | Action::MergeJson { .. } if base.contains_key(key) => {}
            Action::Conflict { .. } | Action::DeleteRemote { .. } => {}
            _ => continue,
        }
        changed.push(key.to_string());
        if !matches!(action, Action::DeleteRemote { .. })
            && guard.low_entropy.contains(key)
            && sample(root, key).is_some_and(|s| entropy(&s) > HIGH_ENTROPY)
        {
            encrypted.push(key.to_string());
        }
    }
    let this_run = (now, changed.len(), encrypted.len());
    let changes = this_run.1 + guard.recent.iter().map(|r| r.1).sum::<usize>();
    let rewrites = this_run.2 + guard.recent.iter().map(|r| r.2).sum::<usize>();
    let tripped = if rewrites >= ENTROPY_REWRITES {
        Some((PauseReason::Encryption, rewrites, encrypted))
    } else if changes > BURST_CHANGES {
        Some((PauseReason::Burst, changes, changed))
    } else {
        None
    };
    match (tripped, guard.paused.take()) {
        (Some((reason, changes, mut files)), earlier) => {
            files.sort();
            files.truncate(PAUSE_FILES);
            let pause = Pause {
                reason,
                since: earlier.map_or(now, |p| p.since),
                changes,
                files,
            };
            guard.paused = Some(pause.clone());
            Some(pause)
        }
        (None, Some(earlier)) => {
            guard.paused = Some(earlier.clone());
            Some(earlier)
        }
        (None, None) => {
            if this_run.1 > 0 {
                guard.recent.push(this_run);
            }
            None
        }
    }
}

/// Whether `action` sends something to the drive (what a pause holds back).
pub(crate) fn sends(action: &Action) -> bool {
    matches!(
        action,
        Action::Upload { .. }
            | Action::DeleteRemote { .. }
            | Action::Conflict { .. }
            | Action::MergeJson { .. }
    )
}

/// After a run: which files here are low-entropy now - those it sent or wrote, and on a folder's
/// first run with the guard every file.
pub(crate) fn learn(guard: &mut Guard, actions: &[Action], root: &Path, scan: &Scan) {
    if !guard.classified {
        guard.low_entropy = scan
            .files
            .keys()
            .filter(|key| is_low(root, key))
            .cloned()
            .collect();
        guard.classified = true;
    }
    for action in actions {
        match action {
            Action::Upload { key } | Action::Download { key } | Action::MergeJson { key } => {
                judge(guard, root, key);
            }
            Action::Conflict { key, copy } => {
                judge(guard, root, key);
                judge(guard, root, copy);
            }
            // A cloud-only file has no copy here to judge; its next download judges it again.
            Action::DeleteLocal { key }
            | Action::DeleteRemote { key }
            | Action::Forget { key }
            | Action::CloudOnly { key } => {
                guard.low_entropy.remove(key);
            }
            Action::Agree { .. } => {}
        }
    }
}

fn judge(guard: &mut Guard, root: &Path, key: &str) {
    if is_low(root, key) {
        guard.low_entropy.insert(key.to_string());
    } else {
        guard.low_entropy.remove(key);
    }
}

fn is_low(root: &Path, key: &str) -> bool {
    sample(root, key).is_some_and(|s| entropy(&s) < LOW_ENTROPY)
}

/// The first [`SAMPLE_BYTES`] of the file `key`; `None` when it cannot be read or is shorter
/// than [`MIN_SAMPLE_BYTES`].
fn sample(root: &Path, key: &str) -> Option<Vec<u8>> {
    let file = fs::File::open(local::path_of(root, key)).ok()?;
    let mut bytes = Vec::with_capacity(SAMPLE_BYTES);
    file.take(SAMPLE_BYTES as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() >= MIN_SAMPLE_BYTES).then_some(bytes)
}

/// The Shannon entropy of `bytes`, in bits per byte (0 to 8).
#[must_use]
pub fn entropy(bytes: &[u8]) -> f64 {
    if bytes.is_empty() {
        return 0.0;
    }
    let mut counts = [0usize; 256];
    for &b in bytes {
        counts[usize::from(b)] += 1;
    }
    let n = bytes.len() as f64;
    counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / n;
            -p * p.log2()
        })
        .sum()
}
