//! What the task list shows: the smart lists (Today, Upcoming, Scheduled, Flagged, All,
//! Completed), a list, a tag or a search, as sections of tasks; their counts; the sort
//! orders; the manual order (reorder by drag or by Alt+Up / Alt+Down); the navigation pane's
//! structure (lists in their groups, tags).
//!
//! The rules (the plan's `SmartListRules`):
//!
//! - **Today**: open tasks due today or earlier; "Overdue" over "Today".
//! - **Upcoming**: open tasks due in the next seven days (today included), one section a day.
//! - **Scheduled**: open tasks with a due date: "Overdue", a section a day for the next
//!   seven days, then one a month.
//! - **Flagged**: open flagged tasks, a section per list.
//! - **All**: open tasks, a section per list.
//! - **Completed**: completed tasks, a section per completion day, newest first.
//! - A list: its open tasks, then (when shown) "Completed (n)", which the app folds.
//! - A tag: open tasks with the tag, a section per list. A search: every task whose title,
//!   notes, tags or steps hold the words, a section per list, completed ones last.
//!
//! Sections hold indices into the task slice they were made from.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};

use azul_appkit::l10n::{t, t_args, Arg};

use crate::model::{self, SortMode, Task, TaskList};

/// The manual order's step, a new task's order, and the lists in the navigation's order (groups
/// unfolded: what Cmd+7 .. Cmd+9 pick after the six smart lists) are the task store's, shared
/// with every To-Do bar (DEDUP_EDITORS B12).
pub use azul_pim::task::{
    lists_in_nav_order, nav_entries, next_order, ordered_lists, NavEntry, ORDER_STEP,
};

/// A smart list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Smart {
    Today,
    Upcoming,
    Scheduled,
    Flagged,
    All,
    Completed,
}

impl Smart {
    /// In the navigation pane's order (Cmd+1 .. Cmd+6).
    pub const ALL: [Smart; 6] = [
        Smart::Today,
        Smart::Upcoming,
        Smart::Scheduled,
        Smart::Flagged,
        Smart::All,
        Smart::Completed,
    ];

    /// The view's name in the window's language.
    #[must_use]
    pub fn label(self) -> String {
        t(match self {
            Smart::Today => "aztasks-smart-today",
            Smart::Upcoming => "aztasks-smart-upcoming",
            Smart::Scheduled => "aztasks-smart-scheduled",
            Smart::Flagged => "aztasks-smart-flagged",
            Smart::All => "aztasks-smart-all",
            Smart::Completed => "aztasks-smart-completed",
        })
    }

    /// A Material icon name.
    #[must_use]
    pub fn icon(self) -> &'static str {
        match self {
            Smart::Today => "today",
            Smart::Upcoming => "date_range",
            Smart::Scheduled => "event",
            Smart::Flagged => "flag",
            Smart::All => "inbox",
            Smart::Completed => "task_alt",
        }
    }

    /// `today`, `upcoming`, ... (the command line's `--view`, stdout).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Smart::Today => "today",
            Smart::Upcoming => "upcoming",
            Smart::Scheduled => "scheduled",
            Smart::Flagged => "flagged",
            Smart::All => "all",
            Smart::Completed => "completed",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Smart> {
        Smart::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// What the task list shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum View {
    Smart(Smart),
    /// A list, by id.
    List(String),
    Tag(String),
    Search(String),
}

impl View {
    /// `today`, `list:<id>`, `tag:<tag>`, `search:<words>`.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            View::Smart(s) => s.name().to_string(),
            View::List(id) => format!("list:{id}"),
            View::Tag(tag) => format!("tag:{tag}"),
            View::Search(q) => format!("search:{q}"),
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<View> {
        if let Some(id) = name.strip_prefix("list:") {
            return Some(View::List(id.to_string()));
        }
        if let Some(tag) = name.strip_prefix("tag:") {
            return Some(View::Tag(tag.to_string()));
        }
        if let Some(q) = name.strip_prefix("search:") {
            return Some(View::Search(q.to_string()));
        }
        Smart::from_name(name).map(View::Smart)
    }
}

/// How the app draws a section's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    /// A plain header (none when the title is empty).
    Plain,
    /// The overdue tasks: the header in the warning colour.
    Overdue,
    /// A list's completed tasks: a header that folds.
    Completed,
}

/// A run of tasks under one header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// Stable within a view: `overdue`, `day-2026-10-02`, `month-2026-11`, `list-<id>`,
    /// `open`, `completed`, `done-2026-10-01`.
    pub key: String,
    pub title: String,
    pub kind: SectionKind,
    /// Indices into the task slice.
    pub tasks: Vec<usize>,
}

impl Section {
    fn new(key: String, title: String, kind: SectionKind) -> Self {
        Section {
            key,
            title,
            kind,
            tasks: Vec::new(),
        }
    }
}

/// Whether an open task is late: due before today, or today at a time that has passed.
#[must_use]
pub fn is_overdue(t: &Task, now: NaiveDateTime) -> bool {
    if t.is_done() {
        return false;
    }
    match t.due {
        None => false,
        Some(due) if due < now.date() => true,
        Some(due) if due == now.date() => t.due_time.is_some_and(|time| time < now.time()),
        Some(_) => false,
    }
}

