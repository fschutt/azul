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
    let title = title.trim();
    (!title.is_empty()).then(|| Task {
        id: event::new_event_id(),
        title: title.to_string(),
        done: false,
    })
}

/// Where the task `id` is stored under `data_dir`.
#[must_use]
pub fn task_path(data_dir: &Path, id: &str) -> PathBuf {
    data_dir
        .join(TASKS_DIR)
        .join(LIST)
        .join(format!("{id}.json"))
}

#[must_use]
pub fn to_json(task: &Task) -> String {
    let file = TaskFile {
        format: FORMAT.to_string(),
        version: VERSION,
        task: task.clone(),
    };
    let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
    text.push('\n');
    text
}

pub fn from_json(text: &str) -> Result<Task, String> {
    let file: TaskFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if file.format != FORMAT || file.version != VERSION {
        return Err(String::from("not an AzCalendar task of this version"));
    }
    if !is_event_id(&file.task.id) {
        return Err(format!(
            "the id {:?} is not a UUID in lower case",
            file.task.id
        ));
    }
    if file.task.title.trim().is_empty() {
        return Err(String::from("the task has no title"));
    }
    Ok(file.task)
}

/// Writes `task` to its file, atomically.
pub fn save(data_dir: &Path, task: &Task) -> std::io::Result<PathBuf> {
    let path = task_path(data_dir, &task.id);
    let dir = data_dir.join(TASKS_DIR).join(LIST);
    std::fs::create_dir_all(&dir)?;
    let temp = dir.join(format!(".{}.json.tmp", task.id));
    std::fs::write(&temp, to_json(task))?;
    if let Err(e) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    Ok(path)
}

/// Every task of the list: the open ones first, then the done ones, each by title.
#[must_use]
pub fn load_all(data_dir: &Path) -> Vec<Task> {
    let mut tasks = Vec::new();
    let Ok(entries) = std::fs::read_dir(data_dir.join(TASKS_DIR).join(LIST)) else {
        return tasks;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
            continue;
        };
        if !is_event_id(id) {
            continue;
        }
        let Ok(task) = std::fs::read_to_string(entry.path())
            .map_err(|e| e.to_string())
            .and_then(|text| from_json(&text))
        else {
            continue;
        };
        if task.id == id {
            tasks.push(task);
        }
    }
    sort(&mut tasks);
    tasks
}

/// Open tasks first, then done ones, each by title.
pub fn sort(tasks: &mut [Task]) {
    tasks.sort_by(|a, b| {
        (a.done, a.title.to_lowercase(), &a.id).cmp(&(b.done, b.title.to_lowercase(), &b.id))
    });
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
