//! What AzMeet's window shows: the start screen, a room's view, the waiting room, the call (on
//! the S10 `CallShell` of azul's app shells) and the settings (azul-appkit's settings page).
//!
//! - **Start screen** (`lobby`): "New meeting", "New chat room", "Schedule", joining with a link
//!   or a code, and "Your rooms" with their unread counts. The meetings service is not shown
//!   here: only a line when it cannot be used, with "Connection settings" (its field is in
//!   Settings > Meetings). Nothing is captured here: no camera, no microphone.
//! - **Room view** (`room_view`), a room outside a call: its encrypted chat, its times and link,
//!   this device's safety code, the members with theirs ("Mark verified"), the devices asking to
//!   join ("Admit"), "Join call", "Leave room" and Back.
//! - **Waiting room** (`waiting_room`), between the start screen and the call, for a meeting just
//!   found or made: this side's camera large (mirrored, as a mirror shows it; the test pattern's
//!   still in a headless run; the initials while the camera is off), the microphone and camera
//!   switches under it with the level meter and the sound check ("Test speakers", "Record a test
//!   clip"), and beside it who is in the meeting already (their
//!   faces and "Ada is in this meeting" / "No one else is here yet"), the meeting's code and link
//!   ("Copy link"), the name others see, the microphone, speaker and camera pickers, and "Join
//!   now" ("Start meeting" for a meeting this side made) or Back. It mounts the one camera widget
//!   and the one microphone widget the call mounts too: no device is opened twice.
//! - **Call**: the tiles (`tiles::arrange`): a shared screen or, in the speaker view, the active
//!   speaker on the shell's stage over a filmstrip, else an even gallery; the side panel shows the
//!   people, the chat or the statistics; the devices slot the invite link and the level meter;
//!   the controls bar: microphone, camera, share, deafen, view, leave.
//! - **Settings**: the gear at the top right of every screen opens azul-appkit's settings page
//!   (the one AzMail opens from File > Options): AzMeet's categories first ([`APP_CATEGORIES`]:
//!   Audio & Video, Meetings, Recording), then the kit's Appearance, Data, Shortcuts and About.
//!   No tabs, no ribbon.
//!
//! The window is `NoTitle`; the title row is azul's `Titlebar`, and the top bar under it holds
//! the notice and the gear. Colours are the system's (`system:text`,
//! `system:window-background`, ...) and the widgets' own, so the window follows the app theme
//! (flat / flora) and the mode (light / dark); only the video tiles keep a dark backdrop in both
//! modes, as every call app does. Everything here is built from a [`CallView`] the app makes from
//! its state (`lib.rs` `snapshot`); the callbacks are the app's.

use azul::{
    audio::AudioConfig,
    callbacks::{
        ButtonOnClickCallbackType, CallbackType, CheckBoxOnToggleCallbackType,
        DropDownOnChoiceChangeCallbackType, SegmentedOnChangeCallbackType,
        TextInputOnFocusLostCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType,
    },
    camera::{CameraConfig, CameraFacing},
    image::{ImageRef, RawImageFormat},
    option::OptionString,
    prelude::*,
    screen::ScreenCaptureConfig,
    shells::{CallShell, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::{DomVec, StringVec, U8VecRef},
    widgets::{
        Avatar, AvatarSize, Badge, Button, ButtonType, CameraWidget, DropDown, LevelMeter,
        MicrophoneWidget, ScreenCaptureWidget, Segmented, Titlebar,
    },
};
use azul_appkit::{
    l10n::{self, and_list, label, t, t_args, Arg},
    ui::{self as kit, AppSection},
};
use azul_pim::initials::initials;

use crate::{ids, tiles::TileKind};

// ==== The view model: what the window shows, made by the app from its state ====

/// Which screen the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiScreen {
    /// The start screen: new meeting, join with a link, the meeting server, "Your rooms".
    Lobby,
    /// A room outside a call: its chat, its members and their safety codes.
    Room,
    /// A meeting found or made, not joined yet: the preview and "Join now".
    Waiting,
    Call,
    Settings,
    /// Settings > Meetings > "Direct call...": this computer's address, the Call field, the
    /// recent calls.
    Direct,
}

/// One tile: a participant's camera or shared screen.
#[derive(Debug, Clone)]
pub(crate) struct TileView {
    pub kind: TileKind,
    /// This side's own camera or screen (a local preview).
    pub me: bool,
    pub name: String,
    /// The marker of the image node a remote stream's pictures go into (`tile_marker`); `None`
    /// while no stream of it arrives (the tile shows the person's initials).
    pub marker: Option<String>,
    pub muted: bool,
    pub speaking: bool,
}

/// One row of the people panel.
#[derive(Debug, Clone)]
pub(crate) struct PersonView {
    pub name: String,
    /// This side (shown as "<name> (you)").
    pub me: bool,
    /// "connected", "connecting", "you" ...
    pub status: String,
    pub muted: bool,
    pub deafened: bool,
    pub speaking: bool,
    /// The safety code of the person's device (CRYPTO.md section 3), when known.
    pub code: Option<String>,
    /// The user compared it and marked the device verified.
    pub verified: bool,
}

/// One message of the chat panel.
#[derive(Debug, Clone)]
pub(crate) struct ChatLine {
    pub name: String,
    pub text: String,
    pub mine: bool,
    /// This side's own message on its way to the meeting server.
    pub sending: bool,
}

/// A room of the start screen's "Your rooms".
#[derive(Debug, Clone)]
pub(crate) struct RoomRow {
    /// The room id: the row's button opens its view.
    pub room: String,
    /// "Chat room xq4-8kd-2nm", "Meeting xq4-8kd-2nm".
    pub title: String,
    /// "3 members · Fri 9 Oct 2026, 16:00-17:00 · starts in 2 h".
    pub detail: String,
    pub unread: usize,
}

/// A member, or a device knocking, as a room's lists show it.
#[derive(Debug, Clone)]
pub(crate) struct MemberRow {
    /// The room of the list (the row's buttons name it).
    pub room: String,
    pub device: String,
    pub name: String,
    /// Its safety code: what two people compare.
    pub code: String,
    pub verified: bool,
    /// This device.
    pub me: bool,
}

/// The room view: a room outside a call.
#[derive(Debug, Clone)]
pub(crate) struct RoomPage {
    /// "Chat room xq4-8kd-2nm".
    pub title: String,
    /// When a meeting is: "Fri 9 Oct 2026, 16:00-17:00 · starts in 2 h".
    pub times: Option<String>,
    /// The link others join with (with its invite secret).
    pub link: String,
    pub copied: bool,
    /// This device's safety code.
    pub my_code: String,
    /// Where this device stands: "End-to-end encrypted: ...", "Waiting for a member ...".
    pub status: String,
    pub members: Vec<MemberRow>,
    pub knocks: Vec<MemberRow>,
    /// This device is a member: "Join call" works.
    pub can_call: bool,
    /// Messages that failed a check, and messages sealed before this device joined.
    pub unreadable: u32,
    pub before_join: u32,
    /// "Room key 3, held by the 2 members now."
    pub key_line: String,
}

/// One section of the statistics panel: a title and its lines.
#[derive(Debug, Clone)]
pub(crate) struct StatSection {
    pub title: String,
    pub lines: Vec<String>,
}

/// What the side panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelView {
    People,
    Chat,
    Statistics,
    Closed,
}

/// The start screen's form.
#[derive(Debug, Clone)]
pub(crate) struct LobbyView {
    pub opening: bool,
    /// The meetings service answered its last check. The start screen says only when it did
    /// not; its address and status are in Settings > Meetings.
    pub server_ok: bool,
    /// The meetings service is being asked for the first time (the window opened first).
    pub server_pending: bool,
    /// No meetings service is set at all.
    pub server_unset: bool,
    pub join_text: String,
    /// The "Schedule" form: the start (local time) and the minutes.
    pub schedule_start: String,
    pub schedule_minutes: String,
    /// The rooms this device is in, the newest first.
    pub rooms: Vec<RoomRow>,
    /// This device's identity is loaded: rooms can be made and joined.
    pub identity_ready: bool,
}

/// The meeting the waiting room is about.
#[derive(Debug, Clone)]
pub(crate) struct WaitingView {
    /// The meeting's code (`abc-defg-hij`); empty when the meeting server gave none.
    pub code: String,
    /// The link others join with.
    pub link: String,
    /// "Copy link" was clicked.
    pub copied: bool,
    /// This side just made the meeting: "Start meeting" rather than "Join now".
    pub created: bool,
    /// Who is in the meeting already (the meeting server's list, read every 2 seconds while
    /// waiting); `None` until the first answer.
    pub people: Option<Vec<String>>,
    /// When the meeting is: "Fri 9 Oct 2026, 16:00-17:00 · starts in 25 min".
    pub times: Option<String>,
    /// Joined with only the code: `Some(false)` "Ask to join", `Some(true)` asked, waiting to be
    /// let in; `None` with the link.
    pub knock: Option<bool>,
    /// The meeting was made here because no meetings service answered: the waiting room says
    /// so and offers "Retry".
    pub local: bool,
}

/// What the settings sections and the device pickers show.
#[derive(Debug, Clone)]
pub(crate) struct SettingsView {
    pub mics: Vec<String>,
    pub mic_choice: usize,
    pub speakers: Vec<String>,
    pub speaker_choice: usize,
    pub cameras: Vec<String>,
    pub camera_choice: usize,
    /// `QUALITY_LABELS` index.
    pub quality: usize,
    /// This side's own picture is mirrored.
    pub mirror: bool,
    /// A meeting is joined with the microphone off, with the camera off.
    pub join_muted: bool,
    pub join_camera_off: bool,
    /// The meetings service in use, or "none set": shown while it cannot be changed.
    pub server: String,
    /// The meetings service field as typed, what its last check said, whether it answered,
    /// whether none is set.
    pub server_text: String,
    pub server_status: String,
    pub server_ok: bool,
    pub server_unset: bool,
    /// The meetings service and the relay may be changed: outside a meeting, the waiting room
    /// and a room view (a new relay binds the iroh endpoint anew, which would cut every peer off).
    pub server_editable: bool,
    /// The relay picked: an index into `rooms::RelayChoice::ALL` ("Custom" also while its
    /// address is still being typed).
    pub relay_choice: usize,
    /// No relay is used this run (`--relay off`, or a meetings service on this computer).
    pub relay_off: bool,
    /// The custom relay field as typed, and why what was typed is no relay address.
    pub relay_text: String,
    pub relay_status: String,
    /// The relay in use in words, shown while it cannot be changed.
    pub relay_shown: String,
    pub name: String,
    /// "Video: H.264 (VideoToolbox)".
    pub codec: String,
    /// Where a meeting's files (its recording, later) go: `<data root>/meet/<meeting>/`, or why
    /// none are kept.
    pub recordings: String,
    /// This device: its safety code and where its key lives.
    pub identity: String,
}

