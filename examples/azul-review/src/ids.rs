//! AzReview's DOM ids, each name defined ONCE with the app's `__azreview_`
//! prefix (user ruling 2026-10-02).

use azul::str::String as AzString;

/// The sheet strip (the virtual view the pages scroll in), as text: the
/// page rail looks it up by its id attribute.
pub const STRIP_NAME: &str = "__azreview_sheet-strip";

pub const STRIP: AzString = AzString::from_const_str(STRIP_NAME);
/// The file list (in the flyout).
pub const FILES: AzString = AzString::from_const_str("__azreview_files");
/// The flyout the files show in, over the pages.
pub const FLYOUT: AzString = AzString::from_const_str("__azreview_flyout");
/// The title row's switch that flies the files out and folds them away.
pub const SIDEBAR_TOGGLE: AzString = AzString::from_const_str("__azreview_sidebar-toggle");
/// The page numbers' row, as text: the strip's scroll looks it up to keep
/// the lit number in view.
pub const PAGES_NAME: &str = "__azreview_pages";

pub const PAGES: AzString = AzString::from_const_str(PAGES_NAME);
/// The page rail's record, save and settings buttons.
pub const RECORD: AzString = AzString::from_const_str("__azreview_record");
pub const SAVE: AzString = AzString::from_const_str("__azreview_save");
pub const SETTINGS: AzString = AzString::from_const_str("__azreview_settings");
