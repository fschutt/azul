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
//! | new, no base     | a tombstone of it   | nothing: kept here and not sent again (a file the drive    |
//! |                  |                     | deleted before this device paired; the run lists it)       |
//!
//! A file's content is its bytes and, where the system has one, its executable bit. A conflict
//! never loses a byte, and two devices that sync in turn end with the same files: the side that
//! committed first keeps the name, the other adds its copy. No row deletes a file because of
//! its time, and no login path (a re-pairing, a lost base) deletes one either.
//!
//! "Here" is a [`LocalView`], not a map of the files found: a name the scan could not tell
//! (unreadable, locked, changed while read, refused, settling, behind a link) is
//! [`Local::Unknown`], and the merge plans NOTHING for it - no delete, no overwrite, no
//! download, whatever the base and the drive say. Only a name known to be absent is a delete.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::{
    local::{BaseEntry, Content, Local, LocalView},
    names::{COMPONENT_MAX_BYTES, PATH_MAX_BYTES},
    remote::{RemoteFile, RemoteIndex},
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
    pub local: &'a LocalView,
    pub remote: &'a RemoteIndex,
    /// Keys this side never syncs (its rules): they are neither downloaded
    /// nor deleted on either side.
    pub excluded: &'a dyn Fn(&str) -> bool,
    /// This device's name and today (`2026-10-08`), for conflict copies.
    pub device: &'a str,
    pub date: &'a str,
}

/// The room a number (` 2` to ` 999999`) may take at the end of a copy's label: a stem is cut for it
/// once, so one prefix finds a copy of a key whatever its number ([`cut_conflicts`]).
const NUMBER_ROOM: usize = 7;

/// The longest an extension may be: what follows the last dot of a longer name is no extension but a
/// part of its stem, so a name is never all extension.
const EXTENSION_MAX: usize = 32;

/// The name of this side's copy of a conflicting `key`: `notes/a (conflict
/// laptop 2026-10-08).md` in the same folder, `... 2)`, `... 3)` when that is taken. A copy of a copy
/// is made from the name of the file the first copy was made of - `notes/a (conflict phone
/// 2026-10-09).md`, never `notes/a (conflict laptop 2026-10-08) (conflict phone 2026-10-09).md` -
/// and the name is cut at a character so that it is a name a folder holds, one name's length and one
/// key's at most (X-DELTA-EDGE-4: the copy's key is committed before the rename here, and a name
/// past 255 bytes failed the rename and stayed on the drive for every device to list as unreadable).
#[must_use]
pub fn conflict_name(key: &str, device: &str, date: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let (dir, stem, ext) = name_parts(key);
    let stem = without_conflict_label(stem);
    let label = format!("conflict {device} {date}");
    copy_from(
        dir,
        stem_for(dir, stem, ext, &label),
        ext,
        &label,
        key,
        taken,
    )
}

/// The name of a second copy of `key` that says what it is, `notes/a (label).md` in the same
/// folder, `(label 2)`, `(label 3)` when that is taken: a trashed version put back beside a file that
/// has its name now ([`super::trash`]), and the shape of a conflict's copy ([`conflict_name`]). The
/// name is cut at a character to one name's length and one key's at most.
#[must_use]
pub fn copy_name(key: &str, label: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let (dir, stem, ext) = name_parts(key);
    copy_from(dir, stem_for(dir, stem, ext, label), ext, label, key, taken)
}

