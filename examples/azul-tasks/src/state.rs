//! The app's state and every change to it, without the window: the panes call these and
//! rebuild. A change to a task, a list or the settings goes to the write queue at once
//! (`save_*`); the queue is drained on a `Thread` (`jobs.rs`).
//!
//! On stdout, for scripts (`scripts/aztasks_e2e.py`): `AZTASKS_ADDED <task> <list> <due|->`,
//! `AZTASKS_COMPLETED <task>`, `AZTASKS_REOPENED <task>`, `AZTASKS_SPAWNED <task> <due>`,
//! `AZTASKS_DELETED <task>`, `AZTASKS_MOVED <task> <list>`, `AZTASKS_SELECTED <task>`,
//! `AZTASKS_VIEW <view>`, `AZTASKS_LIST <list> <name>` (a new list).

use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use azul::widgets::ListSelection;
use azul_storage::Drive;
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike};

use crate::{
    model::{self, Settings, Task, TaskList},
    parse::{self, Parsed},
    store::{self, WriteQueue},
    views::{self, Section, Smart, View},
};

/// A FILE (backstage) page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Settings,
    Shortcuts,
    About,
}

impl Page {
    /// In the backstage's order.
    pub const ALL: [Page; 3] = [Page::Settings, Page::Shortcuts, Page::About];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Page::Settings => "Settings",
            Page::Shortcuts => "Keyboard shortcuts",
            Page::About => "About",
        }
    }
}

/// A question a dialog asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    /// Delete this list and its tasks.
    DeleteList(String),
    /// Delete the completed tasks older than 30 days.
    ClearCompleted,
}

/// The tasks the last delete removed, for "Undo"; their attachment folders are deleted
/// when the undo is gone.
#[derive(Debug, Clone, Default)]
pub struct Undo {
    pub tasks: Vec<Task>,
    pub prefixes: Vec<String>,
}

/// The quick-add line.
#[derive(Debug, Clone, Default)]
pub struct QuickAdd {
    pub text: String,
    /// Word indices the user clicked away (taken literally).
    pub ignore: Vec<usize>,
    /// The chip labels the window shows, to rebuild only when they change.
    pub shown: Vec<String>,
}

/// The text fields' texts as typed (the fields are built with them; see the progress
/// file's note on text fields).
#[derive(Debug, Clone, Default)]
pub struct Drafts {
    /// The task the title / notes drafts belong to.
    pub task: String,
    pub title: String,
    pub notes: String,
    /// The "Add a step" line.
    pub step: String,
    /// The "Add a tag" line.
    pub tag: String,
    /// The list being edited in the list settings.
    pub list: String,
    pub list_name: String,
    pub list_group: String,
    /// The To-Do bar's task line.
    pub todo: String,
    /// The repeat editor ("Custom...") is open.
    pub custom_repeat: bool,
}

/// File work running on a thread (besides the write queue).
#[derive(Debug, Clone, Default)]
pub struct FileWork {
    pub running: usize,
    pub last_error: String,
}

/// Everything AzTasks knows.
pub struct Tasks {
    // ---- data
    pub lists: Vec<TaskList>,
    pub tasks: Vec<Task>,
    pub settings: Settings,
    /// The files have been read.
    pub loaded: bool,
    pub load_error: String,
    pub skipped: Vec<store::Skipped>,
    // ---- storage
    pub drive: Arc<dyn Drive>,
    /// The data folder (the drive's root).
    pub root: PathBuf,
    pub queue: WriteQueue,
    pub files: FileWork,
    /// The appearance kept across restarts (`aztasks/settings.json`, `appearance.rs`).
    pub appearance: azul_appkit::settings::AppSettings,
    /// The Data settings' import path, and what the last import or export did.
    pub import_path: String,
    pub io_message: String,
    /// Fill an empty data folder with the sample once it is read.
    pub sample_requested: bool,
    // ---- what is shown
    pub view: View,
    /// The selected tasks, keyed by `ListSelection::key_of(id)` (azul's list
    /// selection: the anchor of a Shift range and the row the keyboard is on).
    pub selection: ListSelection,
    pub search: String,
    pub quick: QuickAdd,
    pub drafts: Drafts,
    /// The navigation pane's groups: smart lists, my lists, tags.
    pub nav_open: [bool; 3],
    pub nav_collapsed: bool,
    /// List groups folded in the "My lists" tree.
    pub folded_groups: BTreeSet<String>,
    /// A list's "Completed (n)" section is unfolded.
    pub completed_open: bool,
    /// Scheduled shows the planned month (else the list of days), and the month it shows (any
    /// day of it).
    pub planned_month: bool,
    pub month: NaiveDate,
    /// A list shows its board (To do / Doing / Done) instead of the list.
    pub board: bool,
    pub ribbon_tab: usize,
    pub page: Option<Page>,
    pub settings_category: usize,
    pub settings_search: String,
    /// The command palette's query while it is open.
    pub palette: Option<String>,
    pub show_todo_bar: bool,
    /// The To-Do bar's day.
    pub todo_day: NaiveDate,
    /// The list whose settings the reading pane shows.
    pub editing_list: Option<String>,
    pub confirm: Option<Confirm>,
    // ---- reminders
    /// Tasks reminding now (ids), shown in the banner.
    pub banners: Vec<String>,
    /// `PlatformCapability::notifications()`: available, and the backend or why not.
    pub os_notifications: (bool, String),
    // ---- feedback
    pub notice: String,
    pub undo: Option<Undo>,
    /// The task a drag carries.
    pub drag: Option<String>,
    /// The moment the window was last built for (a new day rebuilds).
    pub clock: NaiveDateTime,
}

