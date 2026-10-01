//! What AzMeet's window shows, built on the S10 `CallShell` of azul's app shells: the lobby, the
//! call and the settings.
//!
//! - **Lobby** (the start screen): the shell with this side's camera preview as its one tile, the
//!   join form in the side panel (name, meeting server, "New meeting", "Join with a link") and the
//!   devices (microphone, speaker, camera pickers, the level meter) under it; the controls bar
//!   switches the microphone and the camera before joining.
//! - **Call**: the tiles (`tiles::arrange`): a shared screen or, in the speaker view, the active
//!   speaker on the shell's stage over a filmstrip, else an even gallery; the side panel shows the
//!   people, the chat or the statistics; the devices slot the invite link and the level meter;
//!   the controls bar: microphone, camera, share, view, people, chat, statistics, settings,
//!   leave.
//! - **Settings** on `ShellSettingsLayout`: devices, video quality, appearance (theme, light /
//!   dark), network, about.
//!
//! The window is `NoTitle`; the shell's header is azul's `Titlebar`. Colours are the system's
//! (`system:text`, `system:window-background`, ...) and the widgets' own, so the window follows
//! the app theme (flat / flora) and the mode (light / dark); only the video tiles keep a dark
//! backdrop in both modes, as every call app does. Everything here is built from a [`CallView`]
//! the app makes from its state (`lib.rs` `snapshot`); the callbacks are the app's.

use azul::{
    audio::AudioConfig,
    callbacks::{
        ButtonOnClickCallbackType, DropDownOnChoiceChangeCallbackType,
        SegmentedOnChangeCallbackType, ShellSettingsLayoutOnCategoryCallbackType,
        TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
    },
    camera::CameraConfig,
    image::{ImageRef, RawImageFormat},
    option::OptionString,
    prelude::*,
    screen::ScreenCaptureConfig,
    shells::{CallShell, ShellSettingsLayout, ShellSettingsSection, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::{DomVec, StringVec, U8VecRef},
    widgets::{
        Avatar, Badge, Button, ButtonType, CameraWidget, DropDown, MicrophoneWidget, ProgressBar,
        ScreenCaptureWidget, Segmented, Titlebar,
    },
};

use crate::tiles::TileKind;

// ==== The view model: what the window shows, made by the app from its state ====

/// Which screen the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiScreen {
    Lobby,
    Call,
    Settings,
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
    /// "connected", "connecting", "you" ...
    pub status: String,
    pub muted: bool,
    pub deafened: bool,
    pub speaking: bool,
}

