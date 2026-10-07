//! What AzMeet's window shows: the start screen, the waiting room, the call (on the S10
//! `CallShell` of azul's app shells) and the settings (azul-appkit's settings page).
//!
//! - **Start screen** (`lobby`): "New meeting", joining with a link or a code, the meeting server
//!   and whether it answers. Nothing is captured here: no camera, no microphone.
//! - **Waiting room** (`waiting_room`), between the start screen and the call, for a meeting just
//!   found or made: this side's camera large (mirrored, as a mirror shows it; the initials while
//!   the camera is off), the microphone and camera switches under it with the level meter, and
//!   beside it the meeting's code and link ("Copy link"), the name others see, the microphone,
//!   speaker and camera pickers, and "Join now" ("Start meeting" for a meeting this side made) or
//!   Back. It mounts the one camera widget and the one microphone widget the call mounts too:
//!   no device is opened twice.
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
use azul_appkit::ui::{self as kit, AppSection};
use azul_pim::initials::initials;

use crate::{ids, tiles::TileKind};

// ==== The view model: what the window shows, made by the app from its state ====

/// Which screen the window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiScreen {
    /// The start screen: new meeting, join with a link, the meeting server.
    Lobby,
    /// A meeting found or made, not joined yet: the preview and "Join now".
    Waiting,
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
    /// This side (shown as "<name> (you)").
    pub me: bool,
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

/// The start screen's form.
#[derive(Debug, Clone)]
pub(crate) struct LobbyView {
    pub opening: bool,
    pub server_text: String,
    pub server_status: String,
    pub server_ok: bool,
    pub join_text: String,
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
    pub server: String,
    pub name: String,
    /// "Video: H.264 (VideoToolbox)".
    pub codec: String,
    /// Where a meeting's files (its recording, later) go: `<data root>/meet/<meeting>/`, or why
    /// none are kept.
    pub recordings: String,
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
    /// This side's own camera is shown mirrored.
    pub mirror: bool,
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
    pub mic_choice: DropDownOnChoiceChangeCallbackType,
    pub speaker_choice: DropDownOnChoiceChangeCallbackType,
    pub camera_choice: DropDownOnChoiceChangeCallbackType,
    pub quality: DropDownOnChoiceChangeCallbackType,
    pub mirror: CheckBoxOnToggleCallbackType,
    pub join_muted: CheckBoxOnToggleCallbackType,
    pub join_camera_off: CheckBoxOnToggleCallbackType,
    /// A key pressed anywhere in the window (the kit's keys, then the call's shortcuts).
    pub key: CallbackType,
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
                    hidden; background: #1e1f24; color: #e8e8ee;";
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
                          font-size: 12px; white-space: nowrap;";
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
/// The start screen's body: its card in the middle, scrolling in a small window.
const CENTERED: &str = "display: flex; flex-direction: column; align-items: center; flex-grow: 1; \
                        min-height: 0px; overflow-y: auto; padding: 32px 16px;";
/// The start screen's card.
const CARD: &str = "display: flex; flex-direction: column; width: 100%; max-width: 440px;";
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
        UiScreen::Waiting => waiting_room(view, data, actions),
        UiScreen::Call => call(view, data, actions),
        UiScreen::Settings => settings(view, data, kit_ref, actions),
    };
    Dom::create_body()
        .with_css(BODY)
        .with_child(ShellThemeScope::create(content).with_accent(ShellThemeAccent::Blue).dom())
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            data.clone(),
            actions.key,
        )
}

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
        .with_accessibility_name("Settings")
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

/// A person's name as the window shows it: this side's own with "(you)" after it.
pub(crate) fn shown_name(name: &str, me: bool) -> String {
    if me {
        format!("{name} (you)")
    } else {
        name.to_string()
    }
}

