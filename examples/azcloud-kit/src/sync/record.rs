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

use std::collections::{BTreeMap, BTreeSet};

use super::{
    names,
    remote::{self, FolderTombstone, RemoteFile, RemoteFolder, RemoteIndex, Tombstone},
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
        // A link: the drive's file holds its target, which is the content both name.
        file.link.clone_from(&known.link);
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

/// Puts the folders the drive lists (`listed`: its folder entries and the folders its files lie
/// in) into `index`, each with the identity the record gives it - or, when no device named one
/// yet, the one every device derives from its key - and the deletes of folders it does not list.
pub(super) fn overlay_folders(
    index: &mut RemoteIndex,
    listed: &BTreeSet<String>,
    record: &RemoteIndex,
) {
    for key in listed {
        let folder = match record.folders.get(key) {
            Some(known) if remote::is_folder_id(&known.id) => known.clone(),
            _ => RemoteFolder {
                id: remote::derived_folder_id(key),
                ..RemoteFolder::default()
            },
        };
        index.folders.insert(key.clone(), folder);
    }
    for (key, tomb) in &record.deleted_folders {
        let sound = remote::check_key(key).is_ok()
            && names::key_problem(key).is_none()
            && remote::is_folder_id(&tomb.id);
        if sound && !listed.contains(key) {
            index.deleted_folders.insert(key.clone(), tomb.clone());
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
    let mut out = current.clone();
    change(
        &mut out.files,
        &mut out.deleted,
        [files(current), files(read), files(before), files(next)],
        |file| remote::is_hash(&file.hash),
    );
    change(
        &mut out.folders,
        &mut out.deleted_folders,
        [folders(current), folders(read), folders(before), folders(next)],
        |folder| remote::is_folder_id(&folder.id),
    );
    for feature in &next.features {
        if !out.features.contains(feature) {
            out.features.push(feature.clone());
        }
    }
    out.generation = current.generation + 1;
    out.updated_at = next.updated_at;
    out.updated_by.clone_from(&next.updated_by);
    out
}

/// One kind of entry of an index with the deletes of that kind: files and their tombstones, or
/// folders and theirs.
struct Lists<'a, E, T> {
    entries: &'a BTreeMap<String, E>,
    deleted: &'a BTreeMap<String, T>,
}

fn files(index: &RemoteIndex) -> Lists<'_, RemoteFile, Tombstone> {
    Lists {
        entries: &index.files,
        deleted: &index.deleted,
    }
}

fn folders(index: &RemoteIndex) -> Lists<'_, RemoteFolder, FolderTombstone> {
    Lists {
        entries: &index.folders,
        deleted: &index.deleted_folders,
    }
}

/// [`with_changes`] of one kind of entry, from the record as it is now, as the run read it, the
/// drive as the run read it and what it committed. `sound` says whether a run's entry describes
/// anything (a stand-in hash names no content).
fn change<E: Clone + PartialEq, T: Clone + PartialEq>(
    out_entries: &mut BTreeMap<String, E>,
    out_deleted: &mut BTreeMap<String, T>,
    [current, read, before, next]: [Lists<'_, E, T>; 4],
    sound: impl Fn(&E) -> bool,
) {
    let untouched_entry = |key: &str| current.entries.get(key) == read.entries.get(key);
    let untouched_delete = |key: &str| current.deleted.get(key) == read.deleted.get(key);
    // What this run sent, and the deletes it made.
    for (key, entry) in next.entries {
        if before.entries.get(key) != Some(entry) && sound(entry) {
            out_entries.insert(key.clone(), entry.clone());
            out_deleted.remove(key);
        }
    }
    for (key, tomb) in next.deleted {
        if before.deleted.get(key) != Some(tomb) {
            out_deleted.insert(key.clone(), tomb.clone());
        }
    }
    // The names this run deleted or moved away, and the deletes it let expire.
    for key in before.entries.keys() {
        if !next.entries.contains_key(key) && untouched_entry(key.as_str()) {
            out_entries.remove(key);
        }
    }
    for key in before.deleted.keys() {
        if !next.deleted.contains_key(key) && untouched_delete(key.as_str()) {
            out_deleted.remove(key);
        }
    }
    // What the record said of names the drive no longer lists, and of deletes of names it lists
    // again (another tool removed or made them): it describes nothing now.
    for key in read.entries.keys() {
        if !next.entries.contains_key(key) && untouched_entry(key.as_str()) {
            out_entries.remove(key);
        }
    }
    for key in read.deleted.keys() {
        if next.entries.contains_key(key) && untouched_delete(key.as_str()) {
            out_deleted.remove(key);
        }
    }
}
