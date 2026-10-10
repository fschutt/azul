//! Every DOM id and class AzReader names, once, with the app's prefix (`__azreader_`, as the
//! widgets' `__azul_`): user ruling 2026-10-02. Each is a `const AzString`
//! (`AzString::from_const_str` borrows the static bytes); `.with_id(X)` / `.with_class(X)`
//! take it as it is. A row's id is the prefix constant plus its index (`TOC_ROW` + `3`).

use azul::str::String as AzString;

/// The library's covers grid.
pub const LIBRARY_GRID: AzString = AzString::from_const_str("__azreader_library");
/// One book tile of the grid: `__azreader_book-<index>`.
pub const BOOK_TILE: AzString = AzString::from_const_str("__azreader_book-");
/// The class of every book tile.
pub const BOOK_TILE_CLASS: AzString = AzString::from_const_str("__azreader_book");
/// The library's search field.
pub const LIBRARY_SEARCH: AzString = AzString::from_const_str("__azreader_search");
/// The library's shelves (the navigation pane): `__azreader_shelf-<index>`.
pub const SHELF_ROW: AzString = AzString::from_const_str("__azreader_shelf-");
/// The empty state (no book yet).
pub const EMPTY: AzString = AzString::from_const_str("__azreader_empty");
/// The "Add a book" button.
pub const ADD_BOOK: AzString = AzString::from_const_str("__azreader_add");

/// The reading area (the pages and the space around them).
pub const READING_AREA: AzString = AzString::from_const_str("__azreader_reading");
/// The class of one page (its paper).
pub const PAGE_CLASS: AzString = AzString::from_const_str("__azreader_page");
/// The first page shown: `__azreader_page-0`, the second `__azreader_page-1`.
pub const PAGE: AzString = AzString::from_const_str("__azreader_page-");
/// The clip window of a page over the reading column.
pub const PAGE_CLIP_CLASS: AzString = AzString::from_const_str("__azreader_clip");
/// The reading column (the chapter laid out at the page's width).
pub const COLUMN_CLASS: AzString = AzString::from_const_str("__azreader_column");
/// The "previous page" / "next page" zones at the reading area's sides.
pub const PREV_ZONE: AzString = AzString::from_const_str("__azreader_prev");
pub const NEXT_ZONE: AzString = AzString::from_const_str("__azreader_next");
/// The running head over the pages (the chapter's title).
pub const RUNNING_HEAD: AzString = AzString::from_const_str("__azreader_head");
/// The line under the pages (page n of m in the chapter).
pub const FOLIO: AzString = AzString::from_const_str("__azreader_folio");
/// "Laying out ..." while a chapter's pages are not known yet.
pub const LOADING: AzString = AzString::from_const_str("__azreader_loading");

/// The table of contents pane.
pub const TOC: AzString = AzString::from_const_str("__azreader_toc");
/// One entry of it: `__azreader_toc-<index>`.
pub const TOC_ROW: AzString = AzString::from_const_str("__azreader_toc-");
/// The class of every entry, and of the current one.
pub const TOC_ROW_CLASS: AzString = AzString::from_const_str("__azreader_toc-row");
pub const TOC_CURRENT_CLASS: AzString = AzString::from_const_str("__azreader_toc-current");
/// The bookmarks pane.
pub const BOOKMARKS: AzString = AzString::from_const_str("__azreader_bookmarks");
/// One bookmark: `__azreader_mark-<index>`.
pub const BOOKMARK_ROW: AzString = AzString::from_const_str("__azreader_mark-");
/// The class of every bookmark row.
pub const BOOKMARK_ROW_CLASS: AzString = AzString::from_const_str("__azreader_mark-row");

/// The status bar's progress segment.
pub const PROGRESS: AzString = AzString::from_const_str("__azreader_progress");
/// The line with the last problem or notice.
pub const NOTICE: AzString = AzString::from_const_str("__azreader_notice");
/// The About dialog's frame.
pub const ABOUT: AzString = AzString::from_const_str("__azreader_about");
/// The reading settings' section (the settings page).
pub const READING_SETTINGS: AzString = AzString::from_const_str("__azreader_reading-settings");

/// `prefix` + `index` as an id (`TOC_ROW` + 3 = `__azreader_toc-3`).
#[must_use]
pub fn indexed(prefix: &AzString, index: usize) -> AzString {
    AzString::from(format!("{}{index}", prefix.as_str()))
}
