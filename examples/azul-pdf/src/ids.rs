//! AzPdf's DOM ids, each name defined ONCE with the app's `__azpdf_` prefix
//! (user ruling 2026-10-02). The `_NAME` texts are for lookups by id
//! attribute (`get_node_id_by_id_attribute`) and for the per-page ids.

use azul::str::String as AzString;

/// The page view (a VirtualView the pages scroll in).
pub const PAGES_NAME: &str = "__azpdf_pages";
pub const PAGES: AzString = AzString::from_const_str(PAGES_NAME);
/// The thumbnail rail (a VirtualView in the navigation pane).
pub const THUMBS_NAME: &str = "__azpdf_thumbs";
pub const THUMBS: AzString = AzString::from_const_str(THUMBS_NAME);
/// One page's frame: the prefix and the page number (1-based).
pub const PAGE_PREFIX: &str = "__azpdf_page-";
/// One thumbnail: the prefix and the page number (1-based).
pub const THUMB_PREFIX: &str = "__azpdf_thumb-";
/// One outline entry: the prefix and its index.
pub const OUTLINE_PREFIX: &str = "__azpdf_outline-";
/// One search hit: the prefix and its index.
pub const HIT_PREFIX: &str = "__azpdf_hit-";
/// One recent document on the start screen: the prefix and its index.
pub const RECENT_PREFIX: &str = "__azpdf_recent-";

/// The toolbar.
pub const TOOLBAR: AzString = AzString::from_const_str("__azpdf_toolbar");
pub const OPEN: AzString = AzString::from_const_str("__azpdf_open");
pub const PREV: AzString = AzString::from_const_str("__azpdf_prev");
pub const NEXT: AzString = AzString::from_const_str("__azpdf_next");
pub const PAGE_FIELD: AzString = AzString::from_const_str("__azpdf_page-field");
pub const PAGE_COUNT: AzString = AzString::from_const_str("__azpdf_page-count");
pub const ZOOM_OUT: AzString = AzString::from_const_str("__azpdf_zoom-out");
pub const ZOOM: AzString = AzString::from_const_str("__azpdf_zoom");
pub const ZOOM_IN: AzString = AzString::from_const_str("__azpdf_zoom-in");
pub const SEARCH_FIELD: AzString = AzString::from_const_str("__azpdf_search");
pub const SETTINGS: AzString = AzString::from_const_str("__azpdf_settings");

/// The navigation pane: its Pages / Outline switch and the outline list.
pub const NAV_TABS: AzString = AzString::from_const_str("__azpdf_nav-tabs");
pub const OUTLINE: AzString = AzString::from_const_str("__azpdf_outline");
/// The side pane with the search hits.
pub const HITS: AzString = AzString::from_const_str("__azpdf_hits");
/// The start screen and its recent list.
pub const START: AzString = AzString::from_const_str("__azpdf_start");
pub const RECENT: AzString = AzString::from_const_str("__azpdf_recent");
/// The line under the toolbar while a document opens or fails to.
pub const NOTICE: AzString = AzString::from_const_str("__azpdf_notice");

/// `prefix` + `n` (a per-item id).
#[must_use]
pub fn numbered(prefix: &str, n: usize) -> AzString {
    AzString::from(format!("{prefix}{n}"))
}
