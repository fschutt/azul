//! The folder record of a drive synced by name ([`super::named`]): what the drive's own index
//! cannot say of a synced folder, kept as one file of the drive under the folder,
//! `<prefix>.azlin/sync/record.json`. On an encrypted drive it is sealed like every file and its
//! name lies in the sealed index, so the server sees neither; no listing shows it.
//!
//! It is an index in the plain sync's format ([`RemoteIndex`]): each file's content (BLAKE3)
//! with its modification time to the nanosecond and its executable bit, and the tombstones of
//! deleted files with whether the user confirmed them. The drive's files stay the truth of what
//! the folder holds: an entry describes a file only while the drive lists that name with that
//! content, and a tombstone only a name the drive does not list, so a record left behind by a
//! crash, an older version of the app or another tool never makes a file look changed or gone.
//!
//! A run reads the record with its version (the copy kept beside the local index while the
//! version is the same) and, when it commits, writes it before its deletes and only over the
//! version it read: a record another device wrote meanwhile is read again and this run's
//! changes are made to it ([`with_changes`]).

use super::{
    names,
    remote::{self, RemoteIndex},
};

/// The record's place under the folder's prefix.
pub const RECORD_FILE: &str = ".azlin/sync/record.json";

/// Whether `key` (under a prefix) is a folder's record: the folder's own at its root, or that of
/// a folder synced deeper down - bookkeeping either way, never a file of the folder.
#[must_use]
pub fn is_record(key: &str) -> bool {
    key == RECORD_FILE
        || key
            .strip_suffix(RECORD_FILE)
            .is_some_and(|parent| parent.ends_with('/'))
}

/// Puts what `record` knows into `index`, the drive's listing: a file's times and executable bit
/// where the record names the same content, the deletes of names the drive does not list, and
/// the format the record's writer used (a newer one leaves the folder as it is, D43).
pub(super) fn overlay(index: &mut RemoteIndex, record: &RemoteIndex) {
    index.version = index.version.max(record.version);
    index.features.clone_from(&record.features);
    for (key, file) in &mut index.files {
        let Some(known) = record.files.get(key) else {
            continue;
        };
        if known.hash != file.hash || !remote::is_hash(&known.hash) {
            continue;
        }
        file.mtime = known.mtime;
        file.mtime_ns = known.mtime_ns;
        file.exec = known.exec;
        file.gen = known.gen;
        file.device.clone_from(&known.device);
    }
    // The record is as untrusted as any entry of the drive: a delete of a name no folder can
    // hold, or of no content, says nothing.
    for (key, tomb) in &record.deleted {
        let sound = remote::check_key(key).is_ok()
            && names::key_problem(key).is_none()
            && remote::is_hash(&tomb.hash);
        if sound && !index.files.contains_key(key) {
            index.deleted.insert(key.clone(), tomb.clone());
        }
    }
}

/// The record to write for a commit: this run's changes - from `before` (the drive as the run
/// read it, the record laid over it) to `next` (what it committed) - made to `current`, the
/// record as the drive holds it now. `read` is the record the run read: what no device wrote
/// since and no longer describes a file the drive lists is dropped, and an entry another device
/// wrote since the run read the record is never undone.
pub(super) fn with_changes(
    current: &RemoteIndex,
    read: &RemoteIndex,
    before: &RemoteIndex,
    next: &RemoteIndex,
) -> RemoteIndex {
    let untouched_file = |key: &str| current.files.get(key) == read.files.get(key);
    let untouched_tomb = |key: &str| current.deleted.get(key) == read.deleted.get(key);
    let mut out = current.clone();
    // What this run sent, and the deletes it made.
    for (key, file) in &next.files {
        if before.files.get(key) != Some(file) && remote::is_hash(&file.hash) {
            out.files.insert(key.clone(), file.clone());
            out.deleted.remove(key);
        }
    }
    for (key, tomb) in &next.deleted {
        if before.deleted.get(key) != Some(tomb) {
            out.deleted.insert(key.clone(), tomb.clone());
        }
    }
    // The names this run deleted or moved away, and the deletes it let expire.
    for key in before.files.keys() {
        if !next.files.contains_key(key) && untouched_file(key.as_str()) {
            out.files.remove(key);
        }
    }
    for key in before.deleted.keys() {
        if !next.deleted.contains_key(key) && untouched_tomb(key.as_str()) {
            out.deleted.remove(key);
        }
    }
    // What the record said of names the drive no longer lists, and of deletes of names it lists
    // again (another tool removed or made them): it describes nothing now.
    for key in read.files.keys() {
        if !next.files.contains_key(key) && untouched_file(key.as_str()) {
            out.files.remove(key);
        }
    }
    for key in read.deleted.keys() {
        if next.files.contains_key(key) && untouched_tomb(key.as_str()) {
            out.deleted.remove(key);
        }
    }
    out.generation = current.generation + 1;
    out.updated_at = next.updated_at;
    out.updated_by.clone_from(&next.updated_by);
    out
}
