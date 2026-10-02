//! The tasks, the lists and the settings, and their files.
//!
//! Durable data is files in the per-user layout the S3 bucket will have (the azlin cloud
//! storage split), written through `azul-storage`'s `Drive`:
//!
//! ```text
//! tasks/settings.json                         the app's settings
//! tasks/<list id>/list.json                   a list: name, colour, order, group
//! tasks/<list id>/<task id>.json              a task
//! tasks/<list id>/<task id>/<file name>       a task's attachments, next to it
//! ```
//!
//! Ids are version 4 UUIDs (`[0-9a-f-]`); every key is checked to stay inside `tasks/`.
//! Dates and times are the user's wall clock (no time zone in the files), as AzCalendar keeps
//! them: `"due": "2026-10-02"`, `"time": "09:00"`, `"created": "2026-10-01T08:15:00"`.
//!
//! A task file, version 1 (empty or default fields are left out):
//!
//! ```json
//! {
//!   "format": "aztasks.task", "version": 1,
//!   "id": "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a", "list": "9d4c1f3a-...",
//!   "title": "Pay rent", "notes": "Transfer from the joint account",
//!   "due": "2026-10-02", "time": "09:00",
//!   "repeat": { "every": 1, "unit": "month", "day": 2 },
//!   "reminder": { "before_minutes": 15 }, "reminded": "2026-09-02T08:45:00",
//!   "priority": "high", "flagged": true, "tags": ["home"],
//!   "subtasks": [ { "id": "s1", "title": "Check the amount", "done": true } ],
//!   "attachments": [ { "name": "contract.pdf", "size": 48213 } ],
//!   "order": 3072,
//!   "created": "2026-09-01T10:00:00", "modified": "2026-09-30T18:12:40",
//!   "completed": "2026-10-02T09:03:11"
//! }
//! ```
//!
//! A file with a higher `version` was written by a newer AzTasks and is left alone, never
//! guessed at; fields this version does not know are ignored.

use std::fmt;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};

use crate::recur::{self, Repeat, Unit};

/// The `format` of a task file.
pub const TASK_FORMAT: &str = "aztasks.task";
/// The `format` of a list file.
pub const LIST_FORMAT: &str = "aztasks.list";
/// The `format` of the settings file.
pub const SETTINGS_FORMAT: &str = "aztasks.settings";
/// The version this AzTasks writes, and the newest it reads.
pub const VERSION: u64 = 1;
/// The folder (key prefix) of every AzTasks file.
pub const TASKS_DIR: &str = "tasks";
/// A list's own file in its folder.
pub const LIST_FILE: &str = "list.json";
/// The settings file's key.
pub const SETTINGS_KEY: &str = "tasks/settings.json";

const DATE_FORMAT: &str = "%Y-%m-%d";
const TIME_FORMAT: &str = "%H:%M";
const STAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

// ==== Priority ====

/// A task's priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Priority {
    #[default]
    None,
    Low,
    Medium,
    High,
}

impl Priority {
    /// Lowest first, as the priority control lists them.
    pub const ALL: [Priority; 4] = [
        Priority::None,
        Priority::Low,
        Priority::Medium,
        Priority::High,
    ];

    /// The name in a task file.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Priority::None => "none",
            Priority::Low => "low",
            Priority::Medium => "medium",
            Priority::High => "high",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Priority> {
        Priority::ALL.into_iter().find(|p| p.name() == name)
    }

    /// "None", "Low", "Medium", "High".
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Priority::None => "None",
            Priority::Low => "Low",
            Priority::Medium => "Medium",
            Priority::High => "High",
        }
    }

    /// The row's mark: nothing, `!`, `!!`, `!!!`.
    #[must_use]
    pub fn mark(self) -> &'static str {
        match self {
            Priority::None => "",
            Priority::Low => "!",
            Priority::Medium => "!!",
            Priority::High => "!!!",
        }
    }

    /// The position in [`Priority::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    /// The priority at `index` of [`Priority::ALL`] (`None` past the end).
    #[must_use]
    pub fn from_index(index: usize) -> Priority {
        Priority::ALL.get(index).copied().unwrap_or_default()
    }
}

// ==== Task ====

/// One step of a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subtask {
    pub id: String,
    pub title: String,
    pub done: bool,
}

/// A file attached to a task: `tasks/<list>/<task>/<name>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    /// Bytes.
    pub size: u64,
}

/// When a task reminds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reminder {
    /// At this moment.
    At(NaiveDateTime),
    /// This many minutes before the due time (a date without a time: before the
    /// settings' reminder time on that day). Moves with the due date.
    Before(i64),
}

