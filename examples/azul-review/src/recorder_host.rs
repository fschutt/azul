//! The recorder as the app holds it: the [`Recorder`], its settings and what the panel keeps around it. The
//! window asks it what to ask the system for ([`RecorderHost::wants`]: which capture widgets are mounted at
//! this moment, and at what size), hands it the frames those widgets deliver, and draws what
//! [`RecorderHost::hud`] and [`RecorderHost::rows`] say. Plain Rust: no window and no azul, so a recording can
//! be driven from a test with stand-in encoders and a clock the test moves.
//!
//! What the system is asked for follows the state, so that nothing is open while nothing records: the screen,
//! the camera and the microphone are asked for while a recording RUNS, and a pause lets them all go (a camera
//! light that stays on in a pause is a surprise). The camera alone can also be asked for to look at (the
//! preview).

use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use azul_appkit::phrase::{Arg, Phrase, Text};

use crate::{
    recorder::{Deps, DesktopSample, Notice, Recorder, StartRequest, State, TrackName},
    recorder_codec::RawFrame,
    recorder_events::EventSink,
    recorder_export_job::{video_path, ExportJob, FfmpegRenderer, JobState, Renderer},
    recorder_fs::{delete_recording, list_recordings, recover_all, RecordingInfo},
    recorder_hook::RecorderHook,
    recorder_manifest::{is_safe_relative, AppInfo, HostInfo, SessionState, Workspace},
    recorder_panel::{access_perm, duration_text, hud, notice_note, rows, size_text, start_error_text, Access, Hud, Note, Row, Severity},
    recorder_screen::{Tapped, TapFrame},
    recorder_settings::{RecorderSettings, ScreenKind, DEFAULT_FFMPEG},
};

/// How many notices the panel keeps.
const KEPT_NOTICES: usize = 20;

/// The id of the recorder in a capture's consumers (`FrameConsumer`): its picture of the screen is cut for this
/// one, at the size [`RecorderHost::wants`] asks for.
pub const RECORDER_CONSUMER: u32 = 8;

/// What the app knows about itself and its machine, once.
#[derive(Debug, Clone)]
pub struct Facts {
    pub app: AppInfo,
    pub workspace: Workspace,
    pub host: HostInfo,
    /// The local time's offset from UTC, minutes.
    pub utc_offset_minutes: i32,
    /// Where the recordings go when the settings name no folder.
    pub recordings_dir: PathBuf,
    /// Whom the system holds responsible for the permissions ([`crate::recorder_panel::attribution`]).
    pub who: Arg,
    /// The language's decimal mark.
    pub decimal: char,
    /// The pause key as the person reads it (`Cmd+Shift+P`).
    pub hotkey: String,
}

/// What the system says of the three devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accesses {
    pub screen: Access,
    pub camera: Access,
    pub microphone: Access,
}

/// The devices' names, for the manifest.
#[derive(Debug, Clone, Default)]
pub struct Devices {
    pub screen: Option<String>,
    pub camera: Option<String>,
    pub microphone: Option<String>,
}

/// The display the recording is of: the system's id and its size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayFacts {
    pub id: u32,
    pub width: u32,
    pub height: u32,
}

/// The display a recording is of: `wanted` (the system's number, as the settings name it; 0: none named) among
/// `all` the displays there are, else the display the window is on (`window`). A number that names no display
/// is said, and the window's display is recorded: a recording that is of nothing is no better than one of
/// the wrong display, and the wrong display is the one the person is looking at.
#[must_use]
pub fn pick_display(wanted: u32, window: DisplayFacts, all: &[DisplayFacts]) -> (DisplayFacts, Option<Text>) {
    if wanted == 0 {
        return (window, None);
    }
    match all.iter().find(|d| d.id == wanted) {
        Some(display) => (*display, None),
        None => (window, Some(Text::from(Phrase::new("azreview-rec-notice-display-missing").arg("id", wanted)))),
    }
}

/// What the screen capture is asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenWant {
    /// The display, by the system's id.
    pub display_id: u32,
    /// The picture's size: the display's aspect, at most the setting's width.
    pub width: u32,
    pub height: u32,
    /// The apps left out of the picture (bundle ids).
    pub exclude: Vec<String>,
}

/// What the window asks the system for at this moment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wants {
    /// The display's capture.
    pub screen: Option<ScreenWant>,
    /// The window's own frames, from its frame tap (no capture, no permission).
    pub tap: bool,
    /// The camera (to record, or to look at).
    pub camera: bool,
    /// The microphone.
    pub microphone: bool,
}

