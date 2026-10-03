//! Every id AzSheets puts on a node, with the app's prefix (`__azsheets_`,
//! as the widgets carry `__azul_`; user ruling 2026-10-02), each spelled
//! ONCE here. Scripts find the nodes by them (`#__azsheets_grid`).

use azul::str::String as AzString;

/// The cell grid (scripts focus it).
pub const GRID: AzString = AzString::from_const_str("__azsheets_grid");
/// The formula bar's field.
pub const FORMULA: AzString = AzString::from_const_str("__azsheets_formula");
/// The name box.
pub const NAME_BOX: AzString = AzString::from_const_str("__azsheets_name-box");
/// The row of the name box, fx and the formula field.
pub const FORMULA_ROW: AzString = AzString::from_const_str("__azsheets_formula-row");
/// The formula autocomplete's row.
pub const SUGGESTIONS: AzString = AzString::from_const_str("__azsheets_suggestions");
/// The sheet tab strip.
pub const SHEET_TABS: AzString = AzString::from_const_str("__azsheets_sheet-tabs");
/// The field a sheet tab is renamed in.
pub const SHEET_RENAME: AzString = AzString::from_const_str("__azsheets_sheet-rename");
/// The side panel (Insert Function, Name Manager, Find, Charts).
pub const SIDE_PANEL: AzString = AzString::from_const_str("__azsheets_side-panel");
/// The Find field.
pub const FIND: AzString = AzString::from_const_str("__azsheets_find");
/// The backstage's pane.
pub const BACKSTAGE_PANE: AzString = AzString::from_const_str("__azsheets_backstage-pane");
/// Backstage > New > Blank workbook.
pub const NEW_BLANK: AzString = AzString::from_const_str("__azsheets_new-blank");
/// Backstage > New > the sample.
pub const NEW_SAMPLE: AzString = AzString::from_const_str("__azsheets_new-sample");
/// The workbook screen (formula bar, grid, tabs).
pub const WORKBOOK: AzString = AzString::from_const_str("__azsheets_workbook");

/// The prefix of the Open list's rows: `__azsheets_open-<index>`.
pub const OPEN_ROW_PREFIX: &str = "__azsheets_open-";

/// The id of the Open list's row `index`.
#[must_use]
pub fn open_row(index: usize) -> AzString {
    AzString::from(format!("{OPEN_ROW_PREFIX}{index}"))
}