/// A task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// A version 4 UUID, the name of the task's file.
    pub id: String,
    /// The id of the task's list (its folder).
    pub list: String,
    pub title: String,
    pub notes: String,
    pub due: Option<NaiveDate>,
    /// The due time; only with a due date.
    pub due_time: Option<NaiveTime>,
    pub repeat: Option<Repeat>,
    pub reminder: Option<Reminder>,
    /// The reminder moment that was already shown (so a restart does not show it again);
    /// a changed reminder or due date reminds again.
    pub reminded: Option<NaiveDateTime>,
    pub priority: Priority,
    pub flagged: bool,
    /// Without `#`, each once (case-insensitively).
    pub tags: Vec<String>,
    pub subtasks: Vec<Subtask>,
    pub attachments: Vec<Attachment>,
    /// The manual order in its list, ascending.
    pub order: i64,
    pub created: NaiveDateTime,
    pub modified: NaiveDateTime,
    pub completed: Option<NaiveDateTime>,
}

impl Task {
    /// A new open task with nothing set but its title.
    #[must_use]
    pub fn new(id: String, list: String, title: String, now: NaiveDateTime) -> Task {
        Task {
            id,
            list,
            title,
            notes: String::new(),
            due: None,
            due_time: None,
            repeat: None,
            reminder: None,
            reminded: None,
            priority: Priority::None,
            flagged: false,
            tags: Vec::new(),
            subtasks: Vec::new(),
            attachments: Vec::new(),
            order: 0,
            created: now,
            modified: now,
            completed: None,
        }
    }

    #[must_use]
    pub fn is_done(&self) -> bool {
        self.completed.is_some()
    }

    /// The task's file key.
    #[must_use]
    pub fn key(&self) -> String {
        task_key(&self.list, &self.id)
    }

    /// `(done, total)` of the steps, `None` without steps.
    #[must_use]
    pub fn subtask_progress(&self) -> Option<(usize, usize)> {
        if self.subtasks.is_empty() {
            return None;
        }
        let done = self.subtasks.iter().filter(|s| s.done).count();
        Some((done, self.subtasks.len()))
    }

    /// Whether the task carries `tag` (with or without `#`, any case).
    #[must_use]
    pub fn has_tag(&self, tag: &str) -> bool {
        let tag = normalize_tag(tag);
        self.tags.iter().any(|t| t.to_lowercase() == tag.to_lowercase())
    }

    /// Adds `tag` (without its `#`); `false` when it is empty or already there.
    pub fn add_tag(&mut self, tag: &str) -> bool {
        let tag = normalize_tag(tag);
        if tag.is_empty() || self.has_tag(&tag) {
            return false;
        }
        self.tags.push(tag);
        true
    }

    /// Removes `tag`; `false` when the task did not carry it.
    pub fn remove_tag(&mut self, tag: &str) -> bool {
        let tag = normalize_tag(tag).to_lowercase();
        let before = self.tags.len();
        self.tags.retain(|t| t.to_lowercase() != tag);
        self.tags.len() != before
    }

    /// The task a completed repeating task leaves behind: the next occurrence
    /// ([`recur::next_occurrence`]) under `new_id`, open, its steps not done, its reminder
    /// moved by as many days as the due date, no attachments (they stay with the completed
    /// task's files). `None` for a task that does not repeat.
    #[must_use]
    pub fn spawn_next(
        &self,
        new_id: String,
        completed_on: NaiveDate,
        today: NaiveDate,
        now: NaiveDateTime,
    ) -> Option<Task> {
        let repeat = self.repeat.as_ref()?;
        let due = self.due.unwrap_or(completed_on);
        let rule = repeat.clone().anchored(due);
        let next_due = recur::next_occurrence(&rule, due, completed_on, today);
        let shift = next_due - due;
        let mut next = self.clone();
        next.id = new_id;
        next.due = Some(next_due);
        next.repeat = Some(rule);
        next.completed = None;
        next.reminded = None;
        next.reminder = self.reminder.map(|r| match r {
            Reminder::At(at) => Reminder::At(at + shift),
            before => before,
        });
        for step in &mut next.subtasks {
            step.done = false;
        }
        next.attachments = Vec::new();
        next.created = now;
        next.modified = now;
        Some(next)
    }
}

/// A tag as a task keeps it: trimmed, without its leading `#`s.
#[must_use]
pub fn normalize_tag(tag: &str) -> String {
    tag.trim().trim_start_matches('#').trim().to_string()
}

// ==== List ====

/// A list's colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ListColor {
    #[default]
    Blue,
    Green,
    Red,
    Orange,
    Purple,
    Teal,
    Gray,
    Pink,
}

impl ListColor {
    pub const ALL: [ListColor; 8] = [
        ListColor::Blue,
        ListColor::Green,
        ListColor::Red,
        ListColor::Orange,
        ListColor::Purple,
        ListColor::Teal,
        ListColor::Gray,
        ListColor::Pink,
    ];