/// Everything the window shows.
#[derive(Debug, Clone)]
pub(crate) struct CallView {
    /// The room view of the open room.
    pub room_page: Option<RoomPage>,
    /// The devices knocking at the call's room (the people panel lets them in).
    pub knocks: Vec<MemberRow>,
    /// Under the chat: "End-to-end encrypted · 2 earlier messages sealed before you joined".
    pub chat_note: String,
    pub screen: UiScreen,
    /// The window title row's text.
    pub title: String,
    /// One line under the title: what the meeting server said, why nothing can be reached.
    pub notice: String,
    pub name: String,
    pub lobby: Option<LobbyView>,
    pub waiting: Option<WaitingView>,
    pub stage: Option<TileView>,
    pub tiles: Vec<TileView>,
    pub panel: PanelView,
    pub people: Vec<PersonView>,
    pub chat: Vec<ChatLine>,
    pub chat_unread: u32,
    pub chat_draft: String,
    pub stats: Vec<StatSection>,
    /// The meeting's link (empty outside a meeting-server room) and whether it was copied.
    pub link: String,
    pub copied: bool,
    pub mic: bool,
    pub cam: bool,
    pub screen_on: bool,
    pub deafened: bool,
    /// The camera is on, but nobody shows it, so nothing is encoded or sent.
    pub cam_culled: bool,
    pub speaker_view: bool,
    /// In a meeting-server room (the Leave button).
    pub in_room: bool,
    /// The microphone is the test tone, so no `MicrophoneWidget` is mounted.
    pub tone_mic: bool,
    /// The camera and the screen share are test patterns, so no capture widget is mounted.
    pub pattern_video: bool,
    /// A still of the test pattern: this side's own camera tile and the waiting room's preview
    /// show it where a camera would show its picture.
    pub pattern_still: Option<ImageRef>,
    /// This side's own camera is shown mirrored.
    pub mirror: bool,
    /// The renditions of the camera and of the screen someone shows: one capture consumer each.
    pub camera_renditions: Vec<u16>,
    pub screen_renditions: Vec<u16>,
    pub mic_level: f32,
    /// Show the "Drop a video packet" button (`AZMEET_TEST_PATTERN=1`).
    pub video_debug: bool,
    /// What the sound check does now, and how the last one ended (empty while none did).
    pub sound_check: SoundCheckState,
    pub sound_check_note: String,
    pub settings: SettingsView,
    /// The direct-call screen.
    pub direct: DirectView,
    /// A direct call coming in: the prompt over every screen.
    pub incoming: Option<IncomingView>,
}

/// The direct-call screen's facts (`direct_call::view`).
#[derive(Debug, Clone, Default)]
pub(crate) struct DirectView {
    /// This computer's address in the form chosen.
    pub address: String,
    /// The bare endpoint id is shown, else the full ticket.
    pub id_only: bool,
    /// "Copy" was clicked for it.
    pub copied: bool,
    /// The Call field as typed.
    pub call_text: String,
    /// A call is being made: Cancel stands where Call stood.
    pub calling: bool,
    /// The line under the Call field: dialing, ringing with the safety words, or how the last
    /// call ended; empty for nothing.
    pub status: String,
    /// The recent direct calls, newest first.
    pub recents: Vec<RecentRow>,
}

/// One recent direct call.
#[derive(Debug, Clone)]
pub(crate) struct RecentRow {
    pub name: String,
    /// What "Call" dials again.
    pub address: String,
    /// When, as people say it.
    pub when: String,
    /// This side called.
    pub outgoing: bool,
}

/// The prompt of a direct call coming in.
#[derive(Debug, Clone)]
pub(crate) struct IncomingView {
    pub name: String,
    pub words: String,
}

/// What the sound check does now, as its buttons say it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SoundCheckState {
    Idle,
    /// Recording the test clip; the button counts the seconds down.
    Recording { seconds_left: u64 },
    PlayingClip,
    PlayingChime,
}

impl SoundCheckState {
    /// The microphone is recorded: it is mounted (and its meter shown) even while muted.
    pub(crate) fn records(self) -> bool {
        matches!(self, SoundCheckState::Recording { .. })
    }
}

/// The app's callbacks the window wires (all live in `lib.rs`).
pub(crate) struct Actions {
    pub mic: ButtonOnClickCallbackType,
    pub cam: ButtonOnClickCallbackType,
    pub share: ButtonOnClickCallbackType,
    pub deafen: ButtonOnClickCallbackType,
    pub view: ButtonOnClickCallbackType,
    pub leave: ButtonOnClickCallbackType,
    pub panel: SegmentedOnChangeCallbackType,
    /// The gear: the settings page.
    pub settings: ButtonOnClickCallbackType,
    pub copy_link: ButtonOnClickCallbackType,
    pub drop_packet: ButtonOnClickCallbackType,
    pub chat_text: TextInputOnTextInputCallbackType,
    pub chat_key: TextInputOnVirtualKeyDownCallbackType,
    pub chat_send: ButtonOnClickCallbackType,
    /// The chat field lost the focus (a click on Send): the draft as the field holds it.
    pub chat_blur: TextInputOnFocusLostCallbackType,
    pub name_text: TextInputOnTextInputCallbackType,
    /// The name field lost the focus: the name is written.
    pub name_blur: TextInputOnFocusLostCallbackType,
    pub server_text: TextInputOnTextInputCallbackType,
    pub server_key: TextInputOnVirtualKeyDownCallbackType,
    pub server_blur: TextInputOnFocusLostCallbackType,
    pub join_text: TextInputOnTextInputCallbackType,
    pub new_meeting: ButtonOnClickCallbackType,
    pub join: ButtonOnClickCallbackType,
    /// The waiting room's "Join now" / "Start meeting", and its Back.
    pub join_now: ButtonOnClickCallbackType,
    pub waiting_back: ButtonOnClickCallbackType,
    /// A meeting made here: ask the meetings service again.
    pub preview_retry: ButtonOnClickCallbackType,
    pub mic_choice: DropDownOnChoiceChangeCallbackType,
    pub speaker_choice: DropDownOnChoiceChangeCallbackType,
    pub camera_choice: DropDownOnChoiceChangeCallbackType,
    pub quality: DropDownOnChoiceChangeCallbackType,
    pub mirror: CheckBoxOnToggleCallbackType,
    pub join_muted: CheckBoxOnToggleCallbackType,
    pub join_camera_off: CheckBoxOnToggleCallbackType,
    /// A key pressed anywhere in the window (the kit's keys, then the call's shortcuts).
    pub key: CallbackType,
    /// The keyring answered (a window event): this device's identity.
    pub keyring: CallbackType,
    /// The start screen: Retry (the meeting server), "New chat room", "Schedule" and its fields.
    pub retry_server: ButtonOnClickCallbackType,
    pub new_chat_room: ButtonOnClickCallbackType,
    pub schedule: ButtonOnClickCallbackType,
    pub schedule_start: TextInputOnTextInputCallbackType,
    pub schedule_minutes: TextInputOnTextInputCallbackType,
    /// A room of "Your rooms" (its data is a `RowClick` with the room id): its view.
    pub open_room: ButtonOnClickCallbackType,
    /// The room view: Back, "Leave room", "Join call", "Copy link".
    pub room_back: ButtonOnClickCallbackType,
    pub room_leave: ButtonOnClickCallbackType,
    pub room_call: ButtonOnClickCallbackType,
    pub room_copy: ButtonOnClickCallbackType,
    /// "Admit" beside a knock, "Verified" beside a member (`RowClick`: `<room> <device>`).
    pub admit: ButtonOnClickCallbackType,
    pub verify: ButtonOnClickCallbackType,
    /// The sound check: "Test speakers" and "Record a test clip" (a click while it records or
    /// plays the clip back stops it).
    pub test_speakers: ButtonOnClickCallbackType,
    pub test_mic: ButtonOnClickCallbackType,
    /// The start screen's "Connection settings": Settings > Meetings.
    pub connection_settings: ButtonOnClickCallbackType,
    /// Settings > Meetings: the relay's choice, and the custom relay's field.
    pub relay_choice: DropDownOnChoiceChangeCallbackType,
    pub relay_text: TextInputOnTextInputCallbackType,
    pub relay_key: TextInputOnVirtualKeyDownCallbackType,
    pub relay_blur: TextInputOnFocusLostCallbackType,
    /// Direct calls: Settings > Meetings' "Direct call...", the screen's Back, the address form
    /// and Copy, the Call field, Call, a recent call's Call (`RowClick`: its address), Cancel,
    /// and the prompt's Accept and Decline.
    pub direct_open: ButtonOnClickCallbackType,
    pub direct_back: ButtonOnClickCallbackType,
    pub direct_form: SegmentedOnChangeCallbackType,
    pub direct_copy: ButtonOnClickCallbackType,
    pub direct_text: TextInputOnTextInputCallbackType,
    pub direct_call: ButtonOnClickCallbackType,
    pub direct_recent: ButtonOnClickCallbackType,
    pub direct_cancel: ButtonOnClickCallbackType,
    pub direct_accept: ButtonOnClickCallbackType,
    pub direct_decline: ButtonOnClickCallbackType,
}

// ==== Styles: structure and the few colours of their own (video tiles are dark in both modes) ====

/// The window's body: the screen fills it.
const BODY: &str = "display: flex; flex-direction: column; height: 100%; margin: 0px; \
                    font-family: sans-serif; color: system:text; background: \
                    system:window-background;";
/// A screen that fills the window: a column.
const PAGE: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;";
/// The top bar under the title row: the notice, then the gear at the right.
const TOP_BAR: &str = "display: flex; flex-direction: row; align-items: center; padding: 2px 8px \
                       2px 12px;";
/// The notice line: the room the gear leaves.
const NOTICE: &str = "flex-grow: 1; min-width: 0px; padding-right: 8px; font-size: 12px; color: \
                      system:secondary-text;";
/// A tile's box: the video (or the initials) fills it, the name label sits on it.
const TILE: &str = "position: relative; display: flex; flex-direction: column; align-items: \
                    center; justify-content: center; width: 100%; border-radius: 8px; overflow: \
                    hidden; background: #1e1f24; color: #e8e8ee; @theme(flora) { border-radius: \
                    5px; }";
/// A gallery or filmstrip tile (and the waiting room's preview) is 16:9 at its cell's width.
const TILE_IN_ROW: &str = "aspect-ratio: 16 / 9;";
/// The stage's tile takes the stage's room.
const TILE_ON_STAGE: &str = "flex-grow: 1; min-height: 0px;";
/// The ring of the active speaker's tile.
const SPEAKING_RING: &str = "border: 2px solid system:accent;";
/// The video in a tile: it fills the tile's column.
const VIDEO: &str = "flex-grow: 1; width: 100%; min-height: 0px;";
/// This side's own camera, turned as a mirror shows it (what the others get never is).
const MIRRORED: &str = "transform: scaleX(-1);";
/// The name label on a tile.
const TILE_LABEL: &str = "position: absolute; left: 8px; bottom: 8px; padding: 2px 8px; \
                          border-radius: 4px; background: rgba(0, 0, 0, 0.55); color: #ffffff; \
                          font-size: 12px; white-space: nowrap; @theme(flora) { border-radius: \
                          3px; }";
/// A side-panel column that scrolls.
const PANEL_SCROLL: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: \
                            0px; overflow-y: auto; padding: 8px;";
/// A secondary line of text.
const SECONDARY: &str = "font-size: 12px; color: system:secondary-text;";
/// A section title in a side panel or a column.
const SECTION_TITLE: &str = "font-size: 12px; font-weight: bold; color: \
                             system:secondary-text; margin: 10px 0px 4px 0px;";
/// One line of the statistics: wraps, never widens the panel.
const STAT_LINE: &str = "font-size: 12px; padding: 1px 0px; min-width: 0px;";
/// A row of controls.
const ROW: &str = "display: flex; flex-direction: row; align-items: center; min-width: 0px;";
/// A field that fills its row from a zero basis, so its width never follows what is typed.
const ROW_FIELD: &str = "flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-width: 0px; \
                         margin-right: 6px;";
/// The start screen's card, beside the device preview (under it in a narrow window).
const CARD: &str = "display: flex; flex-direction: column; flex-grow: 0; flex-shrink: 1; \
                    flex-basis: 380px; min-width: 280px; max-width: 440px; margin: 0px 12px;";
