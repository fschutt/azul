//! Loading the task store through `azul-storage`'s [`Drive`]: every list, task and the settings
//! under `tasks/`, the way AzTasks loads at start and a To-Do bar reads (moved here from AzTasks'
//! `store.rs`, scripts/DEDUP_EDITORS_2026_10_02.md B12).
//!
//! Every call here blocks; an app makes them on an azul `Thread`, never in a callback, so a
//! `LocalDrive` today and an `S3Drive` later are the same to it. Writing is one `put` of a
//! file's JSON at its key ([`crate::task::Task::key`], [`crate::task::task_to_json`]); AzTasks
//! queues its writes, a To-Do bar puts one file at a time.

use azul_storage::{Drive, DriveError};
use chrono::{NaiveDateTime, Timelike};

use crate::task::{
    self, list_from_json, settings_from_json, task_from_calendar_json, task_from_json,
    task_to_json, unnamed_list_name, FileError, KeyKind, Settings, Task, TaskList, DEFAULT_LIST,
};

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
    /// The keys of task files read from an older format (AzCalendar's `azcalendar.task`): the
    /// app that loaded them writes them back in the shared one.
    pub migrated: Vec<String>,
}

/// Every key under `prefix`, `page` keys per listing call.
pub fn all_keys(drive: &dyn Drive, prefix: &str, page: u32) -> Result<Vec<String>, DriveError> {
    Ok(azul_storage::ops::list_all_paged(drive, prefix, page)?
        .into_iter()
        .map(|o| o.key)
        .collect())
}

/// Reads every list, task and the settings. A file that cannot be read is named in `skipped`
/// and left alone; tasks in a folder without its `list.json` get a list of their own
/// ([`unnamed_list_name`]) so they still show (the list file is written when that list is next
/// changed).
pub fn load_all(drive: &dyn Drive) -> Result<Loaded, DriveError> {
    load_all_paged(drive, PAGE_SIZE)
}

/// [`load_all`] with `page` keys per listing call.
pub fn load_all_paged(drive: &dyn Drive, page: u32) -> Result<Loaded, DriveError> {
    let mut out = Loaded::default();
    // A task migrated from AzCalendar's old file was made "now" (the old file had no dates).
    let now = chrono::Local::now().naive_local();
    let now = now.with_nanosecond(0).unwrap_or(now);
    let prefix = format!("{}/", task::TASKS_DIR);
    for key in all_keys(drive, &prefix, page)? {
        let kind = task::parse_key(&key);
        if matches!(kind, KeyKind::Attachment { .. } | KeyKind::Other) {
            continue;
        }
        let text = match drive.get(&key) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) => {
                out.skipped.push(Skipped {
                    key,
                    reason: e.to_string(),
                });
                continue;
            }
        };
        let skip = |out: &mut Loaded, key: String, reason: String| {
            out.skipped.push(Skipped { key, reason });
        };
        match kind {
            KeyKind::Settings => match settings_from_json(&text) {
                Ok(s) => out.settings = Some(s),
                Err(e) => skip(&mut out, key, e.to_string()),
            },
            KeyKind::List { list } => match list_from_json(&text) {
                Ok(l) if l.id == list => out.lists.push(l),
                Ok(_) => skip(&mut out, key, "its id is not its folder's".into()),
                Err(e) => skip(&mut out, key, e.to_string()),
            },
            KeyKind::Task { list, task } => match task_from_json(&text) {
                Ok(mut t) if t.id == task => {
                    // The folder says which list a task is in.
                    t.list = list;
                    out.tasks.push(t);
                }
                Ok(_) => skip(&mut out, key, "its id is not its file's name".into()),
                // AzCalendar's old To-Do bar file: read, and named for a rewrite.
                Err(FileError::WrongFormat) => match task_from_calendar_json(&text, &list, now) {
                    Ok(t) if t.id == task => {
                        out.migrated.push(key);
                        out.tasks.push(t);
                    }
                    Ok(_) => skip(&mut out, key, "its id is not its file's name".into()),
                    Err(_) => skip(&mut out, key, FileError::WrongFormat.to_string()),
                },
                Err(e) => skip(&mut out, key, e.to_string()),
            },
            KeyKind::Attachment { .. } | KeyKind::Other => {}
        }
    }
    let mut next_order = out.lists.iter().map(|l| l.order).max().unwrap_or(0);
    let mut missing: Vec<String> = out
        .tasks
        .iter()
        .map(|t| t.list.clone())
        .filter(|id| !out.lists.iter().any(|l| l.id == *id))
        .collect();
    missing.sort();
    missing.dedup();
    for id in missing {
        next_order += 1;
        let name = unnamed_list_name(&id).to_string();
        out.lists.push(TaskList::new(id, name, next_order));
    }
    out.lists
        .sort_by(|a, b| (a.order, a.name.to_lowercase()).cmp(&(b.order, b.name.to_lowercase())));
    Ok(out)
}

