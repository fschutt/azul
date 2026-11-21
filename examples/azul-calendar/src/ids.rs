//! Every DOM id AzCalendar names, defined ONCE, each with the app's prefix `__azcal_` (the
//! widgets' own names carry `__azul_`; user ruling 2026-10-02): no clash with a widget's or
//! another app's names, and no string literal repeated. A fixed name is a `const AzString`
//! (`AzString::from_const_str` borrows the static bytes); a name made at run time (a row, a
//! day, an occurrence) is the prefix and its stem, made by the one function here that knows the
//! stem. The E2E scripts (`scripts/azcalendar_e2e.py`, `examples/azul-calendar/scripts/`) put
//! the same prefix in front of the same stems.

use azul::str::String as AzString;
use chrono::NaiveDate;

/// The prefix of every name.
pub const PREFIX: &str = "__azcal_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azcal_", $value));)*
    };
}

names! {
    // ---- the main window ----
    /// The main window's root, inside the shell's theme scope.
    ROOT = "app";
    /// The shell's panes: the navigation pane and the calendar pane.
    NAVIGATION_PANE = "shell-navigation";
    CALENDAR_PANE = "shell-calendar";
    // ---- the navigation pane ----
    DATE_NAVIGATOR = "date-navigator";
    MY_CALENDARS = "my-calendars";
    /// The To-Do bar (right of the calendar).
    TODO_BAR = "todo-bar";
    // ---- the backstage: Info, Open & Export ----
    INFO_SYNC = "info-sync";
    IMPORT_PATH = "import-path";
    IMPORT_BROWSE = "import-browse";
    IMPORT_CALENDAR = "import-calendar";
    IMPORT_RUN = "import-run";
    EXPORT_PATH = "export-path";
    EXPORT_CALENDAR = "export-calendar";
    EXPORT_RUN = "export-run";
    /// What the last import / export said.
    IO_MESSAGE = "io-message";
    // ---- the backstage: Print ----
    /// The printout's first and last day (date pickers).
    PRINT_START = "print-start";
    PRINT_END = "print-end";
    /// How many pages, which sheet.
    PRINT_PAGES = "print-pages";
    PRINT_RUN = "print-run";
    /// What the last Print said.
    PRINT_MESSAGE = "print-message";
    /// The preview: the printout's first pages as pictures.
    PRINT_PREVIEW = "print-preview";
    // ---- the backstage: Calendars ----
    CALENDAR_NEW = "calendar-new";
    CALENDAR_ADD = "calendar-add";
    // ---- the backstage: Options ----
    SETTINGS_SERVER = "settings-server";
    SETTINGS_SYNC = "settings-sync";
    SETTINGS_SAVE = "settings-save";
    SETTINGS_FLAT = "settings-flat";
    SETTINGS_FLORA = "settings-flora";
    SETTINGS_LIGHT = "settings-light";
    SETTINGS_DARK = "settings-dark";
    SETTINGS_TODO = "settings-todo";
    SETTINGS_NAVIGATION = "settings-navigation";
    SETTINGS_LANGUAGE_SYSTEM = "settings-language-system";
    SETTINGS_LANGUAGE_ENGLISH = "settings-language-english";
    SETTINGS_LANGUAGE_GERMAN = "settings-language-german";
    // ---- the calendar pane ----
    /// The calendar pane's content (the lines over the view, the header, the view).
    CALENDAR = "calendar";
    REMINDER = "reminder";
    EMPTY_CALENDAR = "empty-calendar";
    NOTICE = "notice";
    VIEW_PREV = "view-prev";
    VIEW_NEXT = "view-next";
    VIEW_TITLE = "view-title";
    VIEW_TODAY = "view-today";
    MONTH_GRID = "month-grid";
    SCHEDULE = "schedule";
    AGENDA = "agenda";
    // ---- the hours (Day, Work Week, Week) ----
    ALL_DAY = "all-day";
    WEEK_GRID = "week-grid";
    /// The hours' scroll area (a callback finds it by this name).
    WEEK_SCROLL = "week-scroll";
    NOW_LINE = "now-line";
    /// A new event being made on the grid, and its popover.
    DRAFT = "draft";
    DRAFT_PANEL = "draft-panel";
    DRAFT_TITLE = "draft-title";
    DRAFT_WHEN = "draft-when";
    DRAFT_MORE = "draft-more";
    DRAFT_CANCEL = "draft-cancel";
    DRAFT_SAVE = "draft-save";
    DRAFT_MEET = "draft-meet";
    // ---- the event editor window ----
    /// The editor's pane (the form).
    EDITOR_FORM = "editor-form";
    EDITOR_SCOPE = "editor-scope";
    EDITOR_TITLE = "editor-title";
    EDITOR_ATTENDEES = "editor-attendees";
    EDITOR_LOCATION = "editor-location";
    EDITOR_START_DATE = "editor-start-date";
    EDITOR_START_TIME = "editor-start-time";
    EDITOR_ALL_DAY = "editor-all-day";
    EDITOR_END_DATE = "editor-end-date";
    EDITOR_END_TIME = "editor-end-time";
    EDITOR_REPEAT = "editor-repeat";
    EDITOR_REPEAT_OCCURRENCE = "editor-repeat-occurrence";
    EDITOR_REPEAT_CUSTOM = "editor-repeat-custom";
    EDITOR_REPEAT_REPLACE = "editor-repeat-replace";
    EDITOR_REMINDER = "editor-reminder";
    EDITOR_CALENDAR = "editor-calendar";
    EDITOR_MEET = "editor-meet";
    EDITOR_NOTES = "editor-notes";
    EDITOR_ERROR = "editor-error";
    EDITOR_DELETE = "editor-delete";
    EDITOR_CANCEL = "editor-cancel";
    EDITOR_SAVE = "editor-save";
}