    /// The name in a list file.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ListColor::Blue => "blue",
            ListColor::Green => "green",
            ListColor::Red => "red",
            ListColor::Orange => "orange",
            ListColor::Purple => "purple",
            ListColor::Teal => "teal",
            ListColor::Gray => "gray",
            ListColor::Pink => "pink",
        }
    }

    /// The colour a file names; blue for a name this version does not know.
    #[must_use]
    pub fn from_name(name: &str) -> ListColor {
        ListColor::ALL
            .into_iter()
            .find(|c| c.name() == name)
            .unwrap_or_default()
    }

    /// "Blue", "Green", ...
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ListColor::Blue => "Blue",
            ListColor::Green => "Green",
            ListColor::Red => "Red",
            ListColor::Orange => "Orange",
            ListColor::Purple => "Purple",
            ListColor::Teal => "Teal",
            ListColor::Gray => "Gray",
            ListColor::Pink => "Pink",
        }
    }

    /// The colour's dot: `#rrggbb` for the light mode, a lighter twin for the dark one.
    #[must_use]
    pub fn hex(self, dark: bool) -> &'static str {
        match (self, dark) {
            (ListColor::Blue, false) => "#2f6fd6",
            (ListColor::Blue, true) => "#7aa7f0",
            (ListColor::Green, false) => "#2e8b57",
            (ListColor::Green, true) => "#76c893",
            (ListColor::Red, false) => "#c62828",
            (ListColor::Red, true) => "#ef7b7b",
            (ListColor::Orange, false) => "#d9730d",
            (ListColor::Orange, true) => "#f2a65a",
            (ListColor::Purple, false) => "#7b4fc4",
            (ListColor::Purple, true) => "#b39af0",
            (ListColor::Teal, false) => "#13827f",
            (ListColor::Teal, true) => "#5cc6c1",
            (ListColor::Gray, false) => "#6b7280",
            (ListColor::Gray, true) => "#a5abb5",
            (ListColor::Pink, false) => "#c2185b",
            (ListColor::Pink, true) => "#f48fb1",
        }
    }
}

/// A list of tasks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskList {
    /// A version 4 UUID, the name of the list's folder.
    pub id: String,
    pub name: String,
    pub color: ListColor,
    /// The list's place in the navigation pane, ascending.
    pub order: i64,
    /// The group the list sits in ("Azlin launch"); empty = none.
    pub group: String,
}

impl TaskList {
    #[must_use]
    pub fn new(id: String, name: String, order: i64) -> TaskList {
        TaskList {
            id,
            name,
            color: ListColor::Blue,
            order,
            group: String::new(),
        }
    }

    /// The list's file key.
    #[must_use]
    pub fn key(&self) -> String {
        list_key(&self.id)
    }
}

// ==== Settings ====

/// How a list orders its tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SortMode {
    /// The order the user dragged them into.
    #[default]
    Manual,
    Due,
    Priority,
    Title,
    Created,
}

impl SortMode {
    pub const ALL: [SortMode; 5] = [
        SortMode::Manual,
        SortMode::Due,
        SortMode::Priority,
        SortMode::Title,
        SortMode::Created,
    ];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            SortMode::Manual => "manual",
            SortMode::Due => "due",
            SortMode::Priority => "priority",
            SortMode::Title => "title",
            SortMode::Created => "created",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<SortMode> {
        SortMode::ALL.into_iter().find(|m| m.name() == name)
    }

    /// "Manual", "Due date", ...
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SortMode::Manual => "Manual",
            SortMode::Due => "Due date",
            SortMode::Priority => "Priority",
            SortMode::Title => "Title",
            SortMode::Created => "Created",
        }
    }
}

/// The app's settings (`tasks/settings.json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The list new tasks go to outside a list (Today, All, ...); empty = the first list.
    pub default_list: String,
    /// The first day of the week the app shows.
    pub week_start: Weekday,
    /// When a reminder of a task due on a day without a time goes off.
    pub reminder_time: NaiveTime,
    /// Reminders play the system sound.
    pub sounds: bool,
    /// Reminders also post an OS notification (where the OS allows one).
    pub notifications: bool,
    /// A list shows its completed tasks under the open ones.
    pub show_completed: bool,
    pub sort: SortMode,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            default_list: String::new(),
            week_start: Weekday::Mon,
            reminder_time: NaiveTime::from_hms_opt(9, 0, 0).unwrap_or(NaiveTime::MIN),
            sounds: true,
            notifications: true,
            show_completed: true,
            sort: SortMode::Manual,
        }
    }
}

// ==== Keys ====

/// Whether `id` can name a list or task folder / file: 1 to 64 of `[0-9a-z-]`.
#[must_use]
pub fn is_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() || b == b'-')
}

/// `tasks/<list>/list.json`.
#[must_use]
pub fn list_key(list: &str) -> String {
    format!("{TASKS_DIR}/{list}/{LIST_FILE}")
}

/// `tasks/<list>/<task>.json`.
#[must_use]
pub fn task_key(list: &str, task: &str) -> String {
    format!("{TASKS_DIR}/{list}/{task}.json")
}

/// `tasks/<list>/<task>/`: the folder of a task's attachments.
#[must_use]
pub fn attachments_prefix(list: &str, task: &str) -> String {
    format!("{TASKS_DIR}/{list}/{task}/")
}

/// `tasks/<list>/<task>/<name>`, the name made safe for every drive
/// (`azul_storage::key::safe_file_name`); `None` when nothing usable is left.
#[must_use]
pub fn attachment_key(list: &str, task: &str, name: &str) -> Option<String> {
    let name = azul_storage::key::safe_file_name(name)?;
    Some(format!("{}{name}", attachments_prefix(list, task)))
}