/// `{dir}{stem} ({label}[ n]){ext}` for the first number that is neither `key` nor `taken`.
fn copy_from(
    dir: &str,
    stem: &str,
    ext: &str,
    label: &str,
    key: &str,
    taken: &dyn Fn(&str) -> bool,
) -> String {
    let mut n = 1u32;
    loop {
        let suffix = if n == 1 {
            String::new()
        } else {
            format!(" {n}")
        };
        let candidate = format!("{dir}{stem} ({label}{suffix}){ext}");
        if candidate != key && !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// `text` cut to at most `budget` bytes, at a character.
fn cut_to(text: &str, budget: usize) -> &str {
    let mut end = budget.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// The stem to put before ` (label)` in a copy of a file of `dir` that ends in `ext`: `stem` cut so
/// that the copy's name is one name's length at most and its key one key's, a number's room kept.
fn stem_for<'s>(dir: &str, stem: &'s str, ext: &str, label: &str) -> &'s str {
    let labelled = " (".len() + label.len() + NUMBER_ROOM + ")".len() + ext.len();
    let name_room = COMPONENT_MAX_BYTES.saturating_sub(labelled);
    let key_room = PATH_MAX_BYTES.saturating_sub(dir.len() + labelled);
    cut_to(stem, name_room.min(key_room))
}

/// `key`'s folder (with its slash), its name's stem and its extension (with its dot).
fn name_parts(key: &str) -> (&str, &str, &str) {
    let (dir, name) = match key.rfind('/') {
        Some(i) => (&key[..=i], &key[i + 1..]),
        None => ("", key),
    };
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 && name.len() - i <= EXTENSION_MAX => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (dir, stem, ext)
}

/// Whether `text` is a date as a copy's name has it, `2026-10-08`.
fn is_date(text: &str) -> bool {
    text.len() == 10
        && text.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}

/// `stem` without the label of a conflict copy at its end (` (conflict laptop 2026-10-08)`, `... 2)`),
/// as often as there is one: a copy of a copy is made from the name of the file the first copy was
/// made of. A stem that is nothing but a label stays as it is.
fn without_conflict_label(mut stem: &str) -> &str {
    const OPEN: &str = " (conflict ";
    while let Some(inner) = stem.strip_suffix(')') {
        let Some(at) = inner.rfind(OPEN) else { break };
        let tokens: Vec<&str> = inner[at + OPEN.len()..].split(' ').collect();
        let n = tokens.len();
        let is_number = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
        // `<device> <date>` or `<device> <date> <number>`, a device of one name or several.
        let date_at = if n >= 3 && is_number(tokens[n - 1]) && is_date(tokens[n - 2]) {
            n - 2
        } else if n >= 2 && is_date(tokens[n - 1]) {
            n - 1
        } else {
            break;
        };
        if date_at == 0 || at == 0 {
            break;
        }
        stem = &inner[..at];
    }
    stem
}

/// The name every copy of `key` that `device` made begins with (`notes/a (conflict laptop `), up to
/// its date and number, and `key`'s extension: the same cut and fold as [`conflict_name`], whose date
/// has a fixed length, so the prefix does not depend on it.
fn conflict_prefix<'k>(key: &'k str, device: &str) -> (String, &'k str) {
    let (dir, stem, ext) = name_parts(key);
    let stem = without_conflict_label(stem);
    let label = format!("conflict {device} 0000-00-00");
    let cut = stem_for(dir, stem, ext, &label);
    (format!("{dir}{cut} (conflict {device} "), ext)
}

