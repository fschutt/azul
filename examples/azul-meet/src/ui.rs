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
        ButtonOnClickCallbackType, CallbackType, DropDownOnChoiceChangeCallbackType,
        SegmentedOnChangeCallbackType, ShellSettingsLayoutOnCategoryCallbackType,
        TextInputOnFocusLostCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType,
    },
    camera::{CameraConfig, CameraFacing},
    image::{ImageRef, RawImageFormat},
    option::OptionString,
    prelude::*,
    screen::ScreenCaptureConfig,
    shells::{CallShell, ShellSettingsLayout, ShellSettingsSection, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::{DomVec, StringVec, U8VecRef},
    widgets::{
        Avatar, AvatarSize, Badge, Button, ButtonType, CameraWidget, DropDown, MicrophoneWidget,
        ProgressBar, ScreenCaptureWidget, Segmented, Titlebar,
    },
};

use azul_pim::initials::initials;

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
    /// `QUALITY_LABELS` index.
    pub quality: usize,
    /// 0 flat, 1 flora.
    pub theme: usize,
    /// 0 system, 1 light, 2 dark.
    pub mode: usize,
    pub server: String,
    pub name: String,
    /// "Video: H.264 (VideoToolbox)".
    pub codec: String,
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
    pub server_blur: TextInputOnFocusLostCallbackType,
    pub join_text: TextInputOnTextInputCallbackType,
    pub new_meeting: ButtonOnClickCallbackType,
    pub join: ButtonOnClickCallbackType,
    pub mic_choice: DropDownOnChoiceChangeCallbackType,
    pub speaker_choice: DropDownOnChoiceChangeCallbackType,
    pub camera_choice: DropDownOnChoiceChangeCallbackType,
    pub quality: DropDownOnChoiceChangeCallbackType,
    pub theme: SegmentedOnChangeCallbackType,
    pub mode: SegmentedOnChangeCallbackType,
    /// A key pressed anywhere in the window (the keyboard shortcuts).
    pub key: CallbackType,
}

// ==== Styles: structure and the few colours of their own (video tiles are dark in both modes) ====

/// The window's body: the shell fills it.
const BODY: &str = "display: flex; flex-direction: column; height: 100%; margin: 0px; \
                    font-family: sans-serif; color: system:text; background: \
                    system:window-background;";
/// A tile's box: the video (or the initials) fills it, the name label sits on it.
const TILE: &str = "position: relative; display: flex; flex-direction: column; align-items: \
                    center; justify-content: center; width: 100%; border-radius: 8px; overflow: \
                    hidden; background: #1e1f24; color: #e8e8ee;";
/// A gallery or filmstrip tile is 16:9 at its cell's width.
const TILE_IN_ROW: &str = "aspect-ratio: 16 / 9;";
/// The stage's tile takes the stage's room.
const TILE_ON_STAGE: &str = "flex-grow: 1; min-height: 0px;";
/// The ring of the active speaker's tile.
const SPEAKING_RING: &str = "border: 2px solid system:accent;";
/// The video in a tile: it fills the tile's column.
const VIDEO: &str = "flex-grow: 1; width: 100%; min-height: 0px;";
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
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            data.clone(),
            actions.key,
        )
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
        .with_header(header(view))
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
    let picture = if t.me {
        own_picture(view, t, data)
    } else {
        remote_picture(t)
    };
    let mut css = String::from(TILE);
    css.push_str(if on_stage { TILE_ON_STAGE } else { TILE_IN_ROW });
    if t.speaking {
        css.push_str(SPEAKING_RING);
    }
    Dom::create_div()
        .with_css(css.as_str())
        .with_id(AzString::from(tile_id(t).as_str()))
        .with_child(picture)
        .with_child(text(&tile_label(t), TILE_LABEL))
}