/// The waiting room's body: the preview and the join column side by side, one over the other in
/// a narrow window.
const WAITING_BODY: &str = "display: flex; flex-direction: row; flex-wrap: wrap; justify-content: \
                            center; align-items: flex-start; flex-grow: 1; min-height: 0px; \
                            overflow-y: auto; padding: 24px 16px;";
/// The preview's column: as wide as the window lets it be, up to 760 px.
const PREVIEW_COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; flex-shrink: \
                              1; flex-basis: 420px; min-width: 280px; max-width: 760px; margin: \
                              0px 12px 16px 12px;";
/// The join column beside the preview.
const JOIN_COLUMN: &str = "display: flex; flex-direction: column; flex-shrink: 0; width: 320px; \
                           margin: 0px 12px;";
/// The switches under the preview.
const SWITCHES: &str = "display: flex; flex-direction: row; align-items: center; justify-content: \
                        center; padding: 12px 0px 4px 0px;";
/// A button as wide as its column.
const WIDE: &str = "width: 100%; margin-top: 8px;";

/// The window: the screen the view asks for, inside the app theme's scope (its ground, ink,
/// font and accent). `kit_ref` is the participant's azul-appkit kit (the settings page).
pub(crate) fn meet_view(
    view: &CallView,
    data: &RefAny,
    kit_ref: &RefAny,
    actions: &Actions,
) -> Dom {
    let content = match view.screen {
        UiScreen::Lobby => lobby(view, data, actions),
        UiScreen::Room => room_view(view, data, actions),
        UiScreen::Waiting => waiting_room(view, data, actions),
        UiScreen::Call => call(view, data, actions),
        UiScreen::Settings => settings(view, data, kit_ref, actions),
        UiScreen::Direct => direct_screen(view, data, actions),
    };
    // A direct call coming in rings over whatever shows.
    let content = match &view.incoming {
        Some(call) => Dom::create_div()
            .with_css(PAGE)
            .with_child(incoming_prompt(call, data, actions))
            .with_child(content),
        None => content,
    };
    Dom::create_body()
        .with_css(BODY)
        .with_child(ShellThemeScope::create(content).with_accent(ShellThemeAccent::Blue).dom())
        // This device's identity comes from the keyring: its answer is a window event.
        .with_callback(
            EventFilter::Window(WindowEventFilter::KeyringResult),
            data.clone(),
            actions.keyring,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            data.clone(),
            actions.key,
        )
}

/// A button's data for a row (a room of the list, a member, a knock): the app and the row's key.
fn row_data(data: &RefAny, key: String) -> RefAny {
    RefAny::new(crate::RowClick {
        app: data.clone(),
        key,
    })
}

/// A DOM id of `prefix` and the first `len` characters of `key` (a room id, a device id).
fn keyed_id(prefix: &str, key: &str, len: usize) -> AzString {
    let tail: String = key.chars().take(len).collect();
    AzString::from(format!("{prefix}{tail}").as_str())
}

/// How many characters of a device id its buttons' ids carry.
const DEVICE_ID_CHARS: usize = 16;
/// A room id's characters (all of them: a room's button id is `__azmeet_room_<room id>`).
const ROOM_ID_CHARS: usize = crate::crypto::ID_LEN;

/// The title row (the window is `NoTitle`) over the top bar: the notice, and at the right the
/// gear that opens the settings.
fn header(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(title_row(view))
        .with_child(
            Dom::create_div()
                .with_css(TOP_BAR)
                .with_child(text(&view.notice, NOTICE))
                .with_child(gear(data, actions)),
        )
}

/// The window's title row: azul's `Titlebar`.
fn title_row(view: &CallView) -> Dom {
    Titlebar::create(AzString::from(view.title.as_str()))
        .without_border_bottom()
        .dom()
}

/// The gear: azul-appkit's settings page (Mod+, opens it too).
fn gear(data: &RefAny, actions: &Actions) -> Dom {
    Button::create("")
        .with_icon("settings")
        .with_on_click(data.clone(), actions.settings)
        .dom()
        .with_id(ids::SETTINGS)
        .with_accessibility_name(label("azmeet-settings"))
        .with_css("flex-shrink: 0;")
}

/// A `<span>` of `s` in `css`.
fn text(s: &str, css: &str) -> Dom {
    Dom::create_span_with_text(s).with_css(css)
}

/// A strings vector.
fn strings(items: &[String]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| AzString::from(s.as_str())).collect())
}

// ==== The call: the shell with the arranged tiles ====

/// The call: the shell with the arranged tiles (the stage over the filmstrip, or the gallery),
/// the side panel, the devices slot and the controls bar.
fn call(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let tiles: Vec<Dom> = view
        .tiles
        .iter()
        .map(|t| tile(view, t, data, false))
        .collect();
    let mut shell = CallShell::create(DomVec::from_vec(tiles), controls(view, data, actions))
        .with_header(header(view, data, actions))
        .with_devices(devices(view, data, actions));
    if let Some(stage) = &view.stage {
        shell = shell.with_stage(tile(view, stage, data, true));
    }
    if let Some(panel) = side_panel(view, data, actions) {
        shell = shell.with_side_panel(panel);
    }
    shell.dom()
}

/// One tile: the picture (a remote stream's image node, this side's own capture, or the
/// person's initials), the name label on it, and the speaking ring. On the stage it takes the
/// stage's room; in a row it is 16:9 at its cell's width.
fn tile(view: &CallView, t: &TileView, data: &RefAny, on_stage: bool) -> Dom {
    let size = if on_stage { TILE_ON_STAGE } else { TILE_IN_ROW };
    tile_box(view, t, data, size, on_stage)
}

/// A tile of `size` (its sizing CSS); `large`: the initials are drawn large (the stage, the
/// waiting room's preview).
fn tile_box(view: &CallView, t: &TileView, data: &RefAny, size: &str, large: bool) -> Dom {
    let picture = if t.me {
        own_picture(view, t, data, large)
    } else {
        remote_picture(t)
    };
    let mut css = String::from(TILE);
    css.push_str(size);
    if t.speaking {
        css.push_str(SPEAKING_RING);
    }
    Dom::create_div()
        .with_css(css.as_str())
        .with_id(AzString::from(tile_id(t).as_str()))
        .with_child(picture)
        .with_child(text(&tile_label(t), TILE_LABEL))
}

/// A tile's DOM id, for scripts: `__azmeet_tile_me_camera`, `__azmeet_tile_ben_screen` (the name
/// in lower case, every other character an underscore; [`ids::TILE_PREFIX`]).
pub(crate) fn tile_id(t: &TileView) -> String {
    let who = if t.me {
        String::from("me")
    } else {
        t.name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect()
    };
    let kind = match t.kind {
        TileKind::Camera => "camera",
        TileKind::Screen => "screen",
    };
    format!("{}{who}_{kind}", ids::TILE_PREFIX)
}

/// Another person's name as the window shows it: as they gave it, the nameless one's
/// ([`crate::chatroom::GUEST`]) in the window's language.
pub(crate) fn person_name(name: &str) -> String {
    if name == crate::chatroom::GUEST {
        t("azmeet-guest")
    } else {
        name.to_string()
    }
}

/// A person's name as the window shows it: this side's own with "(you)" after it.
pub(crate) fn shown_name(name: &str, me: bool) -> String {
    let name = person_name(name);
    if me {
        t_args("azmeet-you-name", &[("name", Arg::from(name))])
    } else {
        name
    }
}

/// "Ada", "Ada · muted", "Ada's screen", "Your screen".
fn tile_label(t: &TileView) -> String {
    let mut label = match t.kind {
        TileKind::Camera => shown_name(&t.name, t.me),
        TileKind::Screen if t.me => l10n::t("azmeet-your-screen"),
        TileKind::Screen => t_args("azmeet-their-screen", &[("name", Arg::from(person_name(&t.name)))]),
    };
    if t.muted && t.kind == TileKind::Camera {
        label.push_str(" · ");
        label.push_str(&l10n::t("azmeet-tile-muted"));
    }
    label
}

/// The initials of `name` in an avatar, large or of the default size.
fn avatar(name: &str, large: bool) -> Dom {
    let avatar = Avatar::create(AzString::from(initials(name).as_str()));
    if large {
        avatar.with_size(AvatarSize::Large).dom()
    } else {
        avatar.dom()
    }
}

/// A remote stream's picture: the image node its frames go into (found by its marker), or the
/// person's initials while nothing arrives.
fn remote_picture(t: &TileView) -> Dom {
    match &t.marker {
        Some(marker) => Dom::create_image(ImageRef::null_image(
            2,
            2,
            RawImageFormat::RGBA8,
            U8VecRef::from(&[][..]),
        ))
        .with_marker(OptionString::Some(AzString::from(marker.as_str())))
        .with_css(VIDEO),
        None => avatar(&t.name, false),
    }
}

/// This side's own camera or screen: the capture widget (whose consumers cut every rendition
/// someone shows - nothing is captured for nobody; the camera mirrored when the settings say
/// so), the test pattern's still (mirrored the same way), or the initials and "Camera is off".
fn own_picture(view: &CallView, t: &TileView, data: &RefAny, large: bool) -> Dom {
    match t.kind {
        TileKind::Camera if view.cam && !view.pattern_video => {
            let mut camera = CameraWidget::create(CameraConfig {
                facing: match view.settings.camera_choice {
                    1 => CameraFacing::Back,
                    2 => CameraFacing::External,
                    _ => CameraFacing::Front,
                },
                output_format: crate::VIDEO_FORMAT,
                ..CameraConfig::default()
            });
            for height in &view.camera_renditions {
                camera = camera.with_consumer(crate::feed_consumer(crate::CAMERA_TRACK, *height));
            }
            camera
                .with_on_consumer_frame(data.clone(), crate::send_feed_frame)
                .dom()
                .with_css(own_video_css(view.mirror).as_str())
        }
        TileKind::Camera if view.cam && view.cam_culled => {
            text(&l10n::t("azmeet-pattern-unseen"), SECONDARY)
        }
        TileKind::Camera if view.cam => match &view.pattern_still {
            Some(still) => Dom::create_image(still.clone())
                .with_css(own_video_css(view.mirror).as_str())
                .with_accessibility_name(label("azmeet-pattern")),
            None => text(&l10n::t("azmeet-pattern"), SECONDARY),
        },
        TileKind::Camera => Dom::create_div()
            .with_css("display: flex; flex-direction: column; align-items: center;")
            .with_child(avatar(&t.name, large))
            .with_child(text(&l10n::t("azmeet-camera-is-off"), "font-size: 12px; margin-top: 6px;")),
        TileKind::Screen if view.pattern_video => text(&l10n::t("azmeet-screen-pattern"), SECONDARY),
        TileKind::Screen => {
            let mut screen = ScreenCaptureWidget::create(ScreenCaptureConfig {
                output_format: crate::VIDEO_FORMAT,
                ..ScreenCaptureConfig::default()
            });
            for height in &view.screen_renditions {
                screen = screen.with_consumer(crate::feed_consumer(crate::SCREEN_TRACK, *height));
            }
            screen
                .with_on_consumer_frame(data.clone(), crate::send_feed_frame)
                .dom()
                .with_css(VIDEO)
        }
    }
}

/// This side's own video in its tile: it fills the tile, turned as a mirror shows it when the
/// settings say so (what the others get never is).
fn own_video_css(mirror: bool) -> String {
    let mut css = String::from(VIDEO);
    if mirror {
        css.push(' ');
        css.push_str(MIRRORED);
    }
    css
}

// ==== The side panel: people, chat, statistics ====

/// The side panel's tabs, in `PanelView` order.
const PANEL_TABS: [PanelView; 3] = [PanelView::People, PanelView::Chat, PanelView::Statistics];