/// What a key under `tasks/` is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyKind {
    Settings,
    List { list: String },
    Task { list: String, task: String },
    Attachment { list: String, task: String, name: String },
    /// Anything else (a file this version does not know, a bad id): left alone.
    Other,
}

/// What `key` is in the AzTasks layout.
#[must_use]
pub fn parse_key(key: &str) -> KeyKind {
    if key == SETTINGS_KEY {
        return KeyKind::Settings;
    }
    let Some(rest) = key.strip_prefix(TASKS_DIR).and_then(|r| r.strip_prefix('/')) else {
        return KeyKind::Other;
    };
    let parts: Vec<&str> = rest.split('/').collect();
    match parts.as_slice() {
        [list, file] if is_id(list) => {
            if *file == LIST_FILE {
                KeyKind::List {
                    list: (*list).to_string(),
                }
            } else {
                match file.strip_suffix(".json") {
                    Some(task) if is_id(task) => KeyKind::Task {
                        list: (*list).to_string(),
                        task: task.to_string(),
                    },
                    _ => KeyKind::Other,
                }
            }
        }
        [list, task, name]
            if is_id(list) && is_id(task) && !name.is_empty() && *name != "." && *name != ".." =>
        {
            KeyKind::Attachment {
                list: (*list).to_string(),
                task: (*task).to_string(),
                name: (*name).to_string(),
            }
        }
        _ => KeyKind::Other,
    }
}

// ==== Files ====

/// Why a file cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    /// The file is not JSON.
    NotJson(String),
    /// JSON, but not this kind of AzTasks file (`format` missing or different).
    WrongFormat,
    /// Written by a newer AzTasks.
    NewerVersion(u64),
    /// A field is missing or of the wrong form.
    Malformed(String),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileError::NotJson(e) => write!(f, "not JSON: {e}"),
            FileError::WrongFormat => write!(f, "not an AzTasks file of this kind"),
            FileError::NewerVersion(v) => {
                write!(f, "written by a newer AzTasks (version {v}); left alone")
            }
            FileError::Malformed(e) => write!(f, "malformed: {e}"),
        }
    }
}

impl std::error::Error for FileError {}