/// One message of the chat panel.
#[derive(Debug, Clone)]
pub(crate) struct ChatLine {
    pub name: String,
    pub text: String,
    pub mine: bool,
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

/// The lobby's join form.
#[derive(Debug, Clone)]
pub(crate) struct LobbyView {
    pub opening: bool,
    pub server_text: String,
    pub server_status: String,
    pub server_ok: bool,
    pub join_text: String,
}

/// The settings screen.
#[derive(Debug, Clone)]
pub(crate) struct SettingsView {
    pub category: usize,
    pub mics: Vec<String>,
    pub mic_choice: usize,
    pub speakers: Vec<String>,
    pub speaker_choice: usize,
    pub cameras: Vec<String>,
    pub camera_choice: usize,
    /// `Quality::labels` index.
    pub quality: usize,
    pub server: String,
    pub name: String,
}

/// Everything the window shows.
#[derive(Debug, Clone)]
pub(crate) struct CallView {
    pub screen: UiScreen,
    /// The window title row's text.
    pub title: String,
    /// One line under the title: why the demo runs, what the meeting server said.
    pub notice: String,
    pub name: String,
    pub lobby: Option<LobbyView>,
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
    /// The renditions of the camera and of the screen someone shows: one capture consumer each.
    pub camera_renditions: Vec<u16>,
    pub screen_renditions: Vec<u16>,
    pub mic_level: f32,
    /// Show the "Drop a video packet" button (`AZMEET_TEST_PATTERN=1`).
    pub video_debug: bool,
    pub settings: SettingsView,
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
    pub settings: ButtonOnClickCallbackType,
    pub settings_back: ButtonOnClickCallbackType,
    pub settings_category: ShellSettingsLayoutOnCategoryCallbackType,
    pub copy_link: ButtonOnClickCallbackType,
    pub drop_packet: ButtonOnClickCallbackType,
    pub chat_text: TextInputOnTextInputCallbackType,
    pub chat_key: TextInputOnVirtualKeyDownCallbackType,
    pub chat_send: ButtonOnClickCallbackType,
    pub name_text: TextInputOnTextInputCallbackType,
    pub server_text: TextInputOnTextInputCallbackType,
    pub server_key: TextInputOnVirtualKeyDownCallbackType,
    pub join_text: TextInputOnTextInputCallbackType,
    pub new_meeting: ButtonOnClickCallbackType,
    pub join: ButtonOnClickCallbackType,
    pub mic_choice: DropDownOnChoiceChangeCallbackType,
    pub speaker_choice: DropDownOnChoiceChangeCallbackType,
    pub camera_choice: DropDownOnChoiceChangeCallbackType,
    pub quality: DropDownOnChoiceChangeCallbackType,
    pub theme: SegmentedOnChangeCallbackType,
    pub mode: SegmentedOnChangeCallbackType,
}

// ==== Styles: structure and the few colours of their own (video tiles are dark in both modes) ====

/// The window's body: the shell fills it.
const BODY: &str = "display: flex; flex-direction: column; height: 100%; margin: 0px; \
                    font-family: sans-serif; color: system:text; background: \
                    system:window-background;";
/// A tile's box: the video (or the initials) fills it, the name label sits on it.
const TILE: &str = "position: relative; display: flex; flex-direction: column; align-items: \
                    center; justify-content: center; width: 100%; height: 100%; min-height: \
                    60px; border-radius: 8px; overflow: hidden; background: #1e1f24; color: \
                    #e8e8ee;";
/// The ring of the active speaker's tile.
const SPEAKING_RING: &str = "border: 2px solid system:accent;";
/// The video in a tile.
const VIDEO: &str = "width: 100%; height: 100%;";
/// The name label on a tile.
const TILE_LABEL: &str = "position: absolute; left: 8px; bottom: 8px; padding: 2px 8px; \
                          border-radius: 4px; background: rgba(0, 0, 0, 0.55); color: #ffffff; \
                          font-size: 12px; white-space: nowrap;";
/// A side-panel column that scrolls.
const PANEL_SCROLL: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: \
                            0px; overflow-y: auto; padding: 8px;";
/// A secondary line of text.
const SECONDARY: &str = "font-size: 12px; color: system:secondary-text;";
/// A section title in a side panel.
const SECTION_TITLE: &str = "font-size: 12px; font-weight: bold; color: \
                             system:secondary-text; margin: 10px 0px 4px 0px;";
/// One line of the statistics: wraps, never widens the panel.
const STAT_LINE: &str = "font-size: 12px; padding: 1px 0px; min-width: 0px;";
/// A row of controls.
const ROW: &str = "display: flex; flex-direction: row; align-items: center; min-width: 0px;";
/// The chat field: fills its row from a zero basis, so its width never follows what is typed.
const CHAT_FIELD: &str = "flex-grow: 1; flex-shrink: 1; flex-basis: 0px; min-width: 0px; \
                          margin-right: 6px;";
/// The notice line.
const NOTICE: &str = "padding: 4px 12px; font-size: 12px; color: system:secondary-text;";

/// The window: the screen the view asks for, inside the app theme's scope (its ground, ink,
/// font and accent), with the title row on top.
pub(crate) fn meet_view(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let content = match view.screen {
        UiScreen::Lobby => lobby(view, data, actions),
        UiScreen::Call => call(view, data, actions),
        UiScreen::Settings => settings(view, data, actions),
    };
    Dom::create_body()
        .with_css(BODY)
        .with_child(ShellThemeScope::create(content).with_accent(ShellThemeAccent::Blue).dom())
}

/// The title row (the window is `NoTitle`) and, when there is one, the notice under it.
fn header(view: &CallView) -> Dom {
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(
            Titlebar::create(AzString::from(view.title.as_str()))
                .without_border_bottom()
                .dom(),
        );
    if !view.notice.is_empty() {
        column = column.with_child(text(&view.notice, NOTICE));
    }
    column
}

/// A `<span>` of `s` in `css`.
fn text(s: &str, css: &str) -> Dom {
    Dom::create_span_with_text(s).with_css(css)
}

/// A strings vector.
fn strings(items: &[String]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| AzString::from(s.as_str())).collect())
}
