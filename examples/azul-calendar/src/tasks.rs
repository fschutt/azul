//! The To-Do bar's tasks: the task store every Azlin app shares (`azul_pim::task`), one file per
//! task, `tasks/<list>/<task-uuid>.json` in AzTasks' format, so a task added here is a task in
//! AzTasks and the other way round (scripts/DEDUP_EDITORS_2026_10_02.md, B12).
//!
//! The store is AzTasks' data root ([`tasks_root`]: `AZTASKS_DATA`, else `AZLIN_DATA`, else
//! `<user data>/Azlin`) unless the calendar was given a data folder (`--data`, `AZCAL_DATA`):
//! then everything, the tasks too, is in that folder (name the same folder for AzTasks to share
//! it). The files this To-Do bar wrote before (`azcalendar.task`, `<calendar
//! data>/tasks/default/`) are moved into the store once ([`migrate_old_folder`]) or, in the same
//! folder, rewritten where they are ([`load`]). New tasks go to the store's default list
//! (AzTasks' setting, else its first list, else the list `default`).
//!
//! The files are written through azul-storage's `LocalDrive`, synchronously in the callback as
//! the calendar's event files still are (DEDUP_EDITORS B2: the calendar moves onto a Thread
//! with its events).

use std::path::{Path, PathBuf};

use azul_pim::{
    task::{default_list, task_to_json, DEFAULT_LIST},
    task_store,
};
use azul_storage::{Drive, DriveError, LocalDrive};
use chrono::NaiveDateTime;

pub use azul_pim::task::Task;

/// The variable AzTasks names its data folder with: the task store's folder.
pub const TASKS_DATA_VAR: &str = "AZTASKS_DATA";

/// What the To-Do bar starts with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    /// Open tasks first, then done ones, each by title ([`sort`]).
    pub tasks: Vec<Task>,
    /// The list a task typed into the To-Do bar goes to.
    pub new_task_list: String,
}

/// The folder the task store is in: the calendar's data folder when one was named
/// (`named_data_dir`), else AzTasks' (`tasks_var`, blank counts as unset), else the Azlin apps'
/// data root: `azlin_var` (`AZLIN_DATA`), else `Azlin` in the user's data folder.
#[must_use]
pub fn tasks_root(
    named_data_dir: Option<&Path>,
    tasks_var: Option<&str>,
    azlin_var: Option<&str>,
    user_data: Option<PathBuf>,
) -> PathBuf {
    let var = tasks_var.filter(|v| !v.trim().is_empty()).or(azlin_var);
    azul_appkit::data::data_root(named_data_dir, var, user_data)
}

/// Moves the files the old To-Do bar wrote into the calendar's own folder (`data_dir`) into the
/// store at `root`; nothing to do when both are one folder ([`load`] rewrites them there).
/// Returns how many moved.
pub fn migrate_old_folder(
    data_dir: &Path,
    root: &Path,
    now: NaiveDateTime,
) -> Result<usize, DriveError> {
    if data_dir == root {
        return Ok(0);
    }
    task_store::migrate_calendar_tasks(&LocalDrive::new(data_dir), &LocalDrive::new(root), now)
}

/// The store at `root`: every task, sorted, and the list new ones go to. A task file in the old
/// To-Do bar format is written back in the shared one; a store that cannot be read is empty.
#[must_use]
pub fn load(root: &Path) -> Loaded {
    let drive = LocalDrive::new(root);
    let loaded = match task_store::load_all(&drive) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("[azcalendar] the tasks in {} could not be read: {e}", root.display());
            return Loaded {
                tasks: Vec::new(),
                new_task_list: DEFAULT_LIST.to_string(),
            };
        }
    };
    for skipped in &loaded.skipped {
        eprintln!("[azcalendar] left out {}: {}", skipped.key, skipped.reason);
    }
    for key in &loaded.migrated {
        if let Some(task) = loaded.tasks.iter().find(|t| t.key() == *key) {
            if let Err(e) = drive.put(key, task_to_json(task).as_bytes()) {
                eprintln!("[azcalendar] {key} could not be rewritten: {e}");
            }
        }
    }
    let settings = loaded.settings.clone().unwrap_or_default();
    let new_task_list =
        default_list(&loaded.lists, &settings).unwrap_or_else(|| DEFAULT_LIST.to_string());
    let mut tasks = loaded.tasks;
    sort(&mut tasks);
    Loaded {
        tasks,
        new_task_list,
    }
}

