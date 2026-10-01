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

use azul_storage::{Drive, DriveError, ListRequest};

use crate::model::{self, KeyKind, Settings, Task, TaskList};

/// Keys per listing page.
pub const PAGE_SIZE: u32 = 1000;

/// A file that could not be read, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub key: String,
    pub reason: String,
}

/// Everything under `tasks/`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    /// By order, then name.
    pub lists: Vec<TaskList>,
    pub tasks: Vec<Task>,
    pub settings: Option<Settings>,
    pub skipped: Vec<Skipped>,
}

/// Every key under `prefix`, `page` keys per listing call.
pub fn all_keys(drive: &dyn Drive, prefix: &str, page: u32) -> Result<Vec<String>, DriveError> { todo!() }

/// Reads every list, task and the settings. A file that cannot be read is named in
/// `skipped` and left alone; tasks in a folder without its `list.json` get an "Untitled
/// list" so they still show (the list file is written when that list is next changed).
pub fn load_all(drive: &dyn Drive) -> Result<Loaded, DriveError> { todo!() }

/// [`load_all`] with `page` keys per listing call.
pub fn load_all_paged(drive: &dyn Drive, page: u32) -> Result<Loaded, DriveError> { todo!() }

// ==== The write queue ====

/// A write the queue holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    Put { key: String, bytes: Vec<u8> },
    Delete { key: String },
}

impl Write {
    #[must_use]
    pub fn key(&self) -> &str { todo!() }
}

/// The writes waiting, the batch in flight and the failures kept for a retry.
#[derive(Debug, Clone, Default)]
pub struct WriteQueue {
    pending: Vec<Write>,
    failed: Vec<(Write, String)>,
    in_flight: usize,
}

impl WriteQueue {
    #[must_use]
    pub fn new() -> Self { todo!() }

    /// Writes `bytes` to `key` (replacing a waiting write of `key`).
    pub fn put(&mut self, key: String, bytes: Vec<u8>) { todo!() }

    /// Deletes `key` (replacing a waiting write of `key`).
    pub fn delete(&mut self, key: String) { todo!() }

    fn replace(&mut self, write: Write) { todo!() }

    /// The next batch: everything waiting, unless a batch is still in flight.
    pub fn take(&mut self) -> Option<Vec<Write>> { todo!() }

    /// The batch in flight is done; `failed` are its writes that did not land, with why.
    pub fn finish(&mut self, failed: Vec<(Write, String)>) { todo!() }

    /// Queues the failed writes again (before anything newer).
    pub fn retry(&mut self) { todo!() }

    /// Writes waiting (not counting the batch in flight).
    #[must_use]
    pub fn pending(&self) -> usize { todo!() }

    /// Writes of the batch in flight.
    #[must_use]
    pub fn in_flight(&self) -> usize { todo!() }

    /// The failures kept for a retry: `(key, why)`.
    #[must_use]
    pub fn failures(&self) -> Vec<(String, String)> { todo!() }

    /// Nothing waiting, nothing in flight.
    #[must_use]
    pub fn is_idle(&self) -> bool { todo!() }
}

/// What a batch did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchResult {
    /// The keys written or deleted.
    pub done: Vec<String>,
    /// The writes that failed, with why.
    pub failed: Vec<(Write, String)>,
}

/// Runs a batch against the drive, in order.
pub fn run_batch(drive: &dyn Drive, batch: Vec<Write>) -> BatchResult { todo!() }

// ==== Attachments ====

/// Copies the file at `path` to `key`; returns its size.
pub fn attach_file(drive: &dyn Drive, key: &str, path: &Path) -> Result<u64, DriveError> { todo!() }

/// Fetches `key` into the folder `dir` (made if missing), under its own name; returns the
/// file's path.
pub fn fetch_file(drive: &dyn Drive, key: &str, dir: &Path) -> Result<PathBuf, DriveError> { todo!() }

/// Moves every file under the folder `from` to the folder `to` (a task's attachments when
/// the task moves to another list); returns how many.
pub fn move_files(drive: &dyn Drive, from: &str, to: &str) -> Result<usize, DriveError> { todo!() }

