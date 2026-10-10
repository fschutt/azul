//! Every DOM id and marker AzPhoto sets, defined ONCE, with the app's
//! prefix (`__azphoto_`, like the widgets' `__azul_`): no duplicated string
//! literals, no clash with another app's or a widget's names. The tools'
//! ids are `state::Tool::dom_id` (prefixed there, once each).

use azul::str::String as AzString;

/// `<prefix><n>`: a numbered id (a layer row, a swatch, a History state).
#[must_use]
pub fn numbered(prefix: AzString, n: impl core::fmt::Display) -> AzString {
    AzString::from(format!("{}{n}", prefix.as_str()))
}

pub const MENU: AzString = AzString::from_const_str("__azphoto_menu");
pub const OPTIONS: AzString = AzString::from_const_str("__azphoto_options");
pub const STATUS_LINE: AzString = AzString::from_const_str("__azphoto_status-line");
pub const TOOLS: AzString = AzString::from_const_str("__azphoto_tools");
pub const BG_CHIP: AzString = AzString::from_const_str("__azphoto_bg-chip");
pub const FG_CHIP: AzString = AzString::from_const_str("__azphoto_fg-chip");
pub const NAVIGATOR_IMAGE: AzString = AzString::from_const_str("__azphoto_navigator-image");
pub const ZOOM_INPUT: AzString = AzString::from_const_str("__azphoto_zoom-input");
pub const LAYER_BLEND: AzString = AzString::from_const_str("__azphoto_layer-blend");
pub const LAYER_OPACITY_INPUT: AzString = AzString::from_const_str("__azphoto_layer-opacity-input");
pub const LAYER_OPACITY: AzString = AzString::from_const_str("__azphoto_layer-opacity");
pub const LAYER_LIST: AzString = AzString::from_const_str("__azphoto_layer-list");
pub const LAYER_NEW: AzString = AzString::from_const_str("__azphoto_layer-new");
pub const LAYER_NEW_GROUP: AzString = AzString::from_const_str("__azphoto_layer-new-group");
pub const LAYER_DUPLICATE: AzString = AzString::from_const_str("__azphoto_layer-duplicate");
pub const LAYER_MERGE: AzString = AzString::from_const_str("__azphoto_layer-merge");
pub const LAYER_UP: AzString = AzString::from_const_str("__azphoto_layer-up");
pub const LAYER_DOWN: AzString = AzString::from_const_str("__azphoto_layer-down");
pub const LAYER_DELETE: AzString = AzString::from_const_str("__azphoto_layer-delete");
pub const HISTORY_LIST: AzString = AzString::from_const_str("__azphoto_history-list");
pub const HISTORY_UNDO: AzString = AzString::from_const_str("__azphoto_history-undo");
pub const HISTORY_REDO: AzString = AzString::from_const_str("__azphoto_history-redo");
pub const PANELS: AzString = AzString::from_const_str("__azphoto_panels");
pub const DOC_TAB: AzString = AzString::from_const_str("__azphoto_doc-tab");
pub const STATUS: AzString = AzString::from_const_str("__azphoto_status");
pub const RECENT: AzString = AzString::from_const_str("__azphoto_recent");
pub const START: AzString = AzString::from_const_str("__azphoto_start");
pub const START_OPEN: AzString = AzString::from_const_str("__azphoto_start-open");
pub const START_NEW: AzString = AzString::from_const_str("__azphoto_start-new");
pub const START_SAMPLE: AzString = AzString::from_const_str("__azphoto_start-sample");
pub const SHEET: AzString = AzString::from_const_str("__azphoto_sheet");
pub const SHEET_OK: AzString = AzString::from_const_str("__azphoto_sheet-ok");
pub const SHEET_CANCEL: AzString = AzString::from_const_str("__azphoto_sheet-cancel");
pub const TEXT_FAMILY: AzString = AzString::from_const_str("__azphoto_text-family");
pub const TEXT_COMMIT: AzString = AzString::from_const_str("__azphoto_text-commit");
pub const TEXT_CANCEL: AzString = AzString::from_const_str("__azphoto_text-cancel");
pub const PANEL_NAVIGATOR: AzString = AzString::from_const_str("__azphoto_panel-navigator");
pub const PANEL_COLOR: AzString = AzString::from_const_str("__azphoto_panel-color");
pub const PANEL_LAYERS: AzString = AzString::from_const_str("__azphoto_panel-layers");
pub const PANEL_ADJUSTMENTS: AzString = AzString::from_const_str("__azphoto_panel-adjustments");
pub const PANEL_PROPERTIES: AzString = AzString::from_const_str("__azphoto_panel-properties");
pub const PANEL_HISTORY: AzString = AzString::from_const_str("__azphoto_panel-history");
pub const RULER_X: AzString = AzString::from_const_str("__azphoto_ruler-x");
pub const RULER_Y: AzString = AzString::from_const_str("__azphoto_ruler-y");
pub const TEXT_FIELD: AzString = AzString::from_const_str("__azphoto_text-field");
pub const CANVAS: AzString = AzString::from_const_str("__azphoto_canvas");
pub const CURSOR: AzString = AzString::from_const_str("__azphoto_cursor");
pub const ZOOM: AzString = AzString::from_const_str("__azphoto_zoom");

/// Prefixes of the numbered ids ([`numbered`]).
pub const SWATCH_PREFIX: AzString = AzString::from_const_str("__azphoto_swatch-");
pub const RECENT_PREFIX: AzString = AzString::from_const_str("__azphoto_recent-");
pub const LAYER_ROW_PREFIX: AzString = AzString::from_const_str("__azphoto_layer-row-");
pub const HISTORY_PREFIX: AzString = AzString::from_const_str("__azphoto_history-");
pub const ADJUST_PREFIX: AzString = AzString::from_const_str("__azphoto_adjust-");
