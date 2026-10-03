//! AzReview's DOM ids, each name defined ONCE with the app's `__azreview_`
//! prefix (user ruling 2026-10-02).

use azul::str::String as AzString;

/// The sheet strip (the virtual view the pages scroll in), as text: the
/// page rail looks it up by its id attribute.
pub const STRIP_NAME: &str = "__azreview_sheet-strip";

pub const STRIP: AzString = AzString::from_const_str(STRIP_NAME);
/// The file list (the navigation pane).
pub const FILES: AzString = AzString::from_const_str("__azreview_files");
/// The toolbar's record, save and settings buttons.
pub const RECORD: AzString = AzString::from_const_str("__azreview_record");
pub const SAVE: AzString = AzString::from_const_str("__azreview_save");
pub const SETTINGS: AzString = AzString::from_const_str("__azreview_settings");
