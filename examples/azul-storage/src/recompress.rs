//! The recompression pass of an encrypted drive (feature `encryption`): files written while
//! they were uploaded (zstd level 3, chosen for speed) are written once more, smaller, while
//! the computer has nothing else to do.
//!
//! - **When.** Only while the computer is idle and on mains power: the app asks the OS
//!   (azul's `PowerState`) and calls [`run_pass`] with a `stop` that turns true at the first
//!   user action or when the power cord goes. This module never looks at the clock or the
//!   power itself.
//! - **What.** Every file of at least [`RecompressPolicy::min_size`] that is not compressed
//!   already (its first bytes: JPEG, PNG, ZIP, video, ... - [`crate::crypto::codec`]):
//!   text-like files (UTF-8 without NUL bytes) with brotli at quality 11, everything else with
//!   zstd at level 19 ([`Recoding`]).
//! - **How.** A file is read through its reader (its BLAKE3 checked), written into a NEW
//!   object with a new file key ([`Compression::Recode`]), and kept only when the new object is
//!   at least [`RecompressPolicy::min_saving_percent`] smaller than the old one. The index then
//!   names the new object in the old one's place as a compare-and-swap
//!   ([`crate::encrypted::Expect::Object`] of the old object): a file the user changed meanwhile
//!   keeps the user's version and the new object leaves the bucket again. The size and the date
//!   stay the file's: recompression is no edit. The quota counts STORED bytes (what arrives at
//!   the storage nodes), so every file the pass makes smaller frees quota.
//! - **Resumable.** The pass walks the index in path order and records the last path it did
//!   ([`RecompressState::cursor`]); a stopped pass goes on from there. A finished pass records
//!   when it started ([`RecompressState::since`]): the next one looks only at files modified
//!   since then, so nothing is recompressed twice (a recompressed file keeps its date).
//! - JPEG XL (lossless JPEG transcoding) is left for later: no pure-Rust encoder.

use std::io::{Cursor, Read};

use serde::{Deserialize, Serialize};

use crate::{
    crypto::{
        azl1::{ObjectSummary, WriteOptions},
        codec::{looks_compressed, Compression, Recoding},
    },
    encrypted::{EncryptedDrive, IndexEntry, Rewrite},
    Drive, DriveError,
};

/// The `format` of a saved [`RecompressState`].
pub const STATE_FORMAT: &str = "azul-storage.recompression";
/// The bytes the choice of the codec looks at.
pub const SNIFF_LEN: usize = 4096;

/// Which files are worth it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecompressPolicy {
    /// Smaller files are left as they are (a few kilobytes saved are not worth an object).
    pub min_size: u64,
    /// The new object must be at least this many percent smaller than the old one.
    pub min_saving_percent: u64,
}

impl Default for RecompressPolicy {
    fn default() -> Self {
        RecompressPolicy {
            min_size: 64 * 1024,
            min_saving_percent: 5,
        }
    }
}

impl RecompressPolicy {
    /// Whether an object of `after` bytes is worth replacing one of `before` bytes.
    #[must_use]
    pub fn worth_it(&self, before: u64, after: u64) -> bool {
        let percent = self.min_saving_percent.min(100);
        u128::from(after) * 100 <= u128::from(before) * u128::from(100 - percent)
    }
}

/// What the pass writes a file with, from its first bytes: `None` for a format that is
/// compressed already, brotli for text (UTF-8 without NUL bytes; a character cut at the end of
/// `first_bytes` is fine), zstd for the rest.
#[must_use]
pub fn recoding_for(first_bytes: &[u8]) -> Option<Recoding> {
    if looks_compressed(first_bytes) {
        return None;
    }
    let text = match std::str::from_utf8(first_bytes) {
        Ok(_) => true,
        // Only a character cut off by the end of the sniffed bytes.
        Err(e) => e.error_len().is_none(),
    };
    if text && !first_bytes.contains(&0) {
        Some(Recoding::Brotli)
    } else {
        Some(Recoding::ZstdMax)
    }
}

/// What happened to one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecompressOutcome {
    /// The path names the smaller object now.
    Replaced {
        /// The old object's bytes in the bucket.
        before: u64,
        /// The new object's.
        after: u64,
    },
    /// Recompressed, but it saved too little: the old object stays, nothing was uploaded.
    NotWorthIt,
    /// Not tried: a folder marker, smaller than the policy's minimum, or compressed already.
    Ineligible,
    /// The file changed or went while it was recompressed: the user's version stays.
    Skipped,
}

/// Recompresses the file `path` of `drive` (see the module documentation).
pub fn recompress_one<D: Drive>(
    drive: &EncryptedDrive<D>,
    path: &str,
    policy: &RecompressPolicy,
) -> Result<RecompressOutcome, DriveError> {
    recompress_with(drive, path, policy, &|| {})
}

