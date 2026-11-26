//! The recorder's window: the indicator, the buttons, the camera's preview, the level, what the recorder said
//! and the list of recordings, drawn from what [`crate::recorder_panel`] and [`crate::recorder_host`] say. A
//! thin skin: no decision lives here that a test of those two does not cover. The widgets that feed the
//! recorder (the screen's capture, the camera, the microphone) are mounted while the host asks for them
//! ([`RecorderHost::wants`]), so nothing is open while nothing records, and hand every frame they make to it.
//!
//! The window is a small one that floats over the others and is kept out of the screen's own recording
//! (`WindowFlags::exclude_from_capture`): the camera preview in it is not in the picture twice and the
//! recording does not show the indicator of itself. It keeps the pause key (a global hotkey) while it exists.
//!
//! The app that holds the window gives it a [`Host`]: how to reach the [`RecorderApp`] from the app's state.

use azul::{
    app::{GlobalHotkey, GlobalHotkeyStatus, HotkeyModifiers},
    audio::{AudioConfig, AudioFrame},
    callbacks::{ButtonOnClickCallbackType, CallbackType, LayoutCallbackType},
    camera::{CameraConfig, CameraFacing},
    dom::{AccessibilityInfo, AccessibilityRole, VirtualKeyCode},
    image::RawImageFormat,
    misc::{Capability, PermissionState},
    prelude::*,
    screen::{ScreenCaptureConfig, ScreenCaptureSource},
    sensor::DesktopState,
    str::String as AzString,
    task::TerminateTimer,
    time::SystemTimeDiff,
    video::VideoFrame,
    widgets::{Button, CameraWidget, ConsumerFrame, FrameConsumer, LevelMeter, MicrophoneWidget, ScreenCaptureWidget},
};
use azul_appkit::l10n::{begin_layout, label, phrase_dom, t, t_text, text_dom};

use crate::{
    ids,
    recorder::{DesktopSample, FrontApp},
    recorder_codec::{PixelFormat, RawFrame},
    recorder_host::{Accesses, Devices, DisplayFacts, RecorderHost, Wants, RECORDER_CONSUMER},
    recorder_panel::{Access, Do, Dot, Hud, Note, Row, Severity},
};

/// How often the recorder is given its tick, milliseconds.
const TICK_MS: u64 = 100;

/// Every this-many ticks the desktop is looked at (the app in front, the pointer, the lock): ten times a second
/// would be more than a recording of the desktop needs, and a reading is a few system calls.
const DESKTOP_EVERY: u32 = 3;

/// What the window's callbacks reach through the app's state.
pub struct RecorderApp {
    pub host: RecorderHost,
    /// The display the window is on: what a recording is of.
    pub display: Option<DisplayFacts>,
    /// The tick timer, while there is one.
    pub timer: Option<TimerId>,
    /// What the last frame of the window showed: the timer asks for another only when this changed.
    shown: Shown,
    /// The ticks so far.
    ticks: u32,
}

impl RecorderApp {
    /// An app side with no window yet.
    #[must_use]
    pub fn new(host: RecorderHost) -> Self {
        Self { host, display: None, timer: None, shown: Shown::default(), ticks: 0 }
    }
}

/// What a frame of the window showed, to tell whether the next one would differ.
#[derive(Debug, Clone, PartialEq, Default)]
struct Shown {
    elapsed: String,
    level_step: u8,
    dot: Option<Dot>,
    notes: usize,
    wants: Wants,
    rows: usize,
}

/// How the callbacks of the window reach the recorder in the app's state.
#[derive(Clone)]
pub struct Host {
    /// The app's state (the data every window of the app has).
    pub app: RefAny,
    /// Runs `f` on the recorder inside the app's state; whether the app has one.
    pub with: fn(&mut RefAny, &mut dyn FnMut(&mut RecorderApp)) -> bool,
}

