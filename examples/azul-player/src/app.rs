//! The app's state, its start, the history file, opening a file, keeping the sound (azul's
//! AudioPlayer) with the picture (azul's VideoWidget), the transport (one [`Command`] for every
//! button, menu item and key) and the chrome shown and hidden over the video. The window is
//! `ui.rs`.
//!
//! WHILE A VIDEO PLAYS THE WINDOW IS NOT REBUILT for what changes often: the chrome over the
//! picture (top and bottom strips) and the OSD are shown and hidden IN PLACE (their `visibility`,
//! `CallbackInfo::set_css_property`), the time played and the seek bar move in place. The video
//! box never changes size when the chrome comes and goes (the chrome lies OVER the picture), so
//! the decoder is never re-targeted by it. A rebuild is left for what the user does (play /
//! pause, a seek, fullscreen) and for a change of the video's phase.

use azul::{
    audio::AudioPlayer,
    callbacks::{TimerCallbackInfo, TimerCallbackReturn, WriteBackCallbackType},
    css::StyleVisibility,
    dialog::{FileDialog, FileOpenResult},
    dom::VirtualKeyCode,
    file::FileTypeList,
    option::{OptionFileTypeList, OptionRendererOptions, OptionString},
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::StringVec,
    video::{VideoPhase, VideoStatus},
    widgets::{SeekBar, SeekBarState},
    window::{HwAcceleration, WindowFrame},
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::{FileJob, FileOutcome},
    shortcuts::Shortcut,
    ui as kit,
};

use crate::{
    history::{History, HISTORY_FILE},
    ids,
    sync::{volume_osd, ControlsVisibility, SyncGuard},
};

// ==== The app's facts ====

/// The screens `--screen` opens.
pub const SCREENS: [&str; 2] = ["library", "settings"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzPlayer",
    binary: "AzPlayer",
    summary: "a video player: MP4 / MOV with H.264 and AAC, fullscreen, resume where you stopped",
    screens: &SCREENS,
    files_help: "the video files to open (the first plays)",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzPlayer",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Video files played by azul's VideoWidget, their sound by azul's AudioPlayer, kept \
              together; recent files resume where you stopped. Part of the Azlin apps, built \
              with azul.",
    license: "MIT",
    app_folder: "player",
};

/// The keyboard shortcuts the settings page lists.
pub const SHORTCUTS: [Shortcut; 9] = [
    Shortcut::new("Playback", "Space", "Play / pause"),
    Shortcut::new("Playback", "Left  Right", "Back / forward 10 seconds"),
    Shortcut::new(
        "Playback",
        "Shift+Left  Shift+Right",
        "Back / forward a minute",
    ),
    Shortcut::new("Playback", "Up  Down", "Volume up / down"),
    Shortcut::new("Playback", "M", "Mute"),
    Shortcut::new("Window", "F  F11  Double-click", "Fullscreen"),
    Shortcut::new("Window", "Escape", "Leave fullscreen"),
    Shortcut::new("Window", "Backspace", "Back to the library"),
    Shortcut::new("File", "Mod+O", "Open a video"),
];

/// The settings page's own categories (none: the kit's are enough).
const APP_CATEGORIES: [&str; 0] = [];

/// How often the controls' auto-hide and the OSD are looked at.
const TICK_MS: u64 = 250;
/// How long the OSD shows.
const OSD_MS: u64 = 1_200;
/// The history is written at most this often while a video plays (media seconds).
const SAVE_EVERY_S: f64 = 10.0;
/// The jumps of the rewind / fast-forward buttons, seconds.
const REWIND_S: f64 = 10.0;
const FORWARD_S: f64 = 30.0;

/// The write-back tags of the app's file jobs.
const TAG_LOAD: u64 = 1;
const TAG_SAVE: u64 = 2;

// ==== State ====

/// The app's state.
pub struct Player {
    /// The appkit kit (settings, data root, the settings page's state).
    pub kit: RefAny,
    pub history: History,
    /// The file playing (its path), or the library screen.
    pub file: Option<String>,
    /// Asked of the video widget: hold (`true`) or play.
    pub paused: bool,
    /// Asked of the video widget: the last seek target, in seconds.
    pub seek_s: f32,
    /// What the video widget last reported.
    pub status: VideoStatus,
    /// The sound: the file's audio track.
    pub audio: Option<AudioPlayer>,
    pub volume: f32,
    pub muted: bool,
    pub fullscreen: bool,
    pub controls: ControlsVisibility,
    /// The chrome over the video showed at the last look (shown and hidden in place).
    pub controls_shown: bool,
    /// When the sound is moved to the picture.
    pub sync: SyncGuard,
    /// The whole second the time label shows (-1: none yet).
    pub elapsed_shown: i64,
    /// The on-screen display and until when it shows (ms on `clock`).
    pub osd: Option<(String, u64)>,
    /// The media time of the last history write.
    pub saved_at_s: f64,
    /// The app's monotonic clock.
    pub clock: std::time::Instant,
    pub ticking: bool,
    /// The window's size.
    pub window: (f32, f32),
    pub args: AppArgs,
}

