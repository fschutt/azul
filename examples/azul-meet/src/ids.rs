//! Every DOM id AzMeet names, defined ONCE, each with the app's prefix `__azmeet_` (the widgets'
//! own names carry `__azul_`): the scripts find the controls and the tiles by them, and no string
//! literal repeats (wave-6 prefix ruling). `AzString::from_const_str` borrows the static bytes.

use azul::str::String as AzString;

// ==== The controls bar (`MIC` and `CAM` are the waiting room's switches too) ====

pub const MIC: AzString = AzString::from_const_str("__azmeet_mic");
pub const CAM: AzString = AzString::from_const_str("__azmeet_cam");
pub const SHARE: AzString = AzString::from_const_str("__azmeet_share");
pub const DEAFEN: AzString = AzString::from_const_str("__azmeet_deafen");
pub const VIEW: AzString = AzString::from_const_str("__azmeet_view");
/// "Drop a video packet" (`AZMEET_TEST_PATTERN=1`).
pub const DROP: AzString = AzString::from_const_str("__azmeet_drop");
pub const LEAVE: AzString = AzString::from_const_str("__azmeet_leave");

// ==== The top bar (every screen) ====

/// The gear at the top right: the settings page azul-appkit shares.
pub const SETTINGS: AzString = AzString::from_const_str("__azmeet_settings");

// ==== The side panel ====

pub const PEOPLE: AzString = AzString::from_const_str("__azmeet_people");
pub const CHAT_MESSAGES: AzString = AzString::from_const_str("__azmeet_chat_messages");
pub const CHAT_FIELD: AzString = AzString::from_const_str("__azmeet_chat_field");
pub const CHAT_SEND: AzString = AzString::from_const_str("__azmeet_chat_send");
pub const STATISTICS: AzString = AzString::from_const_str("__azmeet_statistics");
/// "Copy link" beside the meeting's link (under the side panel; in the waiting room).
pub const COPY_LINK: AzString = AzString::from_const_str("__azmeet_copy_link");

// ==== The start screen ====

pub const SERVER: AzString = AzString::from_const_str("__azmeet_server");
pub const JOIN_FIELD: AzString = AzString::from_const_str("__azmeet_join_field");
pub const NEW_MEETING: AzString = AzString::from_const_str("__azmeet_new");
pub const JOIN: AzString = AzString::from_const_str("__azmeet_join");

// ==== The waiting room ====

/// The waiting room's root.
pub const WAITING: AzString = AzString::from_const_str("__azmeet_waiting");
/// The preview: this side's camera (mirrored) or its initials.
pub const PREVIEW: AzString = AzString::from_const_str("__azmeet_preview");
/// The meeting's code (or its link when it has no code).
pub const MEETING_CODE: AzString = AzString::from_const_str("__azmeet_meeting_code");
/// Who is in the meeting already: their faces and "Ada is in this meeting".
pub const WHO_IS_HERE: AzString = AzString::from_const_str("__azmeet_who_is_here");
/// The name others see (the waiting room and the settings' Meetings).
pub const NAME: AzString = AzString::from_const_str("__azmeet_name");
/// "Join now", or "Start meeting" for a meeting this side just made.
pub const JOIN_NOW: AzString = AzString::from_const_str("__azmeet_join_now");
/// Back to the start screen without joining.
pub const WAITING_BACK: AzString = AzString::from_const_str("__azmeet_waiting_back");

// ==== AzMeet's settings sections (on azul-appkit's settings page) ====

pub const MIRROR: AzString = AzString::from_const_str("__azmeet_mirror");
pub const JOIN_MUTED: AzString = AzString::from_const_str("__azmeet_join_muted");
pub const JOIN_CAMERA_OFF: AzString = AzString::from_const_str("__azmeet_join_camera_off");

// ==== The tiles ====

/// A tile's id is this, then who (`me`, or the name in lower case with every other character an
/// underscore), an underscore and the picture (`camera`, `screen`): `__azmeet_tile_ben_camera`.
pub const TILE_PREFIX: &str = "__azmeet_tile_";