/// A monthly or yearly repeat follows its task's due day of the month (the due date moved).
pub fn reanchor(t: &mut Task) {
    if let (Some(rule), Some(due)) = (t.repeat.as_mut(), t.due) {
        if matches!(rule.unit, crate::recur::Unit::Month | crate::recur::Unit::Year) {
            rule.month_day = Some(due.day());
        }
    }
}

/// The user's wall clock now, to the second.
#[must_use]
pub fn now() -> NaiveDateTime {
    let now = Local::now().naive_local();
    now.with_nanosecond(0).unwrap_or(now)
}

/// A local moment in UTC (the earlier of two at a clock change; as it is in a gap).
#[must_use]
pub fn local_to_utc(at: NaiveDateTime) -> NaiveDateTime {
    Local
        .from_local_datetime(&at)
        .earliest()
        .map_or(at, |local| local.naive_utc())
}

/// A UTC moment in local time.
#[must_use]
pub fn utc_to_local(at: NaiveDateTime) -> NaiveDateTime {
    Local.from_utc_datetime(&at).naive_local()
}

/// A new id: a random version-4 UUID (lower case), azul's `Uuid::from_seed` of a random
/// seed (as AzCalendar mints its event ids).
#[must_use]
pub fn new_id() -> String {
    azul::uuid::Uuid::from_seed(azul_storage::ids::random_seed())
        .as_str()
        .to_string()
}

impl Tasks {
    /// An empty state over `drive`, before the files are read.
    /// Keeps the appearance for the next start: its file goes to the write queue.
    pub fn save_appearance(&mut self) {
        self.queue.put(
            crate::appearance::settings_key(),
            self.appearance.to_json().into_bytes(),
        );
    }

    pub fn new(drive: Arc<dyn Drive>, root: PathBuf, view: View) -> Tasks {
        let clock = now();
        Tasks {
            lists: Vec::new(),
            tasks: Vec::new(),
            settings: Settings::default(),
            loaded: false,
            load_error: String::new(),
            skipped: Vec::new(),
            drive,
            root,
            queue: WriteQueue::new(),
            files: FileWork::default(),
            appearance: azul_appkit::settings::AppSettings::default(),
            import_path: String::new(),
            io_message: String::new(),
            sample_requested: false,
            view,
            selection: ListSelection::create(),
            search: String::new(),
            quick: QuickAdd::default(),
            drafts: Drafts::default(),
            nav_open: [true, true, true],
            nav_collapsed: false,
            folded_groups: BTreeSet::new(),
            completed_open: false,
            planned_month: false,
            month: clock.date(),
            board: false,
            ribbon_tab: 0,
            page: None,
            settings_category: 0,
            settings_search: String::new(),
            palette: None,
            show_todo_bar: true,
            todo_day: clock.date(),
            editing_list: None,
            confirm: None,
            banners: Vec::new(),
            os_notifications: (false, String::new()),
            notice: String::new(),
            undo: None,
            drag: None,
            clock,
        }
    }

    // ==== Lookups ====

