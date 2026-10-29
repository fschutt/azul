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
//!
//! A folder keeps its identity when it is renamed (its id on the drive, its inode here), so a
//! rename is one rename ([`renames`]): before the files are planned, what the drive renamed is
//! renamed here and what was renamed here goes to the drive as one move, and every file is
//! planned under the names both sides end with - an edit made inside the folder elsewhere, a
//! file renamed or put into it, all land under its new name. Renamed differently on both sides,
//! it takes the drive's name.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    local::BaseFolder,
    remote::{self, RemoteFolder, RemoteIndex},
};

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

// ==== Renames ====

/// The folders renamed since the base, here (found by their inode) and on the drive (by their
/// identity), and the moves that bring both sides to one tree: what the drive renamed is renamed
/// here, what was renamed here is renamed on the drive. A folder renamed differently on both
/// sides takes the drive's name, the side that committed first (as a file's conflict does).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Renames {
    /// Moves to make here, each written as its path is when it is made (after the ones before).
    pub here: Vec<(String, String)>,
    /// Moves to make on the drive, in the same way.
    pub there: Vec<(String, String)>,
    /// Where each folder of the base ends (its own key when nothing moves it).
    pub finals: BTreeMap<String, String>,
    /// The folders of the base both sides have moved to the same key already.
    pub agreed: BTreeMap<String, String>,
    /// Folders renamed differently here and on the drive.
    pub clashes: Vec<String>,
}

impl Renames {
    /// Whether no folder of the base moves.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.here.is_empty()
            && self.there.is_empty()
            && self.finals.iter().all(|(key, end)| key == end)
    }
}

/// Where `key` (a file or a folder) ends when the folders of `finals` (base key -> final key)
/// move: under the deepest of them it lies in.
#[must_use]
pub fn rebased(key: &str, finals: &BTreeMap<String, String>) -> String {
    if let Some(end) = finals.get(key) {
        return end.clone();
    }
    let mut cut = key.len();
    while let Some(i) = key[..cut].rfind('/') {
        if let Some(end) = finals.get(&key[..i]) {
            return format!("{end}{}", &key[i..]);
        }
        cut = i;
    }
    key.to_string()
}

/// `path` after `moves`, made one after the other.
#[must_use]
pub fn moved(path: &str, moves: &[(String, String)]) -> String {
    let mut out = path.to_string();
    for (from, to) in moves {
        if out == *from {
            out.clone_from(to);
        } else if let Some(rest) = out
            .strip_prefix(from.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
        {
            out = format!("{to}/{rest}");
        }
    }
    out
}

fn parent_of(key: &str) -> &str {
    key.rfind('/').map_or("", |i| &key[..i])
}

fn name_of(key: &str) -> &str {
    key.rfind('/').map_or(key, |i| &key[i + 1..])
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// Where the folders of the base are now on one side, and the other way round.
struct Side<'a> {
    /// Base key -> key on this side.
    now: BTreeMap<&'a str, &'a str>,
    /// Key on this side -> base key.
    base_of: BTreeMap<&'a str, &'a str>,
}

impl<'a> Side<'a> {
    /// The folders of `base` found among `side` (mark, key) by their `mark`: a mark two
    /// folders of the side share finds neither.
    fn locate<K: Ord + Copy>(
        base: &'a BTreeMap<String, BaseFolder>,
        side: impl Iterator<Item = (K, &'a str)>,
        mark: impl Fn(&'a BaseFolder) -> Option<K>,
    ) -> Side<'a> {
        let mut found: BTreeMap<K, Option<&'a str>> = BTreeMap::new();
        for (m, key) in side {
            found
                .entry(m)
                .and_modify(|at| *at = None)
                .or_insert(Some(key));
        }
        let mut now = BTreeMap::new();
        let mut base_of = BTreeMap::new();
        for (key, folder) in base {
            if let Some(at) = mark(folder).and_then(|m| found.get(&m).copied().flatten()) {
                now.insert(key.as_str(), at);
                base_of.insert(at, key.as_str());
            }
        }
        Side { now, base_of }
    }

    /// Where the base folder `key` moved to on this side: it is not where its folder on this
    /// side and its name put it.
    fn moved(&self, key: &str) -> Option<&'a str> {
        let now = *self.now.get(key)?;
        let parent = parent_of(key);
        let parent_now = if parent.is_empty() {
            ""
        } else {
            self.now.get(parent).copied().unwrap_or(parent)
        };
        (now != join(parent_now, name_of(key))).then_some(now)
    }
}

/// The final keys of the base's folders, each worked out once.
struct Solve<'a> {
    base: &'a BTreeMap<String, BaseFolder>,
    there: Side<'a>,
    here: Side<'a>,
    finals: BTreeMap<String, String>,
    visiting: BTreeSet<String>,
    clashes: Vec<String>,
}