/// A name made at run time: the prefix, then `stem`.
#[must_use]
fn named(stem: &str) -> AzString {
    AzString::from(format!("{PREFIX}{stem}"))
}

/// The date part of a day's names: `yyyymmdd`.
fn ymd(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

/// "My calendars"' row of the calendar at `index`.
#[must_use]
pub fn calendar_row(index: usize) -> AzString {
    named(&format!("calendar-{index}"))
}

/// The backstage Calendars page's name field, colour and Remove of the calendar at `index`.
#[must_use]
pub fn calendar_name(index: usize) -> AzString {
    named(&format!("calendar-name-{index}"))
}

#[must_use]
pub fn calendar_colour(index: usize) -> AzString {
    named(&format!("calendar-colour-{index}"))
}

#[must_use]
pub fn calendar_remove(index: usize) -> AzString {
    named(&format!("calendar-remove-{index}"))
}

/// A backstage page's content, by the page's name (`backstage-options`).
#[must_use]
pub fn backstage_page(page: &str) -> AzString {
    named(&format!("backstage-{page}"))
}

/// The Print page's choice of a print style, by the style's name (`print-style-monthly`).
#[must_use]
pub fn print_style(name: &str) -> AzString {
    named(&format!("print-style-{name}"))
}

/// The preview's picture of the printout's page at `index` (0 = the first).
#[must_use]
pub fn print_sheet(index: usize) -> AzString {
    named(&format!("print-sheet-{index}"))
}

/// The view's root, by the view's name (`view-week`).
#[must_use]
pub fn view(name: &str) -> AzString {
    named(&format!("view-{name}"))
}

/// The month view's cell of `day`, and its "+N more".
#[must_use]
pub fn month_day(day: NaiveDate) -> AzString {
    named(&format!("month-{}", ymd(day)))
}

#[must_use]
pub fn month_more(day: NaiveDate) -> AzString {
    named(&format!("more-{}", ymd(day)))
}

/// An event's box on a day: `event-<id>-<yyyymmdd>` (a repeating event has one box per date,
/// each its own).
#[must_use]
pub fn occurrence(id: &str, date: NaiveDate) -> AzString {
    named(&format!("event-{id}-{}", ymd(date)))
}

/// The hours' all-day cell and day column at `index` (0 = the first day shown).
#[must_use]
pub fn all_day_column(index: usize) -> AzString {
    named(&format!("all-day-{index}"))
}

#[must_use]
pub fn day_column(index: usize) -> AzString {
    named(&format!("day-{index}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_carries_the_app_prefix() {
        for name in [ROOT, CALENDAR, WEEK_SCROLL, DRAFT_SAVE, EDITOR_SAVE, SETTINGS_DARK] {
            assert!(name.as_str().starts_with(PREFIX), "{}", name.as_str());
        }
        let day = NaiveDate::from_ymd_opt(2026, 10, 3).unwrap();
        assert_eq!(occurrence("abc", day).as_str(), "__azcal_event-abc-20261003");
        assert_eq!(month_day(day).as_str(), "__azcal_month-20261003");
        assert_eq!(view("work-week").as_str(), "__azcal_view-work-week");
        assert_eq!(calendar_colour(2).as_str(), "__azcal_calendar-colour-2");
        assert_eq!(print_style("monthly").as_str(), "__azcal_print-style-monthly");
        assert_eq!(PRINT_RUN.as_str(), "__azcal_print-run");
    }
}
