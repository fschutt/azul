//! The three-way merge of one synced folder: per file, the BASE (what both
//! sides agreed on at this device's last sync), the LOCAL file and the
//! REMOTE index entry decide what happens. Pure: no file, no network.
//!
//! | here (vs base)   | the drive (vs base) | action                                                     |
//! |------------------|---------------------|------------------------------------------------------------|
//! | same as there    | same as here        | agree                                                      |
//! | changed / new    | unchanged / absent  | upload                                                     |
//! | unchanged        | changed / new       | download                                                   |
//! | unchanged        | deleted             | delete here                                                |
//! | deleted          | unchanged           | delete there (a tombstone)                                 |
//! | changed          | deleted             | upload: an edit beats a delete                             |
//! | deleted          | changed             | download: an edit beats a delete                           |
//! | changed          | changed (otherwise) | keep both: the drive's version keeps the name, this one    |
//! |                  |                     | becomes "name (conflict <device> <date>).ext" (an          |
//! |                  |                     | unattended sync); a JSON-merge file merges per key         |
//! | new, no base     | a tombstone of it   | delete here: a stale copy of a file deleted elsewhere      |
//!
//! A conflict never loses a byte, and two devices that sync in turn end with
//! the same files: the side that committed first keeps the name, the other
//! adds its copy.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::{
    local::{BaseEntry, Content, LocalFile},
    remote::RemoteIndex,
};

/// What happens to one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Equal on both sides.
    Agree { key: String },
    /// This side's version goes to the drive.
    Upload { key: String },
    /// The drive's version comes here.
    Download { key: String },
    /// Deleted on the drive: deleted here.
    DeleteLocal { key: String },
    /// Deleted here: deleted on the drive.
    DeleteRemote { key: String },
    /// Gone on both sides: the base forgets it.
    Forget { key: String },
    /// Changed on both sides: the drive's version keeps `key`, this side's
    /// moves to `copy` and goes to the drive there.
    Conflict { key: String, copy: String },
    /// A JSON-merge file changed on both sides: merged per key, written here
    /// and uploaded.
    MergeJson { key: String },
    /// The drive's version stays in the cloud (an app's on-demand file: new
    /// there and not wanted here yet, or kept in the cloud only already): the
    /// base takes it as a cloud-only entry, nothing is written here.
    CloudOnly { key: String },
}

impl Action {
    /// The file it is about.
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            Action::Agree { key }
            | Action::Upload { key }
            | Action::Download { key }
            | Action::DeleteLocal { key }
            | Action::DeleteRemote { key }
            | Action::Forget { key }
            | Action::Conflict { key, .. }
            | Action::MergeJson { key }
            | Action::CloudOnly { key } => key,
        }
    }

    /// One line for `--dry-run`.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Action::Agree { key } => format!("= {key}"),
            Action::Upload { key } => format!("up {key}"),
            Action::Download { key } => format!("down {key}"),
            Action::DeleteLocal { key } => format!("delete here {key}"),
            Action::DeleteRemote { key } => format!("delete on the drive {key}"),
            Action::Forget { key } => format!("forget {key}"),
            Action::Conflict { key, copy } => {
                format!("conflict {key}: the drive's stays, this one becomes {copy}")
            }
            Action::MergeJson { key } => format!("merge {key} key by key"),
            Action::CloudOnly { key } => format!("cloud only {key}"),
        }
    }
}

/// What the merge looks at.
pub struct PlanInput<'a> {
    pub base: &'a BTreeMap<String, BaseEntry>,
    pub local: &'a BTreeMap<String, LocalFile>,
    pub remote: &'a RemoteIndex,
    /// Keys this side never syncs (its rules): they are neither downloaded
    /// nor deleted on either side.
    pub excluded: &'a dyn Fn(&str) -> bool,
    /// This device's name and today (`2026-10-08`), for conflict copies.
    pub device: &'a str,
    pub date: &'a str,
}

