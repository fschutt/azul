//! The folders of a synced folder as entries of their own ([`RemoteIndex::folders`]): an empty
//! folder is made, deleted and renamed on every device like a file, and a folder emptied by
//! deleting its files stays a folder. Pure: no file, no network.
//!
//! Per folder, the base (the folders both sides had at the last sync), the folders here and the
//! drive's entries decide, after the files' own plan:
//!
//! | here | the drive                                   | base | action                     |
//! |------|---------------------------------------------|------|----------------------------|
//! | yes  | yes                                         |      | agree                      |
//! | yes  | no, its delete recorded                     | yes  | removed here               |
//! | yes  | no, deleted before this device knew it (D5) | no   | kept here, not sent        |
//! | yes  | no                                          |      | sent there                 |
//! | no   | yes                                         | yes  | its delete goes there      |
//! | no   | yes                                         | no   | made here                  |
//! | no   | no                                          | yes  | forgotten                  |
//!
//! Only a recorded delete removes a folder: one an older writer left out of the index is sent
//! again, never deleted. A folder is removed only when nothing stays in it on that side after
//! the run - no file, no folder - and never with what is in it; otherwise it stays (sent there,
//! or made here) as an edit beats a delete.

use std::collections::{BTreeMap, BTreeSet};

use super::{local::BaseFolder, remote::RemoteIndex};

/// What happens to one folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FolderAction {
    /// Both sides have it.
    Agree { key: String, id: String },
    /// Here and not on the drive: its entry goes there.
    Send { key: String, id: String },
    /// On the drive and not here: made here.
    Make { key: String, id: String },
    /// Deleted on the drive: removed here, empty.
    RemoveHere { key: String },
    /// Deleted here: its delete goes to the drive.
    RemoveThere { key: String, id: String },
    /// Gone on both sides: the base forgets it.
    Forget { key: String },
}

impl FolderAction {
    /// The folder it is about.
    #[must_use]
    pub fn key(&self) -> &str {
        match self {
            FolderAction::Agree { key, .. }
            | FolderAction::Send { key, .. }
            | FolderAction::Make { key, .. }
            | FolderAction::RemoveHere { key }
            | FolderAction::RemoveThere { key, .. }
            | FolderAction::Forget { key } => key,
        }
    }
}

/// What the folders' plan looks at.
pub struct FolderInput<'a> {
    pub base: &'a BTreeMap<String, BaseFolder>,
    /// The folders here (the scan's, of keys a folder may have).
    pub here: &'a BTreeSet<String>,
    /// The drive's index as the run plans with it (its deletes as D5 leaves them).
    pub remote: &'a RemoteIndex,
    /// Keys this side does not sync now (its rules, what cannot be read, links, mounts).
    pub excluded: &'a dyn Fn(&str) -> bool,
    /// What is here and on the drive after the run's file actions (and what stays there
    /// unread): a folder holding any of it is not removed on that side.
    pub files_here: &'a BTreeSet<String>,
    pub files_there: &'a BTreeSet<String>,
}

/// Whether anything of `set` lies inside the folder `key`.
fn holds(set: &BTreeSet<String>, key: &str) -> bool {
    let inside = format!("{key}/");
    set.range(inside.clone()..)
        .next()
        .is_some_and(|k| k.starts_with(&inside))
}

