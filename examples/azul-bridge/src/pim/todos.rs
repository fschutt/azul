//! CalDAV's to-do lists: AzTasks' files - the one task store of the Azlin apps, `tasks/<list
//! id>/list.json` and `tasks/<list id>/<task id>.json` of the drive (azul-pim's task model) -
//! each list a calendar collection of VTODOs at `/calendars/tasks-<list id>/`, what Apple
//! Reminders and Thunderbird's tasks show. The VTODOs are AzTasks' own iCalendar code
//! (azul-pim's `vtodo`):
//!
//! - GET is AzTasks' export of the task: title, notes, due date and time, repeat, priority, tags,
//!   done or started; its `UID` the one the program gave the to-do (kept in `pim-names.json`),
//!   else the task id.
//! - PUT is AzTasks' import of the VTODO into the list; what iCalendar does not carry stays from
//!   the task there was: the steps, the flag, the reminder, the attachments, the order, when it
//!   was made. A new to-do gets a task id of its own, kept for the program's name.
//! - MKCALENDAR for to-dos only (its supported-calendar-component-set) makes an AzTasks list,
//!   its name and the list colour nearest the program's; PROPPATCH renames and recolours one.
//! - Not read: a VTODO's reminders (VALARM). DELETE removes the task file (its attachments stay
//!   for AzTasks to tidy).

use std::collections::{HashMap, HashSet};

use azul_pim::{
    content_line::fold,
    task::{self, KeyKind, ListColor, Task, TaskList},
    vtodo,
};
use azul_storage::{ops, DriveError, ObjectInfo};
use chrono::{NaiveDateTime, TimeZone, Timelike};

use super::{dav_error, precondition, version_of, Item, Kind, Patched, Pim, APPLE};
use crate::{
    dates,
    dav::DAV,
    http::{Head, Response, Status},
};

/// The URL segment of AzTasks' list `<id>` is `tasks-<id>`.
pub const TASKS_PREFIX: &str = "tasks-";
/// What a to-do is served as.
pub const CONTENT_TYPE: &str = "text/calendar; charset=utf-8; component=vtodo";

/// A list's colour as Apple's programs write it (`#RRGGBBAA`).
#[must_use]
pub fn colour_of(list: &TaskList) -> String {
    format!("{}FF", list.color.hex(false).to_ascii_uppercase())
}

fn rgb_of(text: &str) -> Option<(i32, i32, i32)> {
    let hex = text.trim().strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let part = |at: usize| i32::from_str_radix(&hex[at..at + 2], 16).ok();
    Some((part(0)?, part(2)?, part(4)?))
}

/// The list colour nearest a program's (`#RRGGBB` / `#RRGGBBAA`).
#[must_use]
pub fn nearest_colour(text: &str) -> Option<ListColor> {
    let (r, g, b) = rgb_of(text)?;
    ListColor::ALL.into_iter().min_by_key(|colour| {
        let (cr, cg, cb) = rgb_of(colour.hex(false)).unwrap_or((0, 0, 0));
        (r - cr).pow(2) + (g - cg).pow(2) + (b - cb).pow(2)
    })
}

/// Sets a list's name and colour from the properties a program sends.
fn apply(list: &mut TaskList, props: &[Patched]) {
    for p in props {
        match (p.namespace.as_str(), p.name.as_str(), &p.value) {
            (DAV, "displayname", Some(value)) if !value.trim().is_empty() => {
                list.name = value.trim().to_string();
            }
            (APPLE, "calendar-color", Some(value)) => {
                if let Some(colour) = nearest_colour(value) {
                    list.color = colour;
                }
            }
            _ => {}
        }
    }
}

/// This computer's wall clock for a moment in UTC, and back (the task files keep wall-clock
/// times, iCalendar's COMPLETED is UTC).
fn to_local(at: NaiveDateTime) -> NaiveDateTime {
    chrono::Local.from_utc_datetime(&at).naive_local()
}

fn to_utc(at: NaiveDateTime) -> NaiveDateTime {
    chrono::Local
        .from_local_datetime(&at)
        .single()
        .map_or(at, |local| local.naive_utc())
}

/// The UID of the first VTODO of `text`, if it has one.
fn vtodo_uid(text: &str) -> Option<String> {
    let mut inside = false;
    for line in azul_pim::content_line::unfold(text) {
        let upper = line.trim().to_ascii_uppercase();
        if upper == "BEGIN:VTODO" {
            inside = true;
        } else if upper == "END:VTODO" {
            return None;
        } else if inside && upper.starts_with("UID:") {
            let uid = line.trim()[4..].trim().to_string();
            return (!uid.is_empty()).then_some(uid);
        }
    }
    None
}