/// For `skip_serializing_if`: a `false` flag is left out of the file.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde passes a reference
fn is_false(flag: &bool) -> bool {
    !*flag
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde passes a reference
fn is_zero(n: &i64) -> bool {
    *n == 0
}

fn is_no_priority(p: &str) -> bool {
    p.is_empty() || p == Priority::None.name()
}

#[derive(Debug, Serialize, Deserialize)]
struct RepeatFile {
    every: u32,
    unit: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    days: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    day: Option<u32>,
    #[serde(default, skip_serializing_if = "is_false")]
    from_completion: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReminderFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    before_minutes: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SubtaskFile {
    id: String,
    title: String,
    #[serde(default, skip_serializing_if = "is_false")]
    done: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct AttachmentFile {
    name: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct TaskFile {
    format: String,
    version: u64,
    id: String,
    list: String,
    title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repeat: Option<RepeatFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reminder: Option<ReminderFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reminded: Option<String>,
    #[serde(default, skip_serializing_if = "is_no_priority")]
    priority: String,
    #[serde(default, skip_serializing_if = "is_false")]
    flagged: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    subtasks: Vec<SubtaskFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<AttachmentFile>,
    #[serde(default, skip_serializing_if = "is_zero")]
    order: i64,
    created: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    modified: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    completed: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ListFile {
    format: String,
    version: u64,
    id: String,
    name: String,
    #[serde(default)]
    color: String,
    #[serde(default)]
    order: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    group: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
struct SettingsFile {
    format: String,
    version: u64,
    default_list: String,
    week_start: String,
    reminder_time: String,
    sounds: bool,
    notifications: bool,
    show_completed: bool,
    sort: String,
}

impl Default for SettingsFile {
    fn default() -> Self {
        settings_file(&Settings::default())
    }
}

fn settings_file(s: &Settings) -> SettingsFile {
    SettingsFile {
        format: SETTINGS_FORMAT.to_string(),
        version: VERSION,
        default_list: s.default_list.clone(),
        week_start: azul_pim::dates::weekday_key(s.week_start).to_string(),
        reminder_time: format_time(s.reminder_time),
        sounds: s.sounds,
        notifications: s.notifications,
        show_completed: s.show_completed,
        sort: s.sort.name().to_string(),
    }
}

/// `2026-10-02`.
#[must_use]
pub fn format_date(date: NaiveDate) -> String {
    date.format(DATE_FORMAT).to_string()
}

/// `09:00`.
#[must_use]
pub fn format_time(time: NaiveTime) -> String {
    time.format(TIME_FORMAT).to_string()
}

/// `2026-10-02T09:00:00`.
#[must_use]
pub fn format_stamp(stamp: NaiveDateTime) -> String {
    stamp.format(STAMP_FORMAT).to_string()
}

fn parse_date(field: &str, text: &str) -> Result<NaiveDate, FileError> {
    NaiveDate::parse_from_str(text, DATE_FORMAT)
        .map_err(|_| FileError::Malformed(format!("{field} \"{text}\" is not a YYYY-MM-DD date")))
}

fn parse_time(field: &str, text: &str) -> Result<NaiveTime, FileError> {
    NaiveTime::parse_from_str(text, TIME_FORMAT)
        .map_err(|_| FileError::Malformed(format!("{field} \"{text}\" is not an HH:MM time")))
}

fn parse_stamp(field: &str, text: &str) -> Result<NaiveDateTime, FileError> {
    NaiveDateTime::parse_from_str(text, STAMP_FORMAT).map_err(|_| {
        FileError::Malformed(format!("{field} \"{text}\" is not a YYYY-MM-DDTHH:MM:SS moment"))
    })
}

/// Checks `format` and `version` of a parsed file.
fn check_header(format: &str, version: u64, expected: &str) -> Result<(), FileError> {
    if format != expected {
        return Err(FileError::WrongFormat);
    }
    if version > VERSION {
        return Err(FileError::NewerVersion(version));
    }
    Ok(())
}

/// Reads `format` and `version` first, so a newer file is refused for its version and not
/// for a field it renamed.
fn header_of(json: &str, expected: &str) -> Result<(), FileError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| FileError::NotJson(e.to_string()))?;
    let format = value.get("format").and_then(|f| f.as_str()).unwrap_or("");
    let version = value.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0);
    check_header(format, version, expected)
}

fn repeat_file(r: &Repeat) -> RepeatFile {
    RepeatFile {
        every: r.every,
        unit: r.unit.name().to_string(),
        days: r
            .weekdays
            .iter()
            .map(|d| azul_pim::dates::weekday_key(*d).to_string())
            .collect(),
        day: r.month_day,
        from_completion: r.from_completion,
    }
}

fn repeat_of(f: &RepeatFile) -> Result<Repeat, FileError> {
    let unit = Unit::from_name(&f.unit)
        .ok_or_else(|| FileError::Malformed(format!("repeat unit \"{}\"", f.unit)))?;
    let mut days = Vec::new();
    for d in &f.days {
        days.push(
            azul_pim::dates::weekday_from_key(d)
                .ok_or_else(|| FileError::Malformed(format!("repeat day \"{d}\"")))?,
        );
    }
    let mut rule = Repeat::new(f.every, unit)
        .on_weekdays(&days)
        .counting_from_completion(f.from_completion);
    if let Some(day) = f.day {
        rule = rule.on_month_day(day);
    }
    Ok(rule)
}

/// The task's file.
#[must_use]
pub fn task_to_json(t: &Task) -> String {
    let file = TaskFile {
        format: TASK_FORMAT.to_string(),
        version: VERSION,
        id: t.id.clone(),
        list: t.list.clone(),
        title: t.title.clone(),
        notes: t.notes.clone(),
        due: t.due.map(format_date),
        time: t.due.and(t.due_time).map(format_time),
        repeat: t.repeat.as_ref().map(repeat_file),
        reminder: t.reminder.map(|r| match r {
            Reminder::At(at) => ReminderFile {
                at: Some(format_stamp(at)),
                before_minutes: None,
            },
            Reminder::Before(minutes) => ReminderFile {
                at: None,
                before_minutes: Some(minutes),
            },
        }),
        reminded: t.reminded.map(format_stamp),
        priority: t.priority.name().to_string(),
        flagged: t.flagged,
        tags: t.tags.clone(),
        subtasks: t
            .subtasks
            .iter()
            .map(|s| SubtaskFile {
                id: s.id.clone(),
                title: s.title.clone(),
                done: s.done,
            })
            .collect(),
        attachments: t
            .attachments
            .iter()
            .map(|a| AttachmentFile {
                name: a.name.clone(),
                size: a.size,
            })
            .collect(),
        order: t.order,
        created: format_stamp(t.created),
        modified: format_stamp(t.modified),
        completed: t.completed.map(format_stamp),
    };
    serde_json::to_string_pretty(&file).unwrap_or_default()
}

/// A task from its file.
pub fn task_from_json(json: &str) -> Result<Task, FileError> {
    header_of(json, TASK_FORMAT)?;
    let f: TaskFile =
        serde_json::from_str(json).map_err(|e| FileError::Malformed(e.to_string()))?;
    if !is_id(&f.id) {
        return Err(FileError::Malformed(format!("id \"{}\"", f.id)));
    }
    if !is_id(&f.list) {
        return Err(FileError::Malformed(format!("list \"{}\"", f.list)));
    }
    let due = f.due.as_deref().map(|d| parse_date("due", d)).transpose()?;
    let due_time = match (due, f.time.as_deref()) {
        (Some(_), Some(t)) => Some(parse_time("time", t)?),
        _ => None,
    };
    let reminder = match f.reminder {
        None => None,
        Some(ReminderFile { at: Some(at), .. }) => Some(Reminder::At(parse_stamp("reminder", &at)?)),
        Some(ReminderFile {
            before_minutes: Some(minutes),
            ..
        }) => Some(Reminder::Before(minutes)),
        Some(_) => None,
    };
    let created = parse_stamp("created", &f.created)?;
    let modified = if f.modified.is_empty() {
        created
    } else {
        parse_stamp("modified", &f.modified)?
    };
    let priority = if f.priority.is_empty() {
        Priority::None
    } else {
        Priority::from_name(&f.priority)
            .ok_or_else(|| FileError::Malformed(format!("priority \"{}\"", f.priority)))?
    };
    let mut task = Task {
        id: f.id,
        list: f.list,
        title: f.title,
        notes: f.notes,
        due,
        due_time,
        repeat: f.repeat.as_ref().map(repeat_of).transpose()?,
        reminder,
        reminded: f
            .reminded
            .as_deref()
            .map(|r| parse_stamp("reminded", r))
            .transpose()?,
        priority,
        flagged: f.flagged,
        tags: Vec::new(),
        subtasks: f
            .subtasks
            .into_iter()
            .map(|s| Subtask {
                id: s.id,
                title: s.title,
                done: s.done,
            })
            .collect(),
        attachments: f
            .attachments
            .into_iter()
            .map(|a| Attachment {
                name: a.name,
                size: a.size,
            })
            .collect(),
        order: f.order,
        created,
        modified,
        completed: f
            .completed
            .as_deref()
            .map(|c| parse_stamp("completed", c))
            .transpose()?,
    };
    for tag in &f.tags {
        task.add_tag(tag);
    }
    Ok(task)
}

/// The list's file.
#[must_use]
pub fn list_to_json(l: &TaskList) -> String {
    let file = ListFile {
        format: LIST_FORMAT.to_string(),
        version: VERSION,
        id: l.id.clone(),
        name: l.name.clone(),
        color: l.color.name().to_string(),
        order: l.order,
        group: l.group.clone(),
    };
    serde_json::to_string_pretty(&file).unwrap_or_default()
}

/// A list from its file.
pub fn list_from_json(json: &str) -> Result<TaskList, FileError> {
    header_of(json, LIST_FORMAT)?;
    let f: ListFile =
        serde_json::from_str(json).map_err(|e| FileError::Malformed(e.to_string()))?;
    if !is_id(&f.id) {
        return Err(FileError::Malformed(format!("id \"{}\"", f.id)));
    }
    Ok(TaskList {
        id: f.id,
        name: f.name,
        color: ListColor::from_name(&f.color),
        order: f.order,
        group: f.group.trim().to_string(),
    })
}

/// The settings file.
#[must_use]
pub fn settings_to_json(s: &Settings) -> String {
    serde_json::to_string_pretty(&settings_file(s)).unwrap_or_default()
}

/// The settings from their file; a field that is missing or unreadable keeps its default.
pub fn settings_from_json(json: &str) -> Result<Settings, FileError> {
    header_of(json, SETTINGS_FORMAT)?;
    let f: SettingsFile =
        serde_json::from_str(json).map_err(|e| FileError::Malformed(e.to_string()))?;
    let defaults = Settings::default();
    Ok(Settings {
        default_list: if is_id(&f.default_list) {
            f.default_list
        } else {
            String::new()
        },
        week_start: azul_pim::dates::weekday_from_key(&f.week_start).unwrap_or(defaults.week_start),
        reminder_time: NaiveTime::parse_from_str(&f.reminder_time, TIME_FORMAT)
            .unwrap_or(defaults.reminder_time),
        sounds: f.sounds,
        notifications: f.notifications,
        show_completed: f.show_completed,
        sort: SortMode::from_name(&f.sort).unwrap_or(defaults.sort),
    })
}

/// The moment `minutes` before `at`.
#[must_use]
pub fn minutes_before(at: NaiveDateTime, minutes: i64) -> NaiveDateTime {
    at - Duration::minutes(minutes)
}

/// A day as the app names it next to `today`: "Today", "Tomorrow", "Yesterday", else
/// "Fri 2 Oct" (with the year when it is not this year's: "Fri 1 Jan 2027").
#[must_use]
pub fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        _ if date.year() == today.year() => date.format("%a %-d %b").to_string(),
        _ => date.format("%a %-d %b %Y").to_string(),
    }
}

/// A day as a section heading names it: "Today", "Tomorrow", else "Saturday 3 October"
/// (with the year when it is not this year's).
#[must_use]
pub fn day_heading(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        _ if date.year() == today.year() => date.format("%A %-d %B").to_string(),
        _ => date.format("%A %-d %B %Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3";
    const TASK: &str = "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a";

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        day(y, m, d).and_hms_opt(h, min, 0).unwrap()
    }

    fn full_task() -> Task {
        let mut t = Task::new(
            TASK.into(),
            LIST.into(),
            "Pay rent".into(),
            at(2026, 9, 1, 10, 0),
        );
        t.notes = "Transfer from the joint account\nRef: 2026-10".into();
        t.due = Some(day(2026, 10, 2));
        t.due_time = NaiveTime::from_hms_opt(9, 0, 0);
        t.repeat = Some(Repeat::monthly().on_month_day(2));
        t.reminder = Some(Reminder::Before(15));
        t.reminded = Some(at(2026, 9, 2, 8, 45));
        t.priority = Priority::High;
        t.flagged = true;
        t.add_tag("#home");
        t.subtasks = vec![
            Subtask {
                id: "s1".into(),
                title: "Check the amount".into(),
                done: true,
            },
            Subtask {
                id: "s2".into(),
                title: "Send it".into(),
                done: false,
            },
        ];
        t.attachments = vec![Attachment {
            name: "contract.pdf".into(),
            size: 48213,
        }];
        t.order = 3072;
        t.modified = at(2026, 9, 30, 18, 12);
        t
    }

    #[test]
    fn a_task_round_trips_through_its_file() {
        let t = full_task();
        let json = task_to_json(&t);
        assert!(json.contains("\"format\": \"aztasks.task\""), "{json}");
        assert!(json.contains("\"due\": \"2026-10-02\""), "{json}");
        assert!(json.contains("\"time\": \"09:00\""), "{json}");
        assert_eq!(task_from_json(&json), Ok(t));
    }

    #[test]
    fn a_minimal_task_file_reads_with_every_default() {
        let json = format!(
            r#"{{"format":"aztasks.task","version":1,"id":"{TASK}","list":"{LIST}",
               "title":"Buy milk","created":"2026-10-01T08:00:00","later":"ignored"}}"#
        );
        let t = task_from_json(&json).unwrap();
        assert_eq!(t.title, "Buy milk");
        assert_eq!(t.due, None);
        assert_eq!(t.priority, Priority::None);
        assert_eq!(t.modified, t.created, "no modified stamp means the creation");
        assert!(!t.is_done() && t.tags.is_empty() && t.subtasks.is_empty());
        let written = task_to_json(&t);
        assert!(!written.contains("\"priority\""), "defaults are left out: {written}");
        assert!(!written.contains("\"flagged\""), "{written}");
    }

    #[test]
    fn a_file_of_another_format_or_a_newer_version_is_refused() {
        assert_eq!(
            task_from_json(r#"{"format":"azcalendar.event","version":1}"#),
            Err(FileError::WrongFormat)
        );
        assert_eq!(
            task_from_json(r#"{"format":"aztasks.task","version":7,"renamed":"x"}"#),
            Err(FileError::NewerVersion(7))
        );
        assert!(matches!(task_from_json("not json"), Err(FileError::NotJson(_))));
        let bad_due = format!(
            r#"{{"format":"aztasks.task","version":1,"id":"{TASK}","list":"{LIST}",
               "title":"x","due":"tomorrow","created":"2026-10-01T08:00:00"}}"#
        );
        assert!(matches!(task_from_json(&bad_due), Err(FileError::Malformed(_))));
        let bad_id = r#"{"format":"aztasks.task","version":1,"id":"../x","list":"a",
               "title":"x","created":"2026-10-01T08:00:00"}"#;
        assert!(matches!(task_from_json(bad_id), Err(FileError::Malformed(_))));
    }

    #[test]
    fn a_time_without_a_date_is_not_kept() {
        let mut t = full_task();
        t.due = None;
        let back = task_from_json(&task_to_json(&t)).unwrap();
        assert_eq!(back.due_time, None);
    }

    #[test]
    fn a_list_round_trips_and_an_unknown_colour_is_blue() {
        let mut l = TaskList::new(LIST.into(), "Design".into(), 5);
        l.color = ListColor::Teal;
        l.group = "Azlin launch".into();
        assert_eq!(list_from_json(&list_to_json(&l)), Ok(l.clone()));
        let json = format!(
            r#"{{"format":"aztasks.list","version":1,"id":"{LIST}","name":"X","color":"chartreuse"}}"#
        );
        assert_eq!(list_from_json(&json).unwrap().color, ListColor::Blue);
    }

    #[test]
    fn settings_round_trip_and_missing_fields_keep_their_defaults() {
        let s = Settings {
            default_list: LIST.into(),
            week_start: Weekday::Sun,
            reminder_time: NaiveTime::from_hms_opt(7, 30, 0).unwrap(),
            sounds: false,
            notifications: false,
            show_completed: false,
            sort: SortMode::Priority,
        };
        assert_eq!(settings_from_json(&settings_to_json(&s)), Ok(s));
        let partial = r#"{"format":"aztasks.settings","version":1,"week_start":"sat"}"#;
        let read = settings_from_json(partial).unwrap();
        assert_eq!(read.week_start, Weekday::Sat);
        assert_eq!(read.reminder_time, Settings::default().reminder_time);
        assert!(read.sounds && read.notifications && read.show_completed);
    }

    #[test]
    fn keys_follow_the_bucket_layout() {
        assert_eq!(list_key(LIST), format!("tasks/{LIST}/list.json"));
        assert_eq!(task_key(LIST, TASK), format!("tasks/{LIST}/{TASK}.json"));
        assert_eq!(
            attachment_key(LIST, TASK, "a/b:c.pdf"),
            Some(format!("tasks/{LIST}/{TASK}/b_c.pdf")),
            "the last segment, made safe"
        );
        assert_eq!(attachment_key(LIST, TASK, ".."), None);
        assert_eq!(parse_key(SETTINGS_KEY), KeyKind::Settings);
        assert_eq!(
            parse_key(&list_key(LIST)),
            KeyKind::List { list: LIST.into() }
        );
        assert_eq!(
            parse_key(&task_key(LIST, TASK)),
            KeyKind::Task {
                list: LIST.into(),
                task: TASK.into()
            }
        );
        assert_eq!(
            parse_key(&format!("tasks/{LIST}/{TASK}/photo.jpg")),
            KeyKind::Attachment {
                list: LIST.into(),
                task: TASK.into(),
                name: "photo.jpg".into()
            }
        );
        for other in [
            "notes/a/b.md",
            "tasks/UPPER/list.json",
            "tasks/a/b/c/d.txt",
            "tasks/a/readme.txt",
            "tasks",
        ] {
            assert_eq!(parse_key(other), KeyKind::Other, "{other}");
        }
    }

    #[test]
    fn completing_a_repeating_task_leaves_the_next_occurrence_with_fresh_steps() {
        let mut t = full_task();
        t.reminder = Some(Reminder::At(at(2026, 10, 2, 8, 0)));
        let now = at(2026, 10, 2, 9, 3);
        let next = t
            .spawn_next("new-id".into(), day(2026, 10, 2), day(2026, 10, 2), now)
            .unwrap();
        assert_eq!(next.id, "new-id");
        assert_eq!(next.list, t.list);
        assert_eq!(next.due, Some(day(2026, 11, 2)));
        assert_eq!(next.due_time, t.due_time);
        assert_eq!(next.reminder, Some(Reminder::At(at(2026, 11, 2, 8, 0))));
        assert_eq!(next.reminded, None);
        assert!(next.subtasks.iter().all(|s| !s.done));
        assert_eq!(next.subtasks.len(), 2);
        assert!(next.attachments.is_empty(), "files stay with the completed task");
        assert!(!next.is_done());
        assert_eq!(next.created, now);
        assert_eq!(next.tags, t.tags);
    }

    #[test]
    fn a_reminder_before_the_due_time_moves_with_it_and_a_task_without_a_repeat_spawns_nothing() {
        let mut t = full_task();
        t.repeat = Some(Repeat::weekly());
        let next = t
            .spawn_next("n".into(), day(2026, 10, 2), day(2026, 10, 2), at(2026, 10, 2, 9, 0))
            .unwrap();
        assert_eq!(next.due, Some(day(2026, 10, 9)));
        assert_eq!(next.reminder, Some(Reminder::Before(15)));
        t.repeat = None;
        assert_eq!(
            t.spawn_next("n".into(), day(2026, 10, 2), day(2026, 10, 2), at(2026, 10, 2, 9, 0)),
            None
        );
    }

    #[test]
    fn tags_are_kept_once_without_their_hash() {
        let mut t = Task::new(TASK.into(), LIST.into(), "x".into(), at(2026, 10, 1, 8, 0));
        assert!(t.add_tag("#Home"));
        assert!(!t.add_tag("home"), "the same tag in another case");
        assert!(!t.add_tag("  # "), "nothing left");
        assert_eq!(t.tags, vec!["Home"]);
        assert!(t.has_tag("#HOME"));
        assert!(t.remove_tag("home"));
        assert!(t.tags.is_empty());
    }

    #[test]
    fn step_progress_counts_the_done_steps() {
        let t = full_task();
        assert_eq!(t.subtask_progress(), Some((1, 2)));
        let bare = Task::new(TASK.into(), LIST.into(), "x".into(), at(2026, 10, 1, 8, 0));
        assert_eq!(bare.subtask_progress(), None);
    }

    #[test]
    fn priorities_and_sort_modes_have_stable_names() {
        for p in Priority::ALL {
            assert_eq!(Priority::from_name(p.name()), Some(p));
            assert_eq!(Priority::from_index(p.index()), p);
        }
        assert_eq!(Priority::High.mark(), "!!!");
        assert!(Priority::High > Priority::Low);
        for m in SortMode::ALL {
            assert_eq!(SortMode::from_name(m.name()), Some(m));
        }
        assert!(is_id(TASK) && !is_id("") && !is_id("a/b") && !is_id("A"));
    }

    #[test]
    fn days_are_named_from_today() {
        let today = day(2026, 10, 1);
        assert_eq!(day_label(today, today), "Today");
        assert_eq!(day_label(day(2026, 10, 2), today), "Tomorrow");
        assert_eq!(day_label(day(2026, 9, 30), today), "Yesterday");
        assert_eq!(day_label(day(2026, 10, 9), today), "Fri 9 Oct");
        assert_eq!(day_label(day(2027, 1, 1), today), "Fri 1 Jan 2027");
        assert_eq!(day_heading(day(2026, 10, 3), today), "Saturday 3 October");
    }
}
