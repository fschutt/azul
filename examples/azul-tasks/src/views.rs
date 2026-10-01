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

use crate::model::{self, SortMode, Task, TaskList};

/// The spacing of the manual order: room for a task between two others without renumbering.
pub const ORDER_STEP: i64 = 1024;

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

    #[must_use]
    pub fn label(self) -> &'static str { todo!() }

    /// A Material icon name.
    #[must_use]
    pub fn icon(self) -> &'static str { todo!() }

    /// `today`, `upcoming`, ... (the command line's `--view`, stdout).
    #[must_use]
    pub fn name(self) -> &'static str { todo!() }

    #[must_use]
    pub fn from_name(name: &str) -> Option<Smart> { todo!() }
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
    pub fn name(&self) -> String { todo!() }

    #[must_use]
    pub fn from_name(name: &str) -> Option<View> { todo!() }
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
    fn new(key: String, title: String, kind: SectionKind) -> Self { todo!() }
}

/// Whether an open task is late: due before today, or today at a time that has passed.
#[must_use]
pub fn is_overdue(t: &Task, now: NaiveDateTime) -> bool { todo!() }

/// Whether `t` belongs to smart list `s` on `today`.
#[must_use]
pub fn in_smart(s: Smart, t: &Task, today: NaiveDate) -> bool { todo!() }

/// The open tasks of smart list `s` (all completed ones for Completed).
#[must_use]
pub fn smart_count(s: Smart, tasks: &[Task], today: NaiveDate) -> usize { todo!() }

/// The open tasks of list `list`.
#[must_use]
pub fn list_count(list: &str, tasks: &[Task]) -> usize { todo!() }

/// Every tag of the open tasks with how many carry it, by name (any case counts as one, the
/// first spelling seen names it).
#[must_use]
pub fn tags(tasks: &[Task]) -> Vec<(String, usize)> { todo!() }

/// Whether every word of `query` is in the task's title, notes, tags or steps (any case).
#[must_use]
pub fn search_matches(t: &Task, query: &str) -> bool { todo!() }

/// The lists in the navigation pane's order: by `order`, then name.
#[must_use]
pub fn ordered_lists(lists: &[TaskList]) -> Vec<usize> { todo!() }

/// An entry of the "My lists" tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavEntry {
    /// A list outside any group (index into the lists).
    List(usize),
    /// A group and its lists, in order.
    Group { name: String, lists: Vec<usize> },
}

/// The "My lists" tree: lists and groups in list order, a group where its first list is.
#[must_use]
pub fn nav_entries(lists: &[TaskList]) -> Vec<NavEntry> { todo!() }

/// The lists in the order the navigation pane shows them (groups unfolded): what Cmd+7 ..
/// Cmd+9 pick after the six smart lists.
#[must_use]
pub fn lists_in_nav_order(lists: &[TaskList]) -> Vec<usize> { todo!() }

/// Orders `idx` by `mode`; ties by the manual order, then creation.
pub fn sort_indices(idx: &mut [usize], tasks: &[Task], mode: SortMode) { todo!() }

/// Due date and time, the undated last, a timed task before an untimed one of its day.
fn due_key(t: &Task) -> (bool, Option<NaiveDate>, bool, Option<chrono::NaiveTime>) { todo!() }

/// Orders the tasks of one day: by time (untimed last), then priority (high first), then the
/// manual order.
fn sort_day(idx: &mut [usize], tasks: &[Task]) { todo!() }

/// The sections of `view` at `now`.
#[must_use]
pub fn sections(
    view: &View,
    tasks: &[Task],
    lists: &[TaskList],
    now: NaiveDateTime,
    sort: SortMode,
    show_completed: bool,
) -> Vec<Section> { todo!() }

/// A key prefix that sorts a Scheduled section by its first day.
fn sort_stamp(due: NaiveDate, today: NaiveDate) -> String { todo!() }

/// The tasks `keep` picks, a section per list in the lists' order.
fn by_list(tasks: &[Task], lists: &[TaskList], sort: SortMode, keep: impl Fn(&Task) -> bool) -> Vec<Section> { todo!() }

fn non_empty(sections: Vec<Section>) -> Vec<Section> { todo!() }

/// Every task index of `sections`, top to bottom (the keyboard's order).
#[must_use]
pub fn flat(sections: &[Section]) -> Vec<usize> { todo!() }

/// The order a new task takes at the end of `list`.
#[must_use]
pub fn next_order(tasks: &[Task], list: &str) -> i64 { todo!() }

/// The open tasks of `list` in their manual order.
fn manual_order(tasks: &[Task], list: &str) -> Vec<usize> { todo!() }

/// Gives the tasks in `order` the orders 1024, 2048, ...; returns those that changed.
fn renumber(tasks: &mut [Task], order: &[usize]) -> Vec<usize> { todo!() }

/// Moves task `moving` in front of task `before` (or to the end of its list with `None`),
/// in the manual order of `moving`'s list; `before` must be in the same list. Returns the
/// tasks whose order changed (to be saved).
pub fn reorder(tasks: &mut [Task], moving: usize, before: Option<usize>) -> Vec<usize> { todo!() }

/// Moves task `moving` one place up or down in its list's manual order (Alt+Up / Alt+Down).
/// Returns the tasks whose order changed.
pub fn move_step(tasks: &mut [Task], moving: usize, up: bool) -> Vec<usize> { todo!() }

/// "Fri 2 Oct", "Today 09:00", "Tomorrow" - the row's due chip.
#[must_use]
pub fn due_label(t: &Task, today: NaiveDate) -> Option<String> { todo!() }

/// `(due today, overdue)` among the open tasks, for the status bar.
#[must_use]
pub fn summary(tasks: &[Task], now: NaiveDateTime) -> (usize, usize) { todo!() }

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
}