/// A tile's DOM id, for scripts: `azmeet-tile-me-camera`, `azmeet-tile-ben-screen` (the name in
/// lower case, every other character a dash).
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
                    '-'
                }
            })
            .collect()
    };
    let kind = match t.kind {
        TileKind::Camera => "camera",
        TileKind::Screen => "screen",
    };
    format!("azmeet-tile-{who}-{kind}")
}

/// "Ada", "Ada · muted", "Ada's screen", "You".
fn tile_label(t: &TileView) -> String {
    let mut label = match t.kind {
        TileKind::Camera => t.name.clone(),
        TileKind::Screen => format!("{}'s screen", t.name),
    };
    if t.muted && t.kind == TileKind::Camera {
        label.push_str(" · muted");
    }
    label
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
        None => Avatar::create(AzString::from(initials(&t.name).as_str())).dom(),
    }
}

/// This side's own camera or screen: the capture widget (whose consumers cut every rendition
/// someone shows - nothing is captured for nobody), the test pattern's word, or "camera off".
fn own_picture(view: &CallView, t: &TileView, data: &RefAny) -> Dom {
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
                .with_css(VIDEO)
        }
        TileKind::Camera if view.cam && view.cam_culled => {
            text("Test pattern - not shown to anyone, not sent", SECONDARY)
        }
        TileKind::Camera if view.cam => text("Test pattern", SECONDARY),
        TileKind::Camera => Dom::create_div()
            .with_css("display: flex; flex-direction: column; align-items: center;")
            .with_child(Avatar::create(AzString::from(initials(&t.name).as_str())).dom())
            .with_child(text("Camera off", "font-size: 12px; margin-top: 6px;")),
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
        .with_id(AzString::from("azmeet-people"));
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
                    .with_child(text(&person.name, "font-size: 13px;"))
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
        .with_id(AzString::from("azmeet-chat-messages"));
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
        .dom()
        .with_css(CHAT_FIELD)
        .with_id(AzString::from("azmeet-chat-field"));
    let send = Button::with_type("Send", ButtonType::Primary)
        .with_on_click(data.clone(), actions.chat_send)
        .dom()
        .with_id(AzString::from("azmeet-chat-send"));
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
        .with_id(AzString::from("azmeet-statistics"));
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
        column = column.with_child(
            Dom::create_div()
                .with_css(ROW)
                .with_child(text(
                    &view.link,
                    "font-size: 12px; flex-grow: 1; min-width: 0px; overflow: hidden; \
                     white-space: nowrap;",
                ))
                .with_child(
                    Button::create(if view.copied { "Copied" } else { "Copy link" })
                        .with_on_click(data.clone(), actions.copy_link)
                        .dom(),
                ),
        );
    }
    column
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
            ProgressBar::create(view.mic_level)
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
fn control(label: &str, id: &str, on: bool, data: &RefAny, action: ButtonOnClickCallbackType) -> Dom {
    let kind = if on {
        ButtonType::Primary
    } else {
        ButtonType::Default
    };
    Button::with_type(label, kind)
        .with_on_click(data.clone(), action)
        .dom()
        .with_id(AzString::from(id))
        .with_css("margin: 0px 4px; flex-shrink: 0;")
}

