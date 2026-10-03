//! The files, through `azul-storage`'s [`Drive`]: loading everything at start, and the
//! write-behind queue every change goes through.
//!
//! Every call here blocks; the app makes them on an azul `Thread`, never in a callback, so
//! a `LocalDrive` today and an `S3Drive` later are the same to it.
//!
//! The queue keeps one write per key (a newer write of a key replaces the older one) and
//! hands out one batch at a time: the batch in flight finishes before the next starts, so
//! two writes of one task can never land out of order. A write that fails is kept aside
//! for a retry (the status bar's sync button), unless a newer write of its key came since.

use std::{
    fs,
    path::{Path, PathBuf},
};

use azul_storage::{Drive, DriveError};

/// Loading the store (every list, task and the settings, a file that cannot be read named and
/// left alone) is the task store's, shared with every To-Do bar (DEDUP_EDITORS B12).
pub use azul_pim::task_store::{all_keys, load_all, load_all_paged, Loaded, Skipped, PAGE_SIZE};

// ==== The write queue ====

/// The write-behind queue (one write per key, one batch in flight, failures kept for a retry)
/// and its batch runner are the PIM apps' one queue: AzCalendar writes through it too.
pub use azul_pim::write_queue::{run_batch, BatchResult, Write, WriteQueue};

// ==== Attachments ====

/// Copies the file at `path` to `key`; returns its size.
pub fn attach_file(drive: &dyn Drive, key: &str, path: &Path) -> Result<u64, DriveError> {
    let bytes = fs::read(path).map_err(|e| DriveError::Io(format!("{}: {e}", path.display())))?;
    drive.put(key, &bytes)?;
    Ok(u64::try_from(bytes.len()).unwrap_or(u64::MAX))
}

/// Fetches `key` into the folder `dir` (made if missing), under its own name; returns the
/// file's path.
pub fn fetch_file(drive: &dyn Drive, key: &str, dir: &Path) -> Result<PathBuf, DriveError> {
    let name = azul_storage::key::safe_file_name(key).ok_or_else(|| DriveError::InvalidKey {
        key: key.to_string(),
        reason: "it has no file name",
    })?;
    let bytes = drive.get(key)?;
    fs::create_dir_all(dir).map_err(|e| DriveError::Io(format!("{}: {e}", dir.display())))?;
    let path = dir.join(name);
    fs::write(&path, bytes).map_err(|e| DriveError::Io(format!("{}: {e}", path.display())))?;
    Ok(path)
}

/// Moves every file under the folder `from` to the folder `to` (a task's attachments when
/// the task moves to another list); returns how many.
pub fn move_files(drive: &dyn Drive, from: &str, to: &str) -> Result<usize, DriveError> {
    let keys = all_keys(drive, from, PAGE_SIZE)?;
    for key in &keys {
        let rest = &key[from.len()..];
        let bytes = drive.get(key)?;
        drive.put(&format!("{to}{rest}"), &bytes)?;
        drive.delete(key)?;
    }
    Ok(keys.len())
}

/// Deletes every file under the folder `prefix`; returns how many.
pub fn delete_files(drive: &dyn Drive, prefix: &str) -> Result<usize, DriveError> {
    let keys = all_keys(drive, prefix, PAGE_SIZE)?;
    for key in &keys {
        drive.delete(key)?;
    }
    Ok(keys.len())
}

#[cfg(test)]
mod tests {
    use azul_pim::testing::TempDir;
    use azul_storage::LocalDrive;
    use chrono::{NaiveDate, NaiveDateTime};

    use super::*;
    use crate::model::{self, list_to_json, task_to_json, Attachment, Task, TaskList};

    const WORK: &str = "9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3";
    const HOME: &str = "1a2b3c4d-0000-4000-8000-000000000001";

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 1)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
    }

    fn task(id: &str, list: &str, order: i64) -> Task {
        let mut t = Task::new(id.into(), list.into(), format!("task {id}"), now());
        t.order = order;
        t
    }

    fn put_all(drive: &dyn Drive, lists: &[TaskList], tasks: &[Task]) {
        let mut q = WriteQueue::new();
        for l in lists {
            q.put(l.key(), list_to_json(l).into_bytes());
        }
        for t in tasks {
            q.put(t.key(), task_to_json(t).into_bytes());
        }
        let r = run_batch(drive, q.take().unwrap());
        assert!(r.failed.is_empty(), "{:?}", r.failed);
    }

    #[test]
    fn a_moved_task_lives_only_in_its_new_list_after_the_batch() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let lists = vec![
            TaskList::new(WORK.into(), "Work".into(), 1),
            TaskList::new(HOME.into(), "Home".into(), 2),
        ];
        let mut t = task("mover", WORK, 1);
        put_all(&drive, &lists, &[t.clone()]);
        let old_key = t.key();
        t.list = HOME.into();
        let mut q = WriteQueue::new();
        q.delete(old_key);
        q.put(t.key(), task_to_json(&t).into_bytes());
        let r = run_batch(&drive, q.take().unwrap());
        assert_eq!(r.done.len(), 2);
        let loaded = load_all(&drive).unwrap();
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].list, HOME);
    }

    #[test]
    fn an_attachment_is_stored_next_to_its_task_and_comes_back() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(dir.0.join("data"));
        let source = dir.0.join("contract.pdf");
        fs::write(&source, b"%PDF-1.7 hello").unwrap();
        let key = model::attachment_key(WORK, "t1", "contract.pdf").unwrap();
        assert_eq!(attach_file(&drive, &key, &source).unwrap(), 14);
        assert!(dir.0.join("data/tasks").join(WORK).join("t1/contract.pdf").is_file());
        let mut t = task("t1", WORK, 1);
        t.attachments.push(Attachment {
            name: "contract.pdf".into(),
            size: 14,
        });
        put_all(&drive, &[TaskList::new(WORK.into(), "Work".into(), 1)], &[t]);
        let loaded = load_all(&drive).unwrap();
        assert_eq!(loaded.tasks[0].attachments.len(), 1, "the file is not a task");
        let out = fetch_file(&drive, &key, &dir.0.join("open")).unwrap();
        assert_eq!(fs::read(out).unwrap(), b"%PDF-1.7 hello");

        let moved = move_files(
            &drive,
            &model::attachments_prefix(WORK, "t1"),
            &model::attachments_prefix(HOME, "t1"),
        )
        .unwrap();
        assert_eq!(moved, 1);
        assert!(drive.get(&key).is_err());
        let home_key = model::attachment_key(HOME, "t1", "contract.pdf").unwrap();
        assert_eq!(drive.get(&home_key).unwrap(), b"%PDF-1.7 hello");
        assert_eq!(delete_files(&drive, &model::attachments_prefix(HOME, "t1")).unwrap(), 1);
        assert!(drive.get(&home_key).is_err());
    }
}