    #[must_use]
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.tasks.iter().position(|t| t.id == id)
    }

    #[must_use]
    pub fn list_index(&self, id: &str) -> Option<usize> {
        self.lists.iter().position(|l| l.id == id)
    }

    /// The name of list `id` ("" when it is not there).
    #[must_use]
    pub fn list_name(&self, id: &str) -> String {
        self.list_index(id)
            .map(|i| self.lists[i].name.clone())
            .unwrap_or_default()
    }

    /// The selected task, when exactly one is selected.
    #[must_use]
    pub fn selected_one(&self) -> Option<usize> {
        if self.selection.len() != 1 {
            return None;
        }
        self.selected().first().copied()
    }

    /// The selected tasks' indices.
    #[must_use]
    pub fn selected(&self) -> Vec<usize> {
        (0..self.tasks.len())
            .filter(|&i| self.is_selected(&self.tasks[i].id))
            .collect()
    }

    /// Whether task `id` is selected.
    #[must_use]
    pub fn is_selected(&self, id: &str) -> bool {
        self.selection.contains(ListSelection::key_of(id))
    }

    /// The selected tasks' ids.
    #[must_use]
    pub fn selected_ids(&self) -> Vec<String> {
        self.selected()
            .into_iter()
            .map(|i| self.tasks[i].id.clone())
            .collect()
    }

    /// The selection keys of the tasks on screen, top to bottom.
    fn shown_keys(&self) -> Vec<u64> {
        self.visible_order(now())
            .into_iter()
            .map(|i| ListSelection::key_of(self.tasks[i].id.as_str()))
            .collect()
    }

    /// What the task list shows now.
    #[must_use]
    pub fn sections(&self, now: NaiveDateTime) -> Vec<Section> {
        views::sections(
            &self.view,
            &self.tasks,
            &self.lists,
            now,
            self.settings.sort,
            self.settings.show_completed,
        )
    }

    /// The task list's rows top to bottom (a folded "Completed" section left out): what the
    /// arrow keys walk.
    #[must_use]
    pub fn visible_order(&self, now: NaiveDateTime) -> Vec<usize> {
        self.sections(now)
            .iter()
            .filter(|s| s.kind != views::SectionKind::Completed || self.completed_open)
            .flat_map(|s| s.tasks.iter().copied())
            .collect()
    }

    /// The list new tasks go to outside a list: the settings' default, else the first - the
    /// task store's rule, which every To-Do bar follows too.
    #[must_use]
    pub fn default_list(&self) -> Option<String> {
        model::default_list(&self.lists, &self.settings)
    }

    /// The parse context of the quick-add line now.
    pub fn parse_quick(&self, text: &str, now: NaiveDateTime) -> Parsed {
        let lists: Vec<(String, String)> = self
            .lists
            .iter()
            .map(|l| (l.id.clone(), l.name.clone()))
            .collect();
        let ctx = parse::Context {
            now,
            week_start: self.settings.week_start,
            lists: &lists,
        };
        parse::parse_with(text, &ctx, &self.quick.ignore)
    }

    // ==== Import / export (iCalendar VTODO, `vtodo.rs`) ====

    /// Exports the list shown (outside a list: every task) as an iCalendar file into AzTasks'
    /// folder of the data tree (`aztasks/exports/<list>.ics`), through the write queue; `now`
    /// is local, `to_utc` turns a local moment into UTC. Returns the file's key and how many
    /// to-dos it holds.
    pub fn export_tasks(
        &mut self,
        now: NaiveDateTime,
        to_utc: &dyn Fn(NaiveDateTime) -> NaiveDateTime,
    ) -> (String, usize) {
        let list = match &self.view {
            View::List(id) => self.lists.iter().find(|l| &l.id == id).cloned(),
            _ => None,
        };
        let name = list.as_ref().map_or("Tasks", |l| l.name.as_str());
        let tasks: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|t| list.as_ref().map_or(true, |l| t.list == l.id))
            .collect();
        let count = tasks.len();
        let text = crate::vtodo::write(&tasks, name, to_utc(now), to_utc);
        let key = azul_appkit::data::app_key(
            crate::appearance::APP_FOLDER,
            &format!("exports/{}", crate::vtodo::file_name_for(name)),
        );
        self.queue.put(key.clone(), text.into_bytes());
        (key, count)
    }

    /// Imports the to-dos of the iCalendar `text` into the default list, after its tasks, each
    /// queued; `to_local` turns a UTC moment into local time. Returns what could not be read.
    pub fn import_tasks(
        &mut self,
        text: &str,
        now: NaiveDateTime,
        to_local: &dyn Fn(NaiveDateTime) -> NaiveDateTime,
    ) -> Vec<String> {
        let Some(list) = self.default_list() else {
            return vec![String::from(
                "There is no list to import the to-dos into: make a list first.",
            )];
        };
        let mut ids = new_id;
        let imported = crate::vtodo::read(text, &list, now, &mut ids, to_local);
        for mut t in imported.tasks {
            t.order = model::next_order(&self.tasks, &list);
            self.tasks.push(t);
            self.save_task(self.tasks.len() - 1);
        }
        imported.problems
    }

    // ==== Saving ====

    /// Queues task `i`'s file (stamping it modified).
    pub fn save_task(&mut self, i: usize) {
        let Some(t) = self.tasks.get_mut(i) else {
            return;
        };
        t.modified = now();
        let key = t.key();
        let json = model::task_to_json(t);
        self.queue.put(key, json.into_bytes());
    }

    /// Queues list `i`'s file.
    pub fn save_list(&mut self, i: usize) {
        let Some(l) = self.lists.get(i) else {
            return;
        };
        self.queue.put(l.key(), model::list_to_json(l).into_bytes());
    }

    pub fn save_settings(&mut self) {
        self.queue.put(
            model::SETTINGS_KEY.to_string(),
            model::settings_to_json(&self.settings).into_bytes(),
        );
    }

    // ==== Views and selection ====

    /// Shows `view`, keeping the selection only where it is still on screen.
    pub fn show(&mut self, view: View) {
        self.commit_drafts();
        self.view = view;
        self.editing_list = None;
        self.page = None;
        let shown = self.shown_keys();
        self.selection.retain_in(shown);
        println!("AZTASKS_VIEW {}", self.view.name());
    }

    /// Selects task `id`: alone, toggled into the selection (`ctrl`), or the range from the
    /// anchor (`shift`) in the shown order.
    pub fn select(&mut self, id: &str, shift: bool, ctrl: bool) {
        self.commit_drafts();
        self.editing_list = None;
        let shown = self.shown_keys();
        self.selection
            .select_in(shown, ListSelection::key_of(id), shift, ctrl);
        if !shift {
            println!("AZTASKS_SELECTED {id}");
        }
    }

    /// The tasks a drop lands: the dragged one, or the whole selection when it is part of it;
    /// the drag is over.
    pub fn take_dropped(&mut self) -> Vec<String> {
        let Some(dragged) = self.drag.take() else {
            return Vec::new();
        };
        if self.is_selected(&dragged) {
            self.selected_ids()
        } else {
            vec![dragged]
        }
    }

    /// Moves the selection one row up or down (`extend`: Shift held).
    pub fn step_selection(&mut self, down: bool, extend: bool) {
        self.commit_drafts();
        self.editing_list = None;
        let shown = self.shown_keys();
        let delta = if down { 1 } else { -1 };
        let Some(key) = self.selection.step_in(shown, delta, extend, false).into_option() else {
            return;
        };
        if !extend {
            if let Some(t) = self.tasks.iter().find(|t| ListSelection::key_of(t.id.as_str()) == key) {
                println!("AZTASKS_SELECTED {}", t.id);
            }
        }
    }

    // ==== Drafts ====

    /// Writes the name and group typed in the list settings into their list.
    pub fn commit_list_drafts(&mut self) {
        let Some(li) = self.list_index(&self.drafts.list.clone()) else {
            return;
        };
        let name = self.drafts.list_name.trim().to_string();
        let group = self.drafts.list_group.trim().to_string();
        let mut changed = false;
        if !name.is_empty() && self.lists[li].name != name {
            self.lists[li].name = name;
            changed = true;
        }
        if self.lists[li].group != group {
            self.lists[li].group = group;
            changed = true;
        }
        if changed {
            self.save_list(li);
        }
    }

    /// Writes the title and notes typed in the detail pane into their task (and the list
    /// settings' name and group into their list).
    pub fn commit_drafts(&mut self) {
        self.commit_list_drafts();
        let Some(i) = self.index_of(&self.drafts.task.clone()) else {
            return;
        };
        let title = self.drafts.title.trim().to_string();
        let notes = self.drafts.notes.clone();
        let mut changed = false;
        if !title.is_empty() && self.tasks[i].title != title {
            self.tasks[i].title = title;
            changed = true;
        }
        if self.tasks[i].notes != notes {
            self.tasks[i].notes = notes;
            changed = true;
        }
        if changed {
            self.save_task(i);
        }
    }

    /// Points the title / notes drafts at the selected task (when it changed).
    pub fn sync_drafts(&mut self) {
        let Some(i) = self.selected_one() else {
            self.drafts.task.clear();
            return;
        };
        if self.drafts.task != self.tasks[i].id {
            self.drafts.task = self.tasks[i].id.clone();
            self.drafts.title = self.tasks[i].title.clone();
            self.drafts.notes = self.tasks[i].notes.clone();
            self.drafts.step.clear();
            self.drafts.tag.clear();
            self.drafts.custom_repeat = false;
        }
    }

    // ==== Tasks ====

    /// A list to put a task in: the one named, else the view's, else the default; a first
    /// list "Tasks" is made when there is none.
    pub fn target_list(&mut self, named: Option<String>) -> String {
        if let Some(id) = named.filter(|id| self.list_index(id).is_some()) {
            return id;
        }
        if let View::List(id) = &self.view {
            if self.list_index(id).is_some() {
                return id.clone();
            }
        }
        match self.default_list() {
            Some(id) => id,
            None => self.new_list("Tasks", ""),
        }
    }

    /// Adds the task a quick-add line describes, in the shown view's sense (Today: due
    /// today; Flagged: flagged; a tag: tagged), selects it and returns its index.
    pub fn add_parsed(&mut self, p: Parsed, now: NaiveDateTime) -> usize {
        let list = self.target_list(p.list.clone());
        let mut t = Task::new(new_id(), list.clone(), p.title.clone(), now);
        t.due = p.due;
        t.due_time = p.due.and(p.time);
        t.repeat = p.repeat.clone();
        t.priority = p.priority.unwrap_or_default();
        t.flagged = p.flagged;
        for tag in &p.tags {
            t.add_tag(tag);
        }
        match &self.view {
            View::Smart(Smart::Today | Smart::Upcoming | Smart::Scheduled) if t.due.is_none() => {
                t.due = Some(now.date());
            }
            View::Smart(Smart::Flagged) => t.flagged = true,
            View::Tag(tag) => {
                let tag = tag.clone();
                t.add_tag(&tag);
            }
            _ => {}
        }
        t.order = views::next_order(&self.tasks, &list);
        println!(
            "AZTASKS_ADDED {} {} {}",
            t.id,
            list,
            t.due.map_or_else(|| "-".to_string(), model::format_date)
        );
        let id = t.id.clone();
        self.tasks.push(t);
        let i = self.tasks.len() - 1;
        self.save_task(i);
        self.selection.click(ListSelection::key_of(id.as_str()));
        i
    }

    /// Completes task `i` (or opens it again). A repeating task leaves its next occurrence
    /// behind as a new task, which takes the rule; the completed one keeps none, so opening
    /// it again does not make a second one. Returns the new task's index.
    pub fn toggle_done(&mut self, i: usize, now: NaiveDateTime) -> Option<usize> {
        if i >= self.tasks.len() {
            return None;
        }
        if self.tasks[i].is_done() {
            self.tasks[i].completed = None;
            println!("AZTASKS_REOPENED {}", self.tasks[i].id);
            self.save_task(i);
            return None;
        }
        // The task store's completion (shared with the To-Do bars): a repeating task hands its
        // rule to the next occurrence.
        let next = self.tasks[i].complete(new_id(), now);
        println!("AZTASKS_COMPLETED {}", self.tasks[i].id);
        let spawned = next.map(|t| {
            println!(
                "AZTASKS_SPAWNED {} {}",
                t.id,
                t.due.map_or_else(|| "-".to_string(), model::format_date)
            );
            self.tasks.push(t);
            self.tasks.len() - 1
        });
        self.save_task(i);
        if let Some(n) = spawned {
            self.save_task(n);
        }
        self.banners.retain(|id| *id != self.tasks[i].id);
        spawned
    }

    /// A board's drop: task `i` to `column` at `now` - To do and Doing open it (again) and
    /// mark it not started / started, Done completes it (a repeating task leaves its next
    /// occurrence behind). Returns the index of a spawned next occurrence.
    pub fn move_to_column(&mut self, i: usize, column: views::Column, now: NaiveDateTime) -> Option<usize> {
        if i >= self.tasks.len() || views::Column::of(&self.tasks[i]) == column {
            return None;
        }
        println!("AZTASKS_COLUMN {} {}", self.tasks[i].id, column.key());
        if column == views::Column::Done {
            return self.toggle_done(i, now);
        }
        if self.tasks[i].is_done() {
            self.toggle_done(i, now);
        }
        self.tasks[i].set_started(column == views::Column::Doing, now);
        self.save_task(i);
        None
    }

    /// The planned month's drop: task `i` due on `day` (its time kept; a monthly or yearly
    /// repeat takes the new day of the month; it reminds again).
    pub fn reschedule(&mut self, i: usize, day: NaiveDate) {
        let Some(t) = self.tasks.get_mut(i) else {
            return;
        };
        if t.due == Some(day) {
            return;
        }
        t.due = Some(day);
        t.reminded = None;
        reanchor(t);
        println!("AZTASKS_DUE {} {}", t.id, model::format_date(day));
        self.save_task(i);
    }

    /// Completes the selected tasks (or opens them again when all are completed).
    pub fn toggle_selected(&mut self, now: NaiveDateTime) {
        let picked = self.selected();
        let all_done = !picked.is_empty() && picked.iter().all(|&i| self.tasks[i].is_done());
        for i in picked {
            if self.tasks[i].is_done() == all_done {
                self.toggle_done(i, now);
            }
        }
    }

    /// Deletes the tasks `ids` (kept for "Undo"); returns the attachment folders of the
    /// PREVIOUS undo, which are now gone for good (the caller deletes them).
    pub fn delete_tasks(&mut self, ids: &[String]) -> Vec<String> {
        self.commit_drafts();
        let mut undo = Undo::default();
        for id in ids {
            let Some(i) = self.index_of(id) else {
                continue;
            };
            let t = self.tasks.remove(i);
            self.queue.delete(t.key());
            if !t.attachments.is_empty() {
                undo.prefixes.push(model::attachments_prefix(&t.list, &t.id));
            }
            println!("AZTASKS_DELETED {}", t.id);
            undo.tasks.push(t);
        }
        let gone: Vec<u64> = ids.iter().map(|id| ListSelection::key_of(id.as_str())).collect();
        let kept: Vec<u64> = self
            .selection
            .keys
            .as_ref()
            .iter()
            .copied()
            .filter(|k| !gone.contains(k))
            .collect();
        self.selection.retain_in(kept);
        self.banners.retain(|id| !ids.contains(id));
        let n = undo.tasks.len();
        self.notice = match n {
            0 => String::new(),
            1 => format!("Deleted \"{}\".", undo.tasks[0].title),
            n => format!("Deleted {n} tasks."),
        };
        let gone = self.undo.take().map(|u| u.prefixes).unwrap_or_default();
        if n > 0 {
            self.undo = Some(undo);
        }
        gone
    }

    /// Puts the last deleted tasks back.
    pub fn undo_delete(&mut self) {
        let Some(undo) = self.undo.take() else {
            return;
        };
        for t in undo.tasks {
            let id = t.id.clone();
            self.tasks.push(t);
            let i = self.tasks.len() - 1;
            self.save_task(i);
            self.selection.click(ListSelection::key_of(id.as_str()));
        }
        self.notice.clear();
    }

    /// Moves the tasks `ids` to list `list` (at its end). Returns the attachment folders to
    /// move: `(from, to)`.
    pub fn move_tasks(&mut self, ids: &[String], list: &str) -> Vec<(String, String)> {
        let mut moves = Vec::new();
        if self.list_index(list).is_none() {
            return moves;
        }
        for id in ids {
            let Some(i) = self.index_of(id) else {
                continue;
            };
            if self.tasks[i].list == list {
                continue;
            }
            let old_key = self.tasks[i].key();
            let old_list = self.tasks[i].list.clone();
            self.queue.delete(old_key);
            if !self.tasks[i].attachments.is_empty() {
                moves.push((
                    model::attachments_prefix(&old_list, id),
                    model::attachments_prefix(list, id),
                ));
            }
            self.tasks[i].order = views::next_order(&self.tasks, list);
            self.tasks[i].list = list.to_string();
            println!("AZTASKS_MOVED {id} {list}");
            self.save_task(i);
        }
        moves
    }

    /// Moves task `moving` in front of task `before` (a drop); across lists it moves to
    /// that list first. Returns attachment folders to move.
    pub fn drop_before(&mut self, moving: &str, before: Option<&str>) -> Vec<(String, String)> {
        let Some(m) = self.index_of(moving) else {
            return Vec::new();
        };
        let target_list = before
            .and_then(|b| self.index_of(b))
            .map(|b| self.tasks[b].list.clone());
        let mut moves = Vec::new();
        if let Some(list) = target_list.filter(|l| *l != self.tasks[m].list) {
            moves = self.move_tasks(&[moving.to_string()], &list);
        }
        let (Some(m), b) = (self.index_of(moving), before.and_then(|b| self.index_of(b))) else {
            return moves;
        };
        for i in views::reorder(&mut self.tasks, m, b) {
            self.save_task(i);
        }
        moves
    }

    /// Alt+Up / Alt+Down on the selected task.
    pub fn step_order(&mut self, up: bool) {
        let Some(i) = self.selected_one() else {
            return;
        };
        for j in views::move_step(&mut self.tasks, i, up) {
            self.save_task(j);
        }
    }

    // ==== Lists ====

    /// Makes a list (at the end) and returns its id.
    pub fn new_list(&mut self, name: &str, group: &str) -> String {
        let order = self.lists.iter().map(|l| l.order).max().unwrap_or(0) + 1;
        let mut l = TaskList::new(new_id(), name.to_string(), order);
        l.group = group.trim().to_string();
        let palette = model::ListColor::ALL;
        l.color = palette[self.lists.len() % palette.len()];
        let id = l.id.clone();
        println!("AZTASKS_LIST {} {}", l.id, l.name);
        self.lists.push(l);
        self.save_list(self.lists.len() - 1);
        id
    }

    /// Deletes list `id` and its tasks; returns the folders whose files go with them.
    pub fn delete_list(&mut self, id: &str) -> Vec<String> {
        let Some(li) = self.list_index(id) else {
            return Vec::new();
        };
        let l = self.lists.remove(li);
        self.queue.delete(l.key());
        let ids: Vec<String> = self
            .tasks
            .iter()
            .filter(|t| t.list == id)
            .map(|t| t.id.clone())
            .collect();
        let mut gone = self.delete_tasks(&ids);
        // The list's tasks do not come back with "Undo" (their list is gone).
        if let Some(u) = self.undo.take() {
            gone.extend(u.prefixes);
        }
        gone.push(format!("{}/{}/", model::TASKS_DIR, id));
        self.notice = format!("Deleted the list \"{}\".", l.name);
        if self.view == View::List(id.to_string()) {
            self.view = View::Smart(Smart::Today);
        }
        if self.settings.default_list == id {
            self.settings.default_list.clear();
            self.save_settings();
        }
        gone
    }

    /// Deletes the completed tasks finished more than 30 days ago; returns attachment
    /// folders to delete.
    pub fn clear_completed(&mut self, now: NaiveDateTime) -> Vec<String> {
        let cutoff = now - chrono::Duration::days(30);
        let ids: Vec<String> = self
            .tasks
            .iter()
            .filter(|t| t.completed.is_some_and(|c| c < cutoff))
            .map(|t| t.id.clone())
            .collect();
        let mut gone = self.delete_tasks(&ids);
        if let Some(u) = self.undo.take() {
            gone.extend(u.prefixes);
        }
        self.notice = format!("Cleared {} completed task(s).", ids.len());
        gone
    }

    /// Adds the sample lists and tasks (the empty state's "Add the sample tasks").
    pub fn add_sample(&mut self, now: NaiveDateTime) {
        let mut mint = new_id;
        let (lists, tasks) = crate::sample::sample(now, &mut mint);
        let first_list = self.lists.len();
        let first_task = self.tasks.len();
        let base = self.lists.iter().map(|l| l.order).max().unwrap_or(0);
        for mut l in lists {
            l.order += base;
            self.lists.push(l);
        }
        self.tasks.extend(tasks);
        for i in first_list..self.lists.len() {
            self.save_list(i);
        }
        for i in first_task..self.tasks.len() {
            self.save_task(i);
        }
        self.notice = "Sample lists and tasks were added.".to_string();
    }

    // ==== Loading ====

    /// Takes in what the files said; an empty folder gets a first list ("Tasks"), or the
    /// sample when `--sample` asked for it.
    pub fn take_loaded(&mut self, loaded: store::Loaded, now: NaiveDateTime) {
        self.lists = loaded.lists;
        self.tasks = loaded.tasks;
        if let Some(s) = loaded.settings {
            self.settings = s;
        }
        self.skipped = loaded.skipped;
        // A task file of AzCalendar's old To-Do bar in the shared store: written back in the
        // store's format (DEDUP_EDITORS B12).
        for key in &loaded.migrated {
            if let Some(i) = self.tasks.iter().position(|t| t.key() == *key) {
                self.save_task(i);
            }
        }
        self.loaded = true;
        if self.lists.is_empty() && self.tasks.is_empty() {
            if self.sample_requested {
                self.add_sample(now);
            } else {
                self.new_list("Tasks", "");
            }
        } else if self.sample_requested {
            self.notice = "The data folder has tasks already; --sample adds nothing.".to_string();
        }
        if let View::List(id) = &self.view {
            if self.list_index(id).is_none() {
                self.view = View::Smart(Smart::Today);
            }
        }
        println!(
            "AZTASKS_LOADED {} {} {}",
            self.lists.len(),
            self.tasks.len(),
            self.skipped.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use azul_pim::testing::TempDir;
    use azul_storage::LocalDrive;

    use super::*;

    #[test]
    fn a_task_azcalendars_old_to_do_bar_wrote_is_written_back_in_the_shared_format() {
        // DEDUP_EDITORS B12: AzTasks and AzCalendar share one store; a file of the old
        // `azcalendar.task` format in it is read and rewritten as an AzTasks task.
        let dir = TempDir::create();
        let drive = LocalDrive::new(&dir.0);
        let id = "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a";
        let key = model::task_key("default", id);
        let old = format!(
            "{{\"format\": \"azcalendar.task\", \"version\": 1, \"id\": \"{id}\", \
             \"title\": \"Book the room\", \"done\": false}}"
        );
        drive.put(&key, old.as_bytes()).unwrap();
        let loaded = store::load_all(&drive).unwrap();
        let mut s = Tasks::new(
            Arc::new(LocalDrive::new(&dir.0)),
            dir.0.clone(),
            View::Smart(Smart::Today),
        );
        s.take_loaded(loaded, now());
        assert_eq!(s.tasks.len(), 1);
        assert_eq!(s.tasks[0].title, "Book the room");
        let batch = s.queue.take().expect("the old file is queued for a rewrite");
        assert!(batch.iter().any(|w| w.key() == key), "{batch:?}");
    }

    fn state_in(dir: &TempDir) -> Tasks {
        Tasks::new(
            Arc::new(LocalDrive::new(&dir.0)),
            dir.0.clone(),
            View::Smart(Smart::Today),
        )
    }

    /// The Data settings' export: the list shown (else every task) as an iCalendar file in
    /// AzTasks' own folder of the data tree, written through the queue.
    #[test]
    fn an_export_writes_the_list_shown_into_the_data_tree() {
        let dir = TempDir::create();
        let mut s = state_in(&dir);
        let at = now();
        s.lists = vec![
            TaskList::new("work".into(), "Work".into(), 1),
            TaskList::new("home".into(), "Home".into(), 2),
        ];
        for (id, list, title) in [("a", "work", "Report"), ("b", "home", "Ferns"), ("c", "work", "Mail")] {
            s.tasks.push(Task::new(id.into(), list.into(), title.into(), at));
        }
        s.view = View::List("work".into());
        let (key, count) = s.export_tasks(at, &|d| d);
        assert_eq!((key.as_str(), count), ("aztasks/exports/Work.ics", 2));
        let batch = s.queue.take().expect("the export is queued");
        let text = match batch.iter().find(|w| w.key() == key) {
            Some(store::Write::Put { bytes, .. }) => String::from_utf8_lossy(bytes).into_owned(),
            other => panic!("no put of {key}: {other:?}"),
        };
        assert!(text.contains("SUMMARY:Report") && text.contains("SUMMARY:Mail"), "{text}");
        assert!(!text.contains("SUMMARY:Ferns"), "only the list shown: {text}");
        s.queue.finish(Vec::new());
        // Outside a list: every task, the file named after the app.
        s.view = View::Smart(Smart::Today);
        let (key, count) = s.export_tasks(at, &|d| d);
        assert_eq!((key.as_str(), count), ("aztasks/exports/Tasks.ics", 3));
    }

    /// A board's drop moves a task between To do, Doing and Done: the file keeps it.
    #[test]
    fn a_board_drop_starts_completes_and_reopens_a_task() {
        let dir = TempDir::create();
        let mut s = state_in(&dir);
        let at = now();
        s.lists = vec![TaskList::new("work".into(), "Work".into(), 1)];
        s.tasks.push(Task::new("a".into(), "work".into(), "Report".into(), at));
        let column = |s: &Tasks| views::Column::of(&s.tasks[0]);
        s.move_to_column(0, views::Column::Doing, at);
        assert_eq!(column(&s), views::Column::Doing);
        assert_eq!(s.tasks[0].started, Some(at));
        assert!(s.queue.take().is_some(), "the start is saved");
        s.queue.finish(Vec::new());
        s.move_to_column(0, views::Column::Done, at);
        assert_eq!(column(&s), views::Column::Done);
        s.move_to_column(0, views::Column::ToDo, at);
        assert_eq!(column(&s), views::Column::ToDo, "opened again, not started");
        assert_eq!(s.tasks[0].started, None);
        s.move_to_column(0, views::Column::Doing, at);
        s.move_to_column(0, views::Column::Done, at);
        s.move_to_column(0, views::Column::Doing, at);
        assert_eq!(column(&s), views::Column::Doing, "opened again, started");
    }

    /// The planned month's drop moves the due day, keeps the time, and reminds again.
    #[test]
    fn a_drop_on_a_day_moves_the_due_day_and_keeps_the_time() {
        let dir = TempDir::create();
        let mut s = state_in(&dir);
        let at = now();
        let mut t = Task::new("a".into(), "work".into(), "Rent".into(), at);
        t.due = NaiveDate::from_ymd_opt(2026, 10, 2);
        t.due_time = chrono::NaiveTime::from_hms_opt(9, 0, 0);
        t.repeat = Some(crate::recur::Repeat::monthly().on_month_day(2));
        t.reminded = Some(at);
        s.tasks.push(t);
        let day = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        s.reschedule(0, day);
        assert_eq!(s.tasks[0].due, Some(day));
        assert_eq!(s.tasks[0].due_time, chrono::NaiveTime::from_hms_opt(9, 0, 0));
        assert_eq!(s.tasks[0].reminded, None, "it reminds again");
        assert_eq!(s.tasks[0].repeat.as_ref().and_then(|r| r.month_day), Some(5), "monthly on the 5th now");
        assert!(s.queue.take().is_some(), "the new day is saved");
    }

    /// The imported to-dos join the default list after its tasks, and each is queued.
    #[test]
    fn imported_to_dos_join_the_default_list_after_its_tasks() {
        let dir = TempDir::create();
        let mut s = state_in(&dir);
        let at = now();
        s.lists = vec![TaskList::new("work".into(), "Work".into(), 1)];
        let mut old = Task::new("a".into(), "work".into(), "Report".into(), at);
        old.order = 4;
        s.tasks.push(old);
        let text = "BEGIN:VCALENDAR\r\nBEGIN:VTODO\r\nSUMMARY:One\r\nEND:VTODO\r\n\
                    BEGIN:VTODO\r\nSUMMARY:Two\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
        let problems = s.import_tasks(text, at, &|d| d);
        assert!(problems.is_empty(), "{problems:?}");
        let added: Vec<(&str, &str, i64)> = s
            .tasks
            .iter()
            .filter(|t| t.id != "a")
            .map(|t| (t.title.as_str(), t.list.as_str(), t.order))
            .collect();
        let step = model::ORDER_STEP;
        assert_eq!(added, [("One", "work", 4 + step), ("Two", "work", 4 + 2 * step)]);
        let batch = s.queue.take().expect("the new tasks are queued");
        assert_eq!(batch.len(), 2);
        // No list yet: nothing to import into.
        let mut empty = state_in(&dir);
        let problems = empty.import_tasks(text, at, &|d| d);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(empty.tasks.is_empty());
    }
}