/// The actions for every folder of the base, here and on the drive, in key order. `new_id`
/// makes the identity of a folder that has none yet.
#[must_use]
pub fn plan(input: &FolderInput<'_>, new_id: &dyn Fn() -> String) -> Vec<FolderAction> {
    let mut keys: BTreeSet<&str> = BTreeSet::new();
    keys.extend(input.base.keys().map(String::as_str));
    keys.extend(input.here.iter().map(String::as_str));
    keys.extend(input.remote.folders.keys().map(String::as_str));
    let mut actions: BTreeMap<String, FolderAction> = BTreeMap::new();
    for key in keys {
        // A file of that name on either side: the file's merge decides, not this one.
        if (input.excluded)(key)
            || input.remote.files.contains_key(key)
            || input.files_here.contains(key)
        {
            continue;
        }
        let key_s = key.to_string();
        let base = input.base.get(key);
        let base_id = base.map(|b| b.id.clone()).filter(|id| !id.is_empty());
        let here = input.here.contains(key);
        let action = match (here, input.remote.folders.get(key)) {
            (true, Some(there)) => FolderAction::Agree {
                key: key_s,
                id: there.id.clone(),
            },
            (true, None) => match (base, input.remote.deleted_folders.get(key)) {
                // The delete of the folder the base knows (not of another one of that name).
                (Some(b), Some(tomb)) if b.id.is_empty() || b.id == tomb.id => {
                    FolderAction::RemoveHere { key: key_s }
                }
                // Deleted on the drive before this device knew it: kept here, not sent.
                (None, Some(_)) => continue,
                _ => FolderAction::Send {
                    key: key_s,
                    id: base_id.unwrap_or_else(new_id),
                },
            },
            (false, Some(there)) => match base {
                Some(_) => FolderAction::RemoveThere {
                    key: key_s,
                    id: there.id.clone(),
                },
                None => FolderAction::Make {
                    key: key_s,
                    id: there.id.clone(),
                },
            },
            (false, None) => match base {
                Some(_) => FolderAction::Forget { key: key_s },
                None => continue,
            },
        };
        actions.insert(key.to_string(), action);
    }
    keep_what_stays(&mut actions, input, new_id);
    actions.into_values().collect()
}

/// A removal of a folder that keeps a file or a folder on that side after the run becomes a
/// keep: deepest first, so a folder kept inside another keeps that one too.
fn keep_what_stays(
    actions: &mut BTreeMap<String, FolderAction>,
    input: &FolderInput<'_>,
    new_id: &dyn Fn() -> String,
) {
    let mut staying_here: BTreeSet<String> = input.here.clone();
    let mut staying_there: BTreeSet<String> = input.remote.folders.keys().cloned().collect();
    for action in actions.values() {
        match action {
            FolderAction::RemoveHere { key } => {
                staying_here.remove(key);
            }
            FolderAction::RemoveThere { key, .. } => {
                staying_there.remove(key);
            }
            FolderAction::Make { key, .. } => {
                staying_here.insert(key.clone());
            }
            FolderAction::Send { key, .. } => {
                staying_there.insert(key.clone());
            }
            FolderAction::Agree { .. } | FolderAction::Forget { .. } => {}
        }
    }
    let mut removals: Vec<String> = actions
        .values()
        .filter(|a| {
            matches!(
                a,
                FolderAction::RemoveHere { .. } | FolderAction::RemoveThere { .. }
            )
        })
        .map(|a| a.key().to_string())
        .collect();
    removals.sort_by_key(|key| std::cmp::Reverse(key.matches('/').count()));
    for key in removals {
        let Some(action) = actions.get(&key).cloned() else {
            continue;
        };
        match action {
            FolderAction::RemoveHere { .. }
                if holds(input.files_here, &key) || holds(&staying_here, &key) =>
            {
                // Something stays in it here: the folder stays, and its entry goes back.
                let id = input
                    .base
                    .get(&key)
                    .map(|b| b.id.clone())
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(new_id);
                staying_here.insert(key.clone());
                staying_there.insert(key.clone());
                actions.insert(key.clone(), FolderAction::Send { key, id });
            }
            FolderAction::RemoveThere { id, .. }
                if holds(input.files_there, &key) || holds(&staying_there, &key) =>
            {
                // Something stays in it on the drive: the folder stays, and comes back here.
                staying_there.insert(key.clone());
                staying_here.insert(key.clone());
                actions.insert(key.clone(), FolderAction::Make { key, id });
            }
            _ => {}
        }
    }
}
