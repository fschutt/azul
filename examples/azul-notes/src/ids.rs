//! Every DOM id AzNotes names, once, with the app's prefix (`__aznotes_`, as
//! the widgets' `__azul_`): user ruling 2026-10-02. `&'static str` constants -
//! the generated Rust API has no `const` `AzString` constructor yet;
//! `.with_id(X)` takes a `&str` as it is.

/// The editing host of the open note; its blocks are `__aznotes_note-body-<index>`.
pub const NOTE_BODY: &str = "__aznotes_note-body";
/// The read-only host of a printed note or a version.
pub const NOTE_PRINT: &str = "__aznotes_note-print";
/// A version of the history: `__aznotes_version-<index>`.
pub const VERSION_PREFIX: &str = "__aznotes_version-";

pub const ABOUT: &str = "__aznotes_about";
pub const DELETE_FOREVER: &str = "__aznotes_delete-forever";
pub const EDITOR_PANE: &str = "__aznotes_editor-pane";
pub const EXPORT_MARKDOWN: &str = "__aznotes_export-markdown";
pub const EXPORT_PDF: &str = "__aznotes_export-pdf";
pub const FORMAT_TOOLBAR: &str = "__aznotes_format-toolbar";
pub const HISTORY: &str = "__aznotes_history";
pub const HISTORY_BACK: &str = "__aznotes_history-back";
pub const HISTORY_CHANGES: &str = "__aznotes_history-changes";
pub const HISTORY_DETAIL: &str = "__aznotes_history-detail";
pub const HISTORY_RESTORE: &str = "__aznotes_history-restore";
pub const HISTORY_VERSION: &str = "__aznotes_history-version";
pub const HISTORY_VERSIONS: &str = "__aznotes_history-versions";
pub const NEW_NOTE: &str = "__aznotes_new-note";
pub const NEW_NOTEBOOK: &str = "__aznotes_new-notebook";
pub const NOTE_HISTORY: &str = "__aznotes_note-history";
pub const NOTE_LIST: &str = "__aznotes_note-list";
pub const NOTE_META: &str = "__aznotes_note-meta";
pub const NOTE_SCROLL: &str = "__aznotes_note-scroll";
pub const NOTE_TAGS: &str = "__aznotes_note-tags";
pub const NOTE_TITLE: &str = "__aznotes_note-title";
pub const NOTES_AREA: &str = "__aznotes_notes-area";
pub const NOTICE: &str = "__aznotes_notice";
pub const OPEN_SETTINGS: &str = "__aznotes_open-settings";
pub const PIN_NOTE: &str = "__aznotes_pin-note";
pub const RESTORE_NOTE: &str = "__aznotes_restore-note";
pub const SETTING_AUTOSAVE: &str = "__aznotes_setting-autosave";
pub const SETTING_FOLDER: &str = "__aznotes_setting-folder";
pub const SETTING_MODE: &str = "__aznotes_setting-mode";
pub const SETTING_RELOAD: &str = "__aznotes_setting-reload";
pub const SETTING_TEXT_SIZE: &str = "__aznotes_setting-text-size";
pub const SETTING_THEME: &str = "__aznotes_setting-theme";
pub const SETTING_VERSIONS: &str = "__aznotes_setting-versions";
pub const SETTINGS: &str = "__aznotes_settings";
pub const SETTINGS_BACK: &str = "__aznotes_settings-back";
pub const SHEET_BACKDROP: &str = "__aznotes_sheet-backdrop";
pub const SHEET_CANCEL: &str = "__aznotes_sheet-cancel";
pub const SHEET_DELETE: &str = "__aznotes_sheet-delete";
pub const SHEET_FIELD: &str = "__aznotes_sheet-field";
pub const SHEET_LINK: &str = "__aznotes_sheet-link";
pub const SHEET_NEW_NOTEBOOK: &str = "__aznotes_sheet-new-notebook";
pub const SHEET_OK: &str = "__aznotes_sheet-ok";
pub const SHEET_OPEN: &str = "__aznotes_sheet-open";
pub const SHEET_REMOVE: &str = "__aznotes_sheet-remove";
pub const SHORTCUTS: &str = "__aznotes_shortcuts";
pub const TAG_INPUT: &str = "__aznotes_tag-input";
pub const TOOL_BOLD: &str = "__aznotes_tool-bold";
pub const TOOL_BULLETS: &str = "__aznotes_tool-bullets";
pub const TOOL_CHECKLIST: &str = "__aznotes_tool-checklist";
pub const TOOL_CODE: &str = "__aznotes_tool-code";
pub const TOOL_CODEBLOCK: &str = "__aznotes_tool-codeblock";
pub const TOOL_H1: &str = "__aznotes_tool-h1";
pub const TOOL_H2: &str = "__aznotes_tool-h2";
pub const TOOL_H3: &str = "__aznotes_tool-h3";
pub const TOOL_INDENT: &str = "__aznotes_tool-indent";
pub const TOOL_ITALIC: &str = "__aznotes_tool-italic";
pub const TOOL_LINK: &str = "__aznotes_tool-link";
pub const TOOL_NUMBERS: &str = "__aznotes_tool-numbers";
pub const TOOL_OUTDENT: &str = "__aznotes_tool-outdent";
pub const TOOL_QUOTE: &str = "__aznotes_tool-quote";
pub const TOOL_RULE: &str = "__aznotes_tool-rule";
pub const TOOL_STRIKE: &str = "__aznotes_tool-strike";
pub const TOOL_UNDERLINE: &str = "__aznotes_tool-underline";
pub const TRASH_BAR: &str = "__aznotes_trash-bar";
pub const TRASH_NOTE: &str = "__aznotes_trash-note";
