//! Every DOM id and class AzCode names, once, with the app's prefix
//! (`__azcode_`, as the widgets' `__azul_`): user ruling 2026-10-02. Each
//! is a `const AzString` (no allocation, no second copy); the numbered ones
//! (a tab, a recent folder) are made by the functions at the end.

use azul::str::String as AzString;

/// The code view of the file in front.
pub const EDITOR: AzString = AzString::from_const_str("__azcode_editor");
/// The explorer's tree.
pub const EXPLORER: AzString = AzString::from_const_str("__azcode_explorer");
/// The explorer's Refresh button (in the side bar's title row).
pub const REFRESH: AzString = AzString::from_const_str("__azcode_refresh");
/// The search panel in the side bar.
pub const SEARCH_PANEL: AzString = AzString::from_const_str("__azcode_search-panel");
/// The search panel's text field.
pub const SEARCH_INPUT: AzString = AzString::from_const_str("__azcode_search-input");
/// The activity bar's Explorer icon.
pub const ACTIVITY_EXPLORER: AzString = AzString::from_const_str("__azcode_activity-explorer");
/// The activity bar's Search icon.
pub const ACTIVITY_SEARCH: AzString = AzString::from_const_str("__azcode_activity-search");
/// The activity bar's Settings icon (at its foot).
pub const ACTIVITY_SETTINGS: AzString = AzString::from_const_str("__azcode_activity-settings");
/// The side bar's empty state while no folder is open ("You have not yet
/// opened a folder.").
pub const NO_FOLDER: AzString = AzString::from_const_str("__azcode_no-folder");
/// The empty state's "Open Folder" button.
pub const OPEN_FOLDER: AzString = AzString::from_const_str("__azcode_open-folder");
/// The tab strip over the editor.
pub const TABS: AzString = AzString::from_const_str("__azcode_tabs");
/// Every tab's class.
pub const TAB_CLASS: AzString = AzString::from_const_str("__azcode_tab");
/// The class of the tab in front.
pub const TAB_ACTIVE_CLASS: AzString = AzString::from_const_str("__azcode_tab-active");
/// The class of a tab's dot: its file has unsaved changes.
pub const TAB_DIRTY_CLASS: AzString = AzString::from_const_str("__azcode_tab-dirty");
/// The path of the file in front, under the tabs.
pub const BREADCRUMBS: AzString = AzString::from_const_str("__azcode_breadcrumbs");
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
/// The welcome page: the editor while no file is open (the app's name,
/// Start, Recent, the keyboard shortcuts).
pub const WELCOME: AzString = AzString::from_const_str("__azcode_welcome");
/// The welcome page's "Open Folder...".
pub const WELCOME_OPEN_FOLDER: AzString = AzString::from_const_str("__azcode_welcome-open-folder");
/// The welcome page's "Open File...".
pub const WELCOME_OPEN_FILE: AzString = AzString::from_const_str("__azcode_welcome-open-file");
/// The welcome page's "Open the sample workspace".
pub const OPEN_SAMPLE: AzString = AzString::from_const_str("__azcode_open-sample");
/// Quick open (Mod+P): the host of the command palette over the files.
pub const QUICK_OPEN: AzString = AzString::from_const_str("__azcode_quick-open");
/// The status bar's "Ln, Col" segment marker.
pub const STATUS_CARET: AzString = AzString::from_const_str("__azcode_status-caret");
/// The status bar's branch segment marker.
pub const STATUS_BRANCH: AzString = AzString::from_const_str("__azcode_status-branch");
/// The line with the last problem or notice.
pub const NOTICE: AzString = AzString::from_const_str("__azcode_notice");
/// The explorer's header: the workspace's name.
pub const EXPLORER_FOLDER: AzString = AzString::from_const_str("__azcode_explorer-folder");
/// The explorer's "Collapse Folders" button.
pub const COLLAPSE: AzString = AzString::from_const_str("__azcode_collapse");
/// The class of every row of the explorer.
pub const TREE_ROW_CLASS: AzString = AzString::from_const_str("__azcode_tree-row");
/// The class of the explorer's selected row.
pub const TREE_SELECTED_CLASS: AzString = AzString::from_const_str("__azcode_tree-selected");
/// The search panel's match-case toggle.
pub const SEARCH_MATCH_CASE: AzString = AzString::from_const_str("__azcode_search-match-case");
/// The search panel's whole-word toggle.
pub const SEARCH_WHOLE_WORD: AzString = AzString::from_const_str("__azcode_search-whole-word");
/// The search panel's list of results (a virtualized list).
pub const SEARCH_RESULTS: AzString = AzString::from_const_str("__azcode_search-results");
/// The line over the results ("12 results in 3 files").
pub const SEARCH_SUMMARY: AzString = AzString::from_const_str("__azcode_search-summary");
/// The class of a result row (a file or a match).
pub const RESULT_CLASS: AzString = AzString::from_const_str("__azcode_result");
/// The terminal panel under the editor.
pub const PANEL: AzString = AzString::from_const_str("__azcode_panel");
/// The terminal of the shell in front (azul's TerminalView).
pub const TERMINAL: AzString = AzString::from_const_str("__azcode_terminal");
/// The panel's "New Terminal".
pub const TERMINAL_NEW: AzString = AzString::from_const_str("__azcode_terminal-new");
/// The panel's "Kill Terminal".
pub const TERMINAL_KILL: AzString = AzString::from_const_str("__azcode_terminal-kill");
/// The panel's "Close Panel".
pub const PANEL_CLOSE: AzString = AzString::from_const_str("__azcode_panel-close");
/// The command palette (Mod+Shift+P): the host of the palette over the
/// window's commands.
pub const COMMAND_PALETTE: AzString = AzString::from_const_str("__azcode_command-palette");