/// Every task of the store at `root` ([`load`]).
#[must_use]
pub fn load_all(root: &Path) -> Vec<Task> {
    load(root).tasks
}

/// A new task titled `title` (trimmed) at the end of `list` (`order`); `None` for an empty
/// title. Its id is random, as an event's (`event::new_event_id`).
#[must_use]
pub fn new_task(title: &str, list: &str, order: i64, now: NaiveDateTime) -> Option<Task> {
    let title = title.trim();
    (!title.is_empty()).then(|| {
        let mut task = Task::new(
            crate::event::new_event_id(),
            list.to_string(),
            title.to_string(),
            now,
        );
        task.order = order;
        task
    })
}

/// Writes `task` to its file in the store at `root`.
pub fn save(root: &Path, task: &Task) -> Result<(), DriveError> {
    LocalDrive::new(root).put(&task.key(), task_to_json(task).as_bytes())
}

/// Ticks a task off, or opens a done one again. A repeating task hands back its next
/// occurrence (under a new id), which the caller saves and shows too - as AzTasks does.
pub fn toggle_done(task: &mut Task, now: NaiveDateTime) -> Option<Task> {
    if task.is_done() {
        task.reopen(now);
        None
    } else {
        task.complete(crate::event::new_event_id(), now)
    }
}

