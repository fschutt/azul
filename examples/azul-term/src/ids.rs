//! Every DOM id and class AzTerm names, once, with the app's prefix
//! (`__azterm_`, as the widgets' `__azul_`): user ruling 2026-10-02.
//!
//! Each is a `const AzString` (`AzString::from_const_str` borrows the static
//! bytes); `.with_id(X)` / `.with_class(X)` take it as it is.

use azul::str::String as AzString;

/// The terminal of the active tab (the TerminalView's id).
pub const TERMINAL: AzString = AzString::from_const_str("__azterm_terminal");
/// The positioned box the terminal fills.
pub const PANE: AzString = AzString::from_const_str("__azterm_pane");
/// The strip of tabs over the terminal.
pub const TABS: AzString = AzString::from_const_str("__azterm_tabs");
/// One tab: `__azterm_tab-<index>`.
pub const TAB: AzString = AzString::from_const_str("__azterm_tab-");
/// A tab's close button: `__azterm_tab-close-<index>`.
pub const TAB_CLOSE: AzString = AzString::from_const_str("__azterm_tab-close-");
/// The "new tab" button.
pub const NEW_TAB: AzString = AzString::from_const_str("__azterm_new-tab");
/// The status bar.
pub const STATUS: AzString = AzString::from_const_str("__azterm_status");
/// The empty state (no tab open).
pub const EMPTY: AzString = AzString::from_const_str("__azterm_empty");
/// The settings page's own section.
pub const SETTINGS_VIEW: AzString = AzString::from_const_str("__azterm_settings-view");
/// The About dialog's frame.
pub const ABOUT: AzString = AzString::from_const_str("__azterm_about");