impl Host {
    fn run<R>(&self, f: impl FnOnce(&mut RecorderApp) -> R) -> Option<R> {
        let mut f = Some(f);
        let mut out = None;
        let mut app = self.app.clone();
        (self.with)(&mut app, &mut |r| {
            if let Some(f) = f.take() {
                out = Some(f(r));
            }
        });
        out
    }
}

/// What a button of the window does, with the way to the recorder.
struct Press {
    host: Host,
    what: Do,
}

/// What a row's buttons do.
struct RowPress {
    host: Host,
    name: String,
    delete: bool,
}

/// What the window draws, copied out of the recorder (so that nothing is borrowed while the DOM is built).
struct View {
    hud: Hud,
    wants: Wants,
    error: Option<azul_appkit::phrase::Text>,
    recent: Vec<Note>,
    rows: Vec<Row>,
    will: Vec<azul_appkit::phrase::Phrase>,
    preview: bool,
    recording: bool,
    fps: u32,
    /// The size the camera's picture is cut to for the recording.
    camera: (u32, u32),
}

impl View {
    fn of(app: &mut RecorderApp) -> Self {
        let hud = app.host.hud();
        Self {
            recording: hud.dot != Dot::Idle,
            hud,
            wants: app.host.wants(),
            error: app.host.error().cloned(),
            recent: app.host.recent().cloned().collect(),
            rows: app.host.rows(),
            will: crate::recorder_panel::will_record(app.host.settings()),
            preview: app.host.preview(),
            fps: app.host.settings().screen_fps,
            camera: {
                let preset = app.host.settings().quality.camera();
                (preset.width, preset.height)
            },
        }
    }
}

// ---- the pause key ----

fn pause_hotkey() -> GlobalHotkey {
    if cfg!(target_os = "macos") {
        GlobalHotkey { modifiers: HotkeyModifiers { ctrl: false, alt: false, shift: true, meta: true }, key: VirtualKeyCode::P }
    } else {
        GlobalHotkey { modifiers: HotkeyModifiers { ctrl: true, alt: true, shift: false, meta: false }, key: VirtualKeyCode::P }
    }
}

/// The pause key as the person reads it.
#[must_use]
pub fn pause_hotkey_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Cmd+Shift+P"
    } else {
        "Ctrl+Alt+P"
    }
}

// ---- the window ----

/// The recorder's window: small, above the others, and out of the screen's own recording. `layout` is the app's
/// layout callback for it (it calls [`hud`]).
#[must_use]
pub fn window(layout: LayoutCallbackType) -> WindowCreateOptions {
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.window_id = AzString::from(ids::REC_WINDOW_NAME);
    window.window_state.title = AzString::from(t("azreview-rec-window-title"));
    window.window_state.size.dimensions = LogicalSize::create(340.0, 420.0);
    window.window_state.flags.is_top_level = true;
    window.window_state.flags.exclude_from_capture = true;
    window
}

/// Keeps the dot's colours in one place.
const fn dot_css(dot: Dot) -> &'static str {
    match dot {
        Dot::Idle => "background: system:separator;",
        Dot::Recording => "background: #d93025;",
        Dot::Paused => "background: #f9ab00;",
    }
}

const fn note_css(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "color: system:secondary-text;",
        Severity::Warning => "color: #b3261e;",
    }
}

fn text_line(text: &azul_appkit::phrase::Text, css: &str) -> Dom {
    text_dom(text).with_css(css).with_accessibility_info(AccessibilityInfo::named(AzString::from(t_text(text)), AccessibilityRole::Text))
}

/// The window's content. `info` is the layout's own: the window keeps the pause key while it is drawn.
pub fn hud(host: &Host, info: &LayoutCallbackInfo) -> Dom {
    let _mode = info.get_mode();
    begin_layout(info);

    // The pause key belongs to what this state draws, as a button's callback does: not declared, it is let go.
    info.add_global_hotkey_with_description(
        pause_hotkey(),
        t("azreview-rec-hotkey-description"),
        RefAny::new(Press { host: host.clone(), what: Do::Pause }),
        on_hotkey as CallbackType,
    );
    let hotkey_note = (!matches!(info.get_global_hotkey_status(pause_hotkey()), GlobalHotkeyStatus::Active)).then(|| t("azreview-rec-hotkey-unavailable"));
    hud_dom(host, hotkey_note)
}