/// Deletes every file under the folder `prefix`; returns how many.
pub fn delete_files(drive: &dyn Drive, prefix: &str) -> Result<usize, DriveError> { todo!() }

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use azul_storage::LocalDrive;
    use chrono::{NaiveDate, NaiveDateTime};

    use super::*;
    use crate::model::{list_to_json, settings_to_json, task_to_json, Attachment};

    /// A folder of its own under the system's temporary folder, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn create() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "aztasks-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

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
    fn saved_lists_tasks_and_settings_load_back_from_their_files() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let lists = vec![
            TaskList::new(HOME.into(), "Home".into(), 1),
            TaskList::new(WORK.into(), "Work".into(), 2),
        ];
        let tasks = vec![task("a1", WORK, 1024), task("b2", WORK, 2048), task("c3", HOME, 1024)];
        put_all(&drive, &lists, &tasks);
        let settings = Settings {
            default_list: WORK.into(),
            ..Settings::default()
        };
        drive
            .put(model::SETTINGS_KEY, settings_to_json(&settings).as_bytes())
            .unwrap();
        assert!(dir.0.join("tasks").join(WORK).join("list.json").is_file());
        assert!(dir.0.join("tasks").join(WORK).join("a1.json").is_file());

        let loaded = load_all(&drive).unwrap();
        assert_eq!(loaded.lists, lists);
        let mut got = loaded.tasks.clone();
        got.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(got, tasks);
        assert_eq!(loaded.settings, Some(settings));
        assert!(loaded.skipped.is_empty());
    }

    #[test]
    fn a_file_that_cannot_be_read_is_skipped_and_named() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        put_all(&drive, &[TaskList::new(WORK.into(), "Work".into(), 1)], &[task("ok", WORK, 1)]);
        let broken = model::task_key(WORK, "broken");
        drive.put(&broken, b"{ not json").unwrap();
        let wrong_id = model::task_key(WORK, "other");
        drive.put(&wrong_id, task_to_json(&task("elsewhere", WORK, 2)).as_bytes()).unwrap();
        drive.put("tasks/readme.txt", b"hello").unwrap();
        let loaded = load_all(&drive).unwrap();
        assert_eq!(loaded.tasks.len(), 1);
        let keys: Vec<&str> = loaded.skipped.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(keys, vec![broken.as_str(), wrong_id.as_str()]);
    }

    #[test]
    fn tasks_in_a_folder_without_its_list_file_still_load_under_a_list() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        put_all(&drive, &[], &[task("lost", HOME, 1)]);
        let loaded = load_all(&drive).unwrap();
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.lists.len(), 1);
        assert_eq!(loaded.lists[0].id, HOME);
        assert_eq!(loaded.lists[0].name, "Untitled list");
    }

    #[test]
    fn a_task_file_in_another_folder_belongs_to_that_folders_list() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let t = task("moved", WORK, 1);
        drive.put(&model::task_key(HOME, "moved"), task_to_json(&t).as_bytes()).unwrap();
        let loaded = load_all(&drive).unwrap();
        assert_eq!(loaded.tasks[0].list, HOME);
    }

    #[test]
    fn more_keys_than_a_page_all_load() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let tasks: Vec<Task> = (0..7).map(|n| task(&format!("t{n}"), WORK, n)).collect();
        put_all(&drive, &[TaskList::new(WORK.into(), "Work".into(), 1)], &tasks);
        let loaded = load_all_paged(&drive, 2).unwrap();
        assert_eq!(loaded.tasks.len(), 7);
        assert_eq!(all_keys(&drive, "tasks/", 3).unwrap().len(), 8);
    }

    #[test]
    fn the_write_queue_keeps_only_the_last_write_of_a_key() {
        let mut q = WriteQueue::new();
        q.put("tasks/a/1.json".into(), b"one".to_vec());
        q.put("tasks/a/2.json".into(), b"two".to_vec());
        q.put("tasks/a/1.json".into(), b"three".to_vec());
        q.delete("tasks/a/2.json".into());
        assert_eq!(q.pending(), 2);
        let batch = q.take().unwrap();
        assert_eq!(
            batch,
            vec![
                Write::Put {
                    key: "tasks/a/1.json".into(),
                    bytes: b"three".to_vec()
                },
                Write::Delete {
                    key: "tasks/a/2.json".into()
                }
            ]
        );
    }

    #[test]
    fn the_write_queue_sends_one_batch_at_a_time_and_keeps_failures_for_a_retry() {
        let mut q = WriteQueue::new();
        q.put("k1".into(), b"1".to_vec());
        let first = q.take().unwrap();
        q.put("k2".into(), b"2".to_vec());
        assert_eq!(q.take(), None, "one batch in flight");
        assert_eq!(q.in_flight(), 1);
        q.finish(vec![(first[0].clone(), "disk full".into())]);
        assert_eq!(q.failures(), vec![("k1".to_string(), "disk full".to_string())]);
        assert_eq!(q.take().unwrap().len(), 1, "k2 goes; k1 waits for a retry");
        q.finish(Vec::new());
        assert!(q.is_idle());
        q.retry();
        assert_eq!(q.take().unwrap(), first);
        q.finish(Vec::new());
        assert!(q.failures().is_empty());

        // A newer write of a failed key replaces the failure.
        q.put("k3".into(), b"3".to_vec());
        let batch = q.take().unwrap();
        q.put("k3".into(), b"4".to_vec());
        q.finish(vec![(batch[0].clone(), "offline".into())]);
        assert!(q.failures().is_empty(), "superseded by the waiting write");
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