/// "Ada", "Ada · muted", "Ada's screen", "Your screen".
fn tile_label(t: &TileView) -> String {
    let mut label = match t.kind {
        TileKind::Camera => shown_name(&t.name, t.me),
        TileKind::Screen if t.me => String::from("Your screen"),
        TileKind::Screen => format!("{}'s screen", t.name),
    };
    if t.muted && t.kind == TileKind::Camera {
        label.push_str(" · muted");
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
/// so), the test pattern's word, or the initials and "Camera is off".
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
            let mut css = String::from(VIDEO);
            if view.mirror {
                css.push(' ');
                css.push_str(MIRRORED);
            }
            camera
                .with_on_consumer_frame(data.clone(), crate::send_feed_frame)
                .dom()
                .with_css(css.as_str())
        }
        TileKind::Camera if view.cam && view.cam_culled => {
            text("Test pattern - not shown to anyone, not sent", SECONDARY)
        }
        TileKind::Camera if view.cam => text("Test pattern", SECONDARY),
        TileKind::Camera => Dom::create_div()
            .with_css("display: flex; flex-direction: column; align-items: center;")
            .with_child(avatar(&t.name, large))
            .with_child(text("Camera is off", "font-size: 12px; margin-top: 6px;")),
        TileKind::Screen if view.pattern_video => text("Your screen - test pattern", SECONDARY),
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

// ==== The side panel: people, chat, statistics ====

/// The side panel's tabs, in `PanelView` order.
const PANEL_TABS: [PanelView; 3] = [PanelView::People, PanelView::Chat, PanelView::Statistics];

/// The side panel: tabs over the people, the chat or the statistics; `None` when closed.
fn side_panel(view: &CallView, data: &RefAny, actions: &Actions) -> Option<Dom> {
    let selected = PANEL_TABS.iter().position(|p| *p == view.panel)?;
    let chat_tab = if view.chat_unread > 0 {
        format!("Chat ({})", view.chat_unread)
    } else {
        String::from("Chat")
    };
    let tabs = Segmented::create(strings(&[
        String::from("People"),
        chat_tab,
        String::from("Statistics"),
    ]))
    .with_selected_index(selected)
    .with_on_change(data.clone(), actions.panel)
    .dom();
    let body = match view.panel {
        PanelView::People => people(view),
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

/// Everyone in the call: initials, name, what they do.
fn people(view: &CallView) -> Dom {
    let mut list = Dom::create_div()
        .with_css(PANEL_SCROLL)
        .with_id(ids::PEOPLE);
    for person in &view.people {
        let mut row = Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 0px;")
            .with_child(
                Avatar::create(AzString::from(initials(&person.name).as_str()))
                    .with_size(AvatarSize::Small)
                    .dom(),
            )
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; \
                         margin-left: 8px;",
                    )
                    .with_child(text(&shown_name(&person.name, person.me), "font-size: 13px;"))
                    .with_child(text(&person.status, SECONDARY)),
            );
        if person.speaking {
            row = row.with_child(Badge::create(AzString::from("speaking")).dom());
        }
        if person.muted {
            row = row.with_child(Badge::create(AzString::from("muted")).dom());
        }
        if person.deafened {
            row = row.with_child(Badge::create(AzString::from("deafened")).dom());
        }
        list = list.with_child(row);
    }
    list
}

/// The chat: the messages, oldest first, over the field and its Send button.
fn chat(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let mut messages = Dom::create_div()
        .with_css(PANEL_SCROLL)
        .with_id(ids::CHAT_MESSAGES);
    if view.chat.is_empty() {
        messages = messages.with_child(text("No messages yet.", SECONDARY));
    }
    for line in &view.chat {
        let who = if line.mine { "You" } else { line.name.as_str() };
        messages = messages.with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; padding: 3px 0px;")
                .with_child(text(who, "font-size: 12px; font-weight: bold;"))
                .with_child(text(&line.text, "font-size: 13px;")),
        );
    }
    let field = TextInput::create()
        .with_text(view.chat_draft.as_str())
        .with_placeholder("Message everyone")
        .with_on_text_input(data.clone(), actions.chat_text)
        .with_on_virtual_key_down(data.clone(), actions.chat_key)
        .with_on_focus_lost(data.clone(), actions.chat_blur)
        .dom()
        .with_css(ROW_FIELD)
        .with_id(ids::CHAT_FIELD);
    let send = Button::with_type("Send", ButtonType::Primary)
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
            panel = panel.with_child(text("(nothing yet)", SECONDARY));
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
    Dom::create_div()
        .with_css(ROW)
        .with_child(text(
            link,
            "font-size: 12px; flex-grow: 1; min-width: 0px; overflow: hidden; white-space: \
             nowrap; text-overflow: ellipsis;",
        ))
        .with_child(
            Button::create(if copied { "Copied" } else { "Copy link" })
                .with_on_click(data.clone(), actions.copy_link)
                .dom()
                .with_id(ids::COPY_LINK)
                .with_css("flex-shrink: 0;"),
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
            "Mic level",
            "font-size: 12px; margin-right: 8px; white-space: nowrap;",
        ))
        .with_child(
            LevelMeter::create(view.mic_level)
                .with_accessibility_name("Microphone level")
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
pub(crate) fn mic_label(on: bool) -> &'static str {
    if on {
        "Mute"
    } else {
        "Unmute"
    }
}