/// One task as iCalendar: AzTasks' export of it, stamped with its file's time, its UID `uid`
/// (the program's) when there is one.
#[must_use]
pub fn ics_of(task: &Task, list_name: &str, info: &ObjectInfo, uid: Option<&str>) -> String {
    let secs = i64::try_from(info.modified.unwrap_or(0)).unwrap_or(0);
    let stamp = chrono::DateTime::from_timestamp(secs, 0)
        .map(|time| time.naive_utc())
        .unwrap_or_default();
    let text = vtodo::write(&[task], list_name, stamp, &to_utc);
    match uid {
        Some(uid) if uid != task.id => text.replacen(
            &format!("UID:{}\r\n", task.id),
            &format!("{}\r\n", fold(&format!("UID:{uid}"))),
            1,
        ),
        _ => text,
    }
}

/// A task file and the name its href has.
#[derive(Debug, Clone)]
pub(crate) struct TodoFile {
    pub name: String,
    pub info: ObjectInfo,
    pub task: Task,
}

impl Pim {
    fn lock_tasks(&self) -> std::sync::MutexGuard<'_, HashMap<String, (String, Task)>> {
        self.tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The task of the file `info` (named for `id`), read through the cache.
    fn read_task(&self, info: &ObjectInfo, id: &str) -> Option<Task> {
        let version = version_of(info);
        if let Some((cached, task)) = self.lock_tasks().get(&info.key) {
            if *cached == version {
                return Some(task.clone());
            }
        }
        let task = self
            .contacts
            .get(&info.key)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|text| task::task_from_json(&text).ok())
            .filter(|task| task.id == id)?;
        self.lock_tasks()
            .insert(info.key.clone(), (version, task.clone()));
        Some(task)
    }

    /// AzTasks' lists (a list without its `list.json` too, as AzTasks shows it) and every task
    /// file, under the names the programs gave them.
    pub(crate) fn task_store(&self) -> Result<(Vec<TaskList>, Vec<TodoFile>), DriveError> {
        let prefix = format!("{}/", task::TASKS_DIR);
        let mut lists: Vec<TaskList> = Vec::new();
        let mut files = Vec::new();
        let mut keys = HashSet::new();
        let mut ids = HashSet::new();
        for info in ops::list_all(&*self.contacts, &prefix)? {
            match task::parse_key(&info.key) {
                KeyKind::List { list } => {
                    let read = self
                        .contacts
                        .get(&info.key)
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .and_then(|text| task::list_from_json(&text).ok())
                        .filter(|read| read.id == list);
                    lists.extend(read);
                }
                KeyKind::Task { list, task: id } => {
                    let Some(mut task) = self.read_task(&info, &id) else {
                        continue;
                    };
                    // The folder says which list a task is in.
                    task.list = list;
                    keys.insert(info.key.clone());
                    let name = self
                        .names
                        .name_of(Kind::Todo, &id)
                        .unwrap_or_else(|| id.clone());
                    ids.insert(id);
                    files.push(TodoFile { name, info, task });
                }
                _ => {}
            }
        }
        let mut next = lists.iter().map(|l| l.order).max().unwrap_or(0);
        let mut missing: Vec<String> = files
            .iter()
            .map(|file| file.task.list.clone())
            .filter(|id| !lists.iter().any(|l| l.id == *id))
            .collect();
        missing.sort();
        missing.dedup();
        for id in missing {
            next += 1;
            let name = task::unnamed_list_name(&id).to_string();
            lists.push(TaskList::new(id, name, next));
        }
        lists.sort_by(|a, b| (a.order, a.name.to_lowercase()).cmp(&(b.order, b.name.to_lowercase())));
        self.lock_tasks().retain(|key, _| keys.contains(key));
        self.names.keep_only(Kind::Todo, &ids);
        Ok((lists, files))
    }

    /// A list's URL segment: the path a program made it under, else `tasks-<id>`.
    pub(crate) fn segment_of_list(&self, id: &str) -> String {
        self.names
            .name_of(Kind::TaskList, id)
            .unwrap_or_else(|| format!("{TASKS_PREFIX}{id}"))
    }

    /// The list id a URL segment names, if it names one of AzTasks' lists' paths.
    pub(crate) fn list_id(&self, segment: &str) -> Option<String> {
        self.names.id_of(Kind::TaskList, segment).or_else(|| {
            segment
                .strip_prefix(TASKS_PREFIX)
                .filter(|id| task::is_id(id))
                .map(str::to_string)
        })
    }

    /// Whether a calendar path is a to-do list's (the lists' paths are the bridge's own:
    /// `tasks-<id>`, or one a program made with MKCALENDAR for to-dos).
    pub(crate) fn is_task_list(&self, segment: &str) -> bool {
        self.list_id(segment).is_some()
    }

    /// Every list as a calendar collection of to-dos, with its CTag.
    pub(crate) fn task_list_items(&self) -> Result<Vec<Item>, DriveError> {
        let (lists, files) = self.task_store()?;
        let ids: HashSet<String> = lists.iter().map(|l| l.id.clone()).collect();
        self.names.keep_only(Kind::TaskList, &ids);
        let mut items = Vec::new();
        for list in lists {
            let mut text = format!("{}\n{}\n", list.name, list.color.name());
            for file in files.iter().filter(|file| file.task.list == list.id) {
                text.push_str(&format!("{}\n{}\n", file.name, version_of(&file.info)));
            }
            items.push(Item::TaskList {
                segment: self.segment_of_list(&list.id),
                list,
                ctag: super::ctag_of(&text),
            });
        }
        Ok(items)
    }

    /// The to-dos of the list at `segment`.
    pub(crate) fn todo_items(&self, segment: &str) -> Result<Vec<Item>, DriveError> {
        let Some(id) = self.list_id(segment) else {
            return Ok(Vec::new());
        };
        let (lists, files) = self.task_store()?;
        let Some(list) = lists.into_iter().find(|l| l.id == id) else {
            return Ok(Vec::new());
        };
        Ok(files
            .into_iter()
            .filter(|file| file.task.list == id)
            .map(|file| Item::Todo {
                segment: segment.to_string(),
                list_name: list.name.clone(),
                name: file.name,
                info: file.info,
                task: file.task,
            })
            .collect())
    }

    /// The task a program's `name` stands for: the id kept for it, else the name itself when it
    /// is a task id.
    fn todo_id(&self, name: &str) -> Option<String> {
        self.names
            .id_of(Kind::Todo, name)
            .or_else(|| task::is_id(name).then(|| name.to_string()))
    }

    /// The to-do `name` of the list at `segment`, if it is there.
    pub(crate) fn todo_item(&self, segment: &str, name: &str) -> Result<Option<Item>, DriveError> {
        let (Some(list), Some(id)) = (self.list_id(segment), self.todo_id(name)) else {
            return Ok(None);
        };
        let info = match self.contacts.head(&task::task_key(&list, &id)) {
            Ok(info) => info,
            Err(DriveError::NotFound { .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        let Some(mut task) = self.read_task(&info, &id) else {
            return Ok(None);
        };
        task.list = list.clone();
        let list_name = self
            .task_store()?
            .0
            .into_iter()
            .find(|l| l.id == list)
            .map(|l| l.name)
            .unwrap_or_default();
        Ok(Some(Item::Todo {
            segment: segment.to_string(),
            list_name,
            name: name.to_string(),
            info,
            task,
        }))
    }

    /// The UID a to-do is served with: the program's, else the task id.
    pub(crate) fn todo_uid(&self, task: &Task) -> Option<String> {
        self.names.id_of(Kind::TaskUid, &task.id)
    }

    pub(crate) fn get_todo(&self, segment: &str, name: &str) -> Result<Response, DriveError> {
        let Some(Item::Todo {
            list_name,
            info,
            task,
            ..
        }) = self.todo_item(segment, name)?
        else {
            return Ok(Response::text(Status::NOT_FOUND, "Not there."));
        };
        let uid = self.todo_uid(&task);
        let mut response = Response::new(Status::OK)
            .with_header("ETag", format!("\"{}\"", version_of(&info)))
            .with_body(CONTENT_TYPE, ics_of(&task, &list_name, &info, uid.as_deref()).into_bytes());
        if let Some(modified) = info.modified {
            response = response.with_header("Last-Modified", dates::http_date(i64::try_from(modified).unwrap_or(0)));
        }
        Ok(response)
    }

    pub(crate) fn put_todo(&self, head: &Head, segment: &str, name: &str, body: &[u8]) -> Result<Response, DriveError> {
        let (lists, _) = self.task_store()?;
        let Some(list) = self.list_id(segment).filter(|id| lists.iter().any(|l| l.id == *id)) else {
            return Ok(Response::text(Status::CONFLICT, "No list has this path."));
        };
        let Ok(text) = std::str::from_utf8(body) else {
            return Ok(dav_error(Status::FORBIDDEN, "<C:valid-calendar-data/>"));
        };
        let id = self.todo_id(name).unwrap_or_else(azul_storage::ids::new_uuid);
        let key = task::task_key(&list, &id);
        let current = match self.contacts.head(&key) {
            Ok(info) => Some(info),
            Err(DriveError::NotFound { .. }) => None,
            Err(e) => return Err(e),
        };
        if let Some(refusal) = precondition(head, current.as_ref().map(version_of).as_deref()) {
            return Ok(refusal);
        }
        let now = chrono::Local::now().naive_local();
        let now = now.with_nanosecond(0).unwrap_or(now);
        let mut new_id = || id.clone();
        let read = vtodo::read(text, &list, now, &mut new_id, &to_local);
        let Some(mut new) = read.tasks.into_iter().next() else {
            return Ok(dav_error(Status::FORBIDDEN, "<C:supported-calendar-component/>"));
        };
        // What iCalendar does not carry stays from the task there was.
        if let Some(old) = current.as_ref().and_then(|info| self.read_task(info, &id)) {
            new.subtasks = old.subtasks;
            new.attachments = old.attachments;
            new.flagged = old.flagged;
            new.reminder = old.reminder;
            new.reminded = old.reminded;
            new.order = old.order;
            new.created = old.created;
            // Still in progress: since when the task says, not since now.
            if new.started.is_some() {
                new.started = old.started.or(new.started);
            }
        }
        self.contacts.put(&key, task::task_to_json(&new).as_bytes())?;
        if id != name {
            self.names.set(Kind::Todo, name, &id);
        }
        if let Some(uid) = vtodo_uid(text).filter(|uid| *uid != id) {
            self.names.set(Kind::TaskUid, &id, &uid);
        }
        // No ETag: the file is AzTasks', not the program's text.
        Ok(Response::new(if current.is_some() { Status::NO_CONTENT } else { Status::CREATED }))
    }

    pub(crate) fn delete_todo(&self, head: &Head, segment: &str, name: &str) -> Result<Response, DriveError> {
        let Some(Item::Todo { info, task, .. }) = self.todo_item(segment, name)? else {
            return Ok(Response::text(Status::NOT_FOUND, "Not there."));
        };
        if let Some(refusal) = precondition(head, Some(version_of(&info).as_str())) {
            return Ok(refusal);
        }
        self.contacts.delete(&info.key)?;
        self.names.forget(Kind::Todo, name);
        self.names.forget(Kind::TaskUid, &task.id);
        Ok(Response::new(Status::NO_CONTENT))
    }

    /// MKCALENDAR for to-dos only: a new AzTasks list at `segment` - its name and colour from
    /// the body (else "Tasks" and the first colour), last in the order; its id the segment's
    /// `tasks-<id>` when that is one, else a new one kept for the segment.
    pub(crate) fn make_task_list(&self, segment: &str, props: &[Patched]) -> Result<Response, DriveError> {
        let (lists, _) = self.task_store()?;
        if self.list_id(segment).is_some_and(|id| lists.iter().any(|l| l.id == id)) {
            return Ok(Response::text(Status::METHOD_NOT_ALLOWED, "A list is there."));
        }
        let id = segment
            .strip_prefix(TASKS_PREFIX)
            .filter(|id| task::is_id(id))
            .map_or_else(azul_storage::ids::new_uuid, str::to_string);
        let order = lists.iter().map(|l| l.order).max().unwrap_or(0) + task::ORDER_STEP;
        let mut list = TaskList::new(id, String::from(task::unnamed_list_name(task::DEFAULT_LIST)), order);
        apply(&mut list, props);
        self.contacts.put(&list.key(), task::list_to_json(&list).as_bytes())?;
        if *segment != format!("{TASKS_PREFIX}{}", list.id) {
            self.names.set(Kind::TaskList, segment, &list.id);
        }
        Ok(Response::new(Status::CREATED))
    }

    /// A list's name and colour as a PROPPATCH sets them.
    pub(crate) fn patch_task_list(&self, list: &TaskList, props: &[Patched]) -> Result<(), DriveError> {
        let mut changed = list.clone();
        apply(&mut changed, props);
        if changed != *list {
            self.contacts.put(&changed.key(), task::list_to_json(&changed).as_bytes())?;
        }
        Ok(())
    }
}