/// The side panel: tabs over the people, the chat or the statistics; `None` when closed.
fn side_panel(view: &CallView, data: &RefAny, actions: &Actions) -> Option<Dom> {
    let selected = PANEL_TABS.iter().position(|p| *p == view.panel)?;
    let chat_tab = if view.chat_unread > 0 {
        t_args("azmeet-tab-chat-unread", &[("count", Arg::from(view.chat_unread.to_string()))])
    } else {
        t("azmeet-tab-chat")
    };
    let tabs = Segmented::create(strings(&[
        t("azmeet-tab-people"),
        chat_tab,
        t("azmeet-tab-statistics"),
    ]))
    .with_selected_index(selected)
    .with_on_change(data.clone(), actions.panel)
    .dom();
    let body = match view.panel {
        PanelView::People => people(view, data, actions),
        PanelView::Chat => chat(view, data, actions),
        PanelView::Statistics | PanelView::Closed => statistics(view),
    };
    Some(
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
            .with_child(
                Dom::create_div()
                    .with_css("padding: 8px 8px 0px 8px;")
                    .with_child(tabs),
            )
            .with_child(body),
    )
}

/// A person's row: the initials, the name and what lies under it.
const PERSON_ROW: &str =
    "display: flex; flex-direction: row; align-items: center; padding: 4px 0px;";
/// The name column of a person's row.
const PERSON_TEXT: &str = "display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; \
                           margin-left: 8px;";

/// "Safety code 05881 39114 50072 66310": what two people compare (CRYPTO.md section 3).
pub(crate) fn safety_line(code: &str) -> String {
    t_args("azmeet-safety-line", &[("code", Arg::from(code))])
}

/// Everyone in the call (initials, name, what they do, their safety code), then the devices
/// asking to join, each with "Admit".
fn people(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let mut list = Dom::create_div()
        .with_css(PANEL_SCROLL)
        .with_id(ids::PEOPLE);
    for person in &view.people {
        let mut column = Dom::create_div()
            .with_css(PERSON_TEXT)
            .with_child(text(&shown_name(&person.name, person.me), "font-size: 13px;"))
            .with_child(text(&person.status, SECONDARY));
        if let Some(code) = &person.code {
            column = column.with_child(text(&safety_line(code), SECONDARY));
        }
        let mut row = Dom::create_div()
            .with_css(PERSON_ROW)
            .with_child(
                Avatar::create(AzString::from(initials(&person.name).as_str()))
                    .with_size(AvatarSize::Small)
                    .dom(),
            )
            .with_child(column);
        if person.verified {
            row = row.with_child(Badge::create(label("azmeet-badge-verified")).dom());
        }
        if person.speaking {
            row = row.with_child(Badge::create(label("azmeet-badge-speaking")).dom());
        }
        if person.muted {
            row = row.with_child(Badge::create(label("azmeet-tile-muted")).dom());
        }
        if person.deafened {
            row = row.with_child(Badge::create(label("azmeet-badge-deafened")).dom());
        }
        list = list.with_child(row);
    }
    if !view.knocks.is_empty() {
        list = list.with_child(text(&t("azmeet-asking-to-join"), SECTION_TITLE));
        for knock in &view.knocks {
            list = list.with_child(knock_row(knock, data, actions));
        }
    }
    list
}

/// A device asking to join: its name, its safety code (compare it before letting it in) and
/// "Admit".
fn knock_row(k: &MemberRow, data: &RefAny, actions: &Actions) -> Dom {
    Dom::create_div()
        .with_css(PERSON_ROW)
        .with_child(
            Avatar::create(AzString::from(initials(&k.name).as_str()))
                .with_size(AvatarSize::Small)
                .dom(),
        )
        .with_child(
            Dom::create_div()
                .with_css(PERSON_TEXT)
                .with_child(text(&person_name(&k.name), "font-size: 13px;"))
                .with_child(text(&safety_line(&k.code), SECONDARY)),
        )
        .with_child(
            Button::with_type(label("azmeet-admit"), ButtonType::Primary)
                .with_on_click(row_data(data, format!("{} {}", k.room, k.device)), actions.admit)
                .dom()
                .with_id(keyed_id(ids::ADMIT_PREFIX, &k.device, DEVICE_ID_CHARS))
                .with_css("flex-shrink: 0;"),
        )
}

/// A member of the room view's list: its name, its safety code, and "verified" or the button
/// that marks it so once the user compared the code.
fn member_row(m: &MemberRow, data: &RefAny, actions: &Actions) -> Dom {
    let mut row = Dom::create_div()
        .with_css(PERSON_ROW)
        .with_child(
            Avatar::create(AzString::from(initials(&m.name).as_str()))
                .with_size(AvatarSize::Small)
                .dom(),
        )
        .with_child(
            Dom::create_div()
                .with_css(PERSON_TEXT)
                .with_child(text(&shown_name(&m.name, m.me), "font-size: 13px;"))
                .with_child(text(&safety_line(&m.code), SECONDARY)),
        );
    if m.me {
        return row;
    }
    row = if m.verified {
        row.with_child(Badge::create(label("azmeet-badge-verified")).dom())
    } else {
        row.with_child(
            Button::create(label("azmeet-mark-verified"))
                .with_on_click(row_data(data, format!("{} {}", m.room, m.device)), actions.verify)
                .dom()
                .with_id(keyed_id(ids::VERIFY_PREFIX, &m.device, DEVICE_ID_CHARS))
                .with_css("flex-shrink: 0;"),
        )
    };
    row
}

/// The chat: the messages, oldest first, over the field and its Send button.
fn chat(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let mut messages = Dom::create_div()
        .with_css(PANEL_SCROLL)
        .with_id(ids::CHAT_MESSAGES);
    if !view.chat_note.is_empty() {
        messages = messages.with_child(text(&view.chat_note, SECONDARY));
    }
    if view.chat.is_empty() {
        messages = messages.with_child(text(&t("azmeet-no-messages"), SECONDARY));
    }
    for line in &view.chat {
        let who = match (line.mine, line.sending) {
            (true, true) => t("azmeet-you-sending"),
            (true, false) => t("azmeet-you"),
            (false, _) => person_name(&line.name),
        };
        messages = messages.with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; padding: 3px 0px;")
                .with_child(text(&who, "font-size: 12px; font-weight: bold;"))
                .with_child(text(&line.text, "font-size: 13px;")),
        );
    }
    let field = TextInput::create()
        .with_text(view.chat_draft.as_str())
        .with_placeholder(label("azmeet-message-everyone"))
        .with_on_text_input(data.clone(), actions.chat_text)
        .with_on_virtual_key_down(data.clone(), actions.chat_key)
        .with_on_focus_lost(data.clone(), actions.chat_blur)
        .dom()
        .with_css(ROW_FIELD)
        .with_id(ids::CHAT_FIELD);
    let send = Button::with_type(label("azmeet-send"), ButtonType::Primary)
        .with_on_click(data.clone(), actions.chat_send)
        .dom()
        .with_id(ids::CHAT_SEND);
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(messages)
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; padding: 8px;")
                .with_child(field)
                .with_child(send),
        )
}

/// The statistics: one titled section per subject, every line wrapping inside the panel.
fn statistics(view: &CallView) -> Dom {
    let mut panel = Dom::create_div()
        .with_css(PANEL_SCROLL)
        .with_id(ids::STATISTICS);
    for section in &view.stats {
        panel = panel.with_child(text(&section.title, SECTION_TITLE));
        if section.lines.is_empty() {
            panel = panel.with_child(text(&t("azmeet-nothing-yet"), SECONDARY));
        }
        for line in &section.lines {
            panel = panel.with_child(text(line, STAT_LINE));
        }
    }
    panel
}

/// The devices slot under the side panel: the microphone's level (and the hidden microphone
/// widget that captures it) and the invite link.
fn devices(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let mut column =
        Dom::create_div().with_css("display: flex; flex-direction: column; padding: 8px;");
    if view.mic && !view.tone_mic {
        column = column.with_child(microphone(data));
    }
    if view.mic {
        column = column.with_child(level_meter(view, data));
    }
    if !view.link.is_empty() {
        column = column.with_child(link_row(&view.link, view.copied, data, actions));
    }
    column
}

/// A meeting's link and "Copy link": the link gives way (an ellipsis), the button keeps its one
/// line.
fn link_row(link: &str, copied: bool, data: &RefAny, actions: &Actions) -> Dom {
    link_row_with(link, copied, data, actions.copy_link, ids::COPY_LINK)
}

/// [`link_row`] with its button's callback and id (the room view copies its room's link).
fn link_row_with(
    link: &str,
    copied: bool,
    data: &RefAny,
    action: ButtonOnClickCallbackType,
    id: AzString,
) -> Dom {
    Dom::create_div()
        .with_css(ROW)
        .with_child(text(
            link,
            "font-size: 12px; flex-grow: 1; min-width: 0px; overflow: hidden; white-space: \
             nowrap; text-overflow: ellipsis;",
        ))
        .with_child(
            Button::create(label(if copied { "azmeet-copied" } else { "azmeet-copy-link" }))
                .with_on_click(data.clone(), action)
                .dom()
                .with_id(id)
                .with_css("flex-shrink: 0;"),
        )
}

/// The room view's body: the chat beside the room's column, one over the other in a narrow
/// window.
const ROOM_BODY: &str = "display: flex; flex-direction: row; flex-wrap: wrap; align-items: \
                         flex-start; flex-grow: 1; min-height: 0px; overflow-y: auto; padding: \
                         16px;";
/// The room view's chat column.
const ROOM_CHAT: &str = "display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 1; \
                         flex-basis: 420px; min-width: 280px; height: 480px; margin: 0px 12px \
                         16px 0px; border-radius: 8px; background: system:control-background; \
                         @theme(flora) { border-radius: 5px; }";

/// The room view: a room outside a call - its chat; beside it the room's title, times and link,
/// this device's safety code, the members with theirs ("Mark verified"), the devices asking to
/// join ("Admit"), and "Join call", "Leave room", Back.
fn room_view(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let Some(page) = &view.room_page else {
        return lobby(view, data, actions);
    };
    let mut facts = Dom::create_div()
        .with_css(JOIN_COLUMN)
        .with_child(text(&page.title, "font-size: 20px; margin-bottom: 6px;"))
        .with_child(text(&page.status, SECONDARY));
    if let Some(times) = &page.times {
        facts = facts.with_child(
            text(times, "font-size: 13px; margin: 6px 0px;").with_id(ids::MEETING_TIMES),
        );
    }
    facts = facts
        .with_child(link_row_with(
            &page.link,
            page.copied,
            data,
            actions.room_copy,
            ids::ROOM_COPY,
        ))
        .with_child(text(&t("azmeet-your-safety-code"), SECTION_TITLE))
        .with_child(
            text(&page.my_code, "font-size: 15px; font-weight: bold;").with_id(ids::MY_CODE),
        )
        .with_child(text(&t("azmeet-compare-codes"), SECONDARY))
        .with_child(text(&page.key_line, SECONDARY));
    if page.before_join > 0 || page.unreadable > 0 {
        facts = facts.with_child(text(
            &t_args(
                "azmeet-sealed-failed",
                &[
                    ("before", Arg::from(page.before_join.to_string())),
                    ("failed", Arg::from(page.unreadable.to_string())),
                ],
            ),
            SECONDARY,
        ));
    }
    let mut members = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_id(ids::MEMBERS)
        .with_child(text(
            &t_args("azmeet-members-count", &[("count", Arg::from(page.members.len().to_string()))]),
            SECTION_TITLE,
        ));
    for m in &page.members {
        members = members.with_child(member_row(m, data, actions));
    }
    facts = facts.with_child(members);
    if !page.knocks.is_empty() {
        let mut knocks = Dom::create_div()
            .with_css("display: flex; flex-direction: column;")
            .with_id(ids::KNOCKS)
            .with_child(text(&t("azmeet-asking-to-join"), SECTION_TITLE));
        for k in &page.knocks {
            knocks = knocks.with_child(knock_row(k, data, actions));
        }
        facts = facts.with_child(knocks);
    }
    if page.can_call {
        facts = facts.with_child(
            Button::with_type(label("azmeet-join-call"), ButtonType::Primary)
                .with_on_click(data.clone(), actions.room_call)
                .dom()
                .with_id(ids::ROOM_CALL)
                .with_css(WIDE),
        );
    }
    facts = facts
        .with_child(
            Button::with_type(label("azmeet-leave-room"), ButtonType::Danger)
                .with_on_click(data.clone(), actions.room_leave)
                .dom()
                .with_id(ids::ROOM_LEAVE)
                .with_css(WIDE),
        )
        .with_child(
            Button::create(label("azmeet-back"))
                .with_on_click(data.clone(), actions.room_back)
                .dom()
                .with_id(ids::ROOM_BACK)
                .with_css(WIDE),
        );
    Dom::create_div()
        .with_css(PAGE)
        .with_id(ids::ROOM_VIEW)
        .with_child(header(view, data, actions))
        .with_child(
            Dom::create_div()
                .with_css(ROOM_BODY)
                .with_child(
                    Dom::create_div()
                        .with_css(ROOM_CHAT)
                        .with_child(chat(view, data, actions)),
                )
                .with_child(facts),
        )
}

