//! Undoing a delete from this device: a file a pass moved into the folder's trash (because the
//! drive deleted it) is put back where it was. The next pass sends it up again: a file made here
//! with the bytes of one deleted earlier is the user's (the merge's D5 rule), so the undo travels
//! to every device. [`trashed`] lists what a restore would bring back, for the app's trash view.
//!
//! What is in the trash is read from the trash's own folders (`.azlin/sync/trash/<pass>/<the
//! file's path>`, AZ-S20): the operations log ([`OpLog`]) names the key of a trashed path and
//! which version of a file a pass trashed last, but it is a cache of descriptions - trimmed by
//! size, lost with the pairing's state folder when a drive is forgotten - and the files are in the
//! trash all the same. A file whose name is taken again comes back beside the file that has it
//! (AZ-U19), never over it and never skipped.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use azul_storage::beneath::{self, Dir, Kind, Root};

use super::{
    merge, names,
    oplog::{OpKind, OpLog},
    root::{TRASH_DAYS, TRASH_DIR},
};
use crate::error::{CloudResult, Context};

/// How many entries one look at the trash goes through at most, the newest passes first: a trash
/// of more is shown by its newest part (the date purge, [`super::root::purge_trash`], empties
/// the rest).
const LOOKS: usize = 200_000;
/// The deepest folder of a trashed path that is looked into.
const DEPTH: usize = 64;

/// A file of the folder's trash: the newest version of `key` a pass moved there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trashed {
    /// The file's key in the synced folder: where a restore puts it back.
    pub key: String,
    /// When it was moved into the trash (seconds since 1970), from its pass's trash folder.
    pub trashed_at: Option<i64>,
    /// Its size.
    pub bytes: u64,
}

impl Trashed {
    /// When the trash lets it go: [`TRASH_DAYS`] after it came (the bytes stay on the drive as
    /// its versions).
    #[must_use]
    pub fn kept_until(&self) -> Option<i64> {
        self.trashed_at.map(|at| at + TRASH_DAYS * 86_400)
    }
}

/// A file in the trash, as the trash's folders show it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Item {
    /// From the synced folder's root: `.azlin/sync/trash/<pass>/<its path>`.
    path: String,
    /// The pass's folder name (`20261024-110000-bbbbbb`): it sorts by time.
    pass: String,
    /// Where a restore puts it: the key the log names for `path`, else its path under the pass's
    /// folder.
    key: String,
    bytes: u64,
}

/// One walk through the trash.
struct Look<'a> {
    pass: String,
    named: &'a BTreeMap<String, String>,
    looks: usize,
    found: Vec<Item>,
}

/// Every file of the trash, the newest pass first.
fn in_trash(tree: &Root, named: &BTreeMap<String, String>) -> Vec<Item> {
    let Ok(trash) = tree.dir_existing(TRASH_DIR) else {
        return Vec::new();
    };
    let Ok(names) = trash.names() else {
        return Vec::new();
    };
    let mut passes: Vec<String> = names
        .iter()
        .filter_map(|name| name.to_str().map(str::to_string))
        .collect();
    passes.sort_unstable_by(|a, b| b.cmp(a));
    let mut out = Vec::new();
    let mut looks = LOOKS;
    for pass in passes {
        let Ok(folder) = trash.child(&pass) else {
            continue;
        };
        let rel = format!("{TRASH_DIR}/{pass}");
        let mut look = Look {
            pass,
            named,
            looks,
            found: Vec::new(),
        };
        walk(&mut look, &folder, &rel, "", 0);
        looks = look.looks;
        out.append(&mut look.found);
        if looks == 0 {
            break;
        }
    }
    out
}

/// The files below `dir` (`rel` from the root, `under` from the pass's folder), never through a
/// link.
fn walk(look: &mut Look<'_>, dir: &Dir, rel: &str, under: &str, depth: usize) {
    let Ok(mut names) = dir.names() else {
        return;
    };
    names.sort_unstable();
    for name in names {
        if look.looks == 0 {
            return;
        }
        look.looks -= 1;
        let Some(name) = name.to_str() else {
            continue;
        };
        let Ok(st) = dir.lstat(name) else {
            continue;
        };
        let here = format!("{rel}/{name}");
        let below = if under.is_empty() {
            name.to_string()
        } else {
            format!("{under}/{name}")
        };
        match st.kind {
            Kind::File => {
                let key = look.named.get(&here).cloned().unwrap_or(below);
                look.found.push(Item {
                    path: here,
                    pass: look.pass.clone(),
                    key,
                    bytes: st.size,
                });
            }
            Kind::Dir if depth < DEPTH => {
                if let Ok(child) = dir.child(name) {
                    walk(look, &child, &here, &below, depth + 1);
                }
            }
            _ => {}
        }
    }
}

