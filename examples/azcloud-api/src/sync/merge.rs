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
//! |                  |                     | becomes "name (conflict <device> <date>).ext" (PLAN §12.3, |
//! |                  |                     | an unattended sync); a JSON-merge file merges per key      |
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
            | Action::MergeJson { key } => key,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        super::remote::{RemoteFile, Tombstone},
        *,
    };

    fn h(c: char) -> String {
        std::iter::repeat(c).take(64).collect()
    }

    fn local(c: char) -> LocalFile {
        LocalFile {
            hash: h(c),
            size: 1,
            mtime_ns: 1,
            blob_size: 1,
            content: Content::Raw,
        }
    }

    fn remote_file(c: char) -> RemoteFile {
        RemoteFile {
            hash: h(c),
            size: 1,
            mtime: 1,
            gen: 1,
            device: String::from("dev-b"),
        }
    }

    fn base(c: char) -> BaseEntry {
        BaseEntry {
            hash: h(c),
            size: 1,
            mtime_ns: 1,
        }
    }

    /// One file through the merge: its base, local and remote hash (or none).
    fn one(b: Option<char>, l: Option<char>, r: Option<char>) -> Action {
        let mut bases = BTreeMap::new();
        let mut locals = BTreeMap::new();
        let mut remote = RemoteIndex::empty();
        if let Some(c) = b {
            bases.insert(String::from("a.md"), base(c));
        }
        if let Some(c) = l {
            locals.insert(String::from("a.md"), local(c));
        }
        if let Some(c) = r {
            remote.files.insert(String::from("a.md"), remote_file(c));
        }
        let never = |_: &str| false;
        let actions = plan(&PlanInput {
            base: &bases,
            local: &locals,
            remote: &remote,
            excluded: &never,
            device: "laptop",
            date: "2026-10-08",
        });
        assert_eq!(actions.len(), 1, "{actions:?}");
        actions.into_iter().next().unwrap()
    }

    fn up() -> Action {
        Action::Upload {
            key: String::from("a.md"),
        }
    }

    fn down() -> Action {
        Action::Download {
            key: String::from("a.md"),
        }
    }

    #[test]
    fn every_row_of_the_table_does_what_it_says() {
        let k = || String::from("a.md");
        assert_eq!(
            one(Some('1'), Some('1'), Some('1')),
            Action::Agree { key: k() }
        );
        assert_eq!(one(None, Some('2'), Some('2')), Action::Agree { key: k() });
        assert_eq!(one(Some('1'), Some('2'), Some('1')), up(), "changed here");
        assert_eq!(one(None, Some('2'), None), up(), "new here");
        assert_eq!(
            one(Some('1'), Some('1'), Some('2')),
            down(),
            "changed there"
        );
        assert_eq!(one(None, None, Some('2')), down(), "new there");
        assert_eq!(
            one(Some('1'), Some('1'), None),
            Action::DeleteLocal { key: k() },
            "deleted there"
        );
        assert_eq!(
            one(Some('1'), None, Some('1')),
            Action::DeleteRemote { key: k() },
            "deleted here"
        );
        assert_eq!(
            one(Some('1'), Some('2'), None),
            up(),
            "an edit beats a delete there"
        );
        assert_eq!(
            one(Some('1'), None, Some('2')),
            down(),
            "an edit beats a delete here"
        );
        assert_eq!(one(Some('1'), None, None), Action::Forget { key: k() });
        assert_eq!(
            one(Some('1'), Some('2'), Some('3')),
            Action::Conflict {
                key: k(),
                copy: String::from("a (conflict laptop 2026-10-08).md")
            }
        );
        assert_eq!(
            one(None, Some('2'), Some('3')),
            Action::Conflict {
                key: k(),
                copy: String::from("a (conflict laptop 2026-10-08).md")
            },
            "new on both sides with other contents"
        );
    }

    #[test]
    fn a_stale_copy_of_a_file_deleted_elsewhere_is_deleted_not_brought_back() {
        let mut remote = RemoteIndex::empty();
        remote.deleted.insert(
            String::from("old.md"),
            Tombstone {
                hash: h('1'),
                gen: 4,
                at: 0,
                device: String::from("dev-b"),
            },
        );
        let locals: BTreeMap<String, LocalFile> = [
            (String::from("old.md"), local('1')),
            (String::from("new.md"), local('2')),
        ]
        .into();
        let never = |_: &str| false;
        let actions = plan(&PlanInput {
            base: &BTreeMap::new(),
            local: &locals,
            remote: &remote,
            excluded: &never,
            device: "d",
            date: "x",
        });
        assert_eq!(
            actions,
            vec![
                Action::Upload {
                    key: String::from("new.md")
                },
                Action::DeleteLocal {
                    key: String::from("old.md")
                },
            ]
        );
    }

    #[test]
    fn an_excluded_key_is_never_downloaded_nor_deleted_on_either_side() {
        let mut remote = RemoteIndex::empty();
        remote.files.insert(String::from("x.log"), remote_file('1'));
        let bases: BTreeMap<String, BaseEntry> = [(String::from("x.log"), base('1'))].into();
        let logs = |k: &str| k.ends_with(".log");
        let actions = plan(&PlanInput {
            base: &bases,
            local: &BTreeMap::new(),
            remote: &remote,
            excluded: &logs,
            device: "d",
            date: "x",
        });
        assert!(actions.is_empty(), "{actions:?}");
    }

    #[test]
    fn a_conflict_copy_takes_a_free_name_and_keeps_its_extension() {
        let free = |_: &str| false;
        assert_eq!(
            conflict_name("notes/a.md", "laptop", "2026-10-08", &free),
            "notes/a (conflict laptop 2026-10-08).md"
        );
        assert_eq!(
            conflict_name("Makefile", "l", "d", &free),
            "Makefile (conflict l d)"
        );
        assert_eq!(
            conflict_name("x/.bashrc", "l", "d", &free),
            "x/.bashrc (conflict l d)",
            "a dot file has no extension"
        );
        let first_taken = |n: &str| n == "a (conflict l d).md";
        assert_eq!(
            conflict_name("a.md", "l", "d", &first_taken),
            "a (conflict l d 2).md"
        );
    }

    #[test]
    fn two_conflicts_in_one_plan_never_pick_the_same_copy_name() {
        let locals: BTreeMap<String, LocalFile> = [
            (String::from("a.md"), local('2')),
            (String::from("a (conflict l d).md"), local('5')),
        ]
        .into();
        let mut remote = RemoteIndex::empty();
        remote.files.insert(String::from("a.md"), remote_file('3'));
        remote
            .files
            .insert(String::from("a (conflict l d).md"), remote_file('6'));
        let never = |_: &str| false;
        let actions = plan(&PlanInput {
            base: &BTreeMap::new(),
            local: &locals,
            remote: &remote,
            excluded: &never,
            device: "l",
            date: "d",
        });
        let copies: Vec<&str> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Conflict { copy, .. } => Some(copy.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(copies.len(), 2);
        assert_ne!(copies[0], copies[1]);
    }

    #[test]
    fn a_json_merge_takes_each_sides_change_and_lets_the_first_committer_win_a_clash() {
        let obj = |v: Value| v.as_object().unwrap().clone();
        let base = obj(json!({"mode": "light", "currentTheme": "flat", "locale": "de"}));
        let ours = obj(json!({"mode": "dark", "currentTheme": "flat", "locale": "en"}));
        let theirs = obj(json!({"mode": "light", "currentTheme": "flora", "locale": "fr"}));
        let (merged, clashes) = merge_json(&base, &ours, &theirs);
        assert_eq!(merged["mode"], "dark", "changed here only");
        assert_eq!(merged["currentTheme"], "flora", "changed there only");
        assert_eq!(merged["locale"], "fr", "both changed: the drive's");
        assert_eq!(clashes, vec![String::from("locale")]);
        let gone = obj(json!({"mode": "light", "currentTheme": "flat"}));
        let (merged, _) = merge_json(&base, &gone, &base);
        assert!(
            merged.get("locale").is_none(),
            "a key removed here stays removed"
        );
    }

    #[test]
    fn a_json_merge_file_changed_on_both_sides_merges_instead_of_conflicting() {
        let mut locals = BTreeMap::new();
        let mut file = local('2');
        file.content = Content::JsonMerge;
        locals.insert(String::from("config.json"), file);
        let mut remote = RemoteIndex::empty();
        remote
            .files
            .insert(String::from("config.json"), remote_file('3'));
        let bases: BTreeMap<String, BaseEntry> = [(String::from("config.json"), base('1'))].into();
        let never = |_: &str| false;
        let actions = plan(&PlanInput {
            base: &bases,
            local: &locals,
            remote: &remote,
            excluded: &never,
            device: "l",
            date: "d",
        });
        assert_eq!(
            actions,
            vec![Action::MergeJson {
                key: String::from("config.json")
            }]
        );
    }
}