impl Player {
    fn new(kit: RefAny, args: AppArgs) -> Self {
        Self {
            kit,
            history: History::default(),
            file: None,
            paused: true,
            seek_s: 0.0,
            status: VideoStatus {
                message: AzString::from(""),
                position_s: 0.0,
                duration_s: 0.0,
                phase: VideoPhase::Loading,
            },
            audio: None,
            volume: 1.0,
            muted: false,
            fullscreen: false,
            controls: ControlsVisibility::default(),
            controls_shown: true,
            sync: SyncGuard::default(),
            elapsed_shown: -1,
            osd: None,
            saved_at_s: 0.0,
            clock: std::time::Instant::now(),
            ticking: false,
            window: args.size.unwrap_or((1100.0, 700.0)),
            args,
        }
    }

    /// Milliseconds on the app's clock.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Whether the video plays (as the widget says).
    #[must_use]
    pub fn playing(&self) -> bool {
        self.status.phase == VideoPhase::Playing && !self.paused
    }

    /// Whether the controls show now.
    #[must_use]
    pub fn controls_visible(&self) -> bool {
        self.file.is_none() || self.controls.visible(self.now_ms(), self.playing())
    }

    /// The data root and the app's key for `name`.
    fn root_and_key(&self, name: &str) -> Option<(std::path::PathBuf, String)> {
        let mut kit = self.kit.clone();
        let found = kit
            .downcast_ref::<kit::Kit>()
            .map(|k| (k.data_root.clone(), k.key(name)));
        found
    }

    /// Shows `text` on the OSD for a moment.
    fn osd(&mut self, text: String) {
        let until = self.now_ms() + OSD_MS;
        self.osd = Some((text, until));
    }

    /// The file's title (its name without the folder and the extension).
    #[must_use]
    pub fn title(&self) -> String {
        self.file
            .as_deref()
            .and_then(|p| std::path::Path::new(p).file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    }
}

/// The app's start: switches, the kit (settings, data root), the window.
pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &APP_CATEGORIES, args.clone());
    if args.screen.as_deref() == Some("settings") {
        kit::open_settings(&kit_ref, None);
    }
    let app = Player::new(kit_ref.clone(), args);
    let mut config = kit::app_config(&kit_ref);
    config.expose_system_media_controls = true;
    let mut window = kit::window_options(
        &kit_ref,
        crate::ui::layout,
        (1100.0, 700.0),
        (560.0, 380.0),
        on_window_created,
    );
    // A player paints a window-sized picture every frame: on the GPU (its YUV shader shows the
    // decoder's NV12 as it is), not on the CPU renderer that is the desktop default. The GPU
    // path falls back to the CPU when it cannot start; `AZ_BACKEND` still decides when it is set
    // (the E2E runs headless).
    window.window_state.renderer_options.hw_accel = HwAcceleration::Enabled;
    if let OptionRendererOptions::Some(renderer) = &mut window.renderer {
        renderer.hw_accel = HwAcceleration::Enabled;
    }
    App::create(RefAny::new(app), config).run(window);
}

// ==== The history file ====

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some((kit_ref, key)) = data
        .downcast_ref::<Player>()
        .map(|s| (s.kit.clone(), s.root_and_key(HISTORY_FILE)))
    else {
        return Update::DoNothing;
    };
    kit::on_window_created(&kit_ref, &mut info);
    if let Some((root, key)) = key {
        kit::spawn_file_jobs(
            &mut info,
            &root,
            vec![FileJob::Get { key }],
            app.clone(),
            TAG_LOAD,
            on_files as WriteBackCallbackType,
        );
    }
    ensure_ticking(&app, &mut info);
    Update::DoNothing
}