/// What the window draws for the state of the recorder: `hotkey_note` says the pause key is not there.
pub fn hud_dom(host: &Host, hotkey_note: Option<String>) -> Dom {
    let Some(view) = host.run(View::of) else { return Dom::create_body() };

    let mut page = Dom::create_div().with_css("display: flex; flex-direction: column; gap: 8px; padding: 12px; font-size: 13px;");

    // The indicator: the dot, what is going on and how long.
    let dot = Dom::create_div().with_css(format!("width: 12px; height: 12px; border-radius: 6px; flex-shrink: 0; {}", dot_css(view.hud.dot)).as_str());
    let head = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 8px;")
        .with_child(dot)
        .with_child(phrase_dom(&view.hud.status).with_css("font-weight: bold; flex-grow: 1;"))
        .with_child(Dom::create_span_with_text(view.hud.elapsed.as_str()).with_css("font-family: monospace;"))
        .with_accessibility_info(AccessibilityInfo::named(AzString::from(azul_appkit::l10n::t_phrase(&view.hud.described)), AccessibilityRole::Text));
    page.add_child(head);
    if let Some(size) = &view.hud.size {
        page.add_child(Dom::create_span_with_text(size.as_str()).with_css("color: system:secondary-text; font-size: 11px;"));
    }

    // The buttons.
    let mut buttons = Dom::create_div().with_css("display: flex; flex-direction: row; gap: 6px; flex-wrap: wrap;");
    for b in &view.hud.buttons {
        let data = RefAny::new(Press { host: host.clone(), what: b.what });
        buttons.add_child(
            Button::create(label_of(&b.label.key))
                .with_on_click(data, on_press as ButtonOnClickCallbackType)
                .dom()
                .with_accessibility_name(AzString::from(azul_appkit::l10n::t_phrase(&b.described))),
        );
    }
    let preview = RefAny::new(PreviewPress { host: host.clone() });
    buttons.add_child(
        Button::create(label(if view.preview { "azreview-rec-preview-off" } else { "azreview-rec-preview-on" }))
            .with_on_click(preview, on_preview as ButtonOnClickCallbackType)
            .dom(),
    );
    page.add_child(buttons);

    if view.wants.microphone || view.recording {
        page.add_child(
            Dom::create_div()
                .with_css("width: 100%;")
                .with_child(LevelMeter::create(view.hud.level).with_accessibility_name(label("azreview-recording-level")).dom().with_id(ids::REC_LEVEL)),
        );
    }

    // The camera: the preview, and what the recorder takes of the same capture.
    if view.wants.camera {
        page.add_child(
            CameraWidget::create(CameraConfig { facing: CameraFacing::Front, output_format: RawImageFormat::RGBA8, ..CameraConfig::default() })
                .with_consumer(FrameConsumer::create(RECORDER_CONSUMER, view.camera.0, view.camera.1))
                .with_on_consumer_frame(RefAny::new(Press { host: host.clone(), what: Do::Start }), on_camera_frame)
                .dom()
                .with_id(ids::REC_CAMERA)
                .with_css("width: 100%; aspect-ratio: 16 / 9; background: #000000; border-radius: 6px;")
                .with_accessibility_name(label("azreview-rec-preview")),
        );
    }

    // What the person is told: the last start's failure, the problems and the disk, what the recorder said.
    if let Some(error) = &view.error {
        page.add_child(text_line(error, note_css(Severity::Warning)));
    }
    if let Some(note) = hotkey_note {
        page.add_child(Dom::create_span_with_text(note.as_str()).with_css(note_css(Severity::Info)));
    }
    for note in view.hud.notes.iter().chain(view.recent.iter().rev().take(4)) {
        page.add_child(text_line(&note.text, note_css(note.severity)));
    }
    if !view.recording {
        for said in &view.will {
            page.add_child(phrase_dom(said).with_css("color: system:secondary-text; font-size: 12px;"));
        }
    }

    // The recordings.
    if !view.rows.is_empty() {
        page.add_child(Dom::create_span_with_text(t("azreview-rec-list-title").as_str()).with_css("font-weight: bold; margin-top: 6px;"));
        for row in &view.rows {
            page.add_child(row_dom(host, row));
        }
    }

    // The devices the recorder asked for, mounted while it asks: a capture widget with no picture, 1 px.
    if let Some(screen) = &view.wants.screen {
        let mut widget = ScreenCaptureWidget::create(ScreenCaptureConfig {
            source: ScreenCaptureSource::DisplayId(screen.display_id),
            fps: view.fps,
            output_format: RawImageFormat::RGBA8,
            show_cursor: true,
            include_own_windows: true,
        })
        .with_consumer(FrameConsumer::create(RECORDER_CONSUMER, screen.width, screen.height));
        for app in &screen.exclude {
            widget = widget.with_excluded_app(AzString::from(app.as_str()));
        }
        page.add_child(
            widget
                .with_on_consumer_frame(RefAny::new(Press { host: host.clone(), what: Do::Start }), on_screen_frame)
                .dom()
                .with_id(ids::REC_SCREEN)
                .with_css("width: 1px; height: 1px; overflow: hidden; flex-shrink: 0;"),
        );
    }
    if view.wants.microphone {
        page.add_child(
            MicrophoneWidget::create(AudioConfig { sample_rate: 48_000, channels: 1 })
                .with_on_frame(RefAny::new(Press { host: host.clone(), what: Do::Start }), on_audio_frame)
                .dom()
                .with_id(ids::REC_MICROPHONE)
                .with_css("width: 1px; height: 1px; overflow: hidden; flex-shrink: 0;"),
        );
    }
    Dom::create_body().with_child(page).with_callback(
        EventFilter::Window(WindowEventFilter::CloseRequested),
        RefAny::new(Press { host: host.clone(), what: Do::Stop }),
        on_close,
    )
}

