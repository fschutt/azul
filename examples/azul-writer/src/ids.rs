//! Every DOM id and class AzWriter names, once, with the app's prefix
//! (`__azwriter_`, as the widgets' `__azul_`): user ruling 2026-10-02.
//!
//! Each is a `const AzString` (`AzString::from_const_str` borrows the static
//! bytes: no allocation, no second copy); `.with_id(X)` / `.with_class(X)`
//! take it as it is.

use azul::str::String as AzString;

/// The editing host of the document: a page's host is
/// `__azwriter_doc-page-<first block>`, a block `__azwriter_doc-<index>`.
pub const DOC_HOST: AzString = AzString::from_const_str("__azwriter_doc");
/// The scrolling canvas the A4 sheets sit on.
pub const CANVAS: AzString = AzString::from_const_str("__azwriter_canvas");
/// The virtual view that materializes the visible pages.
pub const PAGES: AzString = AzString::from_const_str("__azwriter_pages");
/// One A4 sheet (the class of every page's paper).
pub const SHEET: AzString = AzString::from_const_str("__azwriter_sheet");
/// The empty state shown when no document is open.
pub const EMPTY: AzString = AzString::from_const_str("__azwriter_empty");
/// The backstage's list of documents.
pub const DOC_LIST: AzString = AzString::from_const_str("__azwriter_doc-list");
/// A row of the document list: `__azwriter_open-<index>`.
pub const OPEN_ROW: AzString = AzString::from_const_str("__azwriter_open-");
/// The backstage's "Blank document" button.
pub const NEW_DOC: AzString = AzString::from_const_str("__azwriter_new-doc");
/// The backstage's "Import a file" button (a Markdown or Word file).
pub const IMPORT: AzString = AzString::from_const_str("__azwriter_import");
/// The backstage's "Save" button.
pub const SAVE: AzString = AzString::from_const_str("__azwriter_save");
/// The backstage's "Export as PDF" button.
pub const EXPORT_PDF: AzString = AzString::from_const_str("__azwriter_export-pdf");
/// The backstage's "Export as Markdown" button.
pub const EXPORT_MD: AzString = AzString::from_const_str("__azwriter_export-md");
/// The backstage's "Delete" button (the open document's file).
pub const DELETE: AzString = AzString::from_const_str("__azwriter_delete");
/// The line under the page with the last problem or notice.
pub const NOTICE: AzString = AzString::from_const_str("__azwriter_notice");
/// The settings page's own section (the page view).
pub const SETTINGS_VIEW: AzString = AzString::from_const_str("__azwriter_settings-view");
/// The About dialog's frame.
pub const ABOUT: AzString = AzString::from_const_str("__azwriter_about");
