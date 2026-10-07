//! Every DOM id AzMeet names, defined ONCE, each with the app's prefix `__azmeet_` (the widgets'
//! own names carry `__azul_`): the scripts find the controls and the tiles by them, and no string
//! literal repeats (wave-6 prefix ruling). `AzString::from_const_str` borrows the static bytes.

use azul::str::String as AzString;

// ==== The controls bar ====

pub const MIC: AzString = AzString::from_const_str("__azmeet_mic");
pub const CAM: AzString = AzString::from_const_str("__azmeet_cam");
pub const SHARE: AzString = AzString::from_const_str("__azmeet_share");
pub const DEAFEN: AzString = AzString::from_const_str("__azmeet_deafen");
pub const VIEW: AzString = AzString::from_const_str("__azmeet_view");
/// "Drop a video packet" (`AZMEET_TEST_PATTERN=1`).
pub const DROP: AzString = AzString::from_const_str("__azmeet_drop");
pub const SETTINGS: AzString = AzString::from_const_str("__azmeet_settings");
pub const LEAVE: AzString = AzString::from_const_str("__azmeet_leave");

// ==== The side panel ====

pub const PEOPLE: AzString = AzString::from_const_str("__azmeet_people");
pub const CHAT_MESSAGES: AzString = AzString::from_const_str("__azmeet_chat_messages");
pub const CHAT_FIELD: AzString = AzString::from_const_str("__azmeet_chat_field");
pub const CHAT_SEND: AzString = AzString::from_const_str("__azmeet_chat_send");
pub const STATISTICS: AzString = AzString::from_const_str("__azmeet_statistics");
/// The side panel's "Copy link" (the meeting's link beside it).
pub const COPY_LINK: AzString = AzString::from_const_str("__azmeet_copy_link");

// ==== The lobby ====

pub const NAME: AzString = AzString::from_const_str("__azmeet_name");
pub const SERVER: AzString = AzString::from_const_str("__azmeet_server");
pub const JOIN_FIELD: AzString = AzString::from_const_str("__azmeet_join_field");
pub const NEW_MEETING: AzString = AzString::from_const_str("__azmeet_new");
pub const JOIN: AzString = AzString::from_const_str("__azmeet_join");

// ==== The settings ====

/// The settings page (the dialog's root), its parts and its buttons.
pub const SETTINGS_PAGE: AzString = AzString::from_const_str("__azmeet_settings_page");
pub const SETTINGS_HEADER: AzString = AzString::from_const_str("__azmeet_settings_header");
pub const SETTINGS_CATEGORIES: AzString = AzString::from_const_str("__azmeet_settings_categories");
pub const SETTINGS_PANE: AzString = AzString::from_const_str("__azmeet_settings_pane");
pub const SETTINGS_BUTTONS: AzString = AzString::from_const_str("__azmeet_settings_buttons");
/// OK: the changes stay (it was "Back").
pub const SETTINGS_OK: AzString = AzString::from_const_str("__azmeet_settings_ok");
/// Cancel: what the settings found when they opened comes back.
pub const SETTINGS_CANCEL: AzString = AzString::from_const_str("__azmeet_settings_cancel");
/// A category of the settings' list is this and its index: `__azmeet_settings_category_2`.
pub const SETTINGS_CATEGORY_PREFIX: &str = "__azmeet_settings_category_";
/// The settings' About: azul's AboutDialog.
pub const ABOUT: AzString = AzString::from_const_str("__azmeet_about");

// ==== The tiles ====

/// A tile's id is this, then who (`me`, or the name in lower case with every other character an
/// underscore), an underscore and the picture (`camera`, `screen`): `__azmeet_tile_ben_camera`.
pub const TILE_PREFIX: &str = "__azmeet_tile_";