/// Whether `t` belongs to smart list `s` on `today`.
#[must_use]
pub fn in_smart(s: Smart, t: &Task, today: NaiveDate) -> bool {
    if s == Smart::Completed {
        return t.is_done();
    }
    if t.is_done() {
        return false;
    }
    match s {
        Smart::Today => t.due.is_some_and(|d| d <= today),
        Smart::Upcoming => t
            .due
            .is_some_and(|d| d >= today && d < today + Duration::days(7)),
        Smart::Scheduled => t.due.is_some(),
        Smart::Flagged => t.flagged,
        Smart::All => true,
        Smart::Completed => false,
    }
}

/// The open tasks of smart list `s` (all completed ones for Completed).
#[must_use]
pub fn smart_count(s: Smart, tasks: &[Task], today: NaiveDate) -> usize {
    tasks.iter().filter(|t| in_smart(s, t, today)).count()
}

/// The open tasks of list `list`.
#[must_use]
pub fn list_count(list: &str, tasks: &[Task]) -> usize {
    tasks.iter().filter(|t| t.list == list && !t.is_done()).count()
}

/// Every tag of the open tasks with how many carry it, by name (any case counts as one, the
/// first spelling seen names it).
#[must_use]
pub fn tags(tasks: &[Task]) -> Vec<(String, usize)> {
    let mut seen: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for t in tasks.iter().filter(|t| !t.is_done()) {
        for tag in &t.tags {
            let entry = seen
                .entry(tag.to_lowercase())
                .or_insert_with(|| (tag.clone(), 0));
            entry.1 += 1;
        }
    }
    seen.into_values().collect()
}

/// The tags to offer task `t`: the open tasks' tags it does not carry, the most used first
/// (then by name), at most `max` - the tag field's suggestions.
#[must_use]
pub fn tag_suggestions(tasks: &[Task], t: &Task, max: usize) -> Vec<String> {
    let mut all = tags(tasks);
    all.retain(|(tag, _)| !t.has_tag(tag));
    all.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    all.into_iter().take(max).map(|(tag, _)| tag).collect()
}

/// Whether every word of `query` is in the task's title, notes, tags or steps (any case,
/// diacritics folded).
#[must_use]
pub fn search_matches(t: &Task, query: &str) -> bool {
    let haystack = format!(
        "{} {} {} {}",
        t.title,
        t.notes,
        t.tags.join(" "),
        t.subtasks
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );
    // The PIM apps' search (any case, diacritics folded, a tag's `#` dropped); an empty search
    // shows nothing here - the search page asks for words.
    let query = azul_pim::search::Query::parse(query);
    !query.is_empty() && query.matches(&haystack)
}

/// Orders `idx` by `mode`; ties by the manual order, then creation.
pub fn sort_indices(idx: &mut [usize], tasks: &[Task], mode: SortMode) {
    idx.sort_by(|&a, &b| {
        let (x, y) = (&tasks[a], &tasks[b]);
        let by_mode = match mode {
            SortMode::Manual => std::cmp::Ordering::Equal,
            SortMode::Due => due_key(x).cmp(&due_key(y)),
            SortMode::Priority => y.priority.cmp(&x.priority),
            SortMode::Title => x.title.to_lowercase().cmp(&y.title.to_lowercase()),
            SortMode::Created => x.created.cmp(&y.created),
        };
        by_mode
            .then(x.order.cmp(&y.order))
            .then(x.created.cmp(&y.created))
    });
}

/// Due date and time, the undated last, a timed task before an untimed one of its day.
fn due_key(t: &Task) -> (bool, Option<NaiveDate>, bool, Option<chrono::NaiveTime>) {
    (t.due.is_none(), t.due, t.due_time.is_none(), t.due_time)
}

/// Orders the tasks of one day: by time (untimed last), then priority (high first), then the
/// manual order.
fn sort_day(idx: &mut [usize], tasks: &[Task]) {
    idx.sort_by(|&a, &b| {
        let (x, y) = (&tasks[a], &tasks[b]);
        due_key(x)
            .cmp(&due_key(y))
            .then(y.priority.cmp(&x.priority))
            .then(x.order.cmp(&y.order))
            .then(x.created.cmp(&y.created))
    });
}

