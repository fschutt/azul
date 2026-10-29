//! Undoing a delete from this device: a file a pass moved into the folder's trash (because the
//! drive deleted it) is put back where it was, found through the operations log, which names
//! each file's place in the trash. The next pass sends it up again: a file made here with the
//! bytes of one deleted earlier is the user's (the merge's D5 rule), so the undo travels to
//! every device.

use std::{collections::BTreeSet, path::Path};

use azul_storage::beneath::Root;

use super::oplog::{OpKind, OpLog};
use crate::error::{CloudResult, Context};

/// Puts back what the passes of the pairing whose state folder is `state_dir` trashed of `key`
/// (a file, or a folder with everything in it; `""` the whole folder) in the synced folder
/// `root`: the newest trashed version of each file, never over a file that is there now. The
/// keys put back.
///
/// # Errors
///
/// When a file cannot be moved back.
pub fn untrash(root: &Path, state_dir: &Path, key: &str) -> CloudResult<Vec<String>> {
    let folder = key.trim_end_matches('/');
    let under = |k: &str| {
        folder.is_empty()
            || k == folder
            || k.strip_prefix(folder).is_some_and(|rest| rest.starts_with('/'))
    };
    let mut done: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();
    // Both names are reached from the folder's root without a link: a file never goes back
    // into a folder that a link below the synced folder points at.
    let tree = Root::open(root).with_context(|| root.display())?;
    for op in OpLog::new(state_dir).read().into_iter().rev() {
        if op.op != OpKind::Trash || !under(&op.key) || done.contains(&op.key) {
            continue;
        }
        let Some(trashed) = op.to.as_deref() else {
            continue;
        };
        // Gone from the trash already (purged, put back by hand), or a file is there now.
        let (source, from) = match tree.parent_of(trashed, false) {
            Ok((dir, name)) if dir.lstat(name).is_ok() => (dir, name),
            _ => {
                done.insert(op.key.clone());
                continue;
            }
        };
        let (target, to) = tree
            .parent_of(&op.key, true)
            .with_context(|| op.key.clone())?;
        if target.lstat(to).is_ok() {
            done.insert(op.key.clone());
            continue;
        }
        source
            .rename(from, &target, to)
            .with_context(|| trashed.to_string())?;
        target.sync();
        source.sync();
        done.insert(op.key.clone());
        out.push(op.key);
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::sync::{local::path_of, oplog::Op};
    use azul_storage::testing::TempDir;

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
            log.record(&Op { t: 1.0, op: OpKind::Trash, key: key.to_string(), to: Some(to), hash: None })
                .unwrap();
        };
        trash("20261024-100000-aaaaaa", "photos/a.jpg", "old a");
        trash("20261024-110000-bbbbbb", "photos/a.jpg", "new a");
        trash("20261024-110000-bbbbbb", "photos/b.jpg", "b");
        trash("20261024-110000-bbbbbb", "photosets/c.jpg", "not under photos");
        fs::create_dir_all(root.path().join("photos")).unwrap();
        fs::write(root.path().join("photos/b.jpg"), "made again here").unwrap();
        let back = untrash(root.path(), state.path(), "photos").unwrap();
        assert_eq!(back, vec![String::from("photos/a.jpg")]);
        assert_eq!(fs::read_to_string(root.path().join("photos/a.jpg")).unwrap(), "new a");
        assert_eq!(
            fs::read_to_string(root.path().join("photos/b.jpg")).unwrap(),
            "made again here",
            "never over a file that is there"
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
            log.record(&Op { t: 1.0, op: OpKind::Trash, key: key.to_string(), to: Some(to), hash: None })
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
        assert_eq!(keys, ["notes/b.txt", "photos/a.jpg"], "newest first, one row per file");
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
        let keys: Vec<String> = trashed(root.path(), state.path()).into_iter().map(|t| t.key).collect();
        assert_eq!(keys, ["notes/b.txt"]);
    }
}