/// The camera switch's label: what a click does (and that nobody sees a camera that is on).
pub(crate) fn cam_label(on: bool, culled: bool) -> &'static str {
    match (on, culled) {
        (true, true) => "Stop video (not shown to anyone)",
        (true, false) => "Stop video",
        (false, _) => "Start video",
    }
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
    device_switch(mic_label(view.mic), icon, ids::MIC, view.mic, data, actions.mic)
}

/// The camera switch (the waiting room and the call).
fn cam_switch(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let icon = if view.cam { "videocam" } else { "videocam_off" };
    let label = cam_label(view.cam, view.cam_culled);
    device_switch(label, icon, ids::CAM, view.cam, data, actions.cam)
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
            if view.screen_on {
                "Stop sharing"
            } else {
                "Share screen"
            },
            ids::SHARE,
            view.screen_on,
            data,
            actions.share,
        ))
        .with_child(control(
            if view.deafened { "Undeafen" } else { "Deafen" },
            ids::DEAFEN,
            view.deafened,
            data,
            actions.deafen,
        ))
        .with_child(control(
            if view.speaker_view {
                "Gallery view"
            } else {
                "Speaker view"
            },
            ids::VIEW,
            false,
            data,
            actions.view,
        ));
    if view.video_debug {
        row = row.with_child(control(
            "Drop a video packet",
            ids::DROP,
            false,
            data,
            actions.drop_packet,
        ));
    }
    if view.in_room {
        row = row.with_child(
            Button::with_type("Leave", ButtonType::Danger)
                .with_on_click(data.clone(), actions.leave)
                .dom()
                .with_id(ids::LEAVE)
                .with_css("margin: 0px 4px 0px 16px; flex-shrink: 0;"),
        );
    }
    row
}

// ==== The start screen ====

/// The start screen: "New meeting", joining with a link or a code, and the meeting server (and
/// whether it answers). Nothing is captured here: the devices open in the waiting room.
fn lobby(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let mut card = Dom::create_div()
        .with_css(CARD)
        .with_child(text("Video meetings", "font-size: 24px; margin-bottom: 4px;"))
        .with_child(text(
            "Start a meeting, or join one with its link or its code.",
            "font-size: 13px; color: system:secondary-text; margin-bottom: 20px;",
        ));
    if let Some(lobby) = &view.lobby {
        let join_field = TextInput::create()
            .with_text(lobby.join_text.as_str())
            .with_placeholder("azlin://meet/... or a code")
            .with_on_text_input(data.clone(), actions.join_text)
            .dom()
            .with_css(ROW_FIELD)
            .with_id(ids::JOIN_FIELD);
        let server = TextInput::create()
            .with_text(lobby.server_text.as_str())
            .with_placeholder(crate::rooms::LOCAL_WORKER)
            .with_on_text_input(data.clone(), actions.server_text)
            .with_on_virtual_key_down(data.clone(), actions.server_key)
            .with_on_focus_lost(data.clone(), actions.server_blur)
            .dom()
            .with_id(ids::SERVER);
        card = card
            .with_child(
                Button::with_type(
                    if lobby.opening {
                        "Please wait..."
                    } else {
                        "New meeting"
                    },
                    ButtonType::Primary,
                )
                .with_on_click(data.clone(), actions.new_meeting)
                .dom()
                .with_id(ids::NEW_MEETING)
                .with_css("margin-bottom: 20px;"),
            )
            .with_child(labelled(
                "Join with a link or a code",
                Dom::create_div()
                    .with_css(ROW)
                    .with_child(join_field)
                    .with_child(
                        Button::create("Join")
                            .with_on_click(data.clone(), actions.join)
                            .dom()
                            .with_id(ids::JOIN),
                    ),
            ))
            .with_child(labelled("Meeting server", server))
            .with_child(text(
                &lobby.server_status,
                if lobby.server_ok {
                    "font-size: 12px; color: system:secondary-text; margin: -8px 0px 12px 0px;"
                } else {
                    "font-size: 12px; color: system:accent; margin: -8px 0px 12px 0px;"
                },
            ));
    }
    Dom::create_div()
        .with_css(PAGE)
        .with_child(header(view, data, actions))
        .with_child(Dom::create_div().with_css(CENTERED).with_child(card))
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
pub(crate) fn join_label(created: bool) -> &'static str {
    if created {
        "Start meeting"
    } else {
        "Join now"
    }
}