/// The sections of `view` at `now`.
#[must_use]
pub fn sections(
    view: &View,
    tasks: &[Task],
    lists: &[TaskList],
    now: NaiveDateTime,
    sort: SortMode,
    show_completed: bool,
) -> Vec<Section> {
    let today = now.date();
    match view {
        View::Smart(Smart::Today) => {
            let mut overdue =
                Section::new("overdue".into(), t("aztasks-overdue"), SectionKind::Overdue);
            let mut due_today =
                Section::new("today".into(), t("kit-date-today"), SectionKind::Plain);
            for (i, t) in tasks.iter().enumerate() {
                if !in_smart(Smart::Today, t, today) {
                    continue;
                }
                if t.due.is_some_and(|d| d < today) {
                    overdue.tasks.push(i);
                } else {
                    due_today.tasks.push(i);
                }
            }
            sort_day(&mut overdue.tasks, tasks);
            sort_day(&mut due_today.tasks, tasks);
            non_empty(vec![overdue, due_today])
        }
        View::Smart(Smart::Upcoming) => {
            let mut days: Vec<Section> = (0..7)
                .map(|n| {
                    let date = today + Duration::days(n);
                    Section::new(
                        format!("day-{}", model::format_date(date)),
                        model::day_heading(date, today),
                        SectionKind::Plain,
                    )
                })
                .collect();
            for (i, t) in tasks.iter().enumerate() {
                if let (true, Some(due)) = (in_smart(Smart::Upcoming, t, today), t.due) {
                    if let Ok(n) = usize::try_from((due - today).num_days()) {
                        if let Some(day) = days.get_mut(n) {
                            day.tasks.push(i);
                        }
                    }
                }
            }
            for day in &mut days {
                sort_day(&mut day.tasks, tasks);
            }
            non_empty(days)
        }
        View::Smart(Smart::Scheduled) => {
            let mut overdue =
                Section::new("overdue".into(), t("aztasks-overdue"), SectionKind::Overdue);
            let mut by_key: BTreeMap<String, Section> = BTreeMap::new();
            for (i, t) in tasks.iter().enumerate() {
                let (true, Some(due)) = (in_smart(Smart::Scheduled, t, today), t.due) else {
                    continue;
                };
                if due < today {
                    overdue.tasks.push(i);
                    continue;
                }
                // Keys sort by date: `day-` dates come before `month-` ones only within
                // the week, so they carry a prefix that sorts by time first.
                let (key, title) = if due < today + Duration::days(7) {
                    (
                        format!("day-{}", model::format_date(due)),
                        model::day_heading(due, today),
                    )
                } else {
                    (
                        format!("month-{:04}-{:02}", due.year(), due.month()),
                        if due.year() == today.year() {
                            t(azul_pim::dates::month_message_id(due.month()))
                        } else {
                            model::said(azul_appkit::l10n::DateStyle::MonthYear, due)
                        },
                    )
                };
                by_key
                    .entry(sort_stamp(due, today) + &key)
                    .or_insert_with(|| Section::new(key, title, SectionKind::Plain))
                    .tasks
                    .push(i);
            }
            sort_day(&mut overdue.tasks, tasks);
            let mut out = vec![overdue];
            for (_, mut s) in by_key {
                sort_day(&mut s.tasks, tasks);
                out.push(s);
            }
            non_empty(out)
        }
        View::Smart(Smart::Flagged) => by_list(tasks, lists, sort, |t| !t.is_done() && t.flagged),
        View::Smart(Smart::All) => by_list(tasks, lists, sort, |t| !t.is_done()),
        View::Smart(Smart::Completed) => {
            let mut by_day: BTreeMap<NaiveDate, Vec<usize>> = BTreeMap::new();
            for (i, t) in tasks.iter().enumerate() {
                if let Some(done) = t.completed {
                    by_day.entry(done.date()).or_default().push(i);
                }
            }
            by_day
                .into_iter()
                .rev()
                .map(|(date, mut idx)| {
                    idx.sort_by(|&a, &b| tasks[b].completed.cmp(&tasks[a].completed));
                    Section {
                        key: format!("done-{}", model::format_date(date)),
                        title: model::day_heading(date, today),
                        kind: SectionKind::Plain,
                        tasks: idx,
                    }
                })
                .collect()
        }
        View::List(id) => {
            let mut open = Section::new("open".into(), String::new(), SectionKind::Plain);
            let mut done = Vec::new();
            for (i, t) in tasks.iter().enumerate() {
                if t.list != *id {
                    continue;
                }
                if t.is_done() {
                    done.push(i);
                } else {
                    open.tasks.push(i);
                }
            }
            sort_indices(&mut open.tasks, tasks, sort);
            let mut out = vec![open];
            if show_completed && !done.is_empty() {
                done.sort_by(|&a, &b| tasks[b].completed.cmp(&tasks[a].completed));
                out.push(Section {
                    key: "completed".into(),
                    title: t_args(
                        "aztasks-completed-count",
                        &[("count", Arg::from(done.len()))],
                    ),
                    kind: SectionKind::Completed,
                    tasks: done,
                });
            }
            out
        }
        View::Tag(tag) => by_list(tasks, lists, sort, |t| !t.is_done() && t.has_tag(tag)),
        View::Search(query) => {
            let mut out = by_list(tasks, lists, sort, |t| !t.is_done() && search_matches(t, query));
            let mut done: Vec<usize> = (0..tasks.len())
                .filter(|&i| tasks[i].is_done() && search_matches(&tasks[i], query))
                .collect();
            if !done.is_empty() {
                done.sort_by(|&a, &b| tasks[b].completed.cmp(&tasks[a].completed));
                out.push(Section {
                    key: "completed".into(),
                    title: t_args(
                        "aztasks-completed-count",
                        &[("count", Arg::from(done.len()))],
                    ),
                    kind: SectionKind::Completed,
                    tasks: done,
                });
            }
            out
        }
    }
}