/// [`recompress_one`] with `before_bind` run between the upload and the index change.
pub(crate) fn recompress_with<D: Drive>(
    drive: &EncryptedDrive<D>,
    path: &str,
    policy: &RecompressPolicy,
    before_bind: &dyn Fn(),
) -> Result<RecompressOutcome, DriveError> {
    let entry = match drive.entry(path) {
        Ok(entry) => entry,
        Err(DriveError::NotFound { .. }) => return Ok(RecompressOutcome::Skipped),
        Err(e) => return Err(e),
    };
    let Some(object) = entry.object.as_ref() else {
        return Ok(RecompressOutcome::Ineligible);
    };
    if entry.size < policy.min_size.max(1) {
        return Ok(RecompressOutcome::Ineligible);
    }
    let before = object.stored_size;
    let mut reader = match drive.open_reader(path) {
        Ok(reader) => reader,
        Err(DriveError::NotFound { .. }) => return Ok(RecompressOutcome::Skipped),
        Err(e) => return Err(e),
    };
    let mut first = Vec::with_capacity(SNIFF_LEN);
    (&mut reader)
        .take(SNIFF_LEN as u64)
        .read_to_end(&mut first)
        .map_err(|e| read_failed(path, e))?;
    let Some(recoding) = recoding_for(&first) else {
        return Ok(RecompressOutcome::Ineligible);
    };
    let options = WriteOptions {
        segment_size: drive.options().segment_size,
        compression: Compression::Recode(recoding),
    };
    let mut body = Cursor::new(first).chain(reader);
    let keep = |summary: &ObjectSummary| policy.worth_it(before, summary.object_len);
    match drive.rewrite(path, &entry, &mut body, &options, &keep, before_bind)? {
        Rewrite::Replaced(summary) => Ok(RecompressOutcome::Replaced {
            before,
            after: summary.object_len,
        }),
        Rewrite::Declined => Ok(RecompressOutcome::NotWorthIt),
        Rewrite::Conflict => Ok(RecompressOutcome::Skipped),
    }
}

/// A read of the old object that failed, as the drive's error (a damaged object stays
/// `Corrupt`).
fn read_failed(path: &str, e: std::io::Error) -> DriveError {
    crate::crypto::CryptoError::from(e).for_key(path)
}

/// Where the pass is: what the caller saves after every file (a JSON file next to the drive's
/// other local state).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecompressState {
    pub format: String,
    /// Files modified at or after this (seconds since 1970) are the pass's: when the last
    /// finished pass started (0: every file).
    #[serde(default)]
    pub since: u64,
    /// When the pass under way started; `None` when no pass is under way.
    #[serde(default)]
    pub started: Option<u64>,
    /// The last path the pass under way did (paths come in the index's order).
    #[serde(default)]
    pub cursor: Option<String>,
    /// Files replaced by smaller objects, over every pass.
    #[serde(default)]
    pub replaced: u64,
    /// The bucket bytes that saved, over every pass.
    #[serde(default)]
    pub saved: u64,
    /// Files that could not be read (damaged): left as they are, counted here.
    #[serde(default)]
    pub failed: u64,
}

impl Default for RecompressState {
    fn default() -> Self {
        RecompressState {
            format: STATE_FORMAT.to_string(),
            since: 0,
            started: None,
            cursor: None,
            replaced: 0,
            saved: 0,
            failed: 0,
        }
    }
}

impl RecompressState {
    /// The state as the state file keeps it.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// A state file read back.
    pub fn from_json(text: &str) -> Result<RecompressState, DriveError> {
        let state: RecompressState = serde_json::from_str(text).map_err(|e| {
            DriveError::InvalidConfig(format!("not a recompression state (line {})", e.line()))
        })?;
        if state.format != STATE_FORMAT {
            return Err(DriveError::InvalidConfig(format!(
                "not a recompression state (format {:?})",
                state.format
            )));
        }
        Ok(state)
    }
}

/// Whether the pass under way looks at `entry` at `path`.
fn in_pass(state: &RecompressState, path: &str, entry: &IndexEntry) -> bool {
    entry.object.is_some()
        && entry.modified.unwrap_or(0) >= state.since
        && state.cursor.as_deref().map_or(true, |done| path > done)
}

/// Runs (or goes on with) the pass over `drive`, `now` being the time (seconds since 1970)
/// a new pass records as its start. `save` gets the state after every file; `stop` is asked
/// before every file. `Ok(true)` when the pass finished (the next one starts from scratch,
/// looking only at files modified since this one started), `Ok(false)` when `stop` stopped it.
/// A file that does not read (damaged) is counted in [`RecompressState::failed`] and left; any
/// other error (no answer from the bucket) stops the pass, the state saved so far stays good.
pub fn run_pass<D: Drive>(
    drive: &EncryptedDrive<D>,
    state: &mut RecompressState,
    policy: &RecompressPolicy,
    now: u64,
    save: &mut dyn FnMut(&RecompressState) -> Result<(), DriveError>,
    stop: &dyn Fn() -> bool,
) -> Result<bool, DriveError> {
    if state.started.is_none() {
        state.started = Some(now);
        state.cursor = None;
        save(state)?;
    }
    let entries = drive.all_entries("")?;
    for (path, entry) in entries {
        if !in_pass(state, &path, &entry) {
            continue;
        }
        if stop() {
            return Ok(false);
        }
        match recompress_one(drive, &path, policy) {
            Ok(RecompressOutcome::Replaced { before, after }) => {
                state.replaced += 1;
                state.saved += before.saturating_sub(after);
            }
            Ok(_) => {}
            Err(DriveError::Corrupt { .. }) => state.failed += 1,
            Err(e) => return Err(e),
        }
        state.cursor = Some(path);
        save(state)?;
    }
    state.since = state.started.take().unwrap_or(now);
    state.cursor = None;
    save(state)?;
    Ok(true)
}
