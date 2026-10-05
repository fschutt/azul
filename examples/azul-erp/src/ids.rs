//! AzERP's DOM ids and classes, each name defined ONCE, every one carrying
//! the app's `__azerp_` prefix (user ruling 2026-10-02, like the widgets'
//! `__azul_`). A form field's id is made from its view field name by the
//! one helper [`field`] (`__azerp_field-acquisition_cost`): the views name
//! the fields, not this file.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azerp_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azerp_", $value));)*
    };
}

names! {
    /// The section tabs (the RecordsShell's tab row: the views' menu).
    TABS = "tabs";
    /// The page's tool row (the view's actions).
    TOOLS = "tools";
    /// The page's title.
    PAGE_TITLE = "page-title";
    /// A table view's DataTable.
    TABLE = "table";
    /// The empty state while the records load, or of an empty table.
    EMPTY = "empty";
    /// The record form (the side pane, or the modal's content).
    FORM = "form";
    FORM_TITLE = "form-title";
    FORM_SAVE = "form-save";
    FORM_CANCEL = "form-cancel";
    /// The form's problems.
    FORM_PROBLEMS = "form-problems";
    /// A `form_modal` view's modal.
    MODAL = "modal";
    /// A detail view: its header, its title, its status pill, its tabs.
    DETAIL = "detail";
    DETAIL_TITLE = "detail-title";
    DETAIL_STATUS = "detail-status";
    DETAIL_TABS = "detail-tabs";
    /// The detail's Back button.
    BACK = "back";
    /// The named panels.
    OVERVIEW = "overview";
    SCHEDULE = "schedule";
    SCHEDULE_TABLE = "schedule-table";
    SCHEDULE_CHART = "schedule-chart";
    EMBEDDED = "embedded";
    REPORTS = "reports";
    REPORT_TOTALS = "report-totals";
    REPORT_CATEGORIES = "report-categories";
    REPORT_FORECAST = "report-forecast";
    REPORT_DUE = "report-due";
    REPORT_OVERDUE = "report-overdue";
    IMPORT = "import";
    IMPORT_CHOOSE = "import-choose";
    IMPORT_COMMIT = "import-commit";
    IMPORT_SUMMARY = "import-summary";
    RUN = "run";
    RUN_YEAR = "run-year";
    RUN_NEXT = "run-next";
    RUN_POST = "run-post";
    RUN_TOTAL = "run-total";
    /// The status bar's segments.
    STATUS_COUNT = "status-count";
    STATUS_NOTICE = "status-notice";
    STATUS_SYNC = "status-sync";
    /// A field row's class, a label's class, a pill's class.
    FIELD_ROW = "field-row";
    FIELD_LABEL = "field-label";
    PILL = "pill";
}

/// The id of a button made from a view action (`__azerp_action-export_csv`,
/// `__azerp_action-/accounting/assets/new` is not a name: the action's id,
/// else its label's words joined by `-`).
#[must_use]
pub fn action(name: &str) -> AzString {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    AzString::from(format!("{PREFIX}action-{slug}"))
}

/// The id of a form field (`__azerp_field-acquisition_cost`).
#[must_use]
pub fn field(name: &str) -> AzString {
    AzString::from(format!("{PREFIX}field-{name}"))
}

/// The id of a section tab or a detail tab (`__azerp_tab-2`).
#[must_use]
pub fn tab(index: usize) -> AzString {
    AzString::from(format!("{PREFIX}tab-{index}"))
}
