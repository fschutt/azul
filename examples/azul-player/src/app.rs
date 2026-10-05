//! The app's state, its start, the history file, opening a file, keeping the sound (azul's
//! AudioPlayer) with the picture (azul's VideoWidget), the transport and the keys. The window is
//! `ui.rs`.

use azul::{
    audio::AudioPlayer,
    callbacks::{TimerCallbackInfo, TimerCallbackReturn, WriteBackCallbackType},
    dialog::{FileDialog, FileOpenResult},
    dom::VirtualKeyCode,
    file::FileTypeList,
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    vec::StringVec,
    video::{VideoPhase, VideoStatus},
    widgets::{MediaControlsAction, MediaControlsEvent, SeekBar, SeekBarState},
    window::WindowFrame,
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
    sync::{audio_correction, volume_osd, ControlsVisibility},
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
pub const SHORTCUTS: [Shortcut; 8] = [
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
    /// The controls showed at the last look (a change rebuilds the window).
    pub controls_shown: bool,
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
    let window = kit::window_options(
        &kit_ref,
        crate::ui::layout,
        (1100.0, 700.0),
        (560.0, 380.0),
        on_window_created,
    );
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

/// A recent file in the library was picked.
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

// ==== The picture and the sound ====

/// The video widget reports where it is (about four times a second while it plays): the sound
/// follows, the seek bar moves in place, the position is remembered.
pub extern "C" fn on_video_status(
    mut data: RefAny,
    mut info: CallbackInfo,
    status: VideoStatus,
) -> Update {
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let phase_changed = status.phase != s.status.phase;
    s.status = status.clone();
    let position = f64::from(status.position_s);
    if let Some(audio) = s.audio.as_ref() {
        match status.phase {
            VideoPhase::Playing => {
                let heard = audio.get_state();
                if !heard.playing && !heard.finished {
                    audio.play();
                }
                if let Some(target) = audio_correction(position, heard.position_s) {
                    if heard.track != 0 && !heard.finished {
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
            save_history(&app, &s, &mut info);
        }
    }
    if let Some(seek) = info.get_node_id_by_marker(ids::SEEK).into_option() {
        SeekBar::update_position(info, seek, position);
    }
    if phase_changed {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
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
    if let Some(a) = s.audio.as_ref() {
        if s.paused {
            a.pause();
        } else {
            a.play();
        }
    }
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

/// The controls bar.
pub extern "C" fn on_controls(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: MediaControlsEvent,
) -> Update {
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let now = s.now_ms();
    s.controls.activity(now);
    let position = f64::from(s.status.position_s);
    let duration = f64::from(s.status.duration_s);
    match event.action {
        MediaControlsAction::PlayPause => toggle(&mut s),
        MediaControlsAction::SkipBack => seek(&mut s, position - 15.0),
        MediaControlsAction::SkipForward => seek(&mut s, position + 30.0),
        MediaControlsAction::Previous => seek(&mut s, 0.0),
        MediaControlsAction::Next => seek(&mut s, duration),
        MediaControlsAction::Volume => {
            set_volume(&mut s, event.value);
            return Update::DoNothing;
        }
        MediaControlsAction::Shuffle | MediaControlsAction::Repeat => {}
    }
    let _ = &mut info;
    Update::RefreshDom
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

/// The pointer moved over the window: the controls show.
pub extern "C" fn on_pointer(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let now = s.now_ms();
    s.controls.activity(now);
    if s.controls_shown {
        Update::DoNothing
    } else {
        s.controls_shown = true;
        Update::RefreshDom
    }
}

/// A double-click on the picture: fullscreen on / off.
pub extern "C" fn on_video_double_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let on = !s.fullscreen;
    set_fullscreen(&mut s, &mut info, on);
    Update::RefreshDom
}

pub extern "C" fn on_fullscreen(data: RefAny, info: CallbackInfo) -> Update {
    on_video_double_click(data, info)
}

/// The window's keys: the kit's first, then the player's.
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
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return Update::DoNothing;
    };
    let now = s.now_ms();
    s.controls.activity(now);
    let position = f64::from(s.status.position_s);
    let step = if shift { 60.0 } else { 10.0 };
    match key {
        VirtualKeyCode::Space | VirtualKeyCode::PlayPause => toggle(&mut s),
        VirtualKeyCode::Left => seek(&mut s, position - step),
        VirtualKeyCode::Right => seek(&mut s, position + step),
        VirtualKeyCode::Up => {
            let v = s.volume + 0.1;
            set_volume(&mut s, v);
        }
        VirtualKeyCode::Down => {
            let v = s.volume - 0.1;
            set_volume(&mut s, v);
        }
        VirtualKeyCode::M => toggle_mute(&mut s),
        VirtualKeyCode::F | VirtualKeyCode::F11 => {
            let on = !s.fullscreen;
            set_fullscreen(&mut s, &mut info, on);
        }
        VirtualKeyCode::Escape if s.fullscreen => set_fullscreen(&mut s, &mut info, false),
        _ => return Update::DoNothing,
    }
    info.prevent_default();
    Update::RefreshDom
}

// ==== The tick: the controls' auto-hide and the OSD ====

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

/// Every 250 ms: the controls hide (or show) and the OSD goes; the window is rebuilt only then.
extern "C" fn on_tick(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut s) = data.downcast_mut::<Player>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let now = s.now_ms();
    let mut changed = false;
    let visible = s.controls_visible();
    if visible != s.controls_shown {
        s.controls_shown = visible;
        changed = true;
    }
    if s.osd.as_ref().is_some_and(|(_, until)| now >= *until) {
        s.osd = None;
        changed = true;
    }
    if changed {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}