impl Solve<'_> {
    fn final_of(&mut self, key: &str) -> String {
        if let Some(end) = self.finals.get(key) {
            return end.clone();
        }
        // Renames that chase each other (one folder into the other and back): it stays.
        if !self.visiting.insert(key.to_string()) {
            return key.to_string();
        }
        let end = match (self.there.moved(key), self.here.moved(key)) {
            (Some(there), here) => {
                let end = self.placed(there, true);
                if let Some(here) = here {
                    if self.placed(here, false) != end {
                        self.clashes.push(key.to_string());
                    }
                }
                end
            }
            (None, Some(here)) => self.placed(here, false),
            (None, None) => {
                let parent = parent_of(key);
                let parent_end = if self.base.contains_key(parent) {
                    self.final_of(parent)
                } else {
                    parent.to_string()
                };
                join(&parent_end, name_of(key))
            }
        };
        self.visiting.remove(key);
        self.finals.insert(key.to_string(), end.clone());
        end
    }

    /// The final key of a folder found at `at` on one side: under its folder there (that
    /// folder's final key, when the base has it), by its name there.
    fn placed(&mut self, at: &str, there: bool) -> String {
        let parent = parent_of(at);
        let side = if there { &self.there } else { &self.here };
        let base_parent = side.base_of.get(parent).copied();
        let parent_end = match base_parent {
            Some(base_parent) => self.final_of(base_parent),
            None => parent.to_string(),
        };
        join(&parent_end, name_of(at))
    }
}

/// The renames between the base's folders (`base`), the folders here (`here`: key -> inode and
/// device) and the drive's (`there`), and the moves that make both sides agree.
#[must_use]
pub fn renames(
    base: &BTreeMap<String, BaseFolder>,
    here: &BTreeMap<String, (u64, u64)>,
    there: &BTreeMap<String, RemoteFolder>,
) -> Renames {
    let there_side = Side::locate(
        base,
        there
            .iter()
            .filter(|(_, folder)| remote::is_folder_id(&folder.id))
            .map(|(key, folder)| (folder.id.as_str(), key.as_str())),
        |b| remote::is_folder_id(&b.id).then_some(b.id.as_str()),
    );
    let here_side = Side::locate(
        base,
        here.iter()
            .filter(|(_, ino)| ino.0 != 0)
            .map(|(key, ino)| (*ino, key.as_str())),
        |b| (b.ino != 0).then_some((b.ino, b.dev)),
    );
    let mut solve = Solve {
        base,
        there: there_side,
        here: here_side,
        finals: BTreeMap::new(),
        visiting: BTreeSet::new(),
        clashes: Vec::new(),
    };
    for key in base.keys() {
        solve.final_of(key);
    }
    let Solve {
        there,
        here,
        finals,
        clashes,
        ..
    } = solve;
    // The moves, the shallowest end first, each from where the folder is once the moves before
    // it are made (a folder moves with the one it is in).
    let mut order: Vec<(&String, &String)> = finals.iter().collect();
    order.sort_by_key(|(_, end)| (end.matches('/').count(), (*end).clone()));
    let mut out = Renames {
        clashes,
        ..Renames::default()
    };
    for (key, end) in order {
        if let Some(now) = here.now.get(key.as_str()) {
            let current = moved(now, &out.here);
            if current != *end {
                out.here.push((current, end.clone()));
            }
        }
        if let Some(now) = there.now.get(key.as_str()) {
            let current = moved(now, &out.there);
            if current != *end {
                out.there.push((current, end.clone()));
            }
        }
        let at_end = |side: &Side<'_>| {
            side.now
                .get(key.as_str())
                .is_some_and(|at| *at == end.as_str())
        };
        if at_end(&here) && at_end(&there) && key != end {
            out.agreed.insert(key.clone(), end.clone());
        }
    }
    out.finals = finals;
    out
}