/// The waiting room: this side's picture large over the microphone and camera switches and the
/// level meter; beside it the meeting, the name, the devices and "Join now" / "Start meeting".
fn waiting_room(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
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
    if view.mic && !view.tone_mic {
        preview = preview.with_child(microphone(data));
    }
    if view.mic {
        preview = preview.with_child(
            Dom::create_div()
                .with_css("padding: 4px 24px;")
                .with_child(level_meter(view, data)),
        );
    }
    let created = view.waiting.as_ref().is_some_and(|w| w.created);
    let mut column = Dom::create_div().with_css(JOIN_COLUMN).with_child(text(
        if created {
            "Your meeting is ready"
        } else {
            "Ready to join?"
        },
        "font-size: 22px; margin-bottom: 12px;",
    ));
    if let Some(w) = &view.waiting {
        column = column.with_child(meeting_facts(w, data, actions));
    }
    column = column
        .with_child(labelled("Your name", name_field(&view.name, data, actions)))
        .with_child(text("Devices", SECTION_TITLE))
        .with_child(device_pickers(&view.settings, data, actions))
        .with_child(
            Button::with_type(join_label(created), ButtonType::Primary)
                .with_on_click(data.clone(), actions.join_now)
                .dom()
                .with_id(ids::JOIN_NOW)
                .with_css(WIDE),
        )
        .with_child(
            Button::create("Back")
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

/// The name others see (the waiting room, the settings' Meetings): taken as typed, written when
/// the field is left.
fn name_field(name: &str, data: &RefAny, actions: &Actions) -> Dom {
    TextInput::create()
        .with_text(name)
        .with_placeholder("Your name")
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
        .with_child(text("Meeting", SECONDARY))
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
            "Microphone",
            choice(&s.mics, s.mic_choice, "Microphone", data, actions.mic_choice),
        ))
        .with_child(labelled(
            "Speaker",
            choice(&s.speakers, s.speaker_choice, "Speaker", data, actions.speaker_choice),
        ))
        .with_child(labelled(
            "Camera",
            choice(&s.cameras, s.camera_choice, "Camera", data, actions.camera_choice),
        ))
}

// ==== The settings: azul-appkit's page with AzMeet's own categories ====

/// AzMeet's categories on the kit's settings page, before the kit's Appearance, Data, Shortcuts
/// and About.
pub(crate) const APP_CATEGORIES: [&str; 3] = ["Audio & Video", "Meetings", "Recording"];

/// The video quality choices, in `Quality` order.
pub(crate) const QUALITY_LABELS: [&str; 3] = [
    "Automatic (up to 720p)",
    "Data saver (up to 360p)",
    "Low (up to 180p)",
];