/// The microphone: a capture widget with no picture, 1 px.
fn microphone(data: &RefAny) -> Dom {
    MicrophoneWidget::create(AudioConfig {
        sample_rate: crate::MIC_RATE,
        channels: 1,
    })
    .with_on_frame(data.clone(), crate::mic_on_frame)
    .dom()
    .with_css("width: 1px; height: 1px; overflow: hidden;")
}

/// "Mic level" and the bar that moves with it (ten times a second at most).
fn level_meter(view: &CallView, data: &RefAny) -> Dom {
    Dom::create_div()
        .with_css(ROW)
        .with_child(text(
            &t("azmeet-mic-level"),
            "font-size: 12px; margin-right: 8px; white-space: nowrap;",
        ))
        .with_child(
            LevelMeter::create(view.mic_level)
                .with_accessibility_name(label("azmeet-microphone-level"))
                .dom()
                .with_css("flex-grow: 1; min-width: 40px;")
                .with_callback(
                    EventFilter::Component(ComponentEventFilter::AfterMount),
                    data.clone(),
                    crate::meter_mounted,
                )
                .with_callback(
                    EventFilter::Component(ComponentEventFilter::BeforeUnmount),
                    data.clone(),
                    crate::meter_unmounted,
                ),
        )
}

// ==== The controls bar ====

/// A control: a button with an id (the E2E finds it), primary while `on`.
fn control(label: &str, id: AzString, on: bool, data: &RefAny, action: ButtonOnClickCallbackType) -> Dom {
    let kind = if on {
        ButtonType::Primary
    } else {
        ButtonType::Default
    };
    Button::with_type(label, kind)
        .with_on_click(data.clone(), action)
        .dom()
        .with_id(id)
        .with_css("margin: 0px 4px; flex-shrink: 0;")
}

/// The microphone switch's label: what a click does.
pub(crate) fn mic_label(on: bool) -> String {
    t(if on { "azmeet-mute" } else { "azmeet-unmute" })
}

/// The camera switch's label: what a click does (and that nobody sees a camera that is on).
pub(crate) fn cam_label(on: bool, culled: bool) -> String {
    t(match (on, culled) {
        (true, true) => "azmeet-stop-video-unseen",
        (true, false) => "azmeet-stop-video",
        (false, _) => "azmeet-start-video",
    })
}

/// A device switch: the device's icon and what a click does, red while the device is off - the
/// look every call app has.
fn device_switch(
    label: &str,
    icon: &str,
    id: AzString,
    on: bool,
    data: &RefAny,
    action: ButtonOnClickCallbackType,
) -> Dom {
    let kind = if on {
        ButtonType::Default
    } else {
        ButtonType::Danger
    };
    Button::with_type(label, kind)
        .with_icon(icon)
        .with_on_click(data.clone(), action)
        .dom()
        .with_id(id)
        .with_css("margin: 0px 4px; flex-shrink: 0;")
}

/// The microphone switch (the waiting room and the call).
fn mic_switch(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let icon = if view.mic { "mic" } else { "mic_off" };
    device_switch(&mic_label(view.mic), icon, ids::MIC, view.mic, data, actions.mic)
}

/// The camera switch (the waiting room and the call).
fn cam_switch(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let icon = if view.cam { "videocam" } else { "videocam_off" };
    let said = cam_label(view.cam, view.cam_culled);
    device_switch(&said, icon, ids::CAM, view.cam, data, actions.cam)
}

/// The call's controls bar: the microphone and the camera, sharing, deafening, the view, and
/// Leave (the settings are the gear at the top right).
fn controls(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let mut row = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; flex-wrap: wrap; align-items: center; \
             justify-content: center; padding: 8px;",
        )
        .with_child(mic_switch(view, data, actions))
        .with_child(cam_switch(view, data, actions))
        .with_child(control(
            &t(if view.screen_on {
                "azmeet-stop-sharing"
            } else {
                "azmeet-share-screen"
            }),
            ids::SHARE,
            view.screen_on,
            data,
            actions.share,
        ))
        .with_child(control(
            &t(if view.deafened { "azmeet-undeafen" } else { "azmeet-deafen" }),
            ids::DEAFEN,
            view.deafened,
            data,
            actions.deafen,
        ))
        .with_child(control(
            &t(if view.speaker_view {
                "azmeet-gallery-view"
            } else {
                "azmeet-speaker-view"
            }),
            ids::VIEW,
            false,
            data,
            actions.view,
        ));
    if view.video_debug {
        row = row.with_child(control(
            &t("azmeet-drop-packet"),
            ids::DROP,
            false,
            data,
            actions.drop_packet,
        ));
    }
    if view.in_room {
        row = row.with_child(
            Button::with_type(label("azmeet-leave"), ButtonType::Danger)
                .with_on_click(data.clone(), actions.leave)
                .dom()
                .with_id(ids::LEAVE)
                .with_css("margin: 0px 4px 0px 16px; flex-shrink: 0;"),
        );
    }
    row
}

// ==== The start screen ====

/// The start screen: "New meeting", "New chat room", "Schedule", joining with a link or a code,
/// the meeting server (and whether it answers, with Retry when it does not), and "Your rooms",
/// beside this side's camera and microphone ([`device_preview`]), live from the start.
fn lobby(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let mut card = Dom::create_div()
        .with_css(CARD)
        .with_child(text(&t("azmeet-lobby-title"), "font-size: 24px; margin-bottom: 4px;"))
        .with_child(text(
            &t("azmeet-lobby-intro"),
            "font-size: 13px; color: system:secondary-text; margin-bottom: 20px;",
        ));
    if let Some(lobby) = &view.lobby {
        let join_field = TextInput::create()
            .with_text(lobby.join_text.as_str())
            .with_placeholder(label("azmeet-join-placeholder"))
            .with_on_text_input(data.clone(), actions.join_text)
            .dom()
            .with_css(ROW_FIELD)
            .with_id(ids::JOIN_FIELD);
        let new_meeting = Button::with_type(
            label(if lobby.opening {
                "azmeet-please-wait"
            } else {
                "azmeet-new-meeting"
            }),
            ButtonType::Primary,
        )
        .with_on_click(data.clone(), actions.new_meeting)
        .dom()
        .with_id(ids::NEW_MEETING)
        .with_css("margin-right: 8px;");
        let new_chat_room = Button::create(label("azmeet-new-chat-room"))
            .with_on_click(data.clone(), actions.new_chat_room)
            .dom()
            .with_id(ids::NEW_CHAT_ROOM);
        let schedule = Dom::create_div()
            .with_css(ROW)
            .with_child(
                TextInput::create()
                    .with_text(lobby.schedule_start.as_str())
                    .with_placeholder("2026-10-09 14:00")
                    .with_on_text_input(data.clone(), actions.schedule_start)
                    .dom()
                    .with_css(ROW_FIELD)
                    .with_id(ids::SCHEDULE_START),
            )
            .with_child(
                TextInput::create()
                    .with_text(lobby.schedule_minutes.as_str())
                    .with_placeholder(label("azmeet-minutes"))
                    .with_on_text_input(data.clone(), actions.schedule_minutes)
                    .dom()
                    .with_css("width: 80px; flex-shrink: 0; margin-right: 6px;")
                    .with_id(ids::SCHEDULE_MINUTES),
            )
            .with_child(
                Button::create(label("azmeet-schedule"))
                    .with_on_click(data.clone(), actions.schedule)
                    .dom()
                    .with_id(ids::SCHEDULE)
                    .with_css("flex-shrink: 0;"),
            );
        card = card
            .with_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; margin-bottom: 20px;")
                    .with_child(new_meeting)
                    .with_child(new_chat_room),
            )
            .with_child(labelled(
                &t("azmeet-join-with"),
                Dom::create_div()
                    .with_css(ROW)
                    .with_child(join_field)
                    .with_child(
                        Button::create(label("azmeet-join"))
                            .with_on_click(data.clone(), actions.join)
                            .dom()
                            .with_id(ids::JOIN),
                    ),
            ))
            .with_child(labelled(&t("azmeet-schedule-label"), schedule));
        if let Some(problem) = server_problem(lobby) {
            card = card.with_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; align-items: center; margin-bottom: 12px; min-width: 0px;")
                    .with_child(text(
                        &problem,
                        &format!("flex-grow: 1; min-width: 0px; margin-right: 8px; {PROBLEM_LINE}"),
                    ))
                    .with_child(
                        Button::create(label("azmeet-connection-settings"))
                            .with_on_click(data.clone(), actions.connection_settings)
                            .dom()
                            .with_id(ids::CONNECTION_SETTINGS)
                            .with_css("flex-shrink: 0;"),
                    ),
            );
        }
        if !lobby.identity_ready {
            card = card.with_child(text(&t("azmeet-loading-key"), SECONDARY));
        }
        card = card.with_child(rooms_list(&lobby.rooms, data, actions));
    }
    Dom::create_div()
        .with_css(PAGE)
        .with_child(header(view, data, actions))
        .with_child(
            Dom::create_div()
                .with_css(WAITING_BODY)
                .with_child(device_preview(view, data, actions))
                .with_child(card),
        )
}

/// A line that says something is wrong: flat's accent; flora's clay stone (its colour for a
/// problem, by day and by night).
const PROBLEM_LINE: &str = "font-size: 12px; color: system:accent; @theme(flora) { color: \
                            #7E4A42; @media (prefers-color-scheme: dark) { color: #B3837A; } }";

/// What the start screen says about the meetings service: nothing while it answers, else one
/// short line. The address, its check and Retry are in Settings > Meetings.
pub(crate) fn server_problem(lobby: &LobbyView) -> Option<String> {
    if lobby.server_unset {
        Some(t("azmeet-lobby-no-server"))
    } else if lobby.server_ok || lobby.server_pending {
        None
    } else {
        Some(t("azmeet-lobby-server-silent"))
    }
}

