//! Every DOM id and class AzCode names, once, with the app's prefix
//! (`__azcode_`, as the widgets' `__azul_`): user ruling 2026-10-02. Each
//! is a `const AzString` (no allocation, no second copy).

use azul::str::String as AzString;

/// The code view of the file in front.
pub const EDITOR: AzString = AzString::from_const_str("__azcode_editor");
/// The explorer's tree.
pub const EXPLORER: AzString = AzString::from_const_str("__azcode_explorer");
/// The search panel in the side bar.
pub const SEARCH_PANEL: AzString = AzString::from_const_str("__azcode_search-panel");
/// The activity bar's Explorer button.
pub const ACTIVITY_EXPLORER: AzString = AzString::from_const_str("__azcode_activity-explorer");
/// The activity bar's Search button.
pub const ACTIVITY_SEARCH: AzString = AzString::from_const_str("__azcode_activity-search");
/// The document tabs.
pub const TABS: AzString = AzString::from_const_str("__azcode_tabs");
/// The close button of the tab in front.
pub const CLOSE_TAB: AzString = AzString::from_const_str("__azcode_close-tab");
/// The find bar over the editor.
pub const FIND_BAR: AzString = AzString::from_const_str("__azcode_find-bar");
/// The find bar's text field.
pub const FIND_INPUT: AzString = AzString::from_const_str("__azcode_find-input");
/// The find bar's replace field.
pub const REPLACE_INPUT: AzString = AzString::from_const_str("__azcode_replace-input");
/// "Previous match".
pub const FIND_PREVIOUS: AzString = AzString::from_const_str("__azcode_find-previous");
/// "Next match".
pub const FIND_NEXT: AzString = AzString::from_const_str("__azcode_find-next");
/// "Replace" (the current match).
pub const REPLACE_ONE: AzString = AzString::from_const_str("__azcode_replace-one");
/// "Replace all".
pub const REPLACE_ALL: AzString = AzString::from_const_str("__azcode_replace-all");
/// The match-case toggle.
pub const MATCH_CASE: AzString = AzString::from_const_str("__azcode_match-case");
/// The whole-word toggle.
pub const WHOLE_WORD: AzString = AzString::from_const_str("__azcode_whole-word");
/// "3 of 14" in the find bar.
pub const FIND_COUNT: AzString = AzString::from_const_str("__azcode_find-count");
/// The go-to-line bar.
pub const GOTO_BAR: AzString = AzString::from_const_str("__azcode_goto-bar");
/// The go-to-line field.
pub const GOTO_INPUT: AzString = AzString::from_const_str("__azcode_goto-input");
/// The welcome screen (no workspace open).
pub const WELCOME: AzString = AzString::from_const_str("__azcode_welcome");
/// The welcome screen's "Open the sample" button.
pub const OPEN_SAMPLE: AzString = AzString::from_const_str("__azcode_open-sample");
/// The empty editor (no file open).
pub const NO_FILE: AzString = AzString::from_const_str("__azcode_no-file");
/// The status bar's "Ln, Col" segment marker.
pub const STATUS_CARET: AzString = AzString::from_const_str("__azcode_status-caret");
/// The line with the last problem or notice.
pub const NOTICE: AzString = AzString::from_const_str("__azcode_notice");