/// Moves the task files AzCalendar's To-Do bar wrote into its own data folder (`old`:
/// `tasks/default/<uuid>.json`, format `azcalendar.task`) into the store (`store`), in the
/// shared format and the list [`crate::task::DEFAULT_LIST`], and removes them from `old`. A task
/// the store already has (the same id) is kept as it is. Returns how many moved.
pub fn migrate_calendar_tasks(
    old: &dyn Drive,
    store: &dyn Drive,
    now: NaiveDateTime,
) -> Result<usize, DriveError> {
    let prefix = format!("{}/{DEFAULT_LIST}/", task::TASKS_DIR);
    let mut moved = 0;
    for key in all_keys(old, &prefix, PAGE_SIZE)? {
        let KeyKind::Task { list, task: id } = task::parse_key(&key) else {
            continue;
        };
        let text = String::from_utf8_lossy(&old.get(&key)?).into_owned();
        // Anything but an old To-Do bar file is left where it is.
        let Ok(t) = task_from_calendar_json(&text, &list, now) else {
            continue;
        };
        if t.id != id {
            continue;
        }
        let target = t.key();
        if store.head(&target).is_err() {
            store.put(&target, task_to_json(&t).as_bytes())?;
            moved += 1;
        }
        old.delete(&key)?;
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use azul_storage::LocalDrive;
    use chrono::{NaiveDate, NaiveDateTime};

    use super::*;
    use crate::{
        task::{list_to_json, settings_to_json, task_key, task_to_json, SETTINGS_KEY},
        testing::TempDir,
    };

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
        for l in lists {
            drive.put(&l.key(), list_to_json(l).as_bytes()).unwrap();
        }
        for t in tasks {
            drive.put(&t.key(), task_to_json(t).as_bytes()).unwrap();
        }
    }

    #[test]
    fn saved_lists_tasks_and_settings_load_back_from_their_files() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let lists = vec![
            TaskList::new(HOME.into(), "Home".into(), 1),
            TaskList::new(WORK.into(), "Work".into(), 2),
        ];
        let tasks = vec![
            task("a1", WORK, 1024),
            task("b2", WORK, 2048),
            task("c3", HOME, 1024),
        ];
        put_all(&drive, &lists, &tasks);
        let settings = Settings {
            default_list: WORK.into(),
            ..Settings::default()
        };
        drive
            .put(SETTINGS_KEY, settings_to_json(&settings).as_bytes())
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
        put_all(
            &drive,
            &[TaskList::new(WORK.into(), "Work".into(), 1)],
            &[task("ok", WORK, 1)],
        );
        let broken = task_key(WORK, "broken");
        drive.put(&broken, b"{ not json").unwrap();
        let wrong_id = task_key(WORK, "other");
        drive
            .put(
                &wrong_id,
                task_to_json(&task("elsewhere", WORK, 2)).as_bytes(),
            )
            .unwrap();
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
        put_all(
            &drive,
            &[],
            &[task("lost", HOME, 1), task("old", "default", 1)],
        );
        let loaded = load_all(&drive).unwrap();
        assert_eq!(loaded.tasks.len(), 2);
        let names: Vec<(&str, &str)> = loaded
            .lists
            .iter()
            .map(|l| (l.id.as_str(), l.name.as_str()))
            .collect();
        assert_eq!(names, vec![(HOME, "Untitled list"), ("default", "Tasks")]);
    }

    #[test]
    fn a_task_file_in_another_folder_belongs_to_that_folders_list() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let t = task("moved", WORK, 1);
        drive
            .put(&task_key(HOME, "moved"), task_to_json(&t).as_bytes())
            .unwrap();
        let loaded = load_all(&drive).unwrap();
        assert_eq!(loaded.tasks[0].list, HOME);
    }

    #[test]
    fn more_keys_than_a_page_all_load() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let tasks: Vec<Task> = (0..7).map(|n| task(&format!("t{n}"), WORK, n)).collect();
        put_all(
            &drive,
            &[TaskList::new(WORK.into(), "Work".into(), 1)],
            &tasks,
        );
        let loaded = load_all_paged(&drive, 2).unwrap();
        assert_eq!(loaded.tasks.len(), 7);
        assert_eq!(all_keys(&drive, "tasks/", 3).unwrap().len(), 8);
    }

    /// A task file as AzCalendar's To-Do bar wrote it.
    fn calendar_file(id: &str, title: &str, done: bool) -> String {
        format!(
            "{{\n  \"format\": \"azcalendar.task\",\n  \"version\": 1,\n  \"id\": \"{id}\",\n  \
             \"title\": \"{title}\",\n  \"done\": {done}\n}}\n"
        )
    }

    const OLD_A: &str = "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a";
    const OLD_B: &str = "11111111-2222-4333-8444-555555555555";

    #[test]
    fn a_task_file_azcalendar_wrote_loads_into_its_list_and_is_named_for_a_rewrite() {
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let a = task_key("default", OLD_A);
        let b = task_key("default", OLD_B);
        drive
            .put(&a, calendar_file(OLD_A, "Book the room", false).as_bytes())
            .unwrap();
        drive
            .put(&b, calendar_file(OLD_B, "Agenda", true).as_bytes())
            .unwrap();
        put_all(&drive, &[], &[task("t1", WORK, 1)]);
        let loaded = load_all(&drive).unwrap();
        assert!(loaded.skipped.is_empty(), "{:?}", loaded.skipped);
        let mut got: Vec<(&str, &str, &str, bool)> = loaded
            .tasks
            .iter()
            .map(|t| {
                (
                    t.id.as_str(),
                    t.list.as_str(),
                    t.title.as_str(),
                    t.is_done(),
                )
            })
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                (OLD_A, "default", "Book the room", false),
                (OLD_B, "default", "Agenda", true),
                ("t1", WORK, "task t1", false),
            ]
        );
        let mut migrated = loaded.migrated.clone();
        migrated.sort();
        assert_eq!(migrated, vec![a, b]);
        assert!(loaded
            .lists
            .iter()
            .any(|l| l.id == "default" && l.name == "Tasks"));
    }

    #[test]
    fn azcalendars_old_task_folder_moves_into_the_store_once() {
        let dir = TempDir::create();
        let old = LocalDrive::new(dir.0.join("AzCalendar"));
        let store = LocalDrive::new(dir.0.join("Azlin"));
        old.put(
            &task_key("default", OLD_A),
            calendar_file(OLD_A, "Book the room", true).as_bytes(),
        )
        .unwrap();
        old.put(
            &task_key("default", OLD_B),
            calendar_file(OLD_B, "Agenda", false).as_bytes(),
        )
        .unwrap();
        old.put("tasks/default/notes.txt", b"not a task").unwrap();
        // The store already has OLD_B (AzTasks edited it after an earlier move): it stays.
        let mut kept = task(OLD_B, "default", 1);
        kept.title = "Agenda for Monday".into();
        put_all(&store, &[], &[kept.clone()]);

        assert_eq!(migrate_calendar_tasks(&old, &store, now()).unwrap(), 1);
        let a = task_from_json(
            &String::from_utf8(store.get(&task_key("default", OLD_A)).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(a.title, "Book the room");
        assert_eq!(a.list, "default");
        assert!(a.is_done());
        assert_eq!(a.created, now());
        let b = task_from_json(
            &String::from_utf8(store.get(&task_key("default", OLD_B)).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(b, kept, "the store's copy wins");
        assert!(
            old.get(&task_key("default", OLD_A)).is_err(),
            "moved, not copied"
        );
        assert!(old.get(&task_key("default", OLD_B)).is_err());
        assert!(
            old.get("tasks/default/notes.txt").is_ok(),
            "not a task file: left alone"
        );
        assert_eq!(
            migrate_calendar_tasks(&old, &store, now()).unwrap(),
            0,
            "once"
        );
        let empty = LocalDrive::new(dir.0.join("nothing"));
        assert_eq!(migrate_calendar_tasks(&empty, &store, now()).unwrap(), 0);
    }
}
