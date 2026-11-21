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

/// "Connection settings" under a meetings service that cannot be used: Settings > Meetings.
pub const CONNECTION_SETTINGS: AzString = AzString::from_const_str("__azmeet_connection_settings");
pub const JOIN_FIELD: AzString = AzString::from_const_str("__azmeet_join_field");
pub const NEW_MEETING: AzString = AzString::from_const_str("__azmeet_new");
pub const NEW_CHAT_ROOM: AzString = AzString::from_const_str("__azmeet_new_chat_room");
pub const JOIN: AzString = AzString::from_const_str("__azmeet_join");
/// The "Schedule" form: the start, the minutes, the button.
pub const SCHEDULE_START: AzString = AzString::from_const_str("__azmeet_schedule_start");
pub const SCHEDULE_MINUTES: AzString = AzString::from_const_str("__azmeet_schedule_minutes");
pub const SCHEDULE: AzString = AzString::from_const_str("__azmeet_schedule");
/// "Your rooms": the list.
pub const ROOMS: AzString = AzString::from_const_str("__azmeet_rooms");
/// A room of the list is this, then its room id: `__azmeet_room_<room id>`.
pub const ROOM_PREFIX: &str = "__azmeet_room_";

// ==== The room view ====

/// The room view's root.
pub const ROOM_VIEW: AzString = AzString::from_const_str("__azmeet_room_view");
pub const ROOM_BACK: AzString = AzString::from_const_str("__azmeet_room_back");
pub const ROOM_LEAVE: AzString = AzString::from_const_str("__azmeet_room_leave");
pub const ROOM_CALL: AzString = AzString::from_const_str("__azmeet_room_call");
pub const ROOM_COPY: AzString = AzString::from_const_str("__azmeet_room_copy");
/// This device's safety code.
pub const MY_CODE: AzString = AzString::from_const_str("__azmeet_my_code");
/// The members, and the devices knocking.
pub const MEMBERS: AzString = AzString::from_const_str("__azmeet_members");
pub const KNOCKS: AzString = AzString::from_const_str("__azmeet_knocks");
/// "Admit" / "Verified" beside a device are this, then the device id's first 16 characters.
pub const ADMIT_PREFIX: &str = "__azmeet_admit_";
pub const VERIFY_PREFIX: &str = "__azmeet_verify_";

// ==== The waiting room ====

/// The waiting room's root.
pub const WAITING: AzString = AzString::from_const_str("__azmeet_waiting");
/// The preview: this side's camera (mirrored) or its initials.
pub const PREVIEW: AzString = AzString::from_const_str("__azmeet_preview");
/// The meeting's code (or its link when it has no code).
pub const MEETING_CODE: AzString = AzString::from_const_str("__azmeet_meeting_code");
/// When the meeting is (a meeting with times).
pub const MEETING_TIMES: AzString = AzString::from_const_str("__azmeet_meeting_times");
/// Who is in the meeting already: their faces and "Ada is in this meeting".
pub const WHO_IS_HERE: AzString = AzString::from_const_str("__azmeet_who_is_here");
/// The name others see (the waiting room and the settings' Meetings).
pub const NAME: AzString = AzString::from_const_str("__azmeet_name");
/// "Join now", or "Start meeting" for a meeting this side just made.
pub const JOIN_NOW: AzString = AzString::from_const_str("__azmeet_join_now");
/// Back to the start screen without joining.
pub const WAITING_BACK: AzString = AzString::from_const_str("__azmeet_waiting_back");
/// A meeting made here: the line saying no meetings service answers, and its "Retry".
pub const LOCAL_MEETING: AzString = AzString::from_const_str("__azmeet_local_meeting");
pub const SERVICE_RETRY: AzString = AzString::from_const_str("__azmeet_service_retry");

// ==== AzMeet's settings sections (on azul-appkit's settings page) ====

pub const MIRROR: AzString = AzString::from_const_str("__azmeet_mirror");
pub const JOIN_MUTED: AzString = AzString::from_const_str("__azmeet_join_muted");
pub const JOIN_CAMERA_OFF: AzString = AzString::from_const_str("__azmeet_join_camera_off");

// ==== The settings ====

/// The meetings service field (Settings > Meetings).
pub const SERVER: AzString = AzString::from_const_str("__azmeet_server");
/// "Retry" beside a meetings service that does not answer.
pub const RETRY: AzString = AzString::from_const_str("__azmeet_retry");
/// The custom relay's field (Settings > Meetings, with "Custom" picked).
pub const RELAY_FIELD: AzString = AzString::from_const_str("__azmeet_relay_field");

// ==== The sound check (the waiting room, Settings > Audio & Video) ====

pub const TEST_SPEAKERS: AzString = AzString::from_const_str("__azmeet_test_speakers");
pub const TEST_MIC: AzString = AzString::from_const_str("__azmeet_test_mic");

// ==== The tiles ====

/// A tile's id is this, then who (`me`, or the name in lower case with every other character an
/// underscore), an underscore and the picture (`camera`, `screen`): `__azmeet_tile_ben_camera`.
pub const TILE_PREFIX: &str = "__azmeet_tile_";

// ==== Direct calls (`direct_call.rs`) ====

/// Settings > Meetings' "Direct call...".
pub const DIRECT_OPEN: AzString = AzString::from_const_str("__azmeet_direct_open");
/// The direct-call screen's card, its address form switch, the address, Copy, the Call field,
/// Call, Cancel, the line under them, the recent calls and Back.
pub const DIRECT: AzString = AzString::from_const_str("__azmeet_direct");
pub const DIRECT_FORM: AzString = AzString::from_const_str("__azmeet_direct_form");
pub const DIRECT_ADDRESS: AzString = AzString::from_const_str("__azmeet_direct_address");
pub const DIRECT_COPY: AzString = AzString::from_const_str("__azmeet_direct_copy");
pub const DIRECT_FIELD: AzString = AzString::from_const_str("__azmeet_direct_field");
pub const DIRECT_CALL: AzString = AzString::from_const_str("__azmeet_direct_call");
pub const DIRECT_CANCEL: AzString = AzString::from_const_str("__azmeet_direct_cancel");
pub const DIRECT_STATUS: AzString = AzString::from_const_str("__azmeet_direct_status");
pub const DIRECT_RECENTS: AzString = AzString::from_const_str("__azmeet_direct_recents");
/// A recent call's Call button is this and its place in the list: `__azmeet_direct_recent_0`.
pub const DIRECT_RECENT_PREFIX: &str = "__azmeet_direct_recent_";
pub const DIRECT_BACK: AzString = AzString::from_const_str("__azmeet_direct_back");
/// The prompt of a direct call coming in, and its Accept and Decline.
pub const DIRECT_INCOMING: AzString = AzString::from_const_str("__azmeet_direct_incoming");
pub const DIRECT_ACCEPT: AzString = AzString::from_const_str("__azmeet_direct_accept");
pub const DIRECT_DECLINE: AzString = AzString::from_const_str("__azmeet_direct_decline");
