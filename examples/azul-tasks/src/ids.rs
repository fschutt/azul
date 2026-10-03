//! Every DOM id and class AzTasks names, defined ONCE, each with the app's prefix `__aztasks_`
//! (the widgets' own names carry `__azul_`; user ruling 2026-10-02): no clash with a widget's or
//! another app's names, and no string literal repeated. A fixed name is a `const AzString`
//! (`AzString::from_const_str` borrows the static bytes); a name made at run time (a task's row,
//! a step, a section) is the prefix and its stem, made by the one function here that knows the
//! stem. `scripts/aztasks_e2e.py` puts the same prefix in front of the same stems.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__aztasks_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__aztasks_", $value));)*
    };
}

names! {
    // ---- the navigation pane ----
    SEARCH = "search";
    NEW_LIST = "new-list";
    // ---- the task list ----
    TASK_PANE = "task-pane";
    REMINDER_BANNER = "reminder-banner";
    SNOOZE = "snooze";
    DISMISS_REMINDER = "dismiss-reminder";
    NOTICE = "notice";
    UNDO = "undo";
    VIEW_TITLE = "view-title";
    LIST_SETTINGS = "list-settings";
    CLEAR_COMPLETED = "clear-completed";
    QUICK_ADD = "quick-add";
    QUICK_ADD_BUTTON = "quick-add-button";
    QUICK_CHIPS = "quick-chips";
    TASK_LIST = "task-list";
    COMPLETED_TOGGLE = "completed-toggle";
    /// Classes of a task's row, its title and its due chip; the selected rows'.
    TASK_ROW_CLASS = "task-row";
    TASK_ROW_SELECTED_CLASS = "task-row-selected";
    TASK_TITLE_CLASS = "task-title";
    DUE_CHIP_CLASS = "due-chip";
    // ---- the list's settings, and the bulk bar over a multiple selection ----
    LIST_EDIT = "list-edit";
    LIST_NAME = "list-name";
    LIST_GROUP = "list-group";
    LIST_DEFAULT = "list-default";
    LIST_DONE = "list-done";
    LIST_DELETE = "list-delete";
    BULK = "bulk";
    BULK_COMPLETE = "bulk-complete";
    BULK_FLAG = "bulk-flag";
    BULK_MOVE = "bulk-move";
    BULK_DELETE = "bulk-delete";
    /// "Delete?" over the list.
    CONFIRM = "confirm";
    CONFIRM_YES = "confirm-yes";
    CONFIRM_NO = "confirm-no";
    // ---- the task's details ----
    DETAIL = "detail";
    DETAIL_DONE = "detail-done";
    DETAIL_TITLE = "detail-title";
    DETAIL_PRIORITY = "detail-priority";
    DETAIL_FLAG = "detail-flag";
    DETAIL_NOTES = "detail-notes";
    STEPS = "steps";
    ADD_STEP = "add-step";
    DETAIL_DUE = "detail-due";
    DETAIL_TIME = "detail-time";
    ADD_TIME = "add-time";
    DUE_CLEAR = "due-clear";
    DETAIL_REPEAT = "detail-repeat";
    REPEAT_EDITOR = "repeat-editor";
    DETAIL_REMINDER = "detail-reminder";
    DETAIL_LIST = "detail-list";
    DETAIL_TAGS = "detail-tags";
    ADD_TAG = "add-tag";
    /// The other tags offered under the tag field, and the class of each.
    TAG_SUGGESTIONS = "tag-suggestions";
    TAG_SUGGESTION_CLASS = "tag-suggestion";
    ATTACHMENTS = "attachments";
    ATTACH = "attach";
    DETAIL_DELETE = "detail-delete";
    // ---- the backstage ----
    BACKSTAGE = "backstage";
    SETTINGS_DEFAULT_LIST = "settings-default-list";
    SETTINGS_WEEK_START = "settings-week-start";
    SETTINGS_SHOW_COMPLETED = "settings-show-completed";
    SETTINGS_REMINDER_TIME = "settings-reminder-time";
    SETTINGS_SOUNDS = "settings-sounds";
    SETTINGS_NOTIFICATIONS = "settings-notifications";
    SETTINGS_THEME = "settings-theme";
    SETTINGS_MODE = "settings-mode";
    SETTINGS_SAMPLE = "settings-sample";
    SETTINGS_IMPORT_PATH = "settings-import-path";
    SETTINGS_IMPORT_BROWSE = "settings-import-browse";
    SETTINGS_IMPORT = "settings-import";
    SETTINGS_EXPORT = "settings-export";
    SETTINGS_IO_MESSAGE = "settings-io-message";
    SHORTCUTS = "shortcuts";
    ABOUT = "about";
    // ---- the planned month and the board ----
    /// The list header's "List | Month" / "List | Board" switch.
    LAYOUT_SWITCH = "layout-switch";
    PLANNED_MONTH = "planned-month";
    MONTH_PREV = "month-prev";
    MONTH_NEXT = "month-next";
    MONTH_TODAY = "month-today";
    /// The class of a task in a day of the planned month.
    PLANNED_TASK_CLASS = "planned-task";
    BOARD = "board";
    /// The class of a task's card on the board.
    BOARD_CARD_CLASS = "board-card";
}