/// The size of the picture of a display of `display_w` x `display_h` pixels, at most `max_width` wide: the
/// display's aspect, never larger than the display, both sides even (an encoder takes no odd side; the sides
/// are cut down by a pixel, so that neither the display nor the setting is exceeded). `(0, 0)`: no picture.
#[must_use]
pub fn consumer_size(display_w: u32, display_h: u32, max_width: u32) -> (u32, u32) {
    if display_w == 0 || display_h == 0 || max_width == 0 {
        return (0, 0);
    }
    let width = display_w.min(max_width);
    let height = if width == display_w {
        display_h
    } else {
        let scaled = (u64::from(display_h) * u64::from(width) + u64::from(display_w) / 2) / u64::from(display_w);
        u32::try_from(scaled).unwrap_or(u32::MAX)
    };
    let (width, height) = (width & !1, height & !1);
    if width == 0 || height == 0 {
        (0, 0)
    } else {
        (width, height)
    }
}

/// What the review's window tells a recorder ([`RecorderHook`]): each event goes to the sink of the session that is
/// recording and is dropped when none is. One handle goes to the window's state at the start and stays there; the host
/// puts the sink in it when a session begins and takes it out when the session ends, whichever way it ends.
#[derive(Clone, Default)]
pub struct SessionHook {
    sink: Arc<Mutex<Option<EventSink>>>,
}

impl SessionHook {
    /// The sink of the session that records now, or none.
    fn set(&self, sink: Option<EventSink>) {
        if let Ok(mut slot) = self.sink.lock() {
            *slot = sink;
        }
    }

    /// A session is recording: what is told goes somewhere (a caller that has to build its fields may skip that
    /// work when nothing would take them).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.sink.lock().is_ok_and(|slot| slot.is_some())
    }
}

impl RecorderHook for SessionHook {
    fn event(&self, kind: &str, fields: &str) {
        // The sink never blocks (a full queue drops the event and counts it), so holding the lock for the call is short.
        if let Ok(slot) = self.sink.lock() {
            if let Some(sink) = slot.as_ref() {
                RecorderHook::event(sink, kind, fields);
            }
        }
    }
}

/// The recorder, its settings and what the panel keeps.
pub struct RecorderHost {
    recorder: Recorder,
    hook: SessionHook,
    settings: RecorderSettings,
    facts: Facts,
    /// What the system said of the devices when the recording began.
    access: Accesses,
    display: Option<DisplayFacts>,
    /// The camera is asked for to look at, and what the system says of it.
    preview: Option<Access>,
    preview_on: bool,
    error: Option<Text>,
    recent: VecDeque<Note>,
    recordings: Vec<RecordingInfo>,
    /// The exports of this run of the app, by the recording's name (a finished one stays until it is made again).
    exports: BTreeMap<String, ExportJob>,
    /// What an export is made with: ffmpeg; a test puts its own.
    renderer: Box<dyn Fn() -> Box<dyn Renderer>>,
}

/// The ffmpeg the settings name, when they name one: the default is where a Mac's package manager usually puts it,
/// not a choice, and a machine that has it elsewhere is not told that the default was not there.
#[must_use]
pub fn named_ffmpeg(settings: &RecorderSettings) -> Option<PathBuf> {
    (settings.ffmpeg != std::path::Path::new(DEFAULT_FFMPEG)).then(|| settings.ffmpeg.clone())
}

fn allows(access: Access) -> bool {
    matches!(access, Access::Allowed | Access::NotAsked)
}

/// The words of a name for the manifest (data, in English): the name itself, or the English of a word.
fn plain(arg: &Arg) -> String {
    match arg {
        Arg::Str(text) => text.clone(),
        Arg::Word { fallback, .. } => fallback.clone(),
        Arg::Int(n) => n.to_string(),
    }
}

impl RecorderHost {
    /// A host with no recording going.
    #[must_use]
    pub fn new(settings: RecorderSettings, facts: Facts, deps: Deps) -> Self {
        let renderer = Self::ffmpeg_renderer(&settings);
        Self {
            recorder: Recorder::new(settings.clone(), deps),
            hook: SessionHook::default(),
            settings,
            facts,
            access: Accesses { screen: Access::NotAsked, camera: Access::NotAsked, microphone: Access::NotAsked },
            display: None,
            preview: None,
            preview_on: false,
            error: None,
            recent: VecDeque::new(),
            recordings: Vec::new(),
            exports: BTreeMap::new(),
            renderer,
        }
    }