/// "Your rooms": one button per room (its view), with what it is and its unread count.
fn rooms_list(rooms: &[RoomRow], data: &RefAny, actions: &Actions) -> Dom {
    let mut list = Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-top: 8px; min-width: 0px;")
        .with_id(ids::ROOMS)
        .with_child(text(&t("azmeet-your-rooms"), SECTION_TITLE));
    if rooms.is_empty() {
        return list.with_child(text(&t("azmeet-none-yet"), SECONDARY));
    }
    for room in rooms {
        let mut row = Dom::create_div().with_css(PERSON_ROW).with_child(
            Dom::create_div()
                .with_css(PERSON_TEXT)
                .with_child(
                    Button::create(room.title.as_str())
                        .with_on_click(row_data(data, room.room.clone()), actions.open_room)
                        .dom()
                        .with_id(keyed_id(ids::ROOM_PREFIX, &room.room, ROOM_ID_CHARS)),
                )
                .with_child(text(&room.detail, SECONDARY)),
        );
        if room.unread > 0 {
            let unread = t_args("azmeet-unread", &[("count", Arg::from(room.unread.to_string()))]);
            row = row.with_child(Badge::create(AzString::from(unread.as_str())).dom());
        }
        list = list.with_child(row);
    }
    list
}

/// A labelled field.
fn labelled(label: &str, field: Dom) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-bottom: 12px; min-width: 0px;")
        .with_child(text(label, "font-size: 12px; margin-bottom: 4px;"))
        .with_child(field)
}

// ==== The waiting room ====

/// "Join now", or "Start meeting" for a meeting this side just made.
pub(crate) fn join_label(created: bool) -> String {
    t(if created { "azmeet-start-meeting" } else { "azmeet-join-now" })
}

/// The waiting room's button: [`join_label`], or with only the meeting's code (`knock`) "Ask to
/// join", and once asked "Waiting to be let in...".
pub(crate) fn waiting_button_label(created: bool, knock: Option<bool>) -> String {
    match knock {
        Some(false) => t("azmeet-ask-to-join"),
        Some(true) => t("azmeet-waiting-let-in"),
        None => join_label(created),
    }
}

/// This side's devices as the start screen and the waiting room show them, as Meet shows them
/// before a meeting: the live camera picture (or the initials while it is off), the mic and
/// camera switches, the microphone's level and the sound check.
fn device_preview(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let me = TileView {
        kind: TileKind::Camera,
        me: true,
        name: view.name.clone(),
        marker: None,
        muted: !view.mic,
        speaking: false,
    };
    let mut preview = Dom::create_div()
        .with_css(PREVIEW_COLUMN)
        .with_id(ids::PREVIEW)
        .with_child(tile_box(view, &me, data, TILE_IN_ROW, true))
        .with_child(
            Dom::create_div()
                .with_css(SWITCHES)
                .with_child(mic_switch(view, data, actions))
                .with_child(cam_switch(view, data, actions)),
        );
    let listening = view.mic || view.sound_check.records();
    if listening && !view.tone_mic {
        preview = preview.with_child(microphone(data));
    }
    if listening {
        preview = preview.with_child(
            Dom::create_div()
                .with_css("padding: 4px 24px;")
                .with_child(level_meter(view, data)),
        );
    }
    preview = preview.with_child(
        Dom::create_div()
            .with_css("padding: 8px 24px 0px 24px;")
            .with_child(sound_check(view, data, actions)),
    );
    preview
}

/// The waiting room: this side's picture large over the microphone and camera switches and the
/// level meter; beside it the meeting, the name, the devices and "Join now" / "Start meeting".
fn waiting_room(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let preview = device_preview(view, data, actions);
    let created = view.waiting.as_ref().is_some_and(|w| w.created);
    let knock = view.waiting.as_ref().and_then(|w| w.knock);
    let mut column = Dom::create_div().with_css(JOIN_COLUMN).with_child(text(
        &t(if created {
            "azmeet-meeting-ready"
        } else {
            "azmeet-ready-to-join"
        }),
        "font-size: 22px; margin-bottom: 6px;",
    ));
    if let Some(times) = view.waiting.as_ref().and_then(|w| w.times.as_ref()) {
        column = column.with_child(
            text(times, "font-size: 13px; margin-bottom: 10px;").with_id(ids::MEETING_TIMES),
        );
    }
    if let Some(people) = view.waiting.as_ref().and_then(|w| w.people.as_ref()) {
        column = column.with_child(who_is_here_row(people));
    }
    if let Some(w) = &view.waiting {
        column = column.with_child(meeting_facts(w, data, actions));
    }
    if knock.is_some() {
        column = column.with_child(text(
            &t("azmeet-joined-with-code"),
            "font-size: 12px; color: system:secondary-text; margin-bottom: 10px;",
        ));
    }
    if view.waiting.as_ref().is_some_and(|w| w.local) {
        column = column
            .with_child(
                text(&t("azmeet-local-meeting"), "font-size: 12px; color: system:secondary-text; margin-bottom: 6px;")
                    .with_id(ids::LOCAL_MEETING),
            )
            .with_child(
                Button::create(label("azmeet-retry"))
                    .with_on_click(data.clone(), actions.preview_retry)
                    .dom()
                    .with_id(ids::SERVICE_RETRY)
                    // Clear of the name's label under it, as the other blocks of the column are.
                    .with_css(format!("{WIDE} margin-bottom: 16px;").as_str()),
            );
    }
    let join = Button::with_type(AzString::from(waiting_button_label(created, knock)), ButtonType::Primary);
    let join = if knock == Some(true) {
        join.dom()
    } else {
        join.with_on_click(data.clone(), actions.join_now).dom()
    };
    column = column
        .with_child(labelled(&t("azmeet-your-name"), name_field(&view.name, data, actions)))
        .with_child(text(&t("azmeet-devices"), SECTION_TITLE))
        .with_child(device_pickers(&view.settings, data, actions))
        .with_child(join.with_id(ids::JOIN_NOW).with_css(WIDE))
        .with_child(
            Button::create(label("azmeet-back"))
                .with_on_click(data.clone(), actions.waiting_back)
                .dom()
                .with_id(ids::WAITING_BACK)
                .with_css(WIDE),
        );
    Dom::create_div()
        .with_css(PAGE)
        .with_id(ids::WAITING)
        .with_child(header(view, data, actions))
        .with_child(
            Dom::create_div()
                .with_css(WAITING_BODY)
                .with_child(preview)
                .with_child(column),
        )
}

/// The most people the waiting room shows by face before "and N others".
const FACES: usize = 4;

/// Who is in the meeting already, as the waiting room says it: "No one else is here yet", "Ada
/// is in this meeting", "Ada and Ben are ...", "Ada, Ben and Cleo are ...", "Ada, Ben, Cleo and
/// 2 others are ...".
pub(crate) fn who_is_here(people: &[String]) -> String {
    let people: Vec<String> = people.iter().map(|name| person_name(name)).collect();
    let people = people.as_slice();
    let count = people.len();
    match people {
        [] => t("azmeet-nobody-here"),
        [one] => t_args("azmeet-one-here", &[("name", Arg::from(one))]),
        _ if count <= FACES => t_args("azmeet-many-here", &[("names", Arg::from(and_list(people.to_vec())))]),
        _ => {
            let mut names = people[..FACES - 1].to_vec();
            names.push(t_args("azmeet-others", &[("count", Arg::from((count - (FACES - 1)).to_string()))]));
            t_args("azmeet-many-here", &[("names", Arg::from(and_list(names)))])
        }
    }
}

/// The waiting room's line under its heading: the faces of who is in the meeting already (up to
/// `FACES`) and [`who_is_here`].
fn who_is_here_row(people: &[String]) -> Dom {
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; margin-bottom: 14px; min-width: 0px;")
        .with_id(ids::WHO_IS_HERE);
    for name in people.iter().take(FACES) {
        row = row.with_child(
            Avatar::create(AzString::from(initials(name).as_str()))
                .with_size(AvatarSize::Small)
                .dom()
                .with_css("margin-right: 4px; flex-shrink: 0;"),
        );
    }
    row.with_child(text(
        &who_is_here(people),
        "font-size: 13px; color: system:secondary-text; margin-left: 4px; min-width: 0px;",
    ))
}

/// The name others see (the waiting room, the settings' Meetings): taken as typed, written when
/// the field is left.
fn name_field(name: &str, data: &RefAny, actions: &Actions) -> Dom {
    TextInput::create()
        .with_text(name)
        .with_placeholder(label("azmeet-your-name"))
        .with_on_text_input(data.clone(), actions.name_text)
        .with_on_focus_lost(data.clone(), actions.name_blur)
        .dom()
        .with_id(ids::NAME)
}

/// The waiting room's meeting: its code, large, and its link with "Copy link".
fn meeting_facts(w: &WaitingView, data: &RefAny, actions: &Actions) -> Dom {
    let code = if w.code.is_empty() {
        w.link.as_str()
    } else {
        w.code.as_str()
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-bottom: 16px; min-width: 0px;")
        .with_child(text(&t("azmeet-meeting"), SECONDARY))
        .with_child(
            text(code, "font-size: 18px; font-weight: bold; margin: 2px 0px 6px 0px;")
                .with_id(ids::MEETING_CODE),
        )
        .with_child(link_row(&w.link, w.copied, data, actions))
}

/// The microphone, speaker and camera pickers (the waiting room and the settings' Audio &
/// Video).
fn device_pickers(s: &SettingsView, data: &RefAny, actions: &Actions) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(labelled(
            &t("azmeet-microphone"),
            choice(&s.mics, s.mic_choice, "azmeet-microphone", data, actions.mic_choice),
        ))
        .with_child(labelled(
            &t("azmeet-speaker"),
            choice(&s.speakers, s.speaker_choice, "azmeet-speaker", data, actions.speaker_choice),
        ))
        .with_child(labelled(
            &t("azmeet-camera"),
            choice(&s.cameras, s.camera_choice, "azmeet-camera", data, actions.camera_choice),
        ))
}

// ==== The sound check ====

/// "Record a test clip", and while it runs what it does (a click then stops it).
pub(crate) fn record_label(state: SoundCheckState) -> String {
    match state {
        SoundCheckState::Recording { seconds_left } => t_args(
            "azmeet-test-mic-recording",
            &[("seconds", Arg::from(seconds_left.to_string()))],
        ),
        SoundCheckState::PlayingClip => t("azmeet-test-mic-playing"),
        SoundCheckState::Idle | SoundCheckState::PlayingChime => t("azmeet-test-mic"),
    }
}

/// "Test speakers", and while its sound plays that it does.
pub(crate) fn speakers_label(state: SoundCheckState) -> String {
    match state {
        SoundCheckState::PlayingChime => t("azmeet-test-speakers-playing"),
        _ => t("azmeet-test-speakers"),
    }
}

/// The line under the sound check's buttons: how the last check ended.
pub(crate) fn sound_check_note(outcome: crate::soundcheck::Outcome) -> String {
    use crate::soundcheck::Outcome;
    match outcome {
        Outcome::None => String::new(),
        Outcome::PlayedClip => t("azmeet-test-mic-done"),
        Outcome::PlayedChime => t("azmeet-test-speakers-done"),
        Outcome::Quiet => t("azmeet-test-mic-quiet"),
        Outcome::NoMicrophone => t("azmeet-test-mic-none"),
    }
}