/// Open tasks first, then done ones, each by title.
pub fn sort(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        (a.is_done(), a.title.to_lowercase(), &a.id).cmp(&(
            b.is_done(),
            b.title.to_lowercase(),
            &b.id,
        ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_dir::TempDir;

    #[test]
    fn a_task_is_one_file_that_reads_back_and_open_ones_come_first() {
        let dir = TempDir::create();
        let mut a = new_task(" Book the room ", DEFAULT_LIST, 1024, noon()).unwrap();
        assert_eq!(a.title, "Book the room");
        let b = new_task("Agenda", DEFAULT_LIST, 2048, noon()).unwrap();
        save(&dir.0, &a).unwrap();
        save(&dir.0, &b).unwrap();
        assert_eq!(load_all(&dir.0), vec![b.clone(), a.clone()]);
        assert_eq!(toggle_done(&mut a, noon()), None, "a task that does not repeat");
        save(&dir.0, &a).unwrap();
        let c = new_task("Call Ana", DEFAULT_LIST, 3072, noon()).unwrap();
        save(&dir.0, &c).unwrap();
        assert_eq!(load_all(&dir.0), vec![b, c, a.clone()]);
        assert!(dir
            .0
            .join("tasks")
            .join(DEFAULT_LIST)
            .join(format!("{}.json", a.id))
            .is_file());
        let loaded = load(&dir.0);
        assert_eq!(loaded.new_task_list, DEFAULT_LIST, "the store's only list");
    }

    #[test]
    fn an_empty_title_makes_no_task_and_a_broken_file_is_left_out() {
        assert_eq!(new_task("   ", DEFAULT_LIST, 0, noon()), None);
        let dir = TempDir::create();
        let folder = dir.0.join("tasks").join(DEFAULT_LIST);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("11111111-2222-4333-8444-555555555555.json"), "{").unwrap();
        assert!(load_all(&dir.0).is_empty());
        assert!(load_all(&dir.0.join("none")).is_empty());
    }

    #[test]
    fn ticking_off_a_repeating_task_leaves_its_next_occurrence() {
        let mut t = new_task("Water the plants", DEFAULT_LIST, 1024, noon()).unwrap();
        t.due = Some(noon().date());
        t.repeat = Some(azul_pim::repeat::Repeat::new(3, azul_pim::repeat::Unit::Day));
        let next = toggle_done(&mut t, noon()).expect("the next occurrence");
        assert!(t.is_done() && t.repeat.is_none());
        assert_eq!(next.due, Some(noon().date() + chrono::Duration::days(3)));
        assert!(!next.is_done());
        assert_ne!(next.id, t.id);
        assert_eq!(toggle_done(&mut t, noon()), None, "opened again: no second one");
        assert!(!t.is_done());
    }

    #[test]
    fn the_old_folder_moves_into_the_store_unless_it_is_the_store() {
        let dir = TempDir::create();
        let calendar = dir.0.join("AzCalendar");
        let root = dir.0.join("Azlin");
        let folder = calendar.join("tasks").join(DEFAULT_LIST);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join(format!("{SHARED_ID}.json")),
            format!(
                "{{\"format\": \"azcalendar.task\", \"version\": 1, \"id\": \"{SHARED_ID}\", \
                 \"title\": \"Book the room\", \"done\": false}}"
            ),
        )
        .unwrap();
        assert_eq!(migrate_old_folder(&calendar, &calendar, noon()).unwrap(), 0);
        assert_eq!(migrate_old_folder(&calendar, &root, noon()).unwrap(), 1);
        let tasks = load_all(&root);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Book the room");
        assert!(load_all(&calendar).is_empty(), "moved, not copied");
    }

    const SHARED_ID: &str = "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a";

    fn noon() -> chrono::NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 2)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    #[test]
    fn the_to_do_bar_reads_and_writes_the_task_files_aztasks_does() {
        // DEDUP_EDITORS B12: one task store, tasks/<list>/<task-uuid>.json in AzTasks' format.
        let dir = TempDir::create();
        let folder = dir.0.join("tasks").join("inbox");
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join(format!("{SHARED_ID}.json"));
        let written = azul_pim::task::Task::new(
            SHARED_ID.into(),
            "inbox".into(),
            "Pay rent".into(),
            noon(),
        );
        std::fs::write(&file, azul_pim::task::task_to_json(&written)).unwrap();
        let mut tasks = load_all(&dir.0);
        assert_eq!(tasks.len(), 1, "AzTasks' task shows in the To-Do bar");
        let mut first = tasks.remove(0);
        assert_eq!(first.title, "Pay rent");
        first.title = String::from("Pay the rent");
        save(&dir.0, &first).unwrap();
        let back = azul_pim::task::task_from_json(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(back.title, "Pay the rent", "AzTasks reads what the To-Do bar wrote");
        assert_eq!(back.list, "inbox");
    }

    #[test]
    fn a_task_the_old_to_do_bar_wrote_is_read_and_rewritten_in_the_shared_format() {
        let dir = TempDir::create();
        let folder = dir.0.join("tasks").join("default");
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join(format!("{SHARED_ID}.json"));
        std::fs::write(
            &file,
            format!(
                "{{\"format\": \"azcalendar.task\", \"version\": 1, \"id\": \"{SHARED_ID}\", \
                 \"title\": \"Book the room\", \"done\": true}}"
            ),
        )
        .unwrap();
        let tasks = load_all(&dir.0);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Book the room");
        let back = azul_pim::task::task_from_json(&std::fs::read_to_string(&file).unwrap())
            .expect("rewritten as a task AzTasks reads");
        assert_eq!(back.title, "Book the room");
        assert!(back.is_done());
    }

    #[test]
    fn the_tasks_live_with_aztasks_unless_a_data_folder_is_named() {
        let user = Some(PathBuf::from("/home/ada/.local/share"));
        assert_eq!(
            tasks_root(
                Some(Path::new("/tmp/cal")),
                Some("/srv/tasks"),
                None,
                user.clone()
            ),
            PathBuf::from("/tmp/cal"),
            "a named folder holds everything (the tests, the E2E)"
        );
        assert_eq!(
            tasks_root(None, Some(" /srv/tasks "), None, user.clone()),
            PathBuf::from("/srv/tasks"),
            "AzTasks' AZTASKS_DATA"
        );
        assert_eq!(
            tasks_root(None, Some("  "), None, user.clone()),
            PathBuf::from("/home/ada/.local/share/Azlin")
        );
        assert_eq!(
            tasks_root(None, None, None, user),
            PathBuf::from("/home/ada/.local/share/Azlin")
        );
        assert_eq!(tasks_root(None, None, None, None), PathBuf::from("Azlin"));
    }

    #[test]
    fn without_aztasks_data_the_tasks_live_in_the_azlin_data_root() {
        let user = Some(PathBuf::from("/home/ada/.local/share"));
        assert_eq!(
            tasks_root(None, None, Some("/tmp/e2e"), user.clone()),
            PathBuf::from("/tmp/e2e"),
            "AZLIN_DATA, the root every Azlin app shares"
        );
        assert_eq!(
            tasks_root(None, Some("/srv/tasks"), Some("/tmp/e2e"), user.clone()),
            PathBuf::from("/srv/tasks"),
            "AZTASKS_DATA first"
        );
        assert_eq!(
            tasks_root(None, Some(" "), Some("/tmp/e2e"), user),
            PathBuf::from("/tmp/e2e")
        );
    }
}