    /// The real renderer: ffmpeg, the one the settings name when they name one (and it can be run).
    fn ffmpeg_renderer(settings: &RecorderSettings) -> Box<dyn Fn() -> Box<dyn Renderer>> {
        let named = named_ffmpeg(settings);
        Box::new(move || Box::new(FfmpegRenderer { ffmpeg: None, preferred: named.clone(), out: None }))
    }

    /// What exports are made with from now on (the real one is ffmpeg).
    pub fn set_renderer(&mut self, make: Box<dyn Fn() -> Box<dyn Renderer>>) {
        self.renderer = make;
    }

    /// What the window tells the recorder about the review: the events go to the recording that is going, if any. Made
    /// once and kept by the window's state.
    #[must_use]
    pub fn hook(&self) -> SessionHook {
        self.hook.clone()
    }

    /// The settings it runs with.
    #[must_use]
    pub const fn settings(&self) -> &RecorderSettings {
        &self.settings
    }

    fn dir(&self) -> PathBuf {
        self.settings.folder.clone().unwrap_or_else(|| self.facts.recordings_dir.clone())
    }

    fn push(&mut self, severity: Severity, text: Text) {
        if self.recent.len() == KEPT_NOTICES {
            self.recent.pop_front();
        }
        self.recent.push_back(Note { severity, text });
    }

    fn absorb(&mut self, notices: Vec<Notice>) {
        for notice in notices {
            let note = notice_note(&notice, self.facts.decimal);
            self.push(note.severity, note.text);
        }
    }

    // ---- what the panel shows ----

    /// The indicator, the buttons and the notes for what the recorder reports.
    #[must_use]
    pub fn hud(&self) -> Hud {
        hud(&self.recorder.indicator(), &self.facts.hotkey, self.facts.decimal)
    }

    /// Why the last start failed; `None` after one that worked.
    #[must_use]
    pub const fn error(&self) -> Option<&Text> {
        self.error.as_ref()
    }

    /// The notices the recorder gave, oldest first, at most twenty.
    pub fn recent(&self) -> impl Iterator<Item = &Note> {
        self.recent.iter()
    }

    // ---- what the window asks the system for ----

    /// The camera is asked for to look at (`on`), and what the system says of it.
    pub fn set_preview(&mut self, on: bool, access: Access) {
        self.preview_on = on;
        self.preview = Some(access);
    }

    /// The camera's preview is on.
    #[must_use]
    pub const fn preview(&self) -> bool {
        self.preview_on
    }

    /// What the window asks the system for now: nothing while nothing records or the recording is paused, the
    /// screen, the camera and the microphone (as far as the settings and the system's answers allow) while it
    /// runs, and the camera alone for the preview.
    #[must_use]
    pub fn wants(&self) -> Wants {
        let mut wants = Wants::default();
        if self.recorder.indicator().state == State::Recording {
            let s = &self.settings;
            if s.screen {
                match (s.screen_source, self.display) {
                    (ScreenKind::OwnFrames, _) => wants.tap = true,
                    (ScreenKind::Display, Some(d)) if allows(self.access.screen) => {
                        let (width, height) = consumer_size(d.width, d.height, s.screen_width);
                        if width > 0 {
                            wants.screen = Some(ScreenWant { display_id: d.id, width, height, exclude: s.screen_exclude.clone() });
                        }
                    }
                    _ => {}
                }
            }
            wants.camera = s.camera && allows(self.access.camera);
            wants.microphone = s.microphone && allows(self.access.microphone);
        }
        if self.preview_on && self.preview.is_some_and(allows) {
            wants.camera = true;
        }
        wants
    }

    // ---- a recording ----

    /// Starts a recording of the display `pick_display` chooses with what the system allows; `false` (and [`RecorderHost::error`] says why) when it does
    /// not start. A device the system refused is named in the panel and the others record.
    pub fn start(&mut self, access: Accesses, devices: Devices, window_display: DisplayFacts, displays: &[DisplayFacts]) -> bool {
        self.error = None;
        let (display, missing) = pick_display(self.settings.screen_display, window_display, displays);
        if self.settings.screen && self.settings.screen_source == ScreenKind::Window {
            // Recording one window needs the system's id of AzReview's window, which azul does not give yet; the
            // whole display is never recorded in its place (that is more than was asked for).
            self.error = Some(Text::key("azreview-rec-error-window-source"));
            return false;
        }
        let who = plain(&self.facts.who);
        let screen = if self.settings.screen_source == ScreenKind::OwnFrames { access_perm(TrackName::Screen, Access::Allowed, &who) } else { access_perm(TrackName::Screen, access.screen, &who) };
        let request = StartRequest {
            recordings_dir: self.dir(),
            app: self.facts.app.clone(),
            workspace: self.facts.workspace.clone(),
            host: self.facts.host.clone(),
            utc_offset_minutes: self.facts.utc_offset_minutes,
            camera: access_perm(TrackName::Camera, access.camera, &who),
            microphone: access_perm(TrackName::Microphone, access.microphone, &who),
            screen,
            camera_device: devices.camera,
            microphone_device: devices.microphone,
            screen_device: devices.screen,
        };
        match self.recorder.start(request) {
            Ok(_) => {
                self.hook.set(self.recorder.event_sink());
                self.access = access;
                self.display = Some(display);
                self.recent.clear();
                if let Some(text) = missing {
                    self.push(Severity::Warning, text);
                }
                true
            }
            Err(e) => {
                self.error = Some(start_error_text(&e, self.facts.decimal));
                false
            }
        }
    }