/// A key prefix that sorts a Scheduled section by its first day.
fn sort_stamp(due: NaiveDate, today: NaiveDate) -> String {
    if due < today + Duration::days(7) {
        format!("{}:", model::format_date(due))
    } else {
        format!("{:04}-{:02}-99:", due.year(), due.month())
    }
}

/// The tasks `keep` picks, a section per list in the lists' order.
fn by_list(tasks: &[Task], lists: &[TaskList], sort: SortMode, keep: impl Fn(&Task) -> bool) -> Vec<Section> {
    let mut out = Vec::new();
    for li in lists_in_nav_order(lists) {
        let list = &lists[li];
        let mut idx: Vec<usize> = (0..tasks.len())
            .filter(|&i| tasks[i].list == list.id && keep(&tasks[i]))
            .collect();
        if idx.is_empty() {
            continue;
        }
        sort_indices(&mut idx, tasks, sort);
        let title = if list.group.is_empty() {
            list.name.clone()
        } else {
            format!("{} \u{203a} {}", list.group, list.name)
        };
        out.push(Section {
            key: format!("list-{}", list.id),
            title,
            kind: SectionKind::Plain,
            tasks: idx,
        });
    }
    // Tasks of a list that is not there (a folder without its list file) still show.
    let known: Vec<&str> = lists.iter().map(|l| l.id.as_str()).collect();
    let mut orphans: Vec<usize> = (0..tasks.len())
        .filter(|&i| !known.contains(&tasks[i].list.as_str()) && keep(&tasks[i]))
        .collect();
    if !orphans.is_empty() {
        sort_indices(&mut orphans, tasks, sort);
        out.push(Section {
            key: "list-".into(),
            title: t("aztasks-other-list"),
            kind: SectionKind::Plain,
            tasks: orphans,
        });
    }
    out
}

fn non_empty(sections: Vec<Section>) -> Vec<Section> {
    sections.into_iter().filter(|s| !s.tasks.is_empty()).collect()
}

/// Every task index of `sections`, top to bottom (the keyboard's order).
#[must_use]
pub fn flat(sections: &[Section]) -> Vec<usize> {
    sections.iter().flat_map(|s| s.tasks.iter().copied()).collect()
}

/// The open tasks of `list` in their manual order.
fn manual_order(tasks: &[Task], list: &str) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..tasks.len())
        .filter(|&i| tasks[i].list == list && !tasks[i].is_done())
        .collect();
    sort_indices(&mut idx, tasks, SortMode::Manual);
    idx
}

/// Gives the tasks in `order` the orders 1024, 2048, ...; returns those that changed.
fn renumber(tasks: &mut [Task], order: &[usize]) -> Vec<usize> {
    let mut changed = Vec::new();
    for (n, &i) in order.iter().enumerate() {
        let want = (i64::try_from(n).unwrap_or(0) + 1) * ORDER_STEP;
        if tasks[i].order != want {
            tasks[i].order = want;
            changed.push(i);
        }
    }
    changed
}

/// Moves task `moving` in front of task `before` (or to the end of its list with `None`),
/// in the manual order of `moving`'s list; `before` must be in the same list. Returns the
/// tasks whose order changed (to be saved).
pub fn reorder(tasks: &mut [Task], moving: usize, before: Option<usize>) -> Vec<usize> {
    let list = tasks[moving].list.clone();
    if before.is_some_and(|b| tasks[b].list != list || b == moving) {
        return Vec::new();
    }
    let mut order = manual_order(tasks, &list);
    order.retain(|&i| i != moving);
    let at = before
        .and_then(|b| order.iter().position(|&i| i == b))
        .unwrap_or(order.len());
    order.insert(at, moving);
    renumber(tasks, &order)
}

/// Moves task `moving` one place up or down in its list's manual order (Alt+Up / Alt+Down).
/// Returns the tasks whose order changed.
pub fn move_step(tasks: &mut [Task], moving: usize, up: bool) -> Vec<usize> {
    let list = tasks[moving].list.clone();
    let mut order = manual_order(tasks, &list);
    let Some(pos) = order.iter().position(|&i| i == moving) else {
        return Vec::new();
    };
    let target = if up {
        match pos.checked_sub(1) {
            Some(t) => t,
            None => return Vec::new(),
        }
    } else if pos + 1 < order.len() {
        pos + 1
    } else {
        return Vec::new();
    };
    order.swap(pos, target);
    renumber(tasks, &order)
}

/// "Fri 2 Oct", "Today 09:00", "Tomorrow" - the row's due chip.
#[must_use]
pub fn due_label(t: &Task, today: NaiveDate) -> Option<String> {
    let due = t.due?;
    let day = model::day_label(due, today);
    Some(match t.due_time {
        Some(time) => format!("{day} {}", model::format_time(time)),
        None => day,
    })
}

/// `(due today, overdue)` among the open tasks, for the status bar.
#[must_use]
pub fn summary(tasks: &[Task], now: NaiveDateTime) -> (usize, usize) {
    let today = now.date();
    let open = tasks.iter().filter(|t| !t.is_done());
    let mut due_today = 0;
    let mut overdue = 0;
    for t in open {
        if is_overdue(t, now) {
            overdue += 1;
        } else if t.due == Some(today) {
            due_today += 1;
        }
    }
    (due_today, overdue)
}