fn label_of(key: &str) -> AzString {
    label(key)
}

/// A recording of the list: when, how long, how big, how it stands, and what can be done with it.
fn row_dom(host: &Host, row: &Row) -> Dom {
    let facts = format!("{} · {} · {}", row.when, row.length, row.size);
    let reveal = RefAny::new(RowPress { host: host.clone(), name: row.name.clone(), delete: false });
    let delete = RefAny::new(RowPress { host: host.clone(), name: row.name.clone(), delete: true });
    let mut line = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 2px; padding: 4px 0px; border-top: 1px solid system:separator;")
        .with_child(Dom::create_span_with_text(facts.as_str()).with_css("font-family: monospace; font-size: 12px;"))
        .with_child(phrase_dom(&row.state).with_css("color: system:secondary-text; font-size: 11px;"));
    if let Some(problem) = &row.problem {
        line.add_child(text_line(problem, note_css(Severity::Warning)));
    }
    line.with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; gap: 6px;")
            .with_child(Button::create(label("azreview-rec-reveal")).with_on_click(reveal, on_row as ButtonOnClickCallbackType).dom())
            .with_child(Button::create(label("azreview-rec-delete")).with_on_click(delete, on_row as ButtonOnClickCallbackType).dom()),
    )
}

// ---- what the window does ----

struct PreviewPress {
    host: Host,
}

fn access_of(state: PermissionState) -> Access {
    match state {
        PermissionState::Granted(_) | PermissionState::EphemeralGranted(_) => Access::Allowed,
        PermissionState::NotDetermined | PermissionState::Requested => Access::NotAsked,
        PermissionState::Denied => Access::Denied,
        PermissionState::Restricted => Access::Restricted,
    }
}

/// What the system says of the three devices.
fn accesses(info: &CallbackInfo) -> Accesses {
    Accesses {
        screen: access_of(info.get_permission_status(Capability::ScreenCapture)),
        camera: access_of(info.get_permission_status(Capability::Camera)),
        microphone: access_of(info.get_permission_status(Capability::Microphone)),
    }
}