/// Tab `i` (0 = the first).
#[must_use]
pub fn tab(i: usize) -> AzString {
    AzString::from(format!("__azcode_tab-{i}"))
}

/// The close button of tab `i`.
#[must_use]
pub fn tab_close(i: usize) -> AzString {
    AzString::from(format!("__azcode_tab-close-{i}"))
}

/// Recent folder `i` in the side bar's empty state.
#[must_use]
pub fn recent(i: usize) -> AzString {
    AzString::from(format!("__azcode_recent-{i}"))
}

/// Recent folder `i` on the welcome page.
#[must_use]
pub fn welcome_recent(i: usize) -> AzString {
    AzString::from(format!("__azcode_welcome-recent-{i}"))
}

/// The explorer's row of workspace key `key` (`src/` a folder, `src/main.rs`
/// a file): `__azcode_tree-` and the key, every character but a letter, a
/// digit, `_` and `-` as `-` (`__azcode_tree-src-main-rs`), so a test can
/// name it in a selector.
#[must_use]
pub fn tree_row(key: &str) -> AzString {
    AzString::from(tree_row_name(key))
}

/// [`tree_row`] as a plain string.
#[must_use]
pub fn tree_row_name(key: &str) -> String {
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '-' })
        .collect();
    format!("__azcode_tree-{safe}")
}

/// Shell `i` (0 = the first) in the terminal panel's header.
#[must_use]
pub fn terminal_tab(i: usize) -> AzString {
    AzString::from(format!("__azcode_terminal-tab-{i}"))
}

/// Result row `i` of the search panel's list.
#[must_use]
pub fn result_row(i: usize) -> AzString {
    AzString::from(format!("__azcode_result-{i}"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_tree_rows_id_is_its_key_spelled_for_a_selector() {
        assert_eq!(super::tree_row_name("src/"), "__azcode_tree-src-");
        assert_eq!(super::tree_row_name("src/main.rs"), "__azcode_tree-src-main-rs");
        assert_eq!(super::tree_row_name("Cargo.toml"), "__azcode_tree-Cargo-toml");
        assert_eq!(super::tree_row_name("a b/ü.rs"), "__azcode_tree-a-b---rs");
    }
}