extern "C" fn on_files(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let app = data.clone();
    let first = {
        let Some(mut s) = data.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        if reply.tag == TAG_SAVE {
            for outcome in reply.outcomes {
                if let Some(why) = outcome.error() {
                    println!("AZPLAYER_ERROR {why}");
                }
            }
            return Update::DoNothing;
        }
        for outcome in reply.outcomes {
            if let FileOutcome::Got {
                result: Ok(Some(bytes)),
                ..
            } = outcome
            {
                if let Ok(h) = History::from_json(&String::from_utf8_lossy(&bytes)) {
                    s.history = h;
                }
            }
        }
        println!("AZPLAYER_HISTORY {}", s.history.entries.len());
        s.args
            .files
            .first()
            .map(|p| p.to_string_lossy().into_owned())
    };
    if let Some(path) = first {
        open(&app, &mut info, &path);
    }
    Update::RefreshDom
}

/// Writes the history into the data tree, on a Thread.
fn save_history(app: &RefAny, s: &Player, info: &mut CallbackInfo) {
    if let Some((root, key)) = s.root_and_key(HISTORY_FILE) {
        kit::spawn_file_jobs(
            info,
            &root,
            vec![FileJob::Put {
                key,
                bytes: s.history.to_json().into_bytes(),
            }],
            app.clone(),
            TAG_SAVE,
            on_files as WriteBackCallbackType,
        );
    }
}

// ==== Opening a file ====

/// Plays `path`: the picture from where it was left, the sound from the same place.
pub fn open(app: &RefAny, info: &mut CallbackInfo, path: &str) {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    s.history.touch(path, now);
    let resume = s.history.resume_at(path);
    s.file = Some(path.to_string());
    #[allow(clippy::cast_possible_truncation)]
    {
        s.seek_s = resume as f32;
    }
    s.paused = false;
    s.saved_at_s = resume;
    s.sync = SyncGuard::default();
    s.elapsed_shown = -1;
    s.status = VideoStatus {
        message: AzString::from(""),
        position_s: s.seek_s,
        duration_s: 0.0,
        phase: VideoPhase::Loading,
    };
    let volume = if s.muted { 0.0 } else { s.volume };
    let audio = s.audio.get_or_insert_with(AudioPlayer::create);
    let _track = audio.load_file(AzString::from(path));
    audio.set_volume(volume);
    if resume > 0.0 {
        audio.seek(resume);
    }
    let now_ms = s.now_ms();
    s.controls.activity(now_ms);
    s.controls_shown = true;
    println!("AZPLAYER_OPEN {path} {resume:.1}");
    save_history(app, &s, info);
}

/// Opens the file dialog for a video.
pub extern "C" fn on_open(data: RefAny, _info: CallbackInfo) -> Update {
    let filter = OptionFileTypeList::Some(FileTypeList {
        document_types: StringVec::from_vec(vec![
            AzString::from("mp4"),
            AzString::from("m4v"),
            AzString::from("mov"),
        ]),
        document_descriptor: AzString::from("Video"),
    });
    let _request =
        FileDialog::open_file("Open a video", OptionString::None, filter, data, on_picked);
    Update::DoNothing
}

extern "C" fn on_picked(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let Some(path) = picked.path.into_option() else {
        return Update::DoNothing;
    };
    open(&data, &mut info, path.inner.as_str());
    Update::RefreshDom
}

/// A recent file in the library (a tile or a menu item) was picked.
pub struct RecentPick {
    pub app: RefAny,
    pub path: String,
}

pub extern "C" fn on_recent(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, path)) = data
        .downcast_ref::<RecentPick>()
        .map(|p| (p.app.clone(), p.path.clone()))
    else {
        return Update::DoNothing;
    };
    open(&app, &mut info, &path);
    Update::RefreshDom
}

/// Back to the library (the file stops; its position is kept).
pub extern "C" fn on_close_file(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    if let Some(path) = s.file.take() {
        let (position, duration) = (
            f64::from(s.status.position_s),
            f64::from(s.status.duration_s),
        );
        s.history.set_position(&path, position, duration);
        save_history(&app, &s, &mut info);
    }
    if let Some(a) = s.audio.as_ref() {
        a.stop();
    }
    s.paused = true;
    Update::RefreshDom
}

// ==== In place: the chrome, the OSD, the labels ====

/// Shows (`true`) or hides the node marked `marker` in place: its `visibility`, no rebuild, no
/// relayout. Nothing when it is not in the window.
pub fn set_shown(info: &mut CallbackInfo, marker: AzString, shown: bool) {
    if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
        let visibility = if shown {
            StyleVisibility::Visible
        } else {
            StyleVisibility::Hidden
        };
        info.set_css_property(node, CssProperty::visibility(visibility));
    }
}

