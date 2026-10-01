//! The To-Do bar's tasks: one JSON file each, `<data dir>/tasks/default/<task id>.json` - the
//! layout `tasks/<list>/<task-uuid>.json` the tasks of every azul app share, with one list here.
//!
//! ```json
//! { "format": "azcalendar.task", "version": 1, "id": "0b0f6f2e-...", "title": "Book the room", "done": false }
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::event::{self, is_event_id};

/// The `format` of a task file.
pub const FORMAT: &str = "azcalendar.task";
pub const VERSION: u64 = 1;
/// The folder of the task lists, and its one list.
pub const TASKS_DIR: &str = "tasks";
pub const LIST: &str = "default";

/// A task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub done: bool,
}

#[derive(Serialize, Deserialize)]
struct TaskFile {
    format: String,
    version: u64,
    #[serde(flatten)]
    task: Task,
}

/// A new task titled `title` (trimmed); `None` for an empty title.
#[must_use]
pub fn new_task(title: &str) -> Option<Task> {
    todo!()
}

/// Where the task `id` is stored under `data_dir`.
#[must_use]
pub fn task_path(data_dir: &Path, id: &str) -> PathBuf {
    todo!()
}

#[must_use]
pub fn to_json(task: &Task) -> String {
    todo!()
}

pub fn from_json(text: &str) -> Result<Task, String> {
    todo!()
}

/// Writes `task` to its file, atomically.
pub fn save(data_dir: &Path, task: &Task) -> std::io::Result<PathBuf> {
    todo!()
}

/// Every task of the list: the open ones first, then the done ones, each by title.
#[must_use]
pub fn load_all(data_dir: &Path) -> Vec<Task> {
    todo!()
}

/// Open tasks first, then done ones, each by title.
pub fn sort(tasks: &mut [Task]) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_dir::TempDir;

    #[test]
    fn a_task_is_one_file_that_reads_back_and_open_ones_come_first() {
        let dir = TempDir::create();
        let mut a = new_task(" Book the room ").unwrap();
        assert_eq!(a.title, "Book the room");
        let b = new_task("Agenda").unwrap();
        save(&dir.0, &a).unwrap();
        save(&dir.0, &b).unwrap();
        assert_eq!(load_all(&dir.0), vec![b.clone(), a.clone()]);
        a.done = true;
        save(&dir.0, &a).unwrap();
        let c = new_task("Call Ana").unwrap();
        save(&dir.0, &c).unwrap();
        assert_eq!(load_all(&dir.0), vec![b, c, a.clone()]);
        let json: serde_json::Value = serde_json::from_str(&to_json(&a)).unwrap();
        assert_eq!(json["format"], FORMAT);
        assert_eq!(json["done"], true);
        assert!(task_path(&dir.0, &a.id).ends_with(format!("tasks/default/{}.json", a.id)));
    }

    #[test]
    fn an_empty_title_makes_no_task_and_a_broken_file_is_left_out() {
        assert_eq!(new_task("   "), None);
        let dir = TempDir::create();
        let folder = dir.0.join("tasks").join("default");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("11111111-2222-4333-8444-555555555555.json"),
            "{",
        )
        .unwrap();
        assert!(load_all(&dir.0).is_empty());
        assert!(load_all(&dir.0.join("none")).is_empty());
    }
}