// ==== The planned month and the board ====

/// The open tasks due on each of `days` (a month grid's 42, `azul_pim::dates::month_grid`),
/// each day's by time, priority and the manual order: the planned month's cells.
#[must_use]
pub fn planned_month(tasks: &[Task], days: &[NaiveDate]) -> Vec<Vec<usize>> {
    let mut cells: Vec<Vec<usize>> = vec![Vec::new(); days.len()];
    let (Some(&first), Some(&last)) = (days.first(), days.last()) else {
        return cells;
    };
    for (i, t) in tasks.iter().enumerate() {
        let Some(due) = t.due.filter(|d| !t.is_done() && *d >= first && *d <= last) else {
            continue;
        };
        if let Some(n) = days.iter().position(|d| *d == due) {
            cells[n].push(i);
        }
    }
    for cell in &mut cells {
        sort_month_day(cell, tasks);
    }
    cells
}

/// Orders the tasks of one day of the planned month: the untimed first - a month cell shows
/// its first tasks and "+N more", and an untimed task is the day's own, as an all-day event is
/// in a calendar's month view - then by time, priority (high first) and the manual order.
fn sort_month_day(idx: &mut [usize], tasks: &[Task]) {
    idx.sort_by(|&a, &b| {
        let (x, y) = (&tasks[a], &tasks[b]);
        (x.due_time.is_some(), x.due_time)
            .cmp(&(y.due_time.is_some(), y.due_time))
            .then(y.priority.cmp(&x.priority))
            .then(x.order.cmp(&y.order))
            .then(x.created.cmp(&y.created))
    });
}

/// A board's column: where a task of a list stands (the plan's To do / Doing / Done).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    ToDo,
    Doing,
    Done,
}

impl Column {
    /// Left to right.
    pub const ALL: [Column; 3] = [Column::ToDo, Column::Doing, Column::Done];

    /// The column's name in the window's language.
    #[must_use]
    pub fn label(self) -> String {
        t(match self {
            Column::ToDo => "aztasks-column-to-do",
            Column::Doing => "aztasks-column-doing",
            Column::Done => "aztasks-column-done",
        })
    }

    /// The name in ids and on stdout: `todo`, `doing`, `done`.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Column::ToDo => "todo",
            Column::Doing => "doing",
            Column::Done => "done",
        }
    }

    /// The column task `t` stands in: Done when completed, Doing when started, else To do.
    #[must_use]
    pub fn of(t: &Task) -> Column {
        if t.is_done() {
            Column::Done
        } else if t.started.is_some() {
            Column::Doing
        } else {
            Column::ToDo
        }
    }
}

/// The board of list `list`: its tasks in the columns of [`Column::ALL`] - To do and Doing in
/// the list's manual order, Done the latest completed first.
#[must_use]
pub fn board(tasks: &[Task], list: &str) -> [Vec<usize>; 3] {
    let (doing, todo): (Vec<usize>, Vec<usize>) = manual_order(tasks, list)
        .into_iter()
        .partition(|&i| tasks[i].started.is_some());
    let mut done: Vec<usize> = (0..tasks.len())
        .filter(|&i| tasks[i].list == list && tasks[i].is_done())
        .collect();
    done.sort_by(|&a, &b| tasks[b].completed.cmp(&tasks[a].completed));
    [todo, doing, done]
}

#[cfg(test)]
mod tests {
    use chrono::NaiveTime;

    use super::*;
    use crate::model::Priority;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// Thursday 1 October 2026, 10:00.
    fn now() -> NaiveDateTime {
        day(2026, 10, 1).and_hms_opt(10, 0, 0).unwrap()
    }

    fn list(id: &str, name: &str, order: i64, group: &str) -> TaskList {
        let mut l = TaskList::new(id.into(), name.into(), order);
        l.group = group.into();
        l
    }

    fn lists() -> Vec<TaskList> {
        vec![
            list("work", "Work", 2, ""),
            list("home", "Home", 1, ""),
            list("design", "Design", 3, "Azlin launch"),
            list("web", "Website", 4, "Azlin launch"),
        ]
    }

    fn task(id: &str, list: &str, due: Option<NaiveDate>, order: i64) -> Task {
        let mut t = Task::new(id.into(), list.into(), id.into(), now() - Duration::days(30));
        t.due = due;
        t.order = order;
        t
    }

    fn ids(tasks: &[Task], section: &Section) -> Vec<String> {
        section.tasks.iter().map(|&i| tasks[i].id.clone()).collect()
    }

