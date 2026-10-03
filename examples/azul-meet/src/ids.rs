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

// ==== The lobby ====

pub const NAME: AzString = AzString::from_const_str("__azmeet_name");
pub const SERVER: AzString = AzString::from_const_str("__azmeet_server");
pub const JOIN_FIELD: AzString = AzString::from_const_str("__azmeet_join_field");
pub const NEW_MEETING: AzString = AzString::from_const_str("__azmeet_new");
pub const JOIN: AzString = AzString::from_const_str("__azmeet_join");

// ==== The settings ====

pub const SETTINGS_BACK: AzString = AzString::from_const_str("__azmeet_settings_back");
/// The settings' About: azul's AboutDialog.
pub const ABOUT: AzString = AzString::from_const_str("__azmeet_about");

// ==== The tiles ====

/// A tile's id is this, then who (`me`, or the name in lower case with every other character an
/// underscore), an underscore and the picture (`camera`, `screen`): `__azmeet_tile_ben_camera`.
pub const TILE_PREFIX: &str = "__azmeet_tile_";
