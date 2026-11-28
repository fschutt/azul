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
/// The library's shelves (a row of choices under the toolbar): `__azreader_shelf-<index>`.
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
/// The view's own node, named by its chapter and first page
/// (`__azreader_spread-<chapter>-<page>`): a turn brings a new one, which slides in.
pub const SPREAD_PREFIX: &str = "__azreader_spread";
/// The "previous page" / "next page" arrows at a desktop's reading area's sides.
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
/// The two buttons of that section: a copy of the book, the chapter as a PDF.
pub const SETTINGS_DOWNLOAD: AzString = AzString::from_const_str("__azreader_settings-download");
pub const SETTINGS_PRINT: AzString = AzString::from_const_str("__azreader_settings-print");

/// The thin toolbar over the pages, and the library's.
pub const TOOLBAR: AzString = AzString::from_const_str("__azreader_toolbar");
/// The tools of the toolbar: the names a `ToolbarEvent` carries.
pub const TB_STRIP: AzString = AzString::from_const_str("__azreader_tb-strip");
pub const TB_TITLE: AzString = AzString::from_const_str("__azreader_tb-title");
pub const TB_PREV: AzString = AzString::from_const_str("__azreader_tb-prev");
pub const TB_PAGE: AzString = AzString::from_const_str("__azreader_tb-page");
pub const TB_COUNT: AzString = AzString::from_const_str("__azreader_tb-count");
pub const TB_NEXT: AzString = AzString::from_const_str("__azreader_tb-next");
pub const TB_ZOOM_OUT: AzString = AzString::from_const_str("__azreader_tb-zoom-out");
pub const TB_ZOOM: AzString = AzString::from_const_str("__azreader_tb-zoom");
pub const TB_ZOOM_IN: AzString = AzString::from_const_str("__azreader_tb-zoom-in");
pub const TB_FIT: AzString = AzString::from_const_str("__azreader_tb-fit");
pub const TB_BOOKMARK: AzString = AzString::from_const_str("__azreader_tb-bookmark");
pub const TB_DOWNLOAD: AzString = AzString::from_const_str("__azreader_tb-download");
pub const TB_PRINT: AzString = AzString::from_const_str("__azreader_tb-print");
pub const TB_SETTINGS: AzString = AzString::from_const_str("__azreader_tb-settings");
pub const TB_LIBRARY: AzString = AzString::from_const_str("__azreader_tb-library");
pub const TB_ADD: AzString = AzString::from_const_str("__azreader_tb-add");
pub const TB_SEARCH: AzString = AzString::from_const_str("__azreader_tb-search");
pub const TB_SORT: AzString = AzString::from_const_str("__azreader_tb-sort-");
/// The strip of page thumbnails (and the contents, the bookmarks) beside the pages, and its
/// three views.
pub const STRIP: AzString = AzString::from_const_str("__azreader_strip");
pub const STRIP_PAGES: AzString = AzString::from_const_str("__azreader_strip-pages");
pub const STRIP_CONTENTS: AzString = AzString::from_const_str("__azreader_strip-contents");
pub const STRIP_BOOKMARKS: AzString = AzString::from_const_str("__azreader_strip-bookmarks");
/// The slider that jumps through the book, at the bottom of a phone's screen.
pub const SLIDER: AzString = AzString::from_const_str("__azreader_slider");
/// The row under the pages of a phone: the slider and where it is.
pub const SLIDER_ROW: AzString = AzString::from_const_str("__azreader_slider-row");
/// The button beside the slider that opens and closes the sheet of pages, contents and bookmarks.
pub const SLIDER_CONTENTS: AzString = AzString::from_const_str("__azreader_slider-contents");
/// The dimmed rest of a phone's screen beside the open sheet: a tap on it closes the sheet.
pub const STRIP_SCRIM: AzString = AzString::from_const_str("__azreader_strip-scrim");
/// The three zones over a phone's page: left, middle, right.
pub const ZONE_LEFT: AzString = AzString::from_const_str("__azreader_zone-left");
pub const ZONE_MIDDLE: AzString = AzString::from_const_str("__azreader_zone-middle");
pub const ZONE_RIGHT: AzString = AzString::from_const_str("__azreader_zone-right");

/// `prefix` + `index` as an id (`TOC_ROW` + 3 = `__azreader_toc-3`).
#[must_use]
pub fn indexed(prefix: &AzString, index: usize) -> AzString {
    AzString::from(format!("{}{index}", prefix.as_str()))
}
