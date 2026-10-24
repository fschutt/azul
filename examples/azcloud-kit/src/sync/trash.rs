//! Undoing a delete from this device: a file a pass moved into the folder's trash (because the
//! drive deleted it) is put back where it was, found through the operations log, which names
//! each file's place in the trash. The next pass sends it up again: a file made here with the
//! bytes of one deleted earlier is the user's (the merge's D5 rule), so the undo travels to
//! every device.

use std::{collections::BTreeSet, fs, path::Path};

use super::{
    local::path_of,
    oplog::{OpKind, OpLog},
};
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
    for op in OpLog::new(state_dir).read().into_iter().rev() {
        if op.op != OpKind::Trash || !under(&op.key) || done.contains(&op.key) {
            continue;
        }
        let Some(trashed) = op.to.as_deref() else {
            continue;
        };
        let from = path_of(root, trashed);
        let to = path_of(root, &op.key);
        // Gone from the trash already (purged, put back by hand), or a file is there now.
        if fs::symlink_metadata(&from).is_err() || fs::symlink_metadata(&to).is_ok() {
            done.insert(op.key.clone());
            continue;
        }
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).with_context(|| parent.display())?;
        }
        fs::rename(&from, &to).with_context(|| from.display())?;
        done.insert(op.key.clone());
        out.push(op.key);
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::oplog::Op;
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
}