    /// Stops the recording: its files are finished, it is in the list, and the panel says where it was saved.
    pub fn stop(&mut self) {
        self.hook.set(None);
        let Some(finished) = self.recorder.stop("stopped") else { return };
        self.display = None;
        let saved = Phrase::new("azreview-rec-notice-saved")
            .arg("name", finished.name.as_str())
            .arg("length", duration_text(finished.recorded_us))
            .arg("size", size_text(finished.media_bytes, self.facts.decimal));
        self.push(Severity::Info, Text::from(saved));
        self.refresh();
    }

    /// The pause key: pauses a recording, resumes a paused one, does nothing when nothing records.
    pub fn toggle_pause(&mut self) {
        let notices = self.recorder.toggle_pause();
        self.absorb(notices);
    }

    /// The timer's tick: the recorder looks after the disk, the idle time and the length of the session.
    pub fn tick(&mut self) {
        let notices = self.recorder.tick();
        // The recorder ends a session by itself at its length limit: the events then have nowhere to go.
        self.hook.set(self.recorder.event_sink());
        self.absorb(notices);
    }

    /// The screen is locked or unlocked.
    pub fn session_locked(&mut self, locked: bool) {
        let notices = self.recorder.set_locked(locked);
        self.absorb(notices);
    }

    /// What the desktop is doing (the app in front, the pointer, the lock, the display's sleep).
    pub fn desktop_sample(&mut self, sample: &DesktopSample) {
        let notices = self.recorder.desktop_sample(sample);
        self.absorb(notices);
    }

    /// The screen went away for a while (the display slept, was unplugged).
    pub fn screen_gap(&mut self, why: &str) {
        let notices = self.recorder.screen_gap(why);
        self.absorb(notices);
    }

    /// A track's source broke for good.
    pub fn track_failed(&mut self, track: TrackName, why: &str) {
        let notices = self.recorder.track_failed(track, why);
        self.absorb(notices);
    }

    // ---- what comes in ----

    /// A picture of the screen (the capture's, at the size [`RecorderHost::wants`] asked for).
    pub fn screen_frame(&mut self, frame: RawFrame, scale: f32) {
        self.recorder.screen_frame(frame, scale);
    }

    /// The window's own frame, from its tap.
    pub fn tap(&mut self, frame: &TapFrame<'_>) -> Option<Tapped> {
        self.recorder.tap(frame)
    }

    /// A picture of the camera.
    pub fn camera_frame(&mut self, frame: RawFrame) {
        self.recorder.camera_frame(frame);
    }

    /// A chunk of the microphone.
    pub fn audio_chunk(&mut self, samples: &[f32], sample_rate: u32, channels: u16) {
        self.recorder.audio_chunk(samples, sample_rate, channels);
    }

    /// Where the app sends the events of the review (`None` while nothing records).
    #[must_use]
    pub fn event_sink(&self) -> Option<EventSink> {
        self.recorder.event_sink()
    }

    /// One event of the review for the recording, stamped by its clock: nothing is written when nothing records
    /// (and what the caps refuse is dropped, as the recorder counts it).
    pub fn event(&self, kind: &str, fields: &str) {
        let _ = self.recorder.event(kind, fields);
    }

    // ---- the list of recordings ----

    /// The name of the recording that is going (or paused), when there is one.
    #[must_use]
    pub fn current(&self) -> Option<String> {
        self.recorder.session_dir()?.file_name().map(|n| n.to_string_lossy().into_owned())
    }

    /// Reads the list of recordings from the disk.
    pub fn refresh(&mut self) {
        self.recordings = list_recordings(&self.dir());
    }