/// Whether `name` is a copy [`conflict_name`] makes of `key` for `device`, of any date and
/// number.
fn is_conflict_copy_of(name: &str, key: &str, device: &str) -> bool {
    let (prefix, ext) = conflict_prefix(key, device);
    let Some(middle) = name
        .strip_prefix(&prefix)
        .and_then(|rest| rest.strip_suffix(&format!("){ext}")))
    else {
        return false;
    };
    let (date, number) = match middle.split_once(' ') {
        Some((date, number)) => (date, Some(number)),
        None => (middle, None),
    };
    is_date(date) && number.is_none_or(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The conflicts a pass cut short already settled on the drive: a pass commits this side's
/// copy before it renames the file here, so after a crash between the two the drive holds a
/// copy of the key under this device's name, with this side's content, that this side does
/// not know yet. Such a copy finishes that conflict (key -> copy); it is neither downloaded
/// nor copied a second time.
fn cut_conflicts(input: &PlanInput<'_>) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for (key, file) in input.local.files() {
        let here = ident(&file.hash, file.exec, file.link.is_some());
        let there = input
            .remote
            .files
            .get(key)
            .map(|f| ident(&f.hash, f.exec, f.link.is_some()));
        let Some(there) = there else {
            continue;
        };
        let base = input.base.get(key).map(|e| ident(&e.hash, e.exec, e.link));
        if here == there
            || base == Some(here)
            || base == Some(there)
            || file.content == Content::JsonMerge
        {
            continue;
        }
        let (prefix, _) = conflict_prefix(key, input.device);
        let copy = input
            .remote
            .files
            .range(prefix.clone()..)
            .take_while(|(name, _)| name.starts_with(&prefix))
            .find(|(name, f)| {
                is_conflict_copy_of(name, key, input.device)
                    && ident(&f.hash, f.exec, f.link.is_some()) == here
                    && !input.base.contains_key(*name)
                    && input.local.is_absent(name)
                    && !(input.excluded)(name)
            });
        if let Some((name, _)) = copy {
            found.insert(key.clone(), name.clone());
        }
    }
    found
}

/// Whether the executable bit is part of a file's content here: a system without permission
/// bits (Windows) keeps whatever the drive says.
pub const TRACKS_EXEC: bool = cfg!(unix);

/// A file's content as the merge compares it: its bytes, its executable bit where the system
/// has one, and whether it is a link (a link to `a.txt` is never the file holding `a.txt`).
fn ident(hash: &str, exec: bool, link: bool) -> (&str, bool, bool) {
    (hash, exec && TRACKS_EXEC, link)
}

/// Whether the base's entry and the drive's are one version of a file, as the merge compares
/// them: its bytes, its executable bit where the system has one, a link or not.
#[must_use]
pub fn same_version(base: &BaseEntry, remote: &RemoteFile) -> bool {
    ident(&base.hash, base.exec, base.link)
        == ident(&remote.hash, remote.exec, remote.link.is_some())
}

/// The files of the folder whose bytes match a tombstone and that the base never knew: a copy
/// of a file the drive deleted before this device was paired (or after its base was lost). The
/// merge leaves them as they are, on both sides; the run lists them.
#[must_use]
pub fn deleted_elsewhere(input: &PlanInput<'_>) -> Vec<String> {
    input
        .local
        .files()
        .filter(|(key, file)| {
            !(input.excluded)(key)
                && !input.base.contains_key(*key)
                && !input.remote.files.contains_key(*key)
                && input
                    .remote
                    .deleted
                    .get(*key)
                    .is_some_and(|tomb| tomb.hash == file.hash)
        })
        .map(|(key, _)| key.clone())
        .collect()
}

/// The actions for every file of the base, the folder and the index, in key
/// order (excluded keys left out).
#[must_use]
pub fn plan(input: &PlanInput<'_>) -> Vec<Action> {
    let mut keys: BTreeSet<&str> = BTreeSet::new();
    keys.extend(input.base.keys().map(String::as_str));
    keys.extend(input.local.files().map(|(key, _)| key.as_str()));
    keys.extend(input.remote.files.keys().map(String::as_str));
    let mut chosen: BTreeSet<String> = BTreeSet::new();
    let cut = cut_conflicts(input);
    let finishing: BTreeSet<&str> = cut.values().map(String::as_str).collect();
    let mut actions = Vec::new();
    for key in keys {
        if (input.excluded)(key) || finishing.contains(key) {
            continue;
        }
        let base = input.base.get(key).map(|e| ident(&e.hash, e.exec, e.link));
        // A name the scan could not tell gets no action, whatever else says: it is neither
        // deleted nor overwritten, uploaded nor downloaded (S6).
        let local = match input.local.get(key) {
            Local::Unknown(_) => continue,
            Local::Present(file) => Some(file),
            Local::Absent => None,
        };
        let here = local.map(|f| ident(&f.hash, f.exec, f.link.is_some()));
        let there = input
            .remote
            .files
            .get(key)
            .map(|f| ident(&f.hash, f.exec, f.link.is_some()));
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
                // A copy of a file the drive deleted before this side knew it: neither deleted
                // nor sent again ([`deleted_elsewhere`] lists it).
                None => match input.remote.deleted.get(key) {
                    Some(tomb) if tomb.hash == h.0 => continue,
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
                        !input.local.is_absent(name)
                            || input.remote.files.contains_key(name)
                            || chosen.contains(name)
                    };
                    let copy = cut
                        .get(key)
                        .cloned()
                        .unwrap_or_else(|| conflict_name(key, input.device, input.date, &taken));
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