/// The controls bar: the microphone and the camera (the lobby stops there, with the settings),
/// then sharing, deafening, the view, the settings and Leave.
fn controls(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let mut row = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; flex-wrap: wrap; align-items: center; \
             justify-content: center; padding: 8px;",
        )
        .with_child(control(
            if view.mic { "Mute" } else { "Unmute" },
            "azmeet-mic",
            view.mic,
            data,
            actions.mic,
        ))
        .with_child(control(
            match (view.cam, view.cam_culled) {
                (true, true) => "Stop video (not shown to anyone)",
                (true, false) => "Stop video",
                (false, _) => "Start video",
            },
            "azmeet-cam",
            view.cam,
            data,
            actions.cam,
        ));
    if view.screen == UiScreen::Call {
        row = row
            .with_child(control(
                if view.screen_on {
                    "Stop sharing"
                } else {
                    "Share screen"
                },
                "azmeet-share",
                view.screen_on,
                data,
                actions.share,
            ))
            .with_child(control(
                if view.deafened { "Undeafen" } else { "Deafen" },
                "azmeet-deafen",
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
                "azmeet-view",
                false,
                data,
                actions.view,
            ));
        if view.video_debug {
            row = row.with_child(control(
                "Drop a video packet",
                "azmeet-drop",
                false,
                data,
                actions.drop_packet,
            ));
        }
    }
    row = row.with_child(control("Settings", "azmeet-settings", false, data, actions.settings));
    if view.screen == UiScreen::Call && view.in_room {
        row = row.with_child(
            Button::with_type("Leave", ButtonType::Danger)
                .with_on_click(data.clone(), actions.leave)
                .dom()
                .with_id(AzString::from("azmeet-leave"))
                .with_css("margin: 0px 4px 0px 16px; flex-shrink: 0;"),
        );
    }
    row
}

// ==== The lobby ====

/// The lobby: the shell with this side's camera preview as its one tile, the join form in the
/// side panel, the level meter under it, the microphone and camera switches below.
fn lobby(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let me = TileView {
        kind: TileKind::Camera,
        me: true,
        name: view.name.clone(),
        marker: None,
        muted: !view.mic,
        speaking: false,
    };
    CallShell::create(
        DomVec::from_vec(vec![tile(view, &me, data, false)]),
        controls(view, data, actions),
    )
    .with_header(header(view))
    .with_side_panel(join_form(view, data, actions))
    .with_devices(devices(view, data, actions))
    .dom()
}

/// A labelled field of the join form.
fn labelled(label: &str, field: Dom) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-bottom: 12px; min-width: 0px;")
        .with_child(text(label, "font-size: 12px; margin-bottom: 4px;"))
        .with_child(field)
}

/// The join form: name, meeting server (and whether it answers), "New meeting", and joining
/// with a link.
fn join_form(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let Some(lobby) = &view.lobby else {
        return Dom::create_div();
    };
    let name = TextInput::create()
        .with_text(view.name.as_str())
        .with_placeholder("Your name")
        .with_on_text_input(data.clone(), actions.name_text)
        .dom()
        .with_id(AzString::from("azmeet-name"));
    let server = TextInput::create()
        .with_text(lobby.server_text.as_str())
        .with_placeholder(crate::rooms::LOCAL_WORKER)
        .with_on_text_input(data.clone(), actions.server_text)
        .with_on_virtual_key_down(data.clone(), actions.server_key)
        .with_on_focus_lost(data.clone(), actions.server_blur)
        .dom()
        .with_id(AzString::from("azmeet-server"));
    let join_field = TextInput::create()
        .with_text(lobby.join_text.as_str())
        .with_placeholder("azlin://meet/... or a code")
        .with_on_text_input(data.clone(), actions.join_text)
        .dom()
        .with_css(CHAT_FIELD)
        .with_id(AzString::from("azmeet-join-field"));
    Dom::create_div()
        .with_css(PANEL_SCROLL)
        .with_child(text("Ready to join?", "font-size: 18px; margin-bottom: 12px;"))
        .with_child(labelled("Your name", name))
        .with_child(labelled("Meeting server", server))
        .with_child(text(
            &lobby.server_status,
            if lobby.server_ok {
                "font-size: 12px; color: system:secondary-text; margin: -8px 0px 12px 0px;"
            } else {
                "font-size: 12px; color: system:accent; margin: -8px 0px 12px 0px;"
            },
        ))
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
            .with_id(AzString::from("azmeet-new"))
            .with_css("margin-bottom: 16px;"),
        )
        .with_child(labelled(
            "Join with a link",
            Dom::create_div()
                .with_css(ROW)
                .with_child(join_field)
                .with_child(
                    Button::create("Join")
                        .with_on_click(data.clone(), actions.join)
                        .dom()
                        .with_id(AzString::from("azmeet-join")),
                ),
        ))
        .with_child(text("Devices", SECTION_TITLE))
        .with_child(device_pickers(&view.settings, data, actions))
}