/// The settings: the title row over the kit's page (its Back and Escape close it).
fn settings(view: &CallView, data: &RefAny, kit_ref: &RefAny, actions: &Actions) -> Dom {
    Dom::create_div()
        .with_css(PAGE)
        .with_child(title_row(view))
        .with_child(kit::settings_page(
            kit_ref,
            settings_sections(&view.settings, data, actions),
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
fn settings_sections(s: &SettingsView, data: &RefAny, actions: &Actions) -> Vec<AppSection> {
    let audio_video = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row(
            "Microphone",
            choice(&s.mics, s.mic_choice, "Microphone", data, actions.mic_choice),
        ))
        .with_child(kit::row(
            "Speaker",
            choice(&s.speakers, s.speaker_choice, "Speaker", data, actions.speaker_choice),
        ))
        .with_child(kit::row(
            "Camera",
            choice(&s.cameras, s.camera_choice, "Camera", data, actions.camera_choice),
        ))
        .with_child(kit::row(
            "Mirror my video",
            check(s.mirror, ids::MIRROR, data, actions.mirror),
        ))
        .with_child(kit::row(
            "Video I receive",
            choice(
                &QUALITY_LABELS.map(String::from),
                s.quality,
                "Video quality",
                data,
                actions.quality,
            ),
        ))
        .with_child(kit::note(
            "Mirror my video turns only your own picture, as a mirror does: the others see you \
             the right way round.",
        ))
        .with_child(kit::note(
            "The microphone and the speaker are the system's for now; the ones picked here are \
             remembered for when AzMeet can open another.",
        ))
        .with_child(kit::note(&s.codec));
    let meetings = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row("Your name", name_field(&s.name, data, actions)))
        .with_child(kit::row(
            "Join with the microphone off",
            check(s.join_muted, ids::JOIN_MUTED, data, actions.join_muted),
        ))
        .with_child(kit::row(
            "Join with the camera off",
            check(s.join_camera_off, ids::JOIN_CAMERA_OFF, data, actions.join_camera_off),
        ))
        .with_child(kit::row("Meeting server", text(&s.server, "font-size: 13px;")))
        .with_child(kit::note(
            "The waiting room starts with the microphone and the camera as set here; switch \
             them there before you join. The meeting server is changed on the start screen.",
        ));
    let recording = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row(
            "Recordings go to",
            text(&s.recordings, "font-size: 13px;"),
        ))
        .with_child(kit::note(
            "Recording, a transcript made on this computer and a summary sent by mail are on \
             the way. A recording will be kept in its meeting's folder, next to the meeting's \
             chat and record, and travel with your Azlin storage.",
        ));
    [audio_video, meetings, recording]
        .into_iter()
        .enumerate()
        .map(|(category, content)| AppSection {
            category,
            title: APP_CATEGORIES[category].to_string(),
            content,
        })
        .collect()
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
        .with_accessibility_name(AzString::from(name))
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
        let mut me = tile("Ada Lovelace", true, TileKind::Camera);
        me.muted = true;
        assert_eq!(tile_label(&me), "Ada Lovelace (you) · muted");
        assert_eq!(tile_label(&tile("Ada Lovelace", true, TileKind::Screen)), "Your screen");
        assert_eq!(initials(&me.name), "AL", "the avatar reads the name, not the label (\"A(\")");
        assert_eq!(shown_name("Ben", false), "Ben");
    }

    #[test]
    fn a_tile_label_says_whose_screen_and_who_is_muted() {
        let mut ada = tile("Ada", false, TileKind::Camera);
        ada.muted = true;
        assert_eq!(tile_label(&ada), "Ada · muted");
        assert_eq!(tile_label(&tile("Ben", false, TileKind::Screen)), "Ben's screen");
    }

    #[test]
    fn the_switches_say_what_a_click_does_and_the_join_button_who_made_the_meeting() {
        // The scripts click these words (two-clients.mjs: "Mute"; azmeet_e2e.py's waiting room).
        assert_eq!((mic_label(true), mic_label(false)), ("Mute", "Unmute"));
        assert_eq!(cam_label(true, false), "Stop video");
        assert_eq!(cam_label(false, true), "Start video", "an off camera is culled by nobody");
        assert!(cam_label(true, true).starts_with("Stop video"));
        assert_eq!((join_label(true), join_label(false)), ("Start meeting", "Join now"));
    }
}
