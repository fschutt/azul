//! Every DOM id AzNotes names, once, with the app's prefix (`__aznotes_`, as
//! the widgets' `__azul_`): user ruling 2026-10-02. Each is a `const AzString`
//! (`AzString::from_const_str` borrows the static bytes: no allocation, no
//! second copy); `.with_id(X)` takes it as it is.

use azul::str::String as AzString;

/// The editing host of the open note; its blocks are `__aznotes_note-body-<index>`.
pub const NOTE_BODY: AzString = AzString::from_const_str("__aznotes_note-body");
/// The read-only host of a printed note or a version.
pub const NOTE_PRINT: AzString = AzString::from_const_str("__aznotes_note-print");
/// A version of the history: `__aznotes_version-<index>`.
pub const VERSION_PREFIX: AzString = AzString::from_const_str("__aznotes_version-");

pub const ABOUT: AzString = AzString::from_const_str("__aznotes_about");
pub const DELETE_FOREVER: AzString = AzString::from_const_str("__aznotes_delete-forever");
pub const EDITOR_PANE: AzString = AzString::from_const_str("__aznotes_editor-pane");
pub const EXPORT_MARKDOWN: AzString = AzString::from_const_str("__aznotes_export-markdown");
pub const EXPORT_PDF: AzString = AzString::from_const_str("__aznotes_export-pdf");
pub const FORMAT_TOOLBAR: AzString = AzString::from_const_str("__aznotes_format-toolbar");
pub const HISTORY: AzString = AzString::from_const_str("__aznotes_history");
pub const HISTORY_BACK: AzString = AzString::from_const_str("__aznotes_history-back");
pub const HISTORY_CHANGES: AzString = AzString::from_const_str("__aznotes_history-changes");
pub const HISTORY_DETAIL: AzString = AzString::from_const_str("__aznotes_history-detail");
pub const HISTORY_RESTORE: AzString = AzString::from_const_str("__aznotes_history-restore");
pub const HISTORY_VERSION: AzString = AzString::from_const_str("__aznotes_history-version");
pub const HISTORY_VERSIONS: AzString = AzString::from_const_str("__aznotes_history-versions");
pub const NEW_NOTE: AzString = AzString::from_const_str("__aznotes_new-note");
pub const NEW_NOTEBOOK: AzString = AzString::from_const_str("__aznotes_new-notebook");
pub const NOTE_HISTORY: AzString = AzString::from_const_str("__aznotes_note-history");
pub const NOTE_LIST: AzString = AzString::from_const_str("__aznotes_note-list");
pub const NOTE_META: AzString = AzString::from_const_str("__aznotes_note-meta");
pub const NOTE_SCROLL: AzString = AzString::from_const_str("__aznotes_note-scroll");
pub const NOTE_TAGS: AzString = AzString::from_const_str("__aznotes_note-tags");
pub const NOTE_TITLE: AzString = AzString::from_const_str("__aznotes_note-title");
pub const NOTES_AREA: AzString = AzString::from_const_str("__aznotes_notes-area");
pub const NOTICE: AzString = AzString::from_const_str("__aznotes_notice");
pub const OPEN_SETTINGS: AzString = AzString::from_const_str("__aznotes_open-settings");
pub const PIN_NOTE: AzString = AzString::from_const_str("__aznotes_pin-note");
pub const RESTORE_NOTE: AzString = AzString::from_const_str("__aznotes_restore-note");
pub const SETTING_AUTOSAVE: AzString = AzString::from_const_str("__aznotes_setting-autosave");
pub const SETTING_FOLDER: AzString = AzString::from_const_str("__aznotes_setting-folder");
pub const SETTING_MODE: AzString = AzString::from_const_str("__aznotes_setting-mode");
pub const SETTING_RELOAD: AzString = AzString::from_const_str("__aznotes_setting-reload");
pub const SETTING_TEXT_SIZE: AzString = AzString::from_const_str("__aznotes_setting-text-size");
pub const SETTING_THEME: AzString = AzString::from_const_str("__aznotes_setting-theme");
pub const SETTING_VERSIONS: AzString = AzString::from_const_str("__aznotes_setting-versions");
pub const SETTINGS: AzString = AzString::from_const_str("__aznotes_settings");
pub const SETTINGS_BACK: AzString = AzString::from_const_str("__aznotes_settings-back");
pub const SHEET_BACKDROP: AzString = AzString::from_const_str("__aznotes_sheet-backdrop");
pub const SHEET_CANCEL: AzString = AzString::from_const_str("__aznotes_sheet-cancel");
pub const SHEET_DELETE: AzString = AzString::from_const_str("__aznotes_sheet-delete");
pub const SHEET_FIELD: AzString = AzString::from_const_str("__aznotes_sheet-field");
pub const SHEET_LINK: AzString = AzString::from_const_str("__aznotes_sheet-link");
pub const SHEET_NEW_NOTEBOOK: AzString = AzString::from_const_str("__aznotes_sheet-new-notebook");
pub const SHEET_OK: AzString = AzString::from_const_str("__aznotes_sheet-ok");
pub const SHEET_OPEN: AzString = AzString::from_const_str("__aznotes_sheet-open");
pub const SHEET_REMOVE: AzString = AzString::from_const_str("__aznotes_sheet-remove");
pub const SHORTCUTS: AzString = AzString::from_const_str("__aznotes_shortcuts");
pub const TAG_INPUT: AzString = AzString::from_const_str("__aznotes_tag-input");
pub const TOOL_BOLD: AzString = AzString::from_const_str("__aznotes_tool-bold");
pub const TOOL_BULLETS: AzString = AzString::from_const_str("__aznotes_tool-bullets");
pub const TOOL_CHECKLIST: AzString = AzString::from_const_str("__aznotes_tool-checklist");
pub const TOOL_CODE: AzString = AzString::from_const_str("__aznotes_tool-code");
pub const TOOL_CODEBLOCK: AzString = AzString::from_const_str("__aznotes_tool-codeblock");
pub const TOOL_H1: AzString = AzString::from_const_str("__aznotes_tool-h1");
pub const TOOL_H2: AzString = AzString::from_const_str("__aznotes_tool-h2");
pub const TOOL_H3: AzString = AzString::from_const_str("__aznotes_tool-h3");
pub const TOOL_INDENT: AzString = AzString::from_const_str("__aznotes_tool-indent");
pub const TOOL_ITALIC: AzString = AzString::from_const_str("__aznotes_tool-italic");
pub const TOOL_LINK: AzString = AzString::from_const_str("__aznotes_tool-link");
pub const TOOL_NUMBERS: AzString = AzString::from_const_str("__aznotes_tool-numbers");
pub const TOOL_OUTDENT: AzString = AzString::from_const_str("__aznotes_tool-outdent");
pub const TOOL_QUOTE: AzString = AzString::from_const_str("__aznotes_tool-quote");
pub const TOOL_RULE: AzString = AzString::from_const_str("__aznotes_tool-rule");
pub const TOOL_STRIKE: AzString = AzString::from_const_str("__aznotes_tool-strike");
pub const TOOL_UNDERLINE: AzString = AzString::from_const_str("__aznotes_tool-underline");
pub const TRASH_BAR: AzString = AzString::from_const_str("__aznotes_trash-bar");
pub const TRASH_NOTE: AzString = AzString::from_const_str("__aznotes_trash-note");