/// A monitor as the recorder knows a display: the system's number and its size in pixels.
fn facts_of(m: &azul::window::Monitor) -> DisplayFacts {
    let scale = m.scale_factor.max(1.0);
    DisplayFacts { id: m.native_id, width: (m.size.width as f64 * scale).round() as u32, height: (m.size.height as f64 * scale).round() as u32 }
}

/// The display the window is on, and all the displays there are.
fn displays_of(info: &CallbackInfo) -> (DisplayFacts, Vec<DisplayFacts>) {
    let all: Vec<DisplayFacts> = info.get_monitors().as_ref().iter().map(facts_of).collect();
    let window = info.get_current_monitor().into_option().map(|m| facts_of(&m)).or_else(|| info.get_monitors().as_ref().iter().find(|m| m.is_primary_monitor).map(facts_of));
    (window.unwrap_or(DisplayFacts { id: 0, width: 0, height: 0 }), all)
}

fn ensure_timer(host: &Host, info: &mut CallbackInfo) {
    let id = host
        .run(|app| {
            if app.timer.is_some() {
                return None;
            }
            let id = TimerId::unique();
            app.timer = Some(id);
            Some(id)
        })
        .flatten();
    if let Some(id) = id {
        let timer = Timer::create(RefAny::new(Press { host: host.clone(), what: Do::Start }), on_tick, info.get_system_time_fn())
            .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS)));
        info.add_timer(id, timer);
    }
}

fn press(host: &Host, what: Do, info: &mut CallbackInfo) -> Update {
    match what {
        Do::Start => {
            let (access, (display, all)) = (accesses(info), displays_of(info));
            let devices = Devices { screen: Some(String::from("display")), camera: None, microphone: None };
            let started = host.run(|app| {
                app.display = Some(display);
                app.host.start(access, devices, display, &all)
            });
            if started == Some(true) {
                ensure_timer(host, info);
            }
        }
        Do::Stop => {
            host.run(|app| app.host.stop());
        }
        Do::Pause | Do::Resume => {
            host.run(|app| app.host.toggle_pause());
        }
    }
    Update::RefreshDom
}

extern "C" fn on_press(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(p) = data.downcast_ref::<Press>() else { return Update::DoNothing };
    let (host, what) = (p.host.clone(), p.what);
    drop(p);
    press(&host, what, &mut info)
}

/// The window is closed: a recording that is going is finished first (its files are whole), then the window goes.
extern "C" fn on_close(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(p) = data.downcast_ref::<Press>() else { return Update::DoNothing };
    let host = p.host.clone();
    drop(p);
    host.run(|app| app.host.stop());
    Update::DoNothing
}

extern "C" fn on_hotkey(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(p) = data.downcast_ref::<Press>() else { return Update::DoNothing };
    let host = p.host.clone();
    drop(p);
    press(&host, Do::Pause, &mut info)
}

extern "C" fn on_preview(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(p) = data.downcast_ref::<PreviewPress>() else { return Update::DoNothing };
    let host = p.host.clone();
    drop(p);
    let camera = access_of(info.get_permission_status(Capability::Camera));
    host.run(|app| {
        let on = !app.host.preview();
        app.host.set_preview(on, camera);
    });
    Update::RefreshDom
}

extern "C" fn on_row(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(p) = data.downcast_ref::<RowPress>() else { return Update::DoNothing };
    let (host, name, delete) = (p.host.clone(), p.name.clone(), p.delete);
    drop(p);
    host.run(|app| {
        if delete {
            app.host.delete(&name);
        } else if let Some(path) = app.host.path_of(&name) {
            let _ = azul_appkit::files::open_external(&path.to_string_lossy());
        }
    });
    Update::RefreshDom
}