/// The sound check: "Test speakers" and "Record a test clip" side by side, the line about the
/// last check under them. Each is primary while it runs.
fn sound_check(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let state = view.sound_check;
    let speakers_busy = state == SoundCheckState::PlayingChime;
    let mic_busy = matches!(state, SoundCheckState::Recording { .. } | SoundCheckState::PlayingClip);
    let button = |label: String, busy: bool| {
        Button::with_type(
            AzString::from(label),
            if busy {
                ButtonType::Primary
            } else {
                ButtonType::Default
            },
        )
    };
    let mut check = Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-width: 0px;")
        .with_child(
            Dom::create_div()
                .with_css(ROW)
                .with_child(
                    button(speakers_label(state), speakers_busy)
                        .with_on_click(data.clone(), actions.test_speakers)
                        .dom()
                        .with_id(ids::TEST_SPEAKERS)
                        .with_css("margin-right: 8px;"),
                )
                .with_child(
                    button(record_label(state), mic_busy)
                        .with_on_click(data.clone(), actions.test_mic)
                        .dom()
                        .with_id(ids::TEST_MIC),
                ),
        );
    if !view.sound_check_note.is_empty() {
        check = check.with_child(text(&view.sound_check_note, &format!("margin-top: 4px; {SECONDARY}")));
    }
    check
}

// ==== Direct calls ====

/// The size of one QR module on the direct-call screen, px.
const QR_MODULE: usize = 3;

/// The direct-call screen: this computer's address (as text to copy and as a QR code, in the
/// form chosen), the Call field, what the call does, the recent calls, Back.
fn direct_screen(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let d = &view.direct;
    let mut card = Dom::create_div()
        .with_css(CARD)
        .with_id(ids::DIRECT)
        .with_child(text(&t("azmeet-direct-title"), "font-size: 24px; margin-bottom: 4px;"))
        .with_child(text(
            &t("azmeet-direct-intro"),
            "font-size: 13px; color: system:secondary-text; margin-bottom: 16px; white-space: normal;",
        ))
        .with_child(text(&t("azmeet-direct-your-address"), SECTION_TITLE))
        .with_child(
            Segmented::create(strings(&[
                t("azmeet-direct-with-addresses"),
                t("azmeet-direct-id-only"),
            ]))
            .with_selected_index(usize::from(d.id_only))
            .with_on_change(data.clone(), actions.direct_form)
            .dom()
            .with_id(ids::DIRECT_FORM),
        )
        .with_child(text(
            &t(if d.id_only {
                "azmeet-direct-id-only-note"
            } else {
                "azmeet-direct-with-addresses-note"
            }),
            &format!("margin: 4px 0px 6px 0px; white-space: normal; {SECONDARY}"),
        ))
        .with_child(
            Dom::create_div()
                .with_css(ROW)
                .with_child(
                    text(
                        &d.address,
                        "font-size: 12px; flex-grow: 1; min-width: 0px; overflow: hidden; white-space: \
                         nowrap; text-overflow: ellipsis; margin-right: 6px;",
                    )
                    .with_id(ids::DIRECT_ADDRESS),
                )
                .with_child(
                    Button::create(label(if d.copied { "azmeet-copied" } else { "azmeet-direct-copy" }))
                        .with_on_click(data.clone(), actions.direct_copy)
                        .dom()
                        .with_id(ids::DIRECT_COPY)
                        .with_css("flex-shrink: 0;"),
                ),
        );
    // The QR code of the address, when it fits one (a long ticket does not: its text is there).
    if let Ok(symbol) = azul_appkit::qr::QrCode::encode(d.address.as_bytes()) {
        card = card.with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; justify-content: center; margin: 10px 0px;")
                .with_child(kit::qr_dom(&symbol, QR_MODULE)),
        );
    }
    let action = if d.calling {
        Button::create(label("azmeet-direct-cancel"))
            .with_on_click(data.clone(), actions.direct_cancel)
            .dom()
            .with_id(ids::DIRECT_CANCEL)
    } else {
        Button::with_type(label("azmeet-direct-call"), ButtonType::Primary)
            .with_on_click(data.clone(), actions.direct_call)
            .dom()
            .with_id(ids::DIRECT_CALL)
    };
    card = card
        .with_child(text(&t("azmeet-direct-call-someone"), SECTION_TITLE))
        .with_child(
            Dom::create_div()
                .with_css(ROW)
                .with_child(
                    TextInput::create()
                        .with_text(d.call_text.as_str())
                        .with_placeholder(label("azmeet-direct-address-placeholder"))
                        .with_accessibility_name(label("azmeet-direct-call-someone"))
                        .with_on_text_input(data.clone(), actions.direct_text)
                        .dom()
                        .with_css(ROW_FIELD)
                        .with_id(ids::DIRECT_FIELD),
                )
                .with_child(action.with_css("flex-shrink: 0;")),
        );
    if !d.status.is_empty() {
        card = card.with_child(
            text(&d.status, &format!("margin-top: 6px; white-space: normal; {SECONDARY}"))
                .with_id(ids::DIRECT_STATUS),
        );
    }
    if !d.recents.is_empty() {
        let mut list = Dom::create_div()
            .with_css("display: flex; flex-direction: column; margin-top: 8px; min-width: 0px;")
            .with_id(ids::DIRECT_RECENTS)
            .with_child(text(&t("azmeet-direct-recent"), SECTION_TITLE));
        for (index, recent) in d.recents.iter().enumerate() {
            let line = t_args(
                if recent.outgoing { "azmeet-direct-called" } else { "azmeet-direct-was-called" },
                &[("name", Arg::from(recent.name.as_str())), ("when", Arg::from(recent.when.as_str()))],
            );
            list = list.with_child(
                Dom::create_div()
                    .with_css(PERSON_ROW)
                    .with_child(text(&line, "font-size: 13px; flex-grow: 1; min-width: 0px;"))
                    .with_child(
                        Button::create(label("azmeet-direct-call"))
                            .with_on_click(row_data(data, recent.address.clone()), actions.direct_recent)
                            .dom()
                            .with_id(AzString::from(format!("{}{index}", ids::DIRECT_RECENT_PREFIX).as_str()))
                            .with_css("flex-shrink: 0;"),
                    ),
            );
        }
        card = card.with_child(list);
    }
    card = card.with_child(
        Button::create(label("azmeet-back"))
            .with_on_click(data.clone(), actions.direct_back)
            .dom()
            .with_id(ids::DIRECT_BACK)
            .with_css(WIDE),
    );
    Dom::create_div()
        .with_css(PAGE)
        .with_child(header(view, data, actions))
        .with_child(Dom::create_div().with_css(WAITING_BODY).with_child(card))
}

/// A direct call coming in: who calls, the safety words to read to each other, Accept and
/// Decline.
fn incoming_prompt(call: &IncomingView, data: &RefAny, actions: &Actions) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; flex-wrap: wrap; padding: 10px \
             14px; margin: 8px 12px 0px 12px; border-radius: 8px; background: \
             system:control-background; @theme(flora) { border-radius: 5px; }",
        )
        .with_id(ids::DIRECT_INCOMING)
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; margin-right: 8px;")
                .with_child(text(
                    &t_args("azmeet-direct-calls-you", &[("name", Arg::from(call.name.as_str()))]),
                    "font-size: 15px; font-weight: bold;",
                ))
                .with_child(text(
                    &t_args("azmeet-direct-words", &[("words", Arg::from(call.words.as_str()))]),
                    "font-size: 13px; margin-top: 2px;",
                ))
                .with_child(text(&t("azmeet-direct-words-note"), SECONDARY)),
        )
        .with_child(
            Button::with_type(label("azmeet-direct-accept"), ButtonType::Primary)
                .with_on_click(data.clone(), actions.direct_accept)
                .dom()
                .with_id(ids::DIRECT_ACCEPT)
                .with_css("margin-right: 6px; flex-shrink: 0;"),
        )
        .with_child(
            Button::create(label("azmeet-direct-decline"))
                .with_on_click(data.clone(), actions.direct_decline)
                .dom()
                .with_id(ids::DIRECT_DECLINE)
                .with_css("flex-shrink: 0;"),
        )
}

// ==== The settings: azul-appkit's page with AzMeet's own categories ====

/// AzMeet's categories on the kit's settings page, before the kit's Appearance, Data, Shortcuts
/// and About: ids, shown as `azmeet-category-<name>` (`crate::l10n::word`).
pub(crate) const APP_CATEGORIES: [&str; 3] = ["Audio & Video", "Meetings", "Recording"];

/// The video quality choices, in `Quality` order (keys of AzMeet's resources).
pub(crate) const QUALITY_LABELS: [&str; 3] = ["azmeet-quality-auto", "azmeet-quality-saver", "azmeet-quality-low"];

/// The settings: the title row over the kit's page (its Back and Escape close it).
fn settings(view: &CallView, data: &RefAny, kit_ref: &RefAny, actions: &Actions) -> Dom {
    Dom::create_div()
        .with_css(PAGE)
        .with_child(title_row(view))
        .with_child(kit::settings_page(
            kit_ref,
            settings_sections(view, data, actions),
        ))
}

/// A check box of AzMeet's settings.
fn check(on: bool, id: AzString, data: &RefAny, action: CheckBoxOnToggleCallbackType) -> Dom {
    CheckBox::create(on)
        .with_on_toggle(data.clone(), action)
        .dom()
        .with_id(id)
}

/// AzMeet's own sections, one per [`APP_CATEGORIES`] entry.
fn settings_sections(view: &CallView, data: &RefAny, actions: &Actions) -> Vec<AppSection> {
    let s = &view.settings;
    let mut audio_video = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row(
            "azmeet-microphone",
            choice(&s.mics, s.mic_choice, "azmeet-microphone", data, actions.mic_choice),
        ))
        .with_child(kit::row(
            "azmeet-speaker",
            choice(&s.speakers, s.speaker_choice, "azmeet-speaker", data, actions.speaker_choice),
        ))
        .with_child(kit::row(
            "azmeet-camera",
            choice(&s.cameras, s.camera_choice, "azmeet-camera", data, actions.camera_choice),
        ))
        .with_child(kit::row(
            "azmeet-mirror",
            check(s.mirror, ids::MIRROR, data, actions.mirror),
        ))
        .with_child(kit::row(
            "azmeet-video-received",
            choice(
                &QUALITY_LABELS.map(t),
                s.quality,
                "azmeet-video-quality",
                data,
                actions.quality,
            ),
        ))
        .with_child(kit::row("azmeet-sound-check", sound_check(view, data, actions)));
    // The test clip needs the microphone even while it is muted (the call's widget is not
    // mounted while the settings show), and the meter shows that it hears something.
    if view.sound_check.records() {
        if !view.tone_mic {
            audio_video = audio_video.with_child(microphone(data));
        }
        audio_video = audio_video.with_child(kit::row("", level_meter(view, data)));
    }
    let audio_video = audio_video
        .with_child(kit::note("azmeet-mirror-note"))
        .with_child(kit::note("azmeet-devices-note"))
        .with_child(kit::note(&s.codec));
    let meetings = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row("azmeet-your-name", name_field(&s.name, data, actions)))
        .with_child(kit::row(
            "azmeet-join-muted",
            check(s.join_muted, ids::JOIN_MUTED, data, actions.join_muted),
        ))
        .with_child(kit::row(
            "azmeet-join-camera-off",
            check(s.join_camera_off, ids::JOIN_CAMERA_OFF, data, actions.join_camera_off),
        ))
        .with_child(kit::row("azmeet-meeting-server", connection_server(s, data, actions)))
        .with_child(kit::row("azmeet-relay", relay_row(s, data, actions)))
        .with_child(kit::note("azmeet-relay-note"))
        .with_child(kit::row(
            "azmeet-direct-title",
            Button::create(label("azmeet-direct-open"))
                .with_on_click(data.clone(), actions.direct_open)
                .dom()
                .with_id(ids::DIRECT_OPEN),
        ))
        .with_child(kit::note("azmeet-direct-note"))
        .with_child(kit::row(
            "azmeet-device-safety-code",
            text(&s.identity, "font-size: 13px;"),
        ))
        .with_child(kit::note("azmeet-waiting-room-note"))
        .with_child(kit::note("azmeet-encryption-note"));
    let recording = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row(
            "azmeet-recordings-go-to",
            text(&s.recordings, "font-size: 13px;"),
        ))
        .with_child(kit::note("azmeet-recording-note"));
    [audio_video, meetings, recording]
        .into_iter()
        .enumerate()
        .map(|(category, content)| AppSection {
            category,
            title: crate::l10n::word("category", APP_CATEGORIES[category]),
            content,
        })
        .collect()
}