/// The microphone, speaker and camera pickers (the lobby and the settings' Devices).
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

// ==== The settings ====

/// The settings' categories, in order.
pub(crate) const SETTINGS_CATEGORIES: [&str; 4] = ["Devices", "Video", "Appearance", "About"];

/// The video quality choices, in `Quality` order.
pub(crate) const QUALITY_LABELS: [&str; 3] = [
    "Automatic (up to 720p)",
    "Data saver (up to 360p)",
    "Low (up to 180p)",
];

/// The settings on the shell's settings layout: the active category's section, and Back.
fn settings(view: &CallView, data: &RefAny, actions: &Actions) -> Dom {
    let s = &view.settings;
    let category = s.category.min(SETTINGS_CATEGORIES.len() - 1);
    let section = match category {
        0 => ShellSettingsSection::create(
            AzString::from("Devices"),
            device_pickers(s, data, actions),
        ),
        1 => ShellSettingsSection::create(
            AzString::from("Video"),
            labelled(
                "Video I receive",
                choice(
                    &QUALITY_LABELS.map(String::from),
                    s.quality,
                    "Video quality",
                    data,
                    actions.quality,
                ),
            ),
        ),
        2 => ShellSettingsSection::create(
            AzString::from("Appearance"),
            Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(labelled(
                    "Theme",
                    Segmented::create(strings(&[String::from("Flat"), String::from("Flora")]))
                        .with_selected_index(s.theme)
                        .with_on_change(data.clone(), actions.theme)
                        .dom(),
                ))
                .with_child(labelled(
                    "Light or dark",
                    Segmented::create(strings(&[
                        String::from("System"),
                        String::from("Light"),
                        String::from("Dark"),
                    ]))
                    .with_selected_index(s.mode)
                    .with_on_change(data.clone(), actions.mode)
                    .dom(),
                )),
        ),
        _ => ShellSettingsSection::create(
            AzString::from("About"),
            Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(text("AzMeet - video meetings over azul.iroh", "font-size: 13px;"))
                .with_child(text(&format!("You appear as {}", s.name), SECONDARY))
                .with_child(text(&format!("Meeting server: {}", s.server), SECONDARY))
                .with_child(text(&s.codec, SECONDARY)),
        ),
    };
    let layout = ShellSettingsLayout::create(StringVec::from_vec(
        SETTINGS_CATEGORIES.iter().map(|c| AzString::from(*c)).collect(),
    ))
    .with_active_category(category)
    .with_on_category(data.clone(), actions.settings_category)
    .with_section(section)
    .dom();
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(header(view))
        .with_child(
            Dom::create_div().with_css("padding: 8px;").with_child(
                Button::create("Back")
                    .with_on_click(data.clone(), actions.settings_back)
                    .dom()
                    .with_id(AzString::from("azmeet-settings-back")),
            ),
        )
        .with_child(layout)
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
        assert_eq!(tile_id(&tile("Ada (you)", true, TileKind::Camera)), "azmeet-tile-me-camera");
        assert_eq!(tile_id(&tile("Ben", false, TileKind::Screen)), "azmeet-tile-ben-screen");
        assert_eq!(
            tile_id(&tile("Cleo M.", false, TileKind::Camera)),
            "azmeet-tile-cleo-m--camera"
        );
    }


    #[test]
    fn a_tile_label_says_whose_screen_and_who_is_muted() {
        let mut ada = tile("Ada", false, TileKind::Camera);
        ada.muted = true;
        assert_eq!(tile_label(&ada), "Ada · muted");
        assert_eq!(tile_label(&tile("Ben", false, TileKind::Screen)), "Ben's screen");
    }
}
