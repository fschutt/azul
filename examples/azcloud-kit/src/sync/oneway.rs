//! A backup pairing: one way, from this folder to the drive. The folder is the source; what
//! happens on the drive never changes it, and what is deleted here stays on the drive.
//!
//! The three-way plan is made as for every pairing, then each action that would change the
//! folder, or delete on the drive, becomes its one-way answer:
//!
//! | the plan says                     | a backup does                                         |
//! |-----------------------------------|-------------------------------------------------------|
//! | delete on the drive (gone here)   | keep the drive's copy: cloud only here                |
//! | delete here (gone on the drive)   | send this folder's file again (it stays here)         |
//! | download over a file here         | send this folder's version: the source wins           |
//! | download of a file not here       | leave it on the drive: cloud only here                |
//! | conflict (changed on both sides)  | send this folder's version                            |
//!
//! A file kept in the cloud only that the drive deletes is forgotten (nothing of it is here).
//! Nothing here is ever trashed, overwritten or renamed by a backup's pass, and no delete
//! reaches the drive, whichever path (one file, a folder, a whole-folder batch) it came from.

use std::collections::BTreeMap;

use super::{local::BaseEntry, merge::Action, Scan};

/// Turns `actions` (the two-way plan) into the backup pairing's plan.
pub(super) fn backup_plan(actions: &mut Vec<Action>, scan: &Scan, base: &BTreeMap<String, BaseEntry>) {
    let on_disk = |key: &str| scan.files.contains_key(key);
    let kept_in_cloud = |key: &str| base.get(key).is_some_and(|e| e.cloud_only);
    for action in actions.iter_mut() {
        let next = match &*action {
            Action::DeleteRemote { key } => Action::CloudOnly { key: key.clone() },
            Action::DeleteLocal { key } if on_disk(key) => Action::Upload { key: key.clone() },
            Action::DeleteLocal { key } if kept_in_cloud(key) => Action::Forget { key: key.clone() },
            Action::Download { key } | Action::Conflict { key, .. } if on_disk(key) => {
                Action::Upload { key: key.clone() }
            }
            Action::Download { key } => Action::CloudOnly { key: key.clone() },
            _ => continue,
        };
        *action = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::local::LocalFile;

    fn scan_of(keys: &[&str]) -> Scan {
        let mut scan = Scan::default();
        for key in keys {
            scan.files.insert((*key).to_string(), LocalFile::default());
        }
        scan
    }

    fn act(key: &str, f: fn(String) -> Action) -> Action {
        f(key.to_string())
    }

    #[test]
    fn a_backup_never_deletes_on_the_drive_and_never_changes_the_folder() {
        let scan = scan_of(&["here.txt", "both.txt", "theirs.txt"]);
        let mut base = BTreeMap::new();
        base.insert(String::from("in-cloud.txt"), BaseEntry { cloud_only: true, ..BaseEntry::default() });
        let mut actions = vec![
            act("gone-here.txt", |key| Action::DeleteRemote { key }),
            act("here.txt", |key| Action::DeleteLocal { key }),
            act("in-cloud.txt", |key| Action::DeleteLocal { key }),
            act("theirs.txt", |key| Action::Download { key }),
            act("new-there.txt", |key| Action::Download { key }),
            Action::Conflict { key: String::from("both.txt"), copy: String::from("both (conflict).txt") },
            act("same.txt", |key| Action::Agree { key }),
        ];
        backup_plan(&mut actions, &scan, &base);
        assert_eq!(
            actions,
            vec![
                act("gone-here.txt", |key| Action::CloudOnly { key }),
                act("here.txt", |key| Action::Upload { key }),
                act("in-cloud.txt", |key| Action::Forget { key }),
                act("theirs.txt", |key| Action::Upload { key }),
                act("new-there.txt", |key| Action::CloudOnly { key }),
                act("both.txt", |key| Action::Upload { key }),
                act("same.txt", |key| Action::Agree { key }),
            ]
        );
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::DeleteRemote { .. } | Action::DeleteLocal { .. } | Action::Download { .. } | Action::Conflict { .. }
        )));
    }
}
