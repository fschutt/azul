//! The To-Do bar's tasks: the task store every Azlin app shares (`azul_pim::task`), one file
//! per task, `tasks/<list>/<task-uuid>.json` in AzTasks' format under the Azlin data root - so
//! a task typed into AzMail's To-Do bar is a task in AzTasks and in AzCalendar's To-Do bar, and
//! the other way round (DEDUP_EDITORS B12, PIM's one task store).
//!
//! The store is read once at start, before the window (as the kit reads settings.json); every
//! change is written as an azul-appkit file job on a Thread (`ui_main::save_task`), never from
//! a callback. New tasks go to the store's default list (AzTasks' setting, else its first list,
//! else `default`).

use std::path::Path;

use azul_appkit::files::FileJob;
use azul_pim::{
    task::{default_list, next_order, task_to_json, Task, DEFAULT_LIST},
    task_store,
};
use azul_storage::LocalDrive;
use chrono::NaiveDateTime;

/// What the To-Do bar starts with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TodoTasks {
    /// Open tasks first, then done ones, each by title ([`sort`]).
    pub tasks: Vec<Task>,
    /// The list a task typed into the To-Do bar goes to.
    pub new_task_list: String,
}

/// The store under the data root `root`: every task, sorted, and the list new ones go to. A
/// store that cannot be read is empty (and says why on stderr).
#[must_use]
pub fn load(root: &Path) -> TodoTasks {
    let loaded = match task_store::load_all(&LocalDrive::new(root)) {
        Ok(loaded) => loaded,
        Err(e) => {
            eprintln!("[azmail] the tasks in {} could not be read: {e}", root.display());
            return TodoTasks {
                tasks: Vec::new(),
                new_task_list: DEFAULT_LIST.to_string(),
            };
        }
    };
    for skipped in &loaded.skipped {
        eprintln!("[azmail] left out {}: {}", skipped.key, skipped.reason);
    }
    let settings = loaded.settings.clone().unwrap_or_default();
    let new_task_list =
        default_list(&loaded.lists, &settings).unwrap_or_else(|| DEFAULT_LIST.to_string());
    let mut tasks = loaded.tasks;
    sort(&mut tasks);
    TodoTasks {
        tasks,
        new_task_list,
    }
}

/// A new task titled `title` (trimmed) at the end of `list`, with the id `id`; `None` for an
/// empty title.
#[must_use]
pub fn new_task(
    title: &str,
    list: &str,
    tasks: &[Task],
    id: String,
    now: NaiveDateTime,
) -> Option<Task> {
    let title = title.trim();
    (!title.is_empty()).then(|| {
        let mut task = Task::new(id, list.to_string(), title.to_string(), now);
        task.order = next_order(tasks, list);
        task
    })
}

/// Ticks a task off, or opens a done one again. A repeating task hands back its next
/// occurrence under `next_id` (to save and show too), as AzTasks does.
pub fn toggle_done(task: &mut Task, next_id: String, now: NaiveDateTime) -> Option<Task> {
    if task.is_done() {
        task.reopen(now);
        None
    } else {
        task.complete(next_id, now)
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

/// The file job that writes `task` to its file in the store.
#[must_use]
pub fn put_job(task: &Task) -> FileJob {
    FileJob::Put {
        key: task.key(),
        bytes: task_to_json(task).into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use azul_storage::Drive;

    const A: &str = "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a";
    const B: &str = "1c1f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1b";
    const C: &str = "2d2f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1c";

    fn noon() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 3)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
    }

    /// Runs the file job the way the kit's thread does (on the data root's drive).
    fn write(root: &Path, job: FileJob) {
        let drive = LocalDrive::new(root);
        match job {
            FileJob::Put { key, bytes } => drive.put(&key, &bytes).unwrap(),
            other => panic!("a To-Do bar change is a put, got {other:?}"),
        }
    }

    #[test]
    fn a_task_typed_into_the_to_do_bar_is_a_file_of_the_shared_store_and_reads_back() {
        let dir = TempDir::new("todo");
        let empty = load(&dir.0);
        assert!(empty.tasks.is_empty());
        assert_eq!(empty.new_task_list, DEFAULT_LIST, "an empty store: the list `default`");

        let a = new_task("  Order tulip bulbs ", DEFAULT_LIST, &[], A.to_string(), noon()).unwrap();
        assert_eq!(a.title, "Order tulip bulbs");
        assert_eq!(a.list, DEFAULT_LIST);
        let b = new_task("Answer Ben", DEFAULT_LIST, &[a.clone()], B.to_string(), noon()).unwrap();
        assert!(b.order > a.order, "a new task goes to the end of its list");
        write(&dir.0, put_job(&a));
        write(&dir.0, put_job(&b));
        assert!(
            dir.0.join("tasks").join(DEFAULT_LIST).join(format!("{A}.json")).is_file(),
            "tasks/<list>/<task-uuid>.json, AzTasks' file"
        );
        let text = std::fs::read_to_string(
            dir.0.join("tasks").join(DEFAULT_LIST).join(format!("{A}.json")),
        )
        .unwrap();
        assert!(text.contains("\"format\": \"aztasks.task\""), "{text}");

        let loaded = load(&dir.0);
        assert_eq!(
            loaded.tasks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
            ["Answer Ben", "Order tulip bulbs"],
            "by title"
        );
    }

    #[test]
    fn a_done_task_goes_after_the_open_ones_and_an_empty_title_makes_none() {
        assert_eq!(new_task("   ", DEFAULT_LIST, &[], A.to_string(), noon()), None);
        let mut a = new_task("Agenda", DEFAULT_LIST, &[], A.to_string(), noon()).unwrap();
        let b = new_task("Book the room", DEFAULT_LIST, &[], B.to_string(), noon()).unwrap();
        assert_eq!(toggle_done(&mut a, C.to_string(), noon()), None, "it does not repeat");
        assert!(a.is_done());
        let mut tasks = vec![a.clone(), b.clone()];
        sort(&mut tasks);
        assert_eq!(tasks, vec![b, a.clone()]);
        assert_eq!(toggle_done(&mut a, C.to_string(), noon()), None);
        assert!(!a.is_done(), "ticked again: open again");
    }

    #[test]
    fn ticking_off_a_repeating_task_leaves_its_next_occurrence() {
        let mut t = new_task("Water the plants", DEFAULT_LIST, &[], A.to_string(), noon()).unwrap();
        t.due = Some(noon().date());
        t.repeat = Some(azul_pim::repeat::Repeat::new(3, azul_pim::repeat::Unit::Day));
        let next = toggle_done(&mut t, B.to_string(), noon()).expect("the next occurrence");
        assert_eq!(next.id, B);
        assert_eq!(next.due, Some(noon().date() + chrono::Duration::days(3)));
        assert!(t.is_done() && !next.is_done());
    }
}