    fn sample() -> Vec<Task> {
        let today = day(2026, 10, 1);
        let mut late = task("late", "work", Some(day(2026, 9, 28)), 1);
        late.priority = Priority::High;
        let mut nine = task("nine", "home", Some(today), 2);
        nine.due_time = NaiveTime::from_hms_opt(9, 0, 0);
        let noon = {
            let mut t = task("noon", "home", Some(today), 1);
            t.due_time = NaiveTime::from_hms_opt(12, 0, 0);
            t
        };
        let mut flag = task("flag", "design", None, 1);
        flag.flagged = true;
        let mut done = task("done", "work", Some(day(2026, 9, 30)), 2);
        done.completed = Some(day(2026, 9, 30).and_hms_opt(17, 0, 0).unwrap());
        let mut done_today = task("done-today", "home", None, 3);
        done_today.completed = Some(day(2026, 10, 1).and_hms_opt(8, 0, 0).unwrap());
        vec![
            late,
            nine,
            noon,
            task("tomorrow", "work", Some(day(2026, 10, 2)), 3),
            task("sunday", "web", Some(day(2026, 10, 4)), 1),
            task("week-out", "work", Some(day(2026, 10, 8)), 4),
            task("november", "work", Some(day(2026, 11, 12)), 5),
            task("next-year", "home", Some(day(2027, 1, 5)), 4),
            flag,
            task("undated", "work", None, 6),
            done,
            done_today,
        ]
    }