    /// The rows of the list as it was last read.
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let jobs: BTreeMap<String, JobState> = self.exports.iter().map(|(name, job)| (name.clone(), job.state())).collect();
        rows(&self.recordings, self.facts.decimal, &jobs)
    }

    // ---- exporting a recording ----

    fn refuse_export(&mut self, name: &str, why: &str) -> bool {
        self.push(Severity::Warning, Text::from(Phrase::new("azreview-rec-error-export").arg("name", name).arg("why", why)));
        false
    }

    /// Makes a video of the recording called `name`, on a thread of its own; `false` (and the panel says why) when
    /// it cannot be: it is not a recording of the list, it is being recorded or exported now, it is not finished, or
    /// it has no screen (the video is made of the screen's pictures).
    pub fn export(&mut self, name: &str) -> bool {
        let Some(path) = self.path_of(name) else { return self.refuse_export(name, "it is not a recording of the list") };
        if self.current().is_some_and(|n| n == name) {
            return self.refuse_export(name, "it is being recorded now");
        }
        if self.exports.get(name).is_some_and(ExportJob::is_running) {
            return self.refuse_export(name, "it is being exported already");
        }
        let Some(info) = self.recordings.iter().find(|r| r.name == name) else { return self.refuse_export(name, "it is not a recording of the list") };
        if !matches!(info.state, Some(SessionState::Complete | SessionState::Recovered)) {
            return self.refuse_export(name, "it is not finished");
        }
        if !info.tracks.contains(&"screen") {
            return self.refuse_export(name, "it has no screen: the video is made of the screen's pictures");
        }
        self.exports.insert(name.to_string(), ExportJob::start(path, (self.renderer)()));
        true
    }

    /// Asks the export of the recording called `name` to stop.
    pub fn cancel_export(&mut self, name: &str) {
        if let Some(job) = self.exports.get(name) {
            job.cancel();
        }
    }

    /// The video of the recording called `name`, when it has one.
    #[must_use]
    pub fn video_of(&self, name: &str) -> Option<PathBuf> {
        self.path_of(name).map(|dir| video_path(&dir)).filter(|video| video.is_file())
    }

    /// One mark for each export of this run: it changes when the row of that recording would look different (how far
    /// it is, that it is done, failed or cancelled), so that the window draws again then and not otherwise.
    #[must_use]
    pub fn export_marks(&self) -> Vec<(String, u8)> {
        self.exports
            .iter()
            .map(|(name, job)| {
                let mark = match job.state() {
                    JobState::Running { fraction } => (fraction.clamp(0.0, 1.0) * 100.0).floor().min(99.0) as u8,
                    JobState::Done(_) => 100,
                    JobState::Failed(_) => 101,
                    JobState::Cancelled => 102,
                };
                (name.clone(), mark)
            })
            .collect()
    }

    /// An export is going: the window keeps looking at it.
    #[must_use]
    pub fn exports_running(&self) -> bool {
        self.exports.values().any(ExportJob::is_running)
    }

    /// Puts right the recordings an earlier run of the app left cut off, and says how many; how many.
    pub fn recover(&mut self) -> usize {
        let (done, failed) = recover_all(&self.dir());
        if !done.is_empty() {
            self.push(Severity::Info, Text::from(Phrase::new("azreview-rec-notice-recovered").arg("count", done.len())));
        }
        for (name, why) in failed {
            self.push(Severity::Warning, Text::from(Phrase::new("azreview-rec-error-recover").arg("name", name.as_str()).arg("why", why.as_str())));
        }
        self.refresh();
        done.len()
    }

    /// The folder of the recording called `name`, when there is one (a name that is a path is none).
    #[must_use]
    pub fn path_of(&self, name: &str) -> Option<PathBuf> {
        if !is_safe_relative(name) || name.contains('/') {
            return None;
        }
        let path = self.dir().join(name);
        path.is_dir().then_some(path)
    }

    /// Deletes the recording called `name`: only a recording that is not being written. What is refused is said.
    pub fn delete(&mut self, name: &str) -> bool {
        let going = self.current().is_some_and(|n| n == name);
        let exporting = self.exports.get(name).is_some_and(ExportJob::is_running);
        let result = if going {
            Err(String::from("it is being recorded now"))
        } else if exporting {
            Err(String::from("it is being exported now"))
        } else {
            delete_recording(&self.dir(), name).map_err(|e| e.to_string())
        };
        match result {
            Ok(_) => {
                self.exports.remove(name);
                self.refresh();
                true
            }
            Err(why) => {
                self.push(Severity::Warning, Text::from(Phrase::new("azreview-rec-error-delete").arg("name", name).arg("why", why)));
                false
            }
        }
    }
}

#[cfg(test)]
#[path = "recorder_host_test.rs"]
mod recorder_host_test;