/// The meetings service: its field with Retry while it does not answer, and what its last
/// check said. Only its address while it cannot be changed (in a meeting or a room view).
fn connection_server(s: &SettingsView, data: &RefAny, actions: &Actions) -> Dom {
    if !s.server_editable {
        return Dom::create_div()
            .with_css("display: flex; flex-direction: column; min-width: 0px;")
            .with_child(text(&s.server, "font-size: 13px;"))
            .with_child(text(&t("azmeet-server-locked"), SECONDARY));
    }
    let field = TextInput::create()
        .with_text(s.server_text.as_str())
        .with_placeholder(crate::rooms::SERVER_PLACEHOLDER)
        .with_on_text_input(data.clone(), actions.server_text)
        .with_on_virtual_key_down(data.clone(), actions.server_key)
        .with_on_focus_lost(data.clone(), actions.server_blur)
        .dom()
        .with_css(ROW_FIELD)
        .with_id(ids::SERVER);
    let mut row = Dom::create_div().with_css(ROW).with_child(field);
    if !s.server_ok && !s.server_unset {
        row = row.with_child(
            Button::create(label("azmeet-retry"))
                .with_on_click(data.clone(), actions.retry_server)
                .dom()
                .with_id(ids::RETRY)
                .with_css("flex-shrink: 0;"),
        );
    }
    let status = if s.server_ok {
        format!("margin-top: 4px; {SECONDARY}")
    } else {
        format!("margin-top: 4px; {PROBLEM_LINE}")
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-width: 0px;")
        .with_child(row)
        .with_child(text(&s.server_status, &status))
}

/// The relay choices as the settings list them, in `rooms::RelayChoice::ALL` order.
pub(crate) fn relay_labels() -> Vec<String> {
    use crate::rooms::RelayChoice;
    RelayChoice::ALL
        .iter()
        .map(|choice| {
            t(match choice {
                RelayChoice::Default => "azmeet-relay-default",
                RelayChoice::Azlin => "azmeet-relay-azlin",
                RelayChoice::Custom => "azmeet-relay-custom",
            })
        })
        .collect()
}

/// The relay: n0's, Azlin's or a company's own, with the company relay's field under the
/// choice. Only the relay in use while it cannot be changed.
fn relay_row(s: &SettingsView, data: &RefAny, actions: &Actions) -> Dom {
    let column = Dom::create_div().with_css("display: flex; flex-direction: column; min-width: 0px;");
    if !s.server_editable {
        return column.with_child(text(&s.relay_shown, "font-size: 13px;"));
    }
    let mut column = column.with_child(choice(
        &relay_labels(),
        s.relay_choice,
        "azmeet-relay",
        data,
        actions.relay_choice,
    ));
    if s.relay_choice == crate::rooms::RelayChoice::Custom.index() {
        column = column.with_child(
            TextInput::create()
                .with_text(s.relay_text.as_str())
                .with_placeholder(crate::rooms::RELAY_PLACEHOLDER)
                .with_on_text_input(data.clone(), actions.relay_text)
                .with_on_virtual_key_down(data.clone(), actions.relay_key)
                .with_on_focus_lost(data.clone(), actions.relay_blur)
                .dom()
                .with_css("margin-top: 4px; min-width: 0px;")
                .with_id(ids::RELAY_FIELD),
        );
    }
    if s.relay_off {
        column = column.with_child(text(&t("azmeet-relay-off"), &format!("margin-top: 4px; {SECONDARY}")));
    }
    if !s.relay_status.is_empty() {
        column = column.with_child(text(&s.relay_status, &format!("margin-top: 4px; {PROBLEM_LINE}")));
    }
    column
}

/// A drop-down of `choices` with `selected` chosen.
fn choice(
    choices: &[String],
    selected: usize,
    name: &str,
    data: &RefAny,
    action: DropDownOnChoiceChangeCallbackType,
) -> Dom {
    DropDown::create(strings(choices))
        .with_selected(selected.min(choices.len().saturating_sub(1)))
        .with_accessibility_name(label(name))
        .with_on_choice_change(data.clone(), action)
        .dom()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(name: &str, me: bool, kind: TileKind) -> TileView {
        TileView {
            kind,
            me,
            name: name.to_string(),
            marker: None,
            muted: false,
            speaking: false,
        }
    }

    #[test]
    fn a_tile_id_names_the_person_and_the_picture() {
        assert_eq!(tile_id(&tile("Ada (you)", true, TileKind::Camera)), "__azmeet_tile_me_camera");
        assert_eq!(tile_id(&tile("Ben", false, TileKind::Screen)), "__azmeet_tile_ben_screen");
        assert_eq!(
            tile_id(&tile("Cleo M.", false, TileKind::Camera)),
            "__azmeet_tile_cleo_m__camera",
            "the app prefix, every character but a letter or digit an underscore"
        );
    }

    #[test]
    fn this_sides_tile_says_you_and_its_avatar_keeps_the_names_initials() {
        crate::l10n::in_english();
        let mut me = tile("Ada Lovelace", true, TileKind::Camera);
        me.muted = true;
        assert_eq!(tile_label(&me), "Ada Lovelace (you) · muted");
        assert_eq!(tile_label(&tile("Ada Lovelace", true, TileKind::Screen)), "Your screen");
        assert_eq!(initials(&me.name), "AL", "the avatar reads the name, not the label (\"A(\")");
        assert_eq!(shown_name("Ben", false), "Ben");
    }

    #[test]
    fn a_tile_label_says_whose_screen_and_who_is_muted() {
        crate::l10n::in_english();
        let mut ada = tile("Ada", false, TileKind::Camera);
        ada.muted = true;
        assert_eq!(tile_label(&ada), "Ada · muted");
        assert_eq!(tile_label(&tile("Ben", false, TileKind::Screen)), "Ben's screen");
    }

    /// The waiting room says who is in the meeting already, as Meet's "No one else is here".
    #[test]
    fn the_waiting_room_says_who_is_in_the_meeting_already() {
        crate::l10n::in_english();
        let names = |list: &[&str]| list.iter().map(|n| n.to_string()).collect::<Vec<String>>();
        assert_eq!(who_is_here(&[]), "No one else is here yet");
        assert_eq!(who_is_here(&names(&["Ada"])), "Ada is in this meeting");
        assert_eq!(who_is_here(&names(&["Ada", "Ben"])), "Ada and Ben are in this meeting");
        assert_eq!(
            who_is_here(&names(&["Ada", "Ben", "Cleo", "Dan"])),
            "Ada, Ben, Cleo and Dan are in this meeting"
        );
        assert_eq!(
            who_is_here(&names(&["Ada", "Ben", "Cleo", "Dan", "Eve"])),
            "Ada, Ben, Cleo and 2 others are in this meeting"
        );
    }

    #[test]
    fn the_switches_say_what_a_click_does_and_the_join_button_who_made_the_meeting() {
        // The scripts click these words (two-clients.mjs: "Mute"; azmeet_e2e.py's waiting room).
        crate::l10n::in_english();
        assert_eq!((mic_label(true).as_str(), mic_label(false).as_str()), ("Mute", "Unmute"));
        assert_eq!(cam_label(true, false), "Stop video");
        assert_eq!(cam_label(false, true), "Start video", "an off camera is culled by nobody");
        assert!(cam_label(true, true).starts_with("Stop video"));
        assert_eq!((join_label(true).as_str(), join_label(false).as_str()), ("Start meeting", "Join now"));
    }

    /// With only a meeting's code a device knocks: the button says so, and once asked, that it
    /// waits (azmeet_e2e.py's crypto phase clicks "Ask to join").
    #[test]
    fn the_waiting_rooms_button_asks_to_join_with_only_the_code() {
        crate::l10n::in_english();
        assert_eq!(waiting_button_label(false, None), "Join now");
        assert_eq!(waiting_button_label(true, None), "Start meeting");
        assert_eq!(waiting_button_label(false, Some(false)), "Ask to join");
        assert_eq!(waiting_button_label(false, Some(true)), "Waiting to be let in...");
    }

    /// The sound check's buttons say what runs now; the line under them how the last one ended.
    #[test]
    fn the_sound_check_buttons_say_what_is_happening() {
        use crate::soundcheck::Outcome;
        crate::l10n::in_english();
        assert_eq!(record_label(SoundCheckState::Idle), "Record a test clip");
        assert_eq!(record_label(SoundCheckState::Recording { seconds_left: 2 }), "Recording... 2 s");
        assert_eq!(record_label(SoundCheckState::PlayingClip), "Playing it back...");
        assert_eq!(speakers_label(SoundCheckState::Idle), "Test speakers");
        assert_eq!(speakers_label(SoundCheckState::PlayingChime), "Playing a sound...");
        assert_eq!(sound_check_note(Outcome::None), "");
        assert_eq!(sound_check_note(Outcome::PlayedClip), "That is how the others hear you.");
        assert!(sound_check_note(Outcome::Quiet).starts_with("The microphone gave only silence"));
        assert!(sound_check_note(Outcome::NoMicrophone).starts_with("No sound came from the microphone"));
        assert!(sound_check_note(Outcome::PlayedChime).starts_with("Did you hear the sound?"));
    }

    fn lobby(server_ok: bool, server_unset: bool) -> LobbyView {
        LobbyView {
            opening: false,
            server_ok,
            server_pending: false,
            server_unset,
            join_text: String::new(),
            schedule_start: String::new(),
            schedule_minutes: String::new(),
            rooms: Vec::new(),
            identity_ready: true,
        }
    }

    /// The start screen keeps the connection details in the settings: it names no address, it
    /// only says when the meetings service cannot be used.
    #[test]
    fn the_start_screen_only_says_when_the_meetings_service_cannot_be_used() {
        crate::l10n::in_english();
        assert_eq!(server_problem(&lobby(true, false)), None);
        let silent = server_problem(&lobby(false, false)).expect("a silent service is said");
        assert_eq!(silent, "The meetings service does not answer.");
        let unset = server_problem(&lobby(false, true)).expect("a missing service is said");
        assert_eq!(unset, "No meetings service is set.");
        assert!(!silent.contains("://") && !unset.contains("://"));
    }

    /// The relay choices as the settings list them, in `RelayChoice::ALL` order.
    #[test]
    fn the_relay_choices_say_whose_relays_they_are() {
        crate::l10n::in_english();
        assert_eq!(
            relay_labels(),
            vec![
                String::from("Default (n0's public relays)"),
                String::from("Azlin (connect.azlin.io)"),
                String::from("Custom"),
            ]
        );
    }

    /// A row's buttons carry the start of its key: the scripts find "Admit" by the device id.
    #[test]
    fn a_rows_button_id_carries_the_start_of_its_key() {
        crate::l10n::in_english();
        let device = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            keyed_id(ids::ADMIT_PREFIX, device, DEVICE_ID_CHARS).as_str(),
            "__azmeet_admit_0123456789abcdef"
        );
        assert_eq!(
            keyed_id(ids::ROOM_PREFIX, "ab", ROOM_ID_CHARS).as_str(),
            "__azmeet_room_ab"
        );
        assert_eq!(safety_line("05881 39114"), "Safety code 05881 39114");
    }
}