/// The name of this side's copy of a conflicting `key`: `notes/a (conflict
/// laptop 2026-10-08).md` in the same folder, `... 2)`, `... 3)` when that is
/// taken.
#[must_use]
pub fn conflict_name(key: &str, device: &str, date: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let (dir, name) = match key.rfind('/') {
        Some(i) => (&key[..=i], &key[i + 1..]),
        None => ("", key),
    };
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let mut n = 1u32;
    loop {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!(" {n}")
        };
        let candidate = format!("{dir}{stem} (conflict {device} {date}{suffix}){ext}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// The actions for every file of the base, the folder and the index, in key
/// order (excluded keys left out).
#[must_use]
pub fn plan(input: &PlanInput<'_>) -> Vec<Action> {
    let mut keys: BTreeSet<&str> = BTreeSet::new();
    keys.extend(input.base.keys().map(String::as_str));
    keys.extend(input.local.keys().map(String::as_str));
    keys.extend(input.remote.files.keys().map(String::as_str));
    let mut chosen: BTreeSet<String> = BTreeSet::new();
    let mut actions = Vec::new();
    for key in keys {
        if (input.excluded)(key) {
            continue;
        }
        let base = input.base.get(key).map(|e| e.hash.as_str());
        let local = input.local.get(key);
        let here = local.map(|f| f.hash.as_str());
        let there = input.remote.files.get(key).map(|f| f.hash.as_str());
        let key_s = key.to_string();
        let action = match (here, there) {
            (None, None) => match base {
                Some(_) => Action::Forget { key: key_s },
                None => continue,
            },
            (Some(h), Some(t)) if h == t => Action::Agree { key: key_s },
            (Some(h), None) => match base {
                Some(b) if b == h => Action::DeleteLocal { key: key_s },
                Some(_) => Action::Upload { key: key_s },
                None => match input.remote.deleted.get(key) {
                    Some(tomb) if tomb.hash == h => Action::DeleteLocal { key: key_s },
                    _ => Action::Upload { key: key_s },
                },
            },
            (None, Some(t)) => match base {
                Some(b) if b == t => Action::DeleteRemote { key: key_s },
                _ => Action::Download { key: key_s },
            },
            (Some(h), Some(t)) => match base {
                Some(b) if b == h => Action::Download { key: key_s },
                Some(b) if b == t => Action::Upload { key: key_s },
                _ if local.is_some_and(|f| f.content == Content::JsonMerge) => {
                    Action::MergeJson { key: key_s }
                }
                _ => {
                    let taken = |name: &str| {
                        input.local.contains_key(name)
                            || input.remote.files.contains_key(name)
                            || chosen.contains(name)
                    };
                    let copy = conflict_name(key, input.device, input.date, &taken);
                    chosen.insert(copy.clone());
                    Action::Conflict { key: key_s, copy }
                }
            },
        };
        actions.push(action);
    }
    actions
}

/// The three-way merge of two JSON objects key by key: a key changed on one
/// side takes that side's value (a removed key stays removed); a key changed
/// on both sides to different values takes `theirs` - the side that
/// committed first - and is listed.
#[must_use]
pub fn merge_json(
    base: &Map<String, Value>,
    ours: &Map<String, Value>,
    theirs: &Map<String, Value>,
) -> (Map<String, Value>, Vec<String>) {
    let keys: BTreeSet<&String> = base
        .keys()
        .chain(ours.keys())
        .chain(theirs.keys())
        .collect();
    let mut out = Map::new();
    let mut clashes = Vec::new();
    for key in keys {
        let (b, o, t) = (base.get(key), ours.get(key), theirs.get(key));
        let pick = if o == t {
            o
        } else if o == b {
            t
        } else if t == b {
            o
        } else {
            clashes.push(key.clone());
            t
        };
        if let Some(value) = pick {
            out.insert(key.clone(), value.clone());
        }
    }
    (out, clashes)
}