/// The stem of a task row's id: `task-<task id>`.
const TASK_ROW: &str = concat!("__aztasks_", "task-");

/// A name made at run time: the prefix, then `stem`.
#[must_use]
fn named(stem: &str) -> AzString {
    AzString::from(format!("{PREFIX}{stem}"))
}

/// The row of the task `id` in the list.
#[must_use]
pub fn task_row(id: &str) -> AzString {
    AzString::from(format!("{TASK_ROW}{id}"))
}

/// Whether the DOM id `id` is a task's row (a key on it is a command on the task). The list and
/// its pane share the stem's start (`task-list`, `task-pane`) and are no row.
#[must_use]
pub fn is_task_row(id: &str) -> bool {
    id.starts_with(TASK_ROW) && id != TASK_LIST.as_str() && id != TASK_PANE.as_str()
}

/// The check box of the task `id` in the list.
#[must_use]
pub fn task_check(id: &str) -> AzString {
    named(&format!("check-{id}"))
}

/// A section of the list (`section-<key>`, the key of `views::Section`).
#[must_use]
pub fn section(key: &str) -> AzString {
    named(&format!("section-{key}"))
}

/// The planned month's cell of `day` (`month-day-2026-10-01`).
#[must_use]
pub fn month_day(day: &str) -> AzString {
    named(&format!("month-day-{day}"))
}

/// The board's column by its key (`column-todo`, `column-doing`, `column-done`).
#[must_use]
pub fn board_column(key: &str) -> AzString {
    named(&format!("column-{key}"))
}

/// The board's card of the task `id`.
#[must_use]
pub fn board_card(id: &str) -> AzString {
    named(&format!("card-{id}"))
}

/// The task's step at `n` (0 = the first), and the due date's quick choice at `n`.
#[must_use]
pub fn step(n: usize) -> AzString {
    named(&format!("step-{n}"))
}

#[must_use]
pub fn due_quick(n: usize) -> AzString {
    named(&format!("due-quick-{n}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_carries_the_app_prefix() {
        for name in [SEARCH, TASK_LIST, TASK_ROW_CLASS, DETAIL_TITLE, BULK_DELETE, ABOUT] {
            assert!(name.as_str().starts_with(PREFIX), "{}", name.as_str());
        }
        assert_eq!(task_row("t1").as_str(), "__aztasks_task-t1");
        assert!(is_task_row(task_row("t1").as_str()));
        assert!(!is_task_row("task-t1"));
        assert!(!is_task_row(TASK_LIST.as_str()), "the list is no task's row");
        assert_eq!(section("day-2026-10-01").as_str(), "__aztasks_section-day-2026-10-01");
        assert_eq!(step(2).as_str(), "__aztasks_step-2");
    }
}