/// Rewrites the text node marked `marker` in place.
pub fn set_text(info: &mut CallbackInfo, marker: AzString, text: &str) {
    if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
        info.change_node_text(node, AzString::from(text));
    }
}

/// The chrome over the video (top and bottom strips) on or off, in place.
fn show_chrome(info: &mut CallbackInfo, shown: bool) {
    set_shown(info, ids::TOP, shown);
    set_shown(info, ids::BAR, shown);
}

/// The OSD as the state has it, in place: its text and whether it shows.
fn show_osd(s: &Player, info: &mut CallbackInfo) {
    match &s.osd {
        Some((text, _)) => {
            set_text(info, ids::OSD_TEXT, text);
            set_shown(info, ids::OSD, true);
        }
        None => set_shown(info, ids::OSD, false),
    }
}

// ==== The picture and the sound ====

/// The video widget reports where it is (about four times a second while it plays): the sound
/// follows (only a lasting drift, see [`SyncGuard`]), the seek bar and the time move in place,
/// the position is remembered. Only a new phase rebuilds the window.
pub extern "C" fn on_video_status(
    mut data: RefAny,
    mut info: CallbackInfo,
    status: VideoStatus,
) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let now_ms = s.now_ms();
    let phase_changed = status.phase != s.status.phase;
    let duration_changed = (status.duration_s - s.status.duration_s).abs() > 0.01;
    s.status = status.clone();
    let position = f64::from(status.position_s);
    if let Some(audio) = s.audio.as_ref() {
        match status.phase {
            VideoPhase::Playing => {
                let heard = audio.get_state();
                if !heard.playing && !heard.finished {
                    audio.play();
                }
                if heard.track != 0 && !heard.finished {
                    if let Some(target) = s.sync.correct(position, heard.position_s, now_ms) {
                        audio.seek(target);
                    }
                }
            }
            VideoPhase::Paused | VideoPhase::Loading => audio.pause(),
            VideoPhase::Ended | VideoPhase::Failed => audio.pause(),
        }
    }
    if phase_changed {
        let word = match status.phase {
            VideoPhase::Loading => "loading",
            VideoPhase::Paused => "paused",
            VideoPhase::Playing => "playing",
            VideoPhase::Ended => "ended",
            VideoPhase::Failed => "failed",
        };
        println!("AZPLAYER_STATE {word} {position:.1}");
    }
    // Remember where it is, now and then (and at the end: from the start next time).
    if let Some(path) = s.file.clone() {
        let ended = status.phase == VideoPhase::Ended;
        if ended || (position - s.saved_at_s).abs() >= SAVE_EVERY_S {
            let at = if ended { 0.0 } else { position };
            s.history
                .set_position(&path, at, f64::from(status.duration_s));
            s.saved_at_s = position;
            save_history(&app, s, &mut info);
        }
    }
    if phase_changed {
        // The play / pause button and the notes follow the phase: one rebuild.
        return Update::RefreshDom;
    }
    // In place: the seek bar, the time played (once a second), the length when it is known.
    if let Some(seek) = info.get_node_id_by_marker(ids::SEEK).into_option() {
        SeekBar::update_position(info, seek, position);
    }
    #[allow(clippy::cast_possible_truncation)]
    let whole = position.max(0.0).floor() as i64;
    if whole != s.elapsed_shown {
        s.elapsed_shown = whole;
        let text = SeekBar::media_time(position.max(0.0));
        set_text(&mut info, ids::ELAPSED, text.as_str());
    }
    if duration_changed {
        let text = SeekBar::media_time(f64::from(status.duration_s));
        set_text(&mut info, ids::TOTAL, text.as_str());
    }
    Update::DoNothing
}

/// Seeks the picture and the sound to `seconds`.
fn seek(s: &mut Player, seconds: f64) {
    let duration = f64::from(s.status.duration_s);
    let target = if duration > 0.0 {
        seconds.clamp(0.0, duration)
    } else {
        seconds.max(0.0)
    };
    #[allow(clippy::cast_possible_truncation)]
    let mut t = target as f32;
    // The widget seeks when its timestamp CHANGES: a second seek to the same place moves a
    // millisecond so it is a change.
    if (t - s.seek_s).abs() < f32::EPSILON {
        t += 0.001;
    }
    s.seek_s = t;
    s.sync.reset();
    if let Some(a) = s.audio.as_ref() {
        a.seek(f64::from(t));
    }
    // The OSD names the target of a real jump (not of a nudge within the sync band).
    if (f64::from(s.status.position_s) - target).abs() > crate::sync::DEAD_BAND_S {
        let text = SeekBar::media_time(target).as_str().to_string();
        s.osd(text);
    }
}