/// The pass's folder name in a trash path: `20261024-110000-bbbbbb`.
fn pass_of(path: &str) -> &str {
    path.strip_prefix(TRASH_DIR)
        .unwrap_or(path)
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or("")
}

/// What the trash holds that a restore would bring back: each file's newest version in the
/// trash. The log only describes (the key of a trashed path) and tells one thing the folders
/// cannot: when the newest version a pass trashed of a file is not in the trash any more (put
/// back, or purged), the older versions of it are not offered in its place.
fn candidates(tree: &Root, state_dir: &Path) -> Vec<Item> {
    let mut named: BTreeMap<String, String> = BTreeMap::new();
    let mut logged: BTreeMap<String, String> = BTreeMap::new();
    for op in OpLog::new(state_dir).read() {
        if op.op != OpKind::Trash {
            continue;
        }
        let Some(to) = op.to else {
            continue;
        };
        named.insert(to.clone(), op.key.clone());
        // Oldest line first: the newest line of a key is the one that stays.
        logged.insert(op.key, to);
    }
    let mut seen: BTreeSet<String> = BTreeSet::new();
    in_trash(tree, &named)
        .into_iter()
        .filter(|item| {
            if !seen.insert(item.key.clone()) {
                return false;
            }
            !logged
                .get(&item.key)
                .is_some_and(|newest| *newest != item.path && pass_of(newest) > item.pass.as_str())
        })
        .collect()
}

/// Whether the file at the item's key is its bytes already: there is nothing to put back.
fn already_back(root: &Path, tree: &Root, item: &Item) -> bool {
    let Ok((dir, name)) = names::walk_record(tree, &item.key) else {
        return false;
    };
    let Ok(now) = dir.lstat(&name) else {
        return false;
    };
    now.kind == Kind::File
        && now.size == item.bytes
        && super::hash_here(root, &item.key).is_some_and(|here| {
            super::hash_here(root, &item.path).as_deref() == Some(here.as_str())
        })
}

/// What [`untrash`] would put back of the whole folder `root` of the pairing whose state folder
/// is `state_dir`: each file's newest trashed version that is still in the trash, newest first
/// (then by key). A file purged, or put back by hand, is not listed, nor one whose place holds
/// its bytes already; one whose place holds another file is: it comes back beside it.
#[must_use]
pub fn trashed(root: &Path, state_dir: &Path) -> Vec<Trashed> {
    let Ok(tree) = Root::open(root) else {
        return Vec::new();
    };
    let mut out: Vec<Trashed> = candidates(&tree, state_dir)
        .into_iter()
        .filter(|item| !already_back(root, &tree, item))
        .map(|item| Trashed {
            trashed_at: stamp_time(&item.path),
            bytes: item.bytes,
            key: item.key,
        })
        .collect();
    out.sort_by(|a, b| {
        b.trashed_at
            .cmp(&a.trashed_at)
            .then_with(|| a.key.cmp(&b.key))
    });
    out
}

/// When the pass whose trash folder holds `path` ran: the folder's name, `20261024-110000-...`.
fn stamp_time(path: &str) -> Option<i64> {
    let stamp = pass_of(path);
    let (date, rest) = stamp.split_once('-')?;
    let time = rest.get(..6)?;
    let digits = |text: &str| text.bytes().all(|b| b.is_ascii_digit());
    if date.len() != 8 || !digits(date) || !digits(time) {
        return None;
    }
    crate::parse_rfc3339(&format!(
        "{}-{}-{}T{}:{}:{}Z",
        date.get(..4)?,
        date.get(4..6)?,
        date.get(6..)?,
        time.get(..2)?,
        time.get(2..4)?,
        time.get(4..)?
    ))
}