/// What the desktop said, as the recorder takes it.
fn sample_of(desktop: &DesktopState) -> DesktopSample {
    let bundle = desktop.front_bundle.clone().into_option().map(|b| b.as_str().to_string());
    let name = desktop.front_name.clone().into_option().map(|n| n.as_str().to_string());
    DesktopSample {
        front: bundle.map(|bundle| FrontApp { name: name.unwrap_or_else(|| bundle.clone()), bundle }),
        pointer: desktop.pointer.clone().into_option().map(|p| (p.x, p.y)),
        locked: desktop.locked,
        display_asleep: desktop.display_asleep,
    }
}

extern "C" fn on_tick(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(p) = data.downcast_ref::<Press>() else {
        return TimerCallbackReturn { should_update: Update::DoNothing, should_terminate: TerminateTimer::Terminate };
    };
    let host = p.host.clone();
    drop(p);
    // The desktop is read outside the recorder's borrow (it asks the system), and only while a recording runs.
    let look = host
        .run(|app| {
            app.ticks = app.ticks.wrapping_add(1);
            app.ticks % DESKTOP_EVERY == 0 && app.host.hud().dot != Dot::Idle
        })
        .unwrap_or(false);
    let desktop = look.then(|| sample_of(&DesktopState::query()));
    let changed = host.run(|app| {
        if let Some(sample) = &desktop {
            app.host.desktop_sample(sample);
        }
        app.host.tick();
        let hud = app.host.hud();
        let now = Shown {
            elapsed: hud.elapsed.clone(),
            level_step: (hud.level * 10.0).round() as u8,
            dot: Some(hud.dot),
            notes: app.host.recent().count() + hud.notes.len(),
            wants: app.host.wants(),
            rows: app.host.rows().len(),
        };
        let changed = now != app.shown;
        app.shown = now;
        changed
    });
    TimerCallbackReturn { should_update: if changed == Some(true) { Update::RefreshDom } else { Update::DoNothing }, should_terminate: TerminateTimer::Continue }
}

/// A picture the way the recorder takes it, or `None` for a layout it does not know.
fn raw_frame(frame: &VideoFrame) -> Option<RawFrame> {
    let format = match frame.format {
        RawImageFormat::RGBA8 => PixelFormat::Rgba8,
        RawImageFormat::BGRA8 => PixelFormat::Bgra8,
        RawImageFormat::NV12Rec601Video | RawImageFormat::NV12Rec601Full | RawImageFormat::NV12Rec709Video | RawImageFormat::NV12Rec709Full => PixelFormat::Nv12,
        _ => return None,
    };
    Some(RawFrame { width: frame.width, height: frame.height, format, bytes: frame.bytes.as_ref().to_vec() })
}

extern "C" fn on_screen_frame(mut data: RefAny, _info: CallbackInfo, frame: ConsumerFrame) -> Update {
    if frame.consumer.id != RECORDER_CONSUMER {
        return Update::DoNothing;
    }
    let Some(p) = data.downcast_ref::<Press>() else { return Update::DoNothing };
    let host = p.host.clone();
    drop(p);
    if let Some(raw) = raw_frame(&frame.frame) {
        host.run(|app| app.host.screen_frame(raw, 1.0));
    }
    Update::DoNothing
}

extern "C" fn on_camera_frame(mut data: RefAny, _info: CallbackInfo, frame: ConsumerFrame) -> Update {
    if frame.consumer.id != RECORDER_CONSUMER {
        return Update::DoNothing;
    }
    let Some(p) = data.downcast_ref::<Press>() else { return Update::DoNothing };
    let host = p.host.clone();
    drop(p);
    if let Some(raw) = raw_frame(&frame.frame) {
        host.run(|app| app.host.camera_frame(raw));
    }
    Update::DoNothing
}

extern "C" fn on_audio_frame(mut data: RefAny, _info: CallbackInfo, frame: AudioFrame) -> Update {
    let Some(p) = data.downcast_ref::<Press>() else { return Update::DoNothing };
    let host = p.host.clone();
    drop(p);
    host.run(|app| app.host.audio_chunk(frame.samples.as_ref(), frame.sample_rate, frame.channels));
    Update::DoNothing
}

#[cfg(test)]
#[path = "recorder_ui_test.rs"]
mod recorder_ui_test;