/// Plays or pauses (an ended video starts over).
fn toggle(s: &mut Player) {
    if s.file.is_none() {
        return;
    }
    if s.status.phase == VideoPhase::Ended {
        seek(s, 0.0);
        s.paused = false;
    } else {
        s.paused = !s.paused;
    }
    s.sync.reset();
    if let Some(a) = s.audio.as_ref() {
        if s.paused {
            a.pause();
        } else {
            a.play();
        }
    }
}

/// Stops: held at the start (the file stays open).
fn stop(s: &mut Player) {
    if s.file.is_none() {
        return;
    }
    s.paused = true;
    if let Some(a) = s.audio.as_ref() {
        a.pause();
    }
    seek(s, 0.0);
    s.osd(String::from("Stopped"));
}

fn set_volume(s: &mut Player, volume: f32) {
    s.volume = volume.clamp(0.0, 1.0);
    s.muted = false;
    if let Some(a) = s.audio.as_ref() {
        a.set_volume(s.volume);
    }
    let text = volume_osd(s.volume, false);
    s.osd(text);
}

fn toggle_mute(s: &mut Player) {
    s.muted = !s.muted;
    if let Some(a) = s.audio.as_ref() {
        a.set_volume(if s.muted { 0.0 } else { s.volume });
    }
    let text = volume_osd(s.volume, s.muted);
    s.osd(text);
}

/// Fullscreen on / off: the window's frame (and the system kept awake while it plays).
fn set_fullscreen(s: &mut Player, info: &mut CallbackInfo, on: bool) {
    s.fullscreen = on;
    let mut ws = info.get_current_window_state();
    ws.flags.frame = if on {
        WindowFrame::Fullscreen
    } else {
        WindowFrame::Normal
    };
    ws.flags.prevent_system_sleep = on;
    info.modify_window_state(ws);
}

// ==== The transport: one command for every button, menu item and key ====

/// What a button of the chrome, a menu item or a key asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// The file dialog.
    Open,
    /// Back to the library (the file stops, its position is kept).
    Library,
    /// The settings page.
    Settings,
    PlayPause,
    /// Held at the start.
    Stop,
    /// From the start.
    Restart,
    /// Back [`REWIND_S`].
    Rewind,
    /// Forward [`FORWARD_S`].
    Forward,
    Mute,
    VolumeUp,
    VolumeDown,
    Fullscreen,
}

/// A button's or a menu item's payload: the app and what it asks for.
pub struct CommandRef {
    pub app: RefAny,
    pub command: Command,
}

/// A button of the chrome or a menu item.
pub extern "C" fn on_command(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((app, command)) = data
        .downcast_ref::<CommandRef>()
        .map(|c| (c.app.clone(), c.command))
    else {
        return Update::DoNothing;
    };
    run(&app, &mut info, command)
}