/// What [`untrash`] did: the keys it put back, and what it left in the trash because the name the file
/// goes back to is one a synced folder refuses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Untrashed {
    /// The keys put back (a copy's key as it is now), in order.
    pub back: Vec<String>,
    /// What stays in the trash and in the list ([`trashed`]): each key and why its name is refused (a
    /// protected name in any spelling - a log line, a copied folder or an older build can put one into the
    /// trash). The restore goes on with the rest: one refused name is no block for "restore all"
    /// (X-DELTA-STRUCT-DRIVE-5).
    pub refused: Vec<(String, String)>,
}

/// Puts back what the passes of the pairing whose state folder is `state_dir` trashed of `key`
/// (a file, or a folder with everything in it; `""` the whole folder) in the synced folder
/// `root`: the newest trashed version of each file. A file whose place is free goes back to it;
/// one whose place holds another file comes back beside it, as `name (deleted 2026-10-24).ext`
/// (AZ-U19) - never over a file that is there, never skipped. A file whose place holds its bytes
/// already is left in the trash. A file whose name the rows of a synced name refuse is left in the trash
/// too, with its reason, and the restore goes on with the rest. The keys put back (a copy's key as it is
/// now) and the items left.
///
/// # Errors
///
/// When a file cannot be moved back (a refused name is no error: it is in [`Untrashed::refused`]).
pub fn untrash(root: &Path, state_dir: &Path, key: &str) -> CloudResult<Untrashed> {
    let folder = key.trim_end_matches('/');
    let under = |k: &str| {
        folder.is_empty()
            || k == folder
            || k.strip_prefix(folder)
                .is_some_and(|rest| rest.starts_with('/'))
    };
    let mut back = Vec::new();
    let mut refused = Vec::new();
    // Both names are reached from the folder's root without a link: a file never goes back
    // into a folder that a link below the synced folder points at.
    let tree = Root::open(root).with_context(|| root.display())?;
    // The name a file goes back to passes the rows of a synced name first (this system's; the disk's
    // folding does not matter for a file that was here): a protected name makes no folder, whoever
    // put the file into the trash (ARCH29 C6c).
    let policy = names::Policy::here(names::Volume::default());
    for item in candidates(&tree, state_dir) {
        if !under(&item.key) || already_back(root, &tree, &item) {
            continue;
        }
        // The file in the trash is an entry of the trash's folders: walked to, never made.
        let Ok((source, from)) = names::walk_entry(&tree, &item.path) else {
            continue;
        };
        // A name the rows refuse stays in the trash with its reason, and the restore goes on with the rest:
        // it fails nowhere and makes no folder (X-DELTA-STRUCT-DRIVE-5).
        if let Err(refusal) = names::admit_synced(&policy, &item.key) {
            refused.push((item.key.clone(), refusal.why));
            continue;
        }
        let (target, to) =
            names::make_parent_of(&tree, &policy, &item.key).with_context(|| item.key.clone())?;
        let (name, restored) = if target.lstat(&to).is_err() {
            (to, item.key.clone())
        } else {
            let label = match stamp_time(&item.path).map(crate::rfc3339) {
                Some(moment) => format!("deleted {}", moment.get(..10).unwrap_or_default()),
                None => String::from("deleted"),
            };
            let copy = merge::copy_name(&item.key, &label, &|candidate| {
                target.lstat(beneath::split_key(candidate).1).is_ok()
            });
            (beneath::split_key(&copy).1.to_string(), copy)
        };
        source
            .rename_noreplace(&from, &target, &name)
            .with_context(|| item.path.clone())?;
        target.sync();
        source.sync();
        back.push(restored);
    }
    back.sort();
    Ok(Untrashed { back, refused })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::sync::{local::path_of, oplog::Op};
    use azul_storage::testing::TempDir;

    /// ARCH29 C6c. A restore makes the folders of the name a file goes back to through the one create of
    /// the sync, after the rows of a synced name: a file in the trash under a protected spelling (the
    /// trash's folders are on the disk; a log line, a copied folder or an older build can put one there)
    /// is not put back and makes no folder of that name, and it stays in the trash; an ordinary file is
    /// restored with its folders. On the old code the restore made `GIT~1/hooks` and moved the file into it
    /// (the shape rows of Local only): RED by reasoning, not run.
    #[test]
    fn c6c_a_restore_makes_no_folder_for_a_protected_name_and_keeps_the_file_in_the_trash() {
        let root = TempDir::new("azcloud-untrash-c6c");
        let state = TempDir::new("azcloud-untrash-c6c-state");
        let put = |key: &str, text: &str| {
            let to = format!(".azlin/sync/trash/20261024-110000-aaaaaa/{key}");
            let path = path_of(root.path(), &to);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        };
        let poisoned = put("GIT~1/hooks/post-commit", "x");
        let plain = put("docs/2026/a.txt", "a");
        let refused = untrash(root.path(), state.path(), "GIT~1/hooks/post-commit").unwrap();
        assert!(refused.back.is_empty(), "{refused:?}");
        assert_eq!(refused.refused.len(), 1, "{refused:?}");
        assert_eq!(refused.refused[0].0, "GIT~1/hooks/post-commit");
        assert!(
            refused.refused[0]
                .1
                .contains("a version-control name (GIT~1)"),
            "{refused:?}"
        );
        assert!(!root.path().join("GIT~1").exists(), "a folder was made");
        assert_eq!(fs::read(&poisoned).unwrap(), b"x", "it stays in the trash");
        let back = untrash(root.path(), state.path(), "docs/2026/a.txt")
            .unwrap()
            .back;
        assert_eq!(back, vec![String::from("docs/2026/a.txt")]);
        assert_eq!(fs::read(root.path().join("docs/2026/a.txt")).unwrap(), b"a");
        assert!(!plain.exists(), "the file moved out of the trash");
    }

    /// X-DELTA-STRUCT-DRIVE-5: "restore all" of a trash that holds one name the rows refuse and three good
    /// ones puts the three back and reports the one with its reason; the refused one stays in the trash and in
    /// the list. The restore returned on the FIRST item whose name was refused: it moved the items before it,
    /// failed, failed at the same item at every later click, and the dialog showed only the error.
    #[test]
    fn x_delta_struct_drive_5_one_refused_name_does_not_stop_restore_all_and_stays_listed_with_its_reason(
    ) {
        let root = TempDir::new("azcloud-untrash-sd5");
        let state = TempDir::new("azcloud-untrash-sd5-state");
        let put = |key: &str, text: &str| {
            let to = format!(".azlin/sync/trash/20261024-110000-aaaaaa/{key}");
            let path = path_of(root.path(), &to);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        };
        let poisoned = put("GIT~1/hooks/post-commit", "x");
        put("docs/a.txt", "a");
        put("docs/b.txt", "b");
        put("photos/c.jpg", "c");
        assert_eq!(trashed(root.path(), state.path()).len(), 4);
        let done = untrash(root.path(), state.path(), "").expect("restore all finishes");
        assert_eq!(done.back, vec!["docs/a.txt", "docs/b.txt", "photos/c.jpg"]);
        assert_eq!(done.refused.len(), 1, "one reported: {done:?}");
        assert_eq!(done.refused[0].0, "GIT~1/hooks/post-commit");
        assert_eq!(fs::read(root.path().join("photos/c.jpg")).unwrap(), b"c");
        // The refused one stays where it was and in the list, and no folder of its name was made.
        assert_eq!(fs::read(&poisoned).unwrap(), b"x");
        assert!(!root.path().join("GIT~1").exists(), "a folder was made");
        let left = trashed(root.path(), state.path());
        assert_eq!(left.len(), 1, "{left:?}");
        assert_eq!(left[0].key, "GIT~1/hooks/post-commit");
        // The next click is the same: nothing fails at the refused item, nothing more comes back.
        let again = untrash(root.path(), state.path(), "").expect("and again");
        assert!(again.back.is_empty(), "{again:?}");
        assert_eq!(again.refused.len(), 1, "{again:?}");
    }

    #[test]
    fn a_trashed_folder_comes_back_newest_version_first_never_over_a_file() {
        let root = TempDir::new("azcloud-untrash");
        let state = TempDir::new("azcloud-untrash-state");
        let log = OpLog::new(state.path());
        let trash = |stamp: &str, key: &str, text: &str| {
            let to = format!(".azlin/sync/trash/{stamp}/{key}");
            let path = path_of(root.path(), &to);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            log.record(&Op {
                t: 1.0,
                op: OpKind::Trash,
                key: key.to_string(),
                to: Some(to),
                hash: None,
            })
            .unwrap();
        };
        trash("20261024-100000-aaaaaa", "photos/a.jpg", "old a");
        trash("20261024-110000-bbbbbb", "photos/a.jpg", "new a");
        trash("20261024-110000-bbbbbb", "photos/b.jpg", "b");
        trash(
            "20261024-110000-bbbbbb",
            "photosets/c.jpg",
            "not under photos",
        );
        fs::create_dir_all(root.path().join("photos")).unwrap();
        fs::write(root.path().join("photos/b.jpg"), "made again here").unwrap();
        let back = untrash(root.path(), state.path(), "photos").unwrap().back;
        // AZ-U19: this test pinned "a file whose name is taken is skipped"; it comes back beside
        // the file that has the name now.
        assert_eq!(
            back,
            vec![
                String::from("photos/a.jpg"),
                String::from("photos/b (deleted 2026-10-24).jpg")
            ]
        );
        assert_eq!(
            fs::read_to_string(root.path().join("photos/a.jpg")).unwrap(),
            "new a"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("photos/b.jpg")).unwrap(),
            "made again here",
            "never over a file that is there"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("photos/b (deleted 2026-10-24).jpg")).unwrap(),
            "b",
            "the trashed version, beside it"
        );
        assert!(!root.path().join("photosets/c.jpg").exists());
    }

    /// What the trash view lists is what Restore brings back: each file's newest trashed
    /// version that is still in the trash, newest first, with the day it was deleted and its
    /// size; a file purged or put back by hand is not listed.
    #[test]
    fn the_trash_lists_each_files_newest_version_still_there_newest_first() {
        let root = TempDir::new("azcloud-trashed");
        let state = TempDir::new("azcloud-trashed-state");
        let log = OpLog::new(state.path());
        let trash = |stamp: &str, key: &str, text: &str| {
            let to = format!(".azlin/sync/trash/{stamp}/{key}");
            let path = path_of(root.path(), &to);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            log.record(&Op {
                t: 1.0,
                op: OpKind::Trash,
                key: key.to_string(),
                to: Some(to),
                hash: None,
            })
            .unwrap();
            path
        };
        trash("20261024-100000-aaaaaa", "photos/a.jpg", "old a");
        trash("20261024-110000-bbbbbb", "photos/a.jpg", "new a!");
        trash("20261025-090000-cccccc", "notes/b.txt", "b");
        let gone = trash("20261026-080000-dddddd", "notes/c.txt", "purged");
        fs::remove_file(gone).unwrap();
        let list = trashed(root.path(), state.path());
        let keys: Vec<&str> = list.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(
            keys,
            ["notes/b.txt", "photos/a.jpg"],
            "newest first, one row per file"
        );
        let a = &list[1];
        assert_eq!(a.bytes, 6, "the newest version's size");
        assert_eq!(a.trashed_at, crate::parse_rfc3339("2026-10-24T11:00:00Z"));
        assert_eq!(
            a.kept_until(),
            crate::parse_rfc3339("2026-11-23T11:00:00Z"),
            "kept for the trash's days"
        );
        // Restored, it leaves the list.
        untrash(root.path(), state.path(), "photos").unwrap();
        let keys: Vec<String> = trashed(root.path(), state.path())
            .into_iter()
            .map(|t| t.key)
            .collect();
        assert_eq!(keys, ["notes/b.txt"]);
    }

    /// A file put in the trash on `pass` (its folder name) at `key`: where it lies.
    fn put_in_trash(root: &Path, pass: &str, key: &str, text: &str) -> String {
        let to = format!(".azlin/sync/trash/{pass}/{key}");
        let path = path_of(root, &to);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        to
    }

    /// AZ-S20. The trash's index was the operations log, which drops its older half past 4 MiB (and
    /// goes with the state folder when a drive is forgotten): the files were still in the trash
    /// but neither the trash view nor Restore could find them, and the date purge removed them
    /// silently. RED not run: the old `trashed` lists nothing here (no line names the files).
    #[test]
    fn az_s20_the_trash_is_read_from_its_folders_when_the_log_lost_its_lines() {
        let root = TempDir::new("azcloud-trash-s20");
        let state = TempDir::new("azcloud-trash-s20-state");
        let log = OpLog::new(state.path());
        let a = put_in_trash(root.path(), "20261024-100000-aaaaaa", "photos/a.jpg", "a");
        log.record(&Op {
            t: 1.0,
            op: OpKind::Trash,
            key: String::from("photos/a.jpg"),
            to: Some(a),
            hash: None,
        })
        .unwrap();
        put_in_trash(root.path(), "20261024-110000-bbbbbb", "notes/b.txt", "bb");
        // The log grows past its size: the older half goes, the Trash line with it.
        let filler = "x".repeat(1_200_000);
        for _ in 0..4 {
            log.record(&Op {
                t: 2.0,
                op: OpKind::Move,
                key: filler.clone(),
                to: None,
                hash: None,
            })
            .unwrap();
        }
        assert!(
            !log.read().iter().any(|op| op.op == OpKind::Trash),
            "the log trimmed itself and lost the Trash line"
        );
        let list = trashed(root.path(), state.path());
        let keys: Vec<&str> = list.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(keys, ["notes/b.txt", "photos/a.jpg"], "{list:?}");
        assert_eq!(list[1].bytes, 1);
        // A forgotten drive: its state folder is gone, the trash in the folder is not.
        let gone = TempDir::new("azcloud-trash-s20-forgotten");
        let back = untrash(root.path(), gone.path(), "").unwrap().back;
        assert_eq!(back, ["notes/b.txt", "photos/a.jpg"]);
        assert_eq!(
            fs::read_to_string(root.path().join("photos/a.jpg")).unwrap(),
            "a"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("notes/b.txt")).unwrap(),
            "bb"
        );
    }

    /// AZ-U19. A trashed file whose name was taken again was skipped by Restore and dropped from
    /// the list, so the old version could not be reached from the app. RED not run: the old
    /// `untrash` returns nothing for it and `trashed` does not list it.
    #[test]
    fn az_u19_a_trashed_file_whose_name_is_taken_again_comes_back_beside_it() {
        let root = TempDir::new("azcloud-trash-u19");
        let state = TempDir::new("azcloud-trash-u19-state");
        let log = OpLog::new(state.path());
        for (pass, key, text) in [
            (
                "20261024-110000-bbbbbb",
                "docs/plan.txt",
                "the plan as it was",
            ),
            ("20261024-110000-bbbbbb", "docs/same.txt", "same bytes"),
        ] {
            let to = put_in_trash(root.path(), pass, key, text);
            log.record(&Op {
                t: 1.0,
                op: OpKind::Trash,
                key: key.to_string(),
                to: Some(to),
                hash: None,
            })
            .unwrap();
        }
        fs::create_dir_all(root.path().join("docs")).unwrap();
        fs::write(
            root.path().join("docs/plan.txt"),
            "a new plan, made again here",
        )
        .unwrap();
        fs::write(root.path().join("docs/same.txt"), "same bytes").unwrap();
        // Listed although its name is taken; not listed when the bytes are back already.
        let keys: Vec<String> = trashed(root.path(), state.path())
            .into_iter()
            .map(|t| t.key)
            .collect();
        assert_eq!(keys, ["docs/plan.txt"]);
        let back = untrash(root.path(), state.path(), "docs").unwrap().back;
        assert_eq!(back, ["docs/plan (deleted 2026-10-24).txt"]);
        assert_eq!(
            fs::read_to_string(root.path().join("docs/plan.txt")).unwrap(),
            "a new plan, made again here",
            "never over the file that is there"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("docs/plan (deleted 2026-10-24).txt")).unwrap(),
            "the plan as it was"
        );
        // Once more with the name taken again: another copy, never the same name over a file.
        put_in_trash(
            root.path(),
            "20261025-090000-cccccc",
            "docs/plan.txt",
            "plan, third version",
        );
        let again = untrash(root.path(), state.path(), "docs/plan.txt")
            .unwrap()
            .back;
        assert_eq!(again, ["docs/plan (deleted 2026-10-25).txt"]);
    }
}
