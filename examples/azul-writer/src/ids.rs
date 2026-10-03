//! Every DOM id and class AzWriter names, once, with the app's prefix
//! (`__azwriter_`, as the widgets' `__azul_`): user ruling 2026-10-02.
//!
//! They are `&'static str` constants: the generated Rust API has no `const`
//! `AzString` constructor yet (`AzString::from_const_str` is not emitted for
//! the dynamically linked crate), and `.with_id(X)` / `.with_class(X)` take
//! a `&str` as it is.

/// The editing host of the document: a page's host is
/// `__azwriter_doc-page-<first block>`, a block `__azwriter_doc-<index>`.
pub const DOC_HOST: &str = "__azwriter_doc";
/// The scrolling canvas the A4 sheets sit on.
pub const CANVAS: &str = "__azwriter_canvas";
/// The virtual view that materializes the visible pages.
pub const PAGES: &str = "__azwriter_pages";
/// One A4 sheet (the class of every page's paper).
pub const SHEET: &str = "__azwriter_sheet";
/// The empty state shown when no document is open.
pub const EMPTY: &str = "__azwriter_empty";
/// The backstage's list of documents.
pub const DOC_LIST: &str = "__azwriter_doc-list";
/// A row of the document list: `__azwriter_open-<index>`.
pub const OPEN_ROW: &str = "__azwriter_open-";
/// The backstage's "Blank document" button.
pub const NEW_DOC: &str = "__azwriter_new-doc";
/// The backstage's "Import a file" button (a Markdown or Word file).
pub const IMPORT: &str = "__azwriter_import";
/// The backstage's "Save" button.
pub const SAVE: &str = "__azwriter_save";
/// The backstage's "Export as PDF" button.
pub const EXPORT_PDF: &str = "__azwriter_export-pdf";
/// The backstage's "Export as Markdown" button.
pub const EXPORT_MD: &str = "__azwriter_export-md";
/// The backstage's "Delete" button (the open document's file).
pub const DELETE: &str = "__azwriter_delete";
/// The line under the page with the last problem or notice.
pub const NOTICE: &str = "__azwriter_notice";
/// The settings page's own section (the page view).
pub const SETTINGS_VIEW: &str = "__azwriter_settings-view";
/// The About dialog's frame.
pub const ABOUT: &str = "__azwriter_about";