/// Does `command`. The volume changes only the OSD (in place); the rest rebuilds the window once
/// (the play / pause button, the widget's timestamp or `paused` it hands to the decoder).
pub fn run(app: &RefAny, info: &mut CallbackInfo, command: Command) -> Update {
    match command {
        Command::Open => return on_open(app.clone(), *info),
        Command::Library => return on_close_file(app.clone(), *info),
        Command::Settings => {
            let mut app_ref = app.clone();
            let kit_ref = app_ref.downcast_ref::<Player>().map(|s| s.kit.clone());
            if let Some(kit_ref) = kit_ref {
                kit::open_settings(&kit_ref, None);
            }
            return Update::RefreshDom;
        }
        _ => {}
    }
    let mut app_ref = app.clone();
    let Some(mut guard) = app_ref.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let now = s.now_ms();
    s.controls.activity(now);
    let position = f64::from(s.status.position_s);
    let mut rebuild = true;
    match command {
        Command::PlayPause => toggle(s),
        Command::Stop => stop(s),
        Command::Restart => seek(s, 0.0),
        Command::Rewind => seek(s, position - REWIND_S),
        Command::Forward => seek(s, position + FORWARD_S),
        Command::Mute => toggle_mute(s),
        Command::VolumeUp => {
            let v = s.volume + 0.1;
            set_volume(s, v);
            rebuild = false;
        }
        Command::VolumeDown => {
            let v = s.volume - 0.1;
            set_volume(s, v);
            rebuild = false;
        }
        Command::Fullscreen => {
            let on = !s.fullscreen;
            set_fullscreen(s, info, on);
        }
        Command::Open | Command::Library | Command::Settings => {}
    }
    // The chrome and the OSD as the state has them, in place - also what a rebuild then keeps.
    if s.file.is_some() && !s.controls_shown {
        s.controls_shown = true;
        show_chrome(info, true);
    }
    show_osd(s, info);
    if rebuild {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// The seek bar: the picture and the sound seek on release (a drag only moves the bar).
pub extern "C" fn on_seek(mut data: RefAny, _info: CallbackInfo, state: SeekBarState) -> Update {
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let now = s.now_ms();
    s.controls.activity(now);
    if state.dragging {
        return Update::DoNothing;
    }
    seek(&mut s, state.position_s);
    Update::RefreshDom
}

/// The pointer moved over the window: the chrome shows, in place.
pub extern "C" fn on_pointer(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let now = s.now_ms();
    s.controls.activity(now);
    if !s.controls_shown {
        s.controls_shown = true;
        show_chrome(&mut info, true);
    }
    Update::DoNothing
}

/// A double-click on the picture: fullscreen on / off.
pub extern "C" fn on_video_double_click(data: RefAny, mut info: CallbackInfo) -> Update {
    run(&data, &mut info, Command::Fullscreen)
}

/// The window's keys: the kit's first, then the player's (not while the settings show: their
/// search field takes Space and Backspace).
pub extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(kit_ref) = data.downcast_ref::<Player>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    let fullscreen = data.downcast_ref::<Player>().is_some_and(|s| s.fullscreen);
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    // Escape leaves fullscreen before the kit sees it.
    if !(fullscreen && key == Some(VirtualKeyCode::Escape)) {
        if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
            return update;
        }
    }
    let Some(key) = key else {
        return Update::DoNothing;
    };
    let modifiers = info.get_key_modifiers();
    let primary = modifiers.primary_down();
    let shift = modifiers.shift;
    if key == VirtualKeyCode::O && primary {
        info.prevent_default();
        return on_open(app, info);
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let has_file = data.downcast_ref::<Player>().is_some_and(|s| s.file.is_some());
    let command = match key {
        VirtualKeyCode::Space | VirtualKeyCode::PlayPause => Some(Command::PlayPause),
        VirtualKeyCode::Up => Some(Command::VolumeUp),
        VirtualKeyCode::Down => Some(Command::VolumeDown),
        VirtualKeyCode::M => Some(Command::Mute),
        VirtualKeyCode::F | VirtualKeyCode::F11 => Some(Command::Fullscreen),
        VirtualKeyCode::Escape if fullscreen => Some(Command::Fullscreen),
        VirtualKeyCode::Back if has_file => Some(Command::Library),
        _ => None,
    };
    if let Some(command) = command {
        info.prevent_default();
        return run(&app, &mut info, command);
    }
    let step = if shift { 60.0 } else { 10.0 };
    {
        let Some(mut s) = data.downcast_mut::<Player>() else {
            return Update::DoNothing;
        };
        let position = f64::from(s.status.position_s);
        let target = match key {
            VirtualKeyCode::Left => position - step,
            VirtualKeyCode::Right => position + step,
            _ => return Update::DoNothing,
        };
        let now = s.now_ms();
        s.controls.activity(now);
        seek(&mut s, target);
        show_osd(&s, &mut info);
    }
    info.prevent_default();
    Update::RefreshDom
}

// ==== The tick: the controls' auto-hide and the OSD, in place ====

fn ensure_ticking(app: &RefAny, info: &mut CallbackInfo) {
    let mut app_ref = app.clone();
    let Some(mut s) = app_ref.downcast_mut::<Player>() else {
        return;
    };
    if s.ticking {
        return;
    }
    s.ticking = true;
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS))),
    );
}

/// Every 250 ms: the chrome hides (or shows) and the OSD goes - in place, never a rebuild (a
/// rebuild while the video plays is a frame the picture can miss).
extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let now = s.now_ms();
    let visible = s.controls_visible();
    if visible != s.controls_shown {
        s.controls_shown = visible;
        show_chrome(&mut info.callback_info, visible);
    }
    if s.osd.as_ref().is_some_and(|(_, until)| now >= *until) {
        s.osd = None;
        set_shown(&mut info.callback_info, ids::OSD, false);
    }
    TimerCallbackReturn::continue_unchanged()
}