    #[test]
    fn today_holds_the_overdue_and_todays_open_tasks_in_two_sections() {
        let tasks = sample();
        let s = sections(&View::Smart(Smart::Today), &tasks, &lists(), now(), SortMode::Manual, true);
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].key.as_str(), s[0].kind), ("overdue", SectionKind::Overdue));
        assert_eq!(ids(&tasks, &s[0]), vec!["late"]);
        assert_eq!(s[1].title, "Today");
        assert_eq!(ids(&tasks, &s[1]), vec!["nine", "noon"], "by time");
    }

    #[test]
    fn upcoming_groups_the_next_seven_days_by_day() {
        let tasks = sample();
        let s = sections(&View::Smart(Smart::Upcoming), &tasks, &lists(), now(), SortMode::Manual, true);
        let keys: Vec<&str> = s.iter().map(|x| x.key.as_str()).collect();
        assert_eq!(keys, vec!["day-2026-10-01", "day-2026-10-02", "day-2026-10-04"]);
        let titles: Vec<&str> = s.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(titles, vec!["Today", "Tomorrow", "Sunday 4 October"]);
        assert!(!s.iter().any(|x| ids(&tasks, x).contains(&"week-out".to_string())), "8 Oct is a week out");
        assert!(!s.iter().any(|x| ids(&tasks, x).contains(&"late".to_string())), "overdue is Today's");
    }

    #[test]
    fn scheduled_puts_later_tasks_under_their_month() {
        let tasks = sample();
        let s = sections(&View::Smart(Smart::Scheduled), &tasks, &lists(), now(), SortMode::Manual, true);
        let keys: Vec<&str> = s.iter().map(|x| x.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "overdue",
                "day-2026-10-01",
                "day-2026-10-02",
                "day-2026-10-04",
                "month-2026-10",
                "month-2026-11",
                "month-2027-01"
            ]
        );
        assert_eq!(ids(&tasks, &s[4]), vec!["week-out"]);
        assert_eq!(s[5].title, "November");
        assert_eq!(s[6].title, "January 2027");
    }

    #[test]
    fn flagged_and_all_make_a_section_per_list_in_the_lists_order() {
        let tasks = sample();
        let s = sections(&View::Smart(Smart::All), &tasks, &lists(), now(), SortMode::Manual, true);
        let keys: Vec<&str> = s.iter().map(|x| x.key.as_str()).collect();
        assert_eq!(keys, vec!["list-home", "list-work", "list-design", "list-web"]);
        assert_eq!(s[2].title, "Azlin launch \u{203a} Design");
        assert_eq!(ids(&tasks, &s[1]), vec!["late", "tomorrow", "week-out", "november", "undated"]);
        let f = sections(&View::Smart(Smart::Flagged), &tasks, &lists(), now(), SortMode::Manual, true);
        assert_eq!(f.len(), 1);
        assert_eq!(ids(&tasks, &f[0]), vec!["flag"]);
    }

    #[test]
    fn completed_is_grouped_by_completion_day_newest_first() {
        let tasks = sample();
        let s = sections(&View::Smart(Smart::Completed), &tasks, &lists(), now(), SortMode::Manual, true);
        let titles: Vec<&str> = s.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(titles, vec!["Today", "Yesterday"]);
        assert_eq!(ids(&tasks, &s[0]), vec!["done-today"]);
        assert_eq!(ids(&tasks, &s[1]), vec!["done"]);
    }

    #[test]
    fn a_list_shows_its_open_tasks_in_manual_order_then_its_completed_ones() {
        crate::l10n::in_english();
        let tasks = sample();
        let s = sections(&View::List("home".into()), &tasks, &lists(), now(), SortMode::Manual, true);
        assert_eq!(s.len(), 2);
        assert_eq!(ids(&tasks, &s[0]), vec!["noon", "nine", "next-year"]);
        assert_eq!((s[1].title.as_str(), s[1].kind), ("Completed (1)", SectionKind::Completed));
        let hidden = sections(&View::List("home".into()), &tasks, &lists(), now(), SortMode::Manual, false);
        assert_eq!(hidden.len(), 1, "completed tasks hidden by the setting");
        let by_due = sections(&View::List("home".into()), &tasks, &lists(), now(), SortMode::Due, false);
        assert_eq!(ids(&tasks, &by_due[0]), vec!["nine", "noon", "next-year"]);
    }

    #[test]
    fn counts_match_what_the_smart_lists_show() {
        let tasks = sample();
        let today = now().date();
        for s in Smart::ALL {
            let shown = flat(&sections(&View::Smart(s), &tasks, &lists(), now(), SortMode::Manual, true)).len();
            assert_eq!(smart_count(s, &tasks, today), shown, "{s:?}");
        }
        assert_eq!(smart_count(Smart::Today, &tasks, today), 3);
        assert_eq!(list_count("work", &tasks), 5);
    }

    #[test]
    fn a_timed_task_is_overdue_once_its_time_has_passed() {
        let tasks = sample();
        let nine = tasks.iter().find(|t| t.id == "nine").unwrap();
        let noon = tasks.iter().find(|t| t.id == "noon").unwrap();
        assert!(is_overdue(nine, now()), "09:00 at 10:00");
        assert!(!is_overdue(noon, now()));
        assert!(is_overdue(&tasks[0], now()));
        assert_eq!(summary(&tasks, now()), (1, 2), "noon due today; late and nine overdue");
    }

    #[test]
    fn sort_modes_order_by_due_priority_and_title() {
        let mut tasks = sample();
        tasks[3].priority = Priority::Medium;
        let mut idx: Vec<usize> = (0..10).collect();
        sort_indices(&mut idx, &tasks, SortMode::Priority);
        assert_eq!(tasks[idx[0]].id, "late");
        assert_eq!(tasks[idx[1]].id, "tomorrow");
        sort_indices(&mut idx, &tasks, SortMode::Title);
        assert_eq!(tasks[idx[0]].id, "flag");
        sort_indices(&mut idx, &tasks, SortMode::Due);
        assert_eq!(tasks[idx[0]].id, "late");
        assert_eq!(tasks[*idx.last().unwrap()].id, "undated", "undated last");
    }

    #[test]
    fn reorder_moves_a_task_before_another_and_renumbers_its_list() {
        let mut tasks = sample();
        let pos = |tasks: &[Task], id: &str| tasks.iter().position(|t| t.id == id).unwrap();
        let undated = pos(&tasks, "undated");
        let late = pos(&tasks, "late");
        let changed = reorder(&mut tasks, undated, Some(late));
        assert!(!changed.is_empty());
        let s = sections(&View::List("work".into()), &tasks, &lists(), now(), SortMode::Manual, false);
        assert_eq!(ids(&tasks, &s[0]), vec!["undated", "late", "tomorrow", "week-out", "november"]);
        let to_end = reorder(&mut tasks, undated, None);
        assert!(!to_end.is_empty());
        let s = sections(&View::List("work".into()), &tasks, &lists(), now(), SortMode::Manual, false);
        assert_eq!(ids(&tasks, &s[0]).last().map(String::as_str), Some("undated"));
        let other_list = pos(&tasks, "nine");
        assert!(reorder(&mut tasks, undated, Some(other_list)).is_empty(), "not across lists");
    }

    #[test]
    fn move_step_swaps_a_task_with_its_neighbour() {
        let mut tasks = sample();
        let tomorrow = tasks.iter().position(|t| t.id == "tomorrow").unwrap();
        assert!(!move_step(&mut tasks, tomorrow, true).is_empty());
        let s = sections(&View::List("work".into()), &tasks, &lists(), now(), SortMode::Manual, false);
        assert_eq!(ids(&tasks, &s[0])[..2], ["tomorrow".to_string(), "late".to_string()]);
        assert!(move_step(&mut tasks, tomorrow, true).is_empty(), "already first");
        let undated = tasks.iter().position(|t| t.id == "undated").unwrap();
        assert!(move_step(&mut tasks, undated, false).is_empty(), "already last");
    }

    #[test]
    fn tags_are_counted_once_per_spelling_and_search_reads_every_field() {
        let mut tasks = sample();
        tasks[0].tags = vec!["Home".into()];
        tasks[1].tags = vec!["home".into(), "errand".into()];
        tasks[10].tags = vec!["home".into()];
        assert_eq!(tags(&tasks), vec![("errand".to_string(), 1), ("Home".to_string(), 2)]);
        tasks[2].notes = "Ask about the Venue".into();
        assert!(search_matches(&tasks[2], "venue"));
        assert!(search_matches(&tasks[1], "#errand nine"));
        assert!(!search_matches(&tasks[1], "venue"));
        assert!(!search_matches(&tasks[1], "   "), "no words, no match");
        let s = sections(&View::Search("home".into()), &tasks, &lists(), now(), SortMode::Manual, true);
        assert_eq!(s.last().map(|x| x.kind), Some(SectionKind::Completed));
    }

    #[test]
    fn search_ignores_diacritics_and_an_empty_search_still_shows_nothing() {
        // DEDUP_EDITORS B16: the address book finds "Krüger" for "kruger"; AzTasks did not.
        let mut tasks = sample();
        tasks[2].title = "Call Jürgen about the café".into();
        assert!(search_matches(&tasks[2], "jurgen cafe"));
        assert!(search_matches(&tasks[2], "CAFÉ"));
        assert!(!search_matches(&tasks[2], "jurgen tea"));
        assert!(!search_matches(&tasks[2], ""), "no words, no match");
        assert!(!search_matches(&tasks[2], " # "), "a lone hash is no word");
    }

    #[test]
    fn grouped_lists_sit_under_their_group_where_its_first_list_is() {
        let ls = lists();
        assert_eq!(
            nav_entries(&ls),
            vec![
                NavEntry::List(1),
                NavEntry::List(0),
                NavEntry::Group {
                    name: "Azlin launch".into(),
                    lists: vec![2, 3]
                }
            ]
        );
        assert_eq!(lists_in_nav_order(&ls), vec![1, 0, 2, 3]);
    }

    #[test]
    fn views_have_names_for_the_command_line() {
        for v in [
            View::Smart(Smart::Upcoming),
            View::List("abc".into()),
            View::Tag("home".into()),
            View::Search("rent".into()),
        ] {
            assert_eq!(View::from_name(&v.name()), Some(v));
        }
        assert_eq!(View::from_name("nonsense"), None);
    }

    #[test]
    fn the_due_chip_names_the_day_and_the_time() {
        let tasks = sample();
        let today = now().date();
        assert_eq!(due_label(&tasks[1], today).as_deref(), Some("Today 09:00"));
        assert_eq!(due_label(&tasks[3], today).as_deref(), Some("Tomorrow"));
        assert_eq!(due_label(&tasks[4], today).as_deref(), Some("Sun 4 Oct"));
        assert_eq!(due_label(&tasks[9], today), None);
        assert_eq!(next_order(&tasks, "work"), 6 + ORDER_STEP);
    }

    #[test]
    fn tag_suggestions_are_the_other_tags_most_used_first() {
        let mut tasks = sample();
        tasks[0].add_tag("work");
        tasks[1].add_tag("home");
        tasks[2].add_tag("home");
        tasks[3].add_tag("errand");
        tasks[3].add_tag("Home");
        let t = tasks[0].clone();
        assert_eq!(tag_suggestions(&tasks, &t, 5), vec!["home", "errand"], "not its own work");
        assert_eq!(tag_suggestions(&tasks, &t, 1), vec!["home"]);
        let all = tasks[3].clone();
        assert_eq!(tag_suggestions(&tasks, &all, 5), vec!["work"], "any case is its own");
    }

    #[test]
    fn the_planned_month_puts_each_open_task_on_its_due_day() {
        let tasks = sample();
        let days = azul_pim::dates::month_grid(day(2026, 10, 1), chrono::Weekday::Mon);
        let cells = planned_month(&tasks, &days);
        assert_eq!(cells.len(), 42);
        let on = |d: NaiveDate| -> Vec<String> {
            let n = days.iter().position(|x| *x == d).unwrap();
            cells[n].iter().map(|&i| tasks[i].id.clone()).collect()
        };
        assert_eq!(on(day(2026, 9, 28)), vec!["late"]);
        assert_eq!(on(day(2026, 10, 1)), vec!["nine", "noon"], "a day's by time");
        assert_eq!(on(day(2026, 10, 2)), vec!["tomorrow"]);
        assert_eq!(on(day(2026, 10, 8)), vec!["week-out"]);
        assert!(on(day(2026, 9, 30)).is_empty(), "a completed task is not planned");
        let shown: usize = cells.iter().map(Vec::len).sum();
        assert_eq!(shown, 6, "November's 12th and next year are outside the grid");
    }

    #[test]
    fn a_day_of_the_planned_month_lists_its_all_day_tasks_before_its_timed_ones() {
        // A month cell shows its first tasks and "+N more": an untimed task is the day's own,
        // as an all-day event is in Outlook's, Google's and Apple's month views - first, above
        // the day's times, not cut off behind them.
        let mut tasks = sample();
        tasks.push(task("all-day", "home", tasks[1].due, 0));
        let days = azul_pim::dates::month_grid(day(2026, 10, 1), chrono::Weekday::Mon);
        let cells = planned_month(&tasks, &days);
        let n = days.iter().position(|x| *x == day(2026, 10, 1)).unwrap();
        let ids: Vec<&str> = cells[n].iter().map(|&i| tasks[i].id.as_str()).collect();
        assert_eq!(ids, vec!["all-day", "nine", "noon"], "the all-day task, then by time");
    }

    #[test]
    fn the_board_puts_a_lists_tasks_in_to_do_doing_and_done() {
        let mut tasks = sample();
        let started = tasks.iter().position(|t| t.id == "week-out").unwrap();
        tasks[started].set_started(true, now());
        assert_eq!(Column::of(&tasks[started]), Column::Doing);
        let [todo, doing, done] = board(&tasks, "work");
        let names = |c: &[usize]| -> Vec<String> { c.iter().map(|&i| tasks[i].id.clone()).collect() };
        assert_eq!(names(&todo), vec!["late", "tomorrow", "november", "undated"], "the manual order");
        assert_eq!(names(&doing), vec!["week-out"]);
        assert_eq!(names(&done), vec!["done"]);
        assert!(board(&tasks, "nowhere").iter().all(Vec::is_empty));
        assert_eq!(Column::ALL.map(Column::key), ["todo", "doing", "done"]);
    }
}
