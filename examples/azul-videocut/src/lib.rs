//! AzVideoCut: a Premiere-like video editor on the public azul API.
//!
//! The window is azul's S3 `TimelineShell`: the media bin, the source
//! monitor, the program monitor (the `<main>`) and the effect controls over
//! the timeline (azul's `Timeline` widget) with the audio meters at its
//! edge; the menu row is azul's `Titlebar` (the window is `NoTitle`) over a
//! toolbar of the tools; a status bar under it all.
//!
//! The pictures (see `render`, `decode`, `export`): the program monitor
//! shows the sequence at the playhead, composed on the CPU from the frames
//! of the clips under it - generated patterns, or frames a `VideoDecoder`
//! decodes from an MP4 (`Mp4Demuxer`: the keyframe at or before the frame,
//! then forward). Every picture is made by a short-lived worker job (an
//! azul `Thread` that ends when it is done) and shown through ONE image node
//! updated in place (`change_node_image`); a scrub asks for one frame at a
//! time and keeps only the newest request. Play starts a decode-ahead job
//! and a timer at the monitor's frame interval that shows the frame due at
//! each tick; pause ends both, so an idle editor runs no timer and no
//! thread. Export renders on a worker, encodes with `VideoEncoder::encode_at`
//! and muxes with `Mp4Muxer` (Y4M where there is no H.264 encoder).
//!
//! The sound (see `levels`): a file with audio (a sound file, or an MP4 with a
//! sound track) goes on the audio tracks; a worker reads each such file once
//! with `AudioFileDecoder` into its levels per channel, and the meters show
//! the level of the clips under the playhead. Playing at normal speed, the clips' sound is
//! mixed and played (see `sound`), and the meters move in place with every frame shown.
//!
//! The project is files on a Drive (`store`): `videocut/<uuid>/project.json`
//! and `media/`, `exports/`, saved after every edit by a job.
//!
//! On stdout, for scripts (`scripts/azvideocut_e2e.py`): `AZVIDEOCUT_PROJECT
//! <id> <clips> <frames>`, `AZVIDEOCUT_SAMPLE <encoded|generated>`,
//! `AZVIDEOCUT_EDIT <edit> <clips> <frames>`, `AZVIDEOCUT_PLAYHEAD <frame>`,
//! `AZVIDEOCUT_FRAME <frame>` (a program frame shown), `AZVIDEOCUT_SAVED
//! <id>`, `AZVIDEOCUT_EXPORT <done> <total>`, `AZVIDEOCUT_EXPORTED <key>
//! <bytes> <format>`, `AZVIDEOCUT_LEVELS <media> <slices>` (a file's levels
//! read), `AZVIDEOCUT_ERROR <message>`.

pub mod args;
pub mod decode;
pub mod export;
pub mod levels;
/// AzVideoCut's words in English and German.
mod l10n;
#[cfg(test)]
mod l10n_tests;
pub mod model;
pub mod render;
pub mod sample;
pub mod sound;
pub mod store;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Instant as StdInstant,
};

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DialogOnCloseCallbackType, NumberInputOnValueChangeCallbackType,
        StandardDialogOnEventCallbackType,
        SegmentedOnChangeCallbackType, SliderOnValueChangeCallbackType, TimelineOnEventCallbackType,
        UpdateImageType,
    },
    css::DarkLightMode,
    dialog::{FileDialog, FileOpenMultiResult},
    dom::{AccessibilityRole, NodeId, VirtualKeyCode},
    file::{FilePath, FileTypeList},
    image::{ImageRef, RawImage, RawImageFormat},
    option::{OptionFileTypeList, OptionString},
    prelude::*,
    shells::{ShellEmptyState, ShellThemeAccent, ShellThemeScope, TimelineShell},
    str::String as AzString,
    time::SystemTimeDiff,
    uuid::Uuid,
    vec::{
        StatusBarSegmentVec, StringVec, TimelineClipVec, TimelineTrackVec,
        U8Vec, U8VecRef,
    },
    audio::AudioFileDecoder,
    video::{VideoDecoder, VideoEncoder},
    widgets::{
        AboutDialog, ButtonType, Dialog, DialogState, NumberInputState, ProgressDialog, Segmented,
        SegmentedState, Slider, SliderState, StandardDialogEvent, StandardDialogEventKind,
        StatusBar, StatusBarSegment, Timeline, TimelineClip, TimelineClipTint, TimelineEdge,
        LevelMeter, LevelMeterOrientation, LevelMeterThrottle, TimelineEvent, TimelineEventKind,
        TimelineTrack, TimelineTrackKind, Titlebar,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::LanguagePref,
    l10n::{named, t, t_args, t_label, Arg},
    pieces::Clickable,
    shortcuts::Shortcut,
    ui as kit,
};
use azul_pim::write_queue::{run_batch, BatchResult, CloseHold, MainClose, Write, WriteQueue};
use azul_storage::{Drive, LocalDrive};

use crate::{
    args::{Args, Screen, SPEC},
    decode::{Library, MediaFiles},
    export::{ExportRange, ExportSettings, ExportShared, OutputFormat},
    model::{
        Edge, Edit, Effects, Frame, MediaItem, MediaSource, Project, SourceMarks, TrackKind,
        Transition, TransitionKind,
    },
    render::{compose, fit_to, fit_within, Canvas, FrameSource},
};

// ==== constants ====

/// The program and source monitors render at most this size.
const MONITOR_W: u32 = 640;
const MONITOR_H: u32 = 360;
/// The media bin's thumbnails.
const THUMB_W: u32 = 96;
const THUMB_H: u32 = 54;
/// Frames the playback job renders ahead of the playhead.
const PLAY_AHEAD: usize = 6;
/// The markers of the nodes updated in place.
/// (Defined once, with the app's `__azvideocut_` prefix.)
const PROGRAM_IMAGE: AzString = AzString::from_const_str("__azvideocut_program-image");
/// The meters (azul's `LevelMeter`) and the peak under them, moved in place while playing.
const METER_LEFT: AzString = AzString::from_const_str("__azvideocut_meter-left");
const METER_RIGHT: AzString = AzString::from_const_str("__azvideocut_meter-right");
const METER_PEAK: AzString = AzString::from_const_str("__azvideocut_meter-peak");
/// How often a meter may move while playing: every frame of a 30 fps sequence.
const METER_INTERVAL_MS: u64 = 30;
const PROGRAM_TC: AzString = AzString::from_const_str("__azvideocut_program-tc");
const SOURCE_IMAGE: AzString = AzString::from_const_str("__azvideocut_source-image");
/// The export choices.
const EXPORT_SIZES: [(u32, u32); 3] = [(1280, 720), (854, 480), (640, 360)];
const EXPORT_BITRATES: [u32; 3] = [8000, 4000, 1500];

// ==== state ====

/// The timeline tools (Premiere's V / C / B / Y / H / Z).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Razor,
    Ripple,
    Slip,
    Hand,
    Zoom,
}

impl Tool {
    const ALL: [Tool; 6] = [
        Tool::Select,
        Tool::Razor,
        Tool::Ripple,
        Tool::Slip,
        Tool::Hand,
        Tool::Zoom,
    ];

    /// Its words: a key of AzVideoCut's, said in the window's language.
    fn label(self) -> &'static str {
        match self {
            Tool::Select => "azvideocut-tool-select",
            Tool::Razor => "azvideocut-tool-razor",
            Tool::Ripple => "azvideocut-tool-ripple",
            Tool::Slip => "azvideocut-tool-slip",
            Tool::Hand => "azvideocut-tool-hand",
            Tool::Zoom => "azvideocut-tool-zoom",
        }
    }

    fn index(self) -> usize {
        Tool::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// Which monitor I / O and the transport keys act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Monitor {
    Source,
    Program,
}

/// A picture job's coalescing: one running, the newest request waiting.
#[derive(Debug, Clone, Copy, Default)]
struct Coalesce {
    running: bool,
    wanted: Option<Frame>,
}

/// The playback job's queue of rendered frames, shared with its thread.
#[derive(Default)]
struct PlaybackShared {
    frames: Mutex<VecDeque<(Frame, Canvas)>>,
    room: Condvar,
    stop: AtomicBool,
    /// What the sound thread handed to the output.
    sound: sound::SoundReport,
}

/// Playback in progress.
struct Playback {
    shared: Arc<PlaybackShared>,
    timer: TimerId,
    thread: ThreadId,
    /// The frame it started at, when, and how fast (J / L: -4..4).
    start: Frame,
    started: StdInstant,
    speed: i64,
    /// The last frame shown.
    shown: Frame,
    /// When the playhead and timecode were last moved on screen.
    last_ui: StdInstant,
    /// The meters' throttles (left, right): they move with the frames shown, in place.
    meters: [LevelMeterThrottle; 2],
}

/// The export dialog and a running export.
struct ExportState {
    open: bool,
    size: usize,
    bitrate: usize,
    /// 0: the whole sequence, 1: in to out, 2: the first 25 frames.
    range: usize,
    shared: Option<Arc<ExportShared>>,
    timer: Option<TimerId>,
    last_line: String,
}

/// Everything the window shows and its jobs share.
pub struct VideoCut {
    drive: Arc<dyn Drive>,
    /// Where the drive is, for the settings.
    drive_root: String,
    files: Arc<MediaFiles>,
    args: Args,
    project: Option<Project>,
    /// Bumps with every change of the sequence (drops stale pictures).
    revision: u64,
    loading: bool,
    status: String,
    tool: Tool,
    snapping: bool,
    /// The playhead, in sequence frames.
    playhead: Frame,
    /// The timeline's view: its first second and its zoom.
    view_start: f64,
    pps: f32,
    selected: Vec<u64>,
    selected_media: Option<u64>,
    /// The source monitor's media and marks.
    source: Option<SourceMarks>,
    /// The program monitor's in / out marks.
    program_in: Option<Frame>,
    program_out: Option<Frame>,
    active: Monitor,
    program_frame: Option<(Frame, Canvas)>,
    source_frame: Option<(u64, Frame, Canvas)>,
    program_job: Coalesce,
    source_job: Coalesce,
    /// The project's saves: one batch in flight, the newest project waiting behind it, so an
    /// older snapshot never lands after a newer one.
    writes: WriteQueue,
    /// The window's close, held while saves land; a save that failed is said once.
    close_hold: CloseHold,
    /// The bin's thumbnails and the clips' heads.
    thumbs: HashMap<u64, ImageRef>,
    /// The levels of each media item with sound that were read, for the meters.
    levels: HashMap<u64, levels::Envelope>,
    /// The media items whose levels were asked for (read or on their way), so each file is
    /// read once.
    levels_asked: HashSet<u64>,
    playback: Option<Playback>,
    export: ExportState,
    /// azul-appkit's kit: settings (theme and mode remembered), the
    /// settings page, the shortcuts table.
    kit: RefAny,
    about_open: bool,
    window_width: f32,
    dark: bool,
    flora: bool,
    encoder: String,
}

impl VideoCut {
    fn fps(&self) -> u32 {
        self.project.as_ref().map_or(25, |p| p.sequence.fps)
    }

    fn seconds(&self, f: Frame) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let s = f as f64 / f64::from(self.fps());
        s
    }

    fn frame_of(&self, seconds: f64) -> Frame {
        #[allow(clippy::cast_possible_truncation)]
        let f = (seconds * f64::from(self.fps())).round() as Frame;
        f.max(0)
    }

    fn end(&self) -> Frame {
        self.project.as_ref().map_or(0, |p| p.sequence.end())
    }

    /// The monitors' render size for this sequence.
    fn monitor_size(&self) -> (u32, u32) {
        self.project.as_ref().map_or((MONITOR_W, MONITOR_H), |p| {
            fit_within(p.sequence.width, p.sequence.height, MONITOR_W, MONITOR_H)
        })
    }

    fn timecode(&self, f: Frame) -> String {
        Timeline::format_timecode(self.seconds(f), self.fps() as f32)
            .as_str()
            .to_string()
    }

    fn clip_count(&self) -> usize {
        self.project
            .as_ref()
            .map_or(0, |p| p.sequence.tracks.iter().map(|t| t.clips.len()).sum())
    }
}

/// A line for the scripts on stdout.
fn announce(line: &str) {
    println!("AZVIDEOCUT_{line}");
}

// ==== jobs ====
//
// Every blocking piece of work (the Drive, a decoder, an encoder) runs as a
// job on its own azul `Thread`, which ends when the job is done; its answer
// comes back through the thread's write-back (`on_job_done`).

/// A piece of work for a worker.
enum Job {
    /// Make the sample project under `sample_id` (its clips encoded where
    /// the machine can) or open `project` (or the first project there is).
    Load {
        sample_id: Option<String>,
        project: Option<String>,
    },
    /// A batch of the write queue (`project.json` saves).
    Write(Vec<Write>),
    /// The program picture at `frame`.
    Render {
        project: Project,
        frame: Frame,
        width: u32,
        height: u32,
        revision: u64,
    },
    /// The source picture of `media` at `frame`.
    RenderSource {
        media: MediaItem,
        frame: Frame,
        width: u32,
        height: u32,
        fps: u32,
    },
    /// Probe and add the files at `paths`.
    Import { paths: Vec<String>, fps: u32 },
    /// Read the levels of `media`'s sound for the meters.
    Levels { media: MediaItem },
    /// Render, encode, mux and store an export.
    Export {
        project: Project,
        settings: ExportSettings,
        shared: Arc<ExportShared>,
    },
}

/// A job and what it works with.
struct JobInit {
    job: Option<Job>,
    drive: Arc<dyn Drive>,
    files: Arc<MediaFiles>,
}

/// A job's answer.
enum Outcome {
    Loaded {
        result: Result<(Project, Vec<(u64, Canvas)>), String>,
        /// For the sample: whether its clips were encoded to MP4.
        sample_encoded: Option<bool>,
    },
    NoProject,
    Written(BatchResult),
    Rendered {
        frame: Frame,
        revision: u64,
        picture: Canvas,
        error: Option<String>,
    },
    SourceRendered {
        media: u64,
        frame: Frame,
        picture: Option<Canvas>,
        error: Option<String>,
    },
    Imported {
        items: Vec<(MediaItem, Option<Canvas>)>,
        errors: Vec<String>,
    },
    Levels {
        media: u64,
        result: Result<levels::Envelope, String>,
    },
    Exported {
        result: Result<(String, usize, OutputFormat), String>,
    },
}

/// The write-back's payload.
struct Done {
    outcome: Option<Outcome>,
}

/// The bin's thumbnails of `project`'s media (frame 0 of each).
fn thumbnails(project: &Project, drive: &Arc<dyn Drive>, files: &Arc<MediaFiles>) -> Vec<(u64, Canvas)> {
    let mut library = Library::new(files.clone(), drive.clone(), project.sequence.fps);
    project
        .media
        .iter()
        .filter_map(|m| {
            let picture = library.picture(m, 0, THUMB_W * 2, THUMB_H * 2)?;
            Some((m.id, fit_to(&picture, THUMB_W, THUMB_H)))
        })
        .collect()
}

fn run_job(job: Job, drive: &Arc<dyn Drive>, files: &Arc<MediaFiles>) -> Outcome {
    match job {
        Job::Load { sample_id: Some(id), .. } => {
            let mut project = sample::sample_project(id);
            let mut encoded = 0;
            for (name, pattern, frames) in &sample::CLIPS {
                let Some(bytes) = sample::encode_clip(pattern, *frames) else {
                    continue;
                };
                let key = store::media_key(&project.id, name);
                if drive.put(&key, &bytes).is_ok() {
                    sample::use_encoded(&mut project, name, key);
                    encoded += 1;
                }
            }
            let result = store::save_project(drive.as_ref(), &project).map(|()| {
                let thumbs = thumbnails(&project, drive, files);
                (project, thumbs)
            });
            Outcome::Loaded {
                result,
                sample_encoded: Some(encoded == sample::CLIPS.len()),
            }
        }
        Job::Load { sample_id: None, project } => {
            let id = match project {
                Some(id) => id,
                None => match store::list_projects(drive.as_ref()) {
                    Ok(mut ids) if !ids.is_empty() => {
                        ids.sort();
                        ids.remove(0)
                    }
                    Ok(_) => return Outcome::NoProject,
                    Err(e) => {
                        return Outcome::Loaded {
                            result: Err(e),
                            sample_encoded: None,
                        }
                    }
                },
            };
            let result = store::load_project(drive.as_ref(), &id).map(|project| {
                let thumbs = thumbnails(&project, drive, files);
                (project, thumbs)
            });
            Outcome::Loaded {
                result,
                sample_encoded: None,
            }
        }
        Job::Write(batch) => Outcome::Written(run_batch(drive.as_ref(), batch)),
        Job::Render {
            project,
            frame,
            width,
            height,
            revision,
        } => {
            let mut library = Library::new(files.clone(), drive.clone(), project.sequence.fps);
            let picture = compose(&project, frame, width, height, &mut library);
            Outcome::Rendered {
                frame,
                revision,
                picture,
                error: library.error,
            }
        }
        Job::RenderSource {
            media,
            frame,
            width,
            height,
            fps,
        } => {
            let mut library = Library::new(files.clone(), drive.clone(), fps);
            let picture = library
                .picture(&media, frame, width, height)
                .map(|p| fit_to(&p, width, height));
            Outcome::SourceRendered {
                media: media.id,
                frame,
                picture,
                error: library.error,
            }
        }
        Job::Import { paths, fps } => {
            let mut items = Vec::new();
            let mut errors = Vec::new();
            for path in paths {
                let name = PathBuf::from(&path)
                    .file_name()
                    .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
                let probed = std::fs::read(&path)
                    .map_err(|e| format!("{name}: {e}"))
                    .and_then(|bytes| decode::probe(&bytes, THUMB_W, THUMB_H).map_err(|e| format!("{name}: {e}")));
                // The sound: an MP4's sound track, or a file that is sound alone.
                let sound = AudioFileDecoder::open(AzString::from(path.as_str()));
                let sound = sound.is_open().then(|| sound.info());
                match (probed, sound) {
                    (Ok(probe), sound) => {
                        #[allow(clippy::cast_possible_truncation)]
                        let frames = (probe.duration_ms / 1000.0 * f64::from(fps)).floor() as Frame;
                        items.push((
                            MediaItem {
                                id: 0,
                                name,
                                source: MediaSource::Path { path },
                                frames: frames.max(1),
                                width: probe.width,
                                height: probe.height,
                                fps: f64::from(probe.fps),
                                codec: String::from("H.264"),
                                has_video: true,
                                has_audio: sound.is_some(),
                            },
                            probe.thumbnail,
                        ));
                    }
                    (Err(_), Some(sound)) => {
                        #[allow(clippy::cast_possible_truncation)]
                        let frames = (sound.duration_s * f64::from(fps)).floor() as Frame;
                        items.push((
                            MediaItem {
                                id: 0,
                                name,
                                source: MediaSource::Path { path },
                                frames: frames.max(1),
                                width: 0,
                                height: 0,
                                fps: 0.0,
                                codec: sound.codec.as_str().to_string(),
                                has_video: false,
                                has_audio: true,
                            },
                            None,
                        ));
                    }
                    (Err(e), None) => errors.push(e),
                }
            }
            Outcome::Imported { items, errors }
        }
        Job::Levels { media } => {
            let result = sound::open_decoder(&media, drive.as_ref(), &files)
                .map(|mut decoder| levels::envelope_from(&mut decoder));
            Outcome::Levels {
                media: media.id,
                result,
            }
        }
        Job::Export {
            project,
            settings,
            shared,
        } => {
            let mut library = Library::new(files.clone(), drive.clone(), project.sequence.fps);
            let result = export::run_export(&project, &settings, &mut library, &shared).and_then(
                |(bytes, format)| {
                    let key = store::export_key(&project.id, &export::output_name(&settings.name, format));
                    drive
                        .put(&key, &bytes)
                        .map_err(|e| format!("storing the export failed: {e}"))?;
                    Ok((key, bytes.len(), format))
                },
            );
            if let Ok(mut p) = shared.progress.lock() {
                p.finished = true;
                match &result {
                    Ok((key, _, _)) => p.output = Some(key.clone()),
                    Err(e) => p.error = Some(e.clone()),
                }
            }
            Outcome::Exported { result }
        }
    }
}

/// A worker: the blocking job, then its answer to the UI thread.
extern "C" fn job_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((job, drive, files)) = init.downcast_mut::<JobInit>().and_then(|mut i| {
        let job = i.job.take()?;
        Some((job, i.drive.clone(), i.files.clone()))
    }) else {
        return;
    };
    let outcome = run_job(job, &drive, &files);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_job_done,
        RefAny::new(Done {
            outcome: Some(outcome),
        }),
    )));
}

/// Reads the levels of every media item with sound not asked for yet, each on a worker.
fn request_levels(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut) {
    let wanted: Vec<MediaItem> = app.project.as_ref().map_or_else(Vec::new, |p| {
        p.media
            .iter()
            .filter(|m| m.has_audio && !m.is_generated() && !app.levels_asked.contains(&m.id))
            .cloned()
            .collect()
    });
    for media in wanted {
        app.levels_asked.insert(media.id);
        spawn(info, app_ref, app, Job::Levels { media });
    }
}

/// Starts `job` on a worker.
fn spawn(info: &mut CallbackInfo, app_ref: &RefAny, app: &VideoCut, job: Job) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(JobInit {
                job: Some(job),
                drive: app.drive.clone(),
                files: app.files.clone(),
            }),
            app_ref.clone(),
            job_thread,
        ),
    );
}

/// Saves the project (after an edit) through the write queue: one save in flight, the newest
/// project waiting behind it.
fn save(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut) {
    if let Some(project) = app.project.as_ref() {
        store::queue_project(&mut app.writes, project);
    }
    pump_writes(info, app_ref, app);
}

/// Starts the write queue's next batch when none is in flight.
fn pump_writes(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut) {
    if let Some(batch) = app.writes.take() {
        spawn(info, app_ref, app, Job::Write(batch));
    }
}

/// The window is asked to close: it waits for the saves on their way (`on_job_done` closes it
/// once they landed), and says once what did not land.
extern "C" fn on_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    let failures = app.writes.failures().len();
    match app.close_hold.request(!app.writes.is_idle(), failures) {
        MainClose::Close => Update::DoNothing,
        MainClose::Wait => {
            announce("CLOSE_WAITS");
            pump_writes(&mut info, &app_ref, app);
            // Last: the veto rides on whatever this callback queued.
            info.prevent_window_close();
            Update::DoNothing
        }
        MainClose::Tell => {
            app.status = t_args("azvideocut-changes-not-saved", &[("count", Arg::from(failures))]);
            info.prevent_window_close();
            Update::RefreshDom
        }
    }
}

/// A job is done (UI thread).
extern "C" fn on_job_done(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(outcome) = msg.downcast_mut::<Done>().and_then(|mut d| d.outcome.take()) else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    match outcome {
        Outcome::Loaded {
            result,
            sample_encoded,
        } => {
            app.loading = false;
            match result {
                Ok((project, thumbs)) => {
                    for (id, picture) in thumbs {
                        if let Some(image) = image_of(&picture) {
                            app.thumbs.insert(id, image);
                        }
                    }
                    app.project = Some(project);
                    app.revision += 1;
                    app.playhead = 0;
                    app.view_start = 0.0;
                    app.selected.clear();
                    app.status = t("azvideocut-ready");
                    let id = app
                        .project
                        .as_ref()
                        .map_or_else(String::new, |p| p.id.clone());
                    announce(&format!("PROJECT {id} {} {}", app.clip_count(), app.end()));
                    if let Some(encoded) = sample_encoded {
                        announce(if encoded {
                            "SAMPLE encoded"
                        } else {
                            "SAMPLE generated"
                        });
                    }
                    request_program_frame(&mut info, &app_ref, app);
                    request_levels(&mut info, &app_ref, app);
                }
                Err(e) => {
                    app.status = e.clone();
                    announce(&format!("ERROR {e}"));
                }
            }
            Update::RefreshDom
        }
        Outcome::NoProject => {
            app.loading = false;
            Update::RefreshDom
        }
        Outcome::Written(result) => {
            for key in &result.done {
                if let Some(id) = store::project_id_of(key) {
                    announce(&format!("SAVED {id}"));
                }
            }
            let mut refresh = false;
            for (_, why) in &result.failed {
                app.status = t_args("azvideocut-save-failed", &[("why", Arg::from(why.as_str()))]);
                announce(&format!("ERROR {why}"));
                refresh = true;
            }
            app.writes.finish(result.failed);
            pump_writes(&mut info, &app_ref, app);
            let failures = app.writes.failures().len();
            match app.close_hold.landed(!app.writes.is_idle(), failures) {
                Some(MainClose::Close) => {
                    announce("CLOSED_AFTER_SAVE");
                    info.close_window();
                }
                Some(_) => {
                    // Not lost without a word: the window stays, says so, and the next close
                    // passes.
                    app.status =
                        t_args("azvideocut-changes-not-saved", &[("count", Arg::from(failures))]);
                    refresh = true;
                }
                None => {}
            }
            if refresh {
                Update::RefreshDom
            } else {
                Update::DoNothing
            }
        }
        Outcome::Rendered {
            frame,
            revision,
            picture,
            error,
        } => {
            app.program_job.running = false;
            if let Some(e) = error {
                app.status = e;
            }
            if revision == app.revision && app.playback.is_none() {
                show_in_place(&mut info, PROGRAM_IMAGE, &picture);
                app.program_frame = Some((frame, picture));
                announce(&format!("FRAME {frame}"));
            }
            if let Some(wanted) = app.program_job.wanted.take() {
                if wanted != frame || revision != app.revision {
                    start_program_render(&mut info, &app_ref, app, wanted);
                }
            }
            Update::DoNothing
        }
        Outcome::SourceRendered {
            media,
            frame,
            picture,
            error,
        } => {
            app.source_job.running = false;
            if let Some(e) = error {
                app.status = e;
            }
            if let Some(picture) = picture {
                show_in_place(&mut info, SOURCE_IMAGE, &picture);
                app.source_frame = Some((media, frame, picture));
            }
            if let Some(wanted) = app.source_job.wanted.take() {
                if wanted != frame {
                    start_source_render(&mut info, &app_ref, app, wanted);
                }
            }
            Update::DoNothing
        }
        Outcome::Imported { items, errors } => {
            let mut added = 0;
            if let Some(project) = app.project.as_mut() {
                for (item, thumb) in items {
                    let id = project.add_media(item);
                    added += 1;
                    if let Some(image) = thumb.as_ref().and_then(image_of) {
                        app.thumbs.insert(id, image);
                    }
                }
            }
            app.status = if errors.is_empty() {
                t_args("azvideocut-imported", &[("count", Arg::from(added))])
            } else {
                t_args(
                    "azvideocut-imported-errors",
                    &[
                        ("count", Arg::from(added)),
                        ("errors", Arg::from(errors.join("; "))),
                    ],
                )
            };
            save(&mut info, &app_ref, app);
            request_levels(&mut info, &app_ref, app);
            Update::RefreshDom
        }
        Outcome::Levels { media, result } => match result {
            Ok(envelope) => {
                announce(&format!("LEVELS {media} {}", envelope.peaks.len()));
                app.levels.insert(media, envelope);
                // The meters change only when the file sounds at the playhead.
                let sounds = app.project.as_ref().is_some_and(|p| {
                    levels::sounding_at(p, app.playhead)
                        .iter()
                        .any(|(m, _)| *m == media)
                });
                if sounds {
                    Update::RefreshDom
                } else {
                    Update::DoNothing
                }
            }
            Err(e) => {
                announce(&format!("ERROR levels {media}: {e}"));
                Update::DoNothing
            }
        },
        Outcome::Exported { result } => {
            match result {
                Ok((key, bytes, format)) => {
                    app.status = t_args(
                        "azvideocut-exported",
                        &[("key", Arg::from(&key)), ("bytes", Arg::from(bytes))],
                    );
                    announce(&format!("EXPORTED {key} {bytes} {}", format.extension()));
                }
                Err(e) => {
                    app.status = t_args("azvideocut-export-failed", &[("why", Arg::from(&e))]);
                    announce(&format!("ERROR export {e}"));
                }
            }
            Update::RefreshDom
        }
    }
}

// ==== pictures and playback ====

/// A picture as an image for an image node (opaque RGBA8).
fn image_of(c: &Canvas) -> Option<ImageRef> {
    let raw = RawImage::create_rgba8(c.width, c.height, U8Vec::from_vec(c.rgba.clone()), true);
    ImageRef::create_rawimage(raw).into_option()
}

/// An empty picture for an image node with nothing to show yet.
fn blank_image(width: u32, height: u32) -> ImageRef {
    ImageRef::null_image(
        width as usize,
        height as usize,
        RawImageFormat::RGBA8,
        U8VecRef::from(&[][..]),
    )
}

/// Swaps the picture of the image node carrying `marker` in place: no
/// layout, no DOM rebuild.
fn show_in_place(info: &mut CallbackInfo, marker: AzString, c: &Canvas) {
    let Some(image) = image_of(c) else {
        return;
    };
    let Some(node) = info.get_node_id_by_marker(marker).into_option() else {
        return;
    };
    let index = node.node.into_raw();
    if index > 0 {
        info.change_node_image(node.dom, NodeId { inner: index - 1 }, image, UpdateImageType::Content);
    }
}

/// Shows the program picture at the playhead: from the last one when it is
/// that frame, else from a render job (one at a time; while one runs, the
/// newest request waits).
fn request_program_frame(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut) {
    if app.project.is_none() || app.playback.is_some() {
        return;
    }
    let frame = app.playhead;
    if app.program_frame.as_ref().is_some_and(|(f, _)| *f == frame) {
        return;
    }
    if app.program_job.running {
        app.program_job.wanted = Some(frame);
        return;
    }
    start_program_render(info, app_ref, app, frame);
}

fn start_program_render(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut, frame: Frame) {
    let Some(project) = app.project.clone() else {
        return;
    };
    let (width, height) = app.monitor_size();
    app.program_job.running = true;
    let revision = app.revision;
    spawn(
        info,
        app_ref,
        app,
        Job::Render {
            project,
            frame,
            width,
            height,
            revision,
        },
    );
}

/// Shows the source monitor's picture at its position (coalesced like the
/// program's).
fn request_source_frame(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut) {
    let Some(marks) = app.source else {
        return;
    };
    if app
        .source_frame
        .as_ref()
        .is_some_and(|(m, f, _)| *m == marks.media && *f == marks.position)
    {
        return;
    }
    if app.source_job.running {
        app.source_job.wanted = Some(marks.position);
        return;
    }
    start_source_render(info, app_ref, app, marks.position);
}

fn start_source_render(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut, frame: Frame) {
    let Some(marks) = app.source else {
        return;
    };
    let Some(media) = app.project.as_ref().and_then(|p| p.media(marks.media)).cloned() else {
        return;
    };
    // Sound alone has no picture to show.
    if !media.has_video {
        return;
    }
    let fps = app.fps();
    app.source_job.running = true;
    spawn(
        info,
        app_ref,
        app,
        Job::RenderSource {
            media,
            frame,
            width: MONITOR_W,
            height: MONITOR_H,
            fps,
        },
    );
}

/// What the playback job works with.
struct PlaybackInit {
    project: Project,
    start: Frame,
    speed: i64,
    width: u32,
    height: u32,
    shared: Arc<PlaybackShared>,
    drive: Arc<dyn Drive>,
    files: Arc<MediaFiles>,
}

/// Renders the frames playback will show, a few ahead, then waits for the
/// timer to take them (or for the stop).
extern "C" fn playback_thread(mut init: RefAny, _sender: ThreadSender, mut receiver: ThreadReceiver) {
    let Some((project, start, speed, width, height, shared, drive, files)) =
        init.downcast_ref::<PlaybackInit>().map(|i| {
            (
                i.project.clone(),
                i.start,
                i.speed,
                i.width,
                i.height,
                i.shared.clone(),
                i.drive.clone(),
                i.files.clone(),
            )
        })
    else {
        return;
    };
    let mut library = Library::new(files, drive, project.sequence.fps);
    let end = project.sequence.end();
    let mut f = start;
    while f >= 0 && f < end {
        if shared.stop.load(Ordering::Relaxed) {
            break;
        }
        if matches!(receiver.recv().into_option(), Some(ThreadSendMsg::TerminateThread)) {
            break;
        }
        let picture = compose(&project, f, width, height, &mut library);
        let Ok(mut queue) = shared.frames.lock() else {
            break;
        };
        queue.push_back((f, picture));
        while queue.len() >= PLAY_AHEAD && !shared.stop.load(Ordering::Relaxed) {
            queue = match shared.room.wait_timeout(queue, std::time::Duration::from_millis(100)) {
                Ok((q, _)) => q,
                Err(_) => return,
            };
        }
        drop(queue);
        f += speed;
    }
}

/// What the sound job works with.
struct SoundInit {
    project: Project,
    start: Frame,
    shared: Arc<PlaybackShared>,
    drive: Arc<dyn Drive>,
    files: Arc<MediaFiles>,
}

/// Plays the sequence's sound from the playhead until the stop (see `sound`), then says what
/// it handed to the output (`SOUND <samples> <peak>`).
extern "C" fn sound_thread(mut init: RefAny, _sender: ThreadSender, mut receiver: ThreadReceiver) {
    let Some((project, start, shared, drive, files)) = init.downcast_ref::<SoundInit>().map(|i| {
        (
            i.project.clone(),
            i.start,
            i.shared.clone(),
            i.drive.clone(),
            i.files.clone(),
        )
    }) else {
        return;
    };
    let mut stopped = || {
        shared.stop.load(Ordering::Relaxed)
            || matches!(receiver.recv().into_option(), Some(ThreadSendMsg::TerminateThread))
    };
    if let Err(why) = sound::play(&project, start, &drive, &files, &shared.sound, &mut stopped) {
        eprintln!("[azvideocut] playing without sound: {why}");
    }
    announce(&format!(
        "SOUND {} {:.3}",
        shared.sound.samples.load(Ordering::Relaxed),
        shared.sound.peak()
    ));
}

/// The monitor's frame interval in milliseconds (its highest refresh rate;
/// 60 Hz when it says none).
fn monitor_interval_ms(info: &CallbackInfo) -> u64 {
    let hz = info
        .get_current_monitor()
        .into_option()
        .and_then(|m| m.video_modes.as_slice().iter().map(|v| v.refresh_rate).max())
        .filter(|hz| *hz > 0)
        .unwrap_or(60);
    (1000 / u64::from(hz.max(24))).max(4)
}

/// Plays from the playhead at `speed` (frames per frame: 1, 2, 4 forward,
/// negative backward).
fn start_playback(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut, speed: i64) {
    stop_playback(app);
    let Some(project) = app.project.clone() else {
        return;
    };
    let end = project.sequence.end();
    if end == 0 {
        return;
    }
    if speed > 0 && app.playhead >= end - 1 {
        app.playhead = 0;
    }
    if speed < 0 && app.playhead <= 0 {
        app.playhead = end - 1;
    }
    let shared = Arc::new(PlaybackShared::default());
    let (width, height) = app.monitor_size();
    // Only playing forward at normal speed sounds (a shuttle would need time stretching).
    if speed == 1 {
        info.add_thread(
            ThreadId::unique(),
            Thread::create(
                RefAny::new(SoundInit {
                    project: project.clone(),
                    start: app.playhead,
                    shared: shared.clone(),
                    drive: app.drive.clone(),
                    files: app.files.clone(),
                }),
                app_ref.clone(),
                sound_thread,
            ),
        );
    }
    let thread = ThreadId::unique();
    info.add_thread(
        thread,
        Thread::create(
            RefAny::new(PlaybackInit {
                project,
                start: app.playhead,
                speed,
                width,
                height,
                shared: shared.clone(),
                drive: app.drive.clone(),
                files: app.files.clone(),
            }),
            app_ref.clone(),
            playback_thread,
        ),
    );
    let timer = TimerId::unique();
    let get_time = info.get_system_time_fn();
    let interval = monitor_interval_ms(info);
    info.add_timer(
        timer,
        Timer::create(app_ref.clone(), playback_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(interval))),
    );
    app.playback = Some(Playback {
        shared,
        timer,
        thread,
        start: app.playhead,
        started: StdInstant::now(),
        speed,
        shown: app.playhead,
        last_ui: StdInstant::now(),
        meters: [LevelMeterThrottle::create(METER_INTERVAL_MS); 2],
    });
    app.status = t_args("azvideocut-playing-at", &[("speed", Arg::from(speed))]);
}

/// Stops playback; its job ends by itself, its timer ends at its next tick.
fn stop_playback(app: &mut VideoCut) {
    if let Some(pb) = app.playback.take() {
        pb.shared.stop.store(true, Ordering::Relaxed);
        pb.shared.room.notify_all();
        app.playhead = pb.shown;
        app.status = t("azvideocut-stopped");
        let _ = (pb.timer, pb.thread);
    }
}

/// A tick of the monitor's frame interval while playing: shows the frame
/// that is due, moves the timeline a few times a second, ends at the end.
extern "C" fn playback_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut guard) = data.downcast_mut::<VideoCut>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let app = &mut *guard;
    let fps = app.fps();
    let end = app.end();
    let Some(pb) = app.playback.as_mut() else {
        // Stopped by a key or a button: this timer is done.
        return TimerCallbackReturn::terminate_unchanged();
    };
    #[allow(clippy::cast_possible_truncation)]
    let due = pb.start + (pb.started.elapsed().as_secs_f64() * f64::from(fps)) as Frame * pb.speed;
    let mut shown = None;
    if let Ok(mut queue) = pb.shared.frames.lock() {
        while let Some((f, _)) = queue.front() {
            let reached = if pb.speed > 0 { *f <= due } else { *f >= due };
            if !reached {
                break;
            }
            shown = queue.pop_front();
        }
    }
    pb.shared.room.notify_all();
    let finished = if pb.speed > 0 { due >= end } else { due < 0 };
    let refresh_ui = pb.last_ui.elapsed().as_millis() >= 250;
    if refresh_ui {
        pb.last_ui = StdInstant::now();
    }
    if let Some((f, _)) = &shown {
        pb.shown = *f;
    }
    if let Some((f, picture)) = shown {
        show_in_place(&mut info.callback_info, PROGRAM_IMAGE, &picture);
        app.playhead = f;
        app.program_frame = Some((f, picture));
        announce(&format!("FRAME {f}"));
        follow_meters(&mut info.callback_info, app, f);
    }
    if finished {
        stop_playback(app);
        app.playhead = app.playhead.clamp(0, (end - 1).max(0));
        announce(&format!("PLAYHEAD {}", app.playhead));
        return TimerCallbackReturn::terminate_and_refresh_dom();
    }
    if refresh_ui {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// Moves the meters in place to the reading at `frame` (the frame just shown), each when its
/// throttle lets it, and the peak under them with them: the window is rebuilt only a few times
/// a second while playing, and the meters followed those rebuilds.
fn follow_meters(info: &mut CallbackInfo, app: &mut VideoCut, frame: Frame) {
    let reading = app
        .project
        .as_ref()
        .map_or([0.0; 2], |p| levels::reading(p, frame, &app.levels));
    let Some(pb) = app.playback.as_mut() else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let now_ms = pb.started.elapsed().as_millis() as u64 + 1;
    let mut moved = false;
    for (channel, marker) in [METER_LEFT, METER_RIGHT].into_iter().enumerate() {
        let percent = (levels::fill(reading[channel]) * 100.0).round();
        let Some(level) = pb.meters[channel].next(percent, now_ms).into_option() else {
            continue;
        };
        if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
            LevelMeter::update_level(*info, node, level);
            moved = true;
        }
    }
    if !moved {
        return;
    }
    let peak = peak_text(reading);
    if let Some(node) = info.get_node_id_by_marker(METER_PEAK).into_option() {
        info.change_node_text(node, AzString::from(peak.as_str()));
    }
    let db = levels::peak_db(reading[0].max(reading[1])).map_or_else(|| String::from("-inf"), |db| db.to_string());
    announce(&format!("METER {frame} {db}"));
}

/// The meters' number: the louder channel's peak in dB.
fn peak_text(level: [f32; 2]) -> String {
    match levels::peak_db(level[0].max(level[1])) {
        Some(db) => t_args("azvideocut-peak-db", &[("db", Arg::from(db))]),
        None => t("azvideocut-inf-db"),
    }
}

// ==== layout ====
//
// The panes' own surfaces are `system:` colours (they follow the mode); the
// chrome is azul's widgets, which follow the app theme (flat / flora) and
// the mode themselves.

const ROOT_CSS: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;";
const PANE_CSS: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
     min-width: 0px; padding: 6px; background-color: system:window-background;";
// Under flora (`@theme(flora)` after the flat values; flat is unchanged): pane titles are
// flora's label (capitals), the tool bar is flora's strip over a hairline, the timecode is
// Garamond (tabular figures), notes 12px, the bin's items at the house radius.
const PANE_TITLE_CSS: &str = "font-size: 12px; font-weight: bold; color: system:secondary-text; \
     padding: 0px 2px 6px 2px; user-select: none; @theme(flora) { font-size: 11px; \
     text-transform: uppercase; letter-spacing: 0.12em; }";
const STAGE_CSS: &str = "display: flex; flex-grow: 1; min-height: 120px; background-color: #000000; \
     overflow: hidden;";
const IMAGE_CSS: &str = "width: 100%; height: 100%;";
const ROW_CSS: &str = "display: flex; flex-direction: row; align-items: center; flex-wrap: wrap; \
     padding-top: 6px;";
const GAP_CSS: &str = "margin-right: 6px;";
const TC_CSS: &str = "font-size: 13px; font-family: system:monospace; color: system:text; \
     margin-right: 10px; user-select: none; @theme(flora) { font-family: EB Garamond, Georgia, \
     serif; font-size: 14px; font-variant-numeric: tabular-nums lining-nums; }";
const NOTE_CSS: &str = "font-size: 11px; color: system:secondary-text; user-select: none; \
     @theme(flora) { font-size: 12px; }";
const LIST_CSS: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
     overflow-y: auto;";
const BIN_ITEM_CSS: &str = "display: flex; flex-direction: row; align-items: center; padding: 4px; \
     margin-bottom: 2px; border-radius: 4px; cursor: pointer; @theme(flora) { border-radius: 3px; }";
const BIN_ITEM_SELECTED_CSS: &str = "background-color: system:selection-background;";
const BIN_THUMB_CSS: &str = "width: 64px; height: 36px; margin-right: 8px; background-color: #000000;";
const BIN_NAME_CSS: &str = "font-size: 12px; color: system:text; user-select: none;";
const FIELD_CSS: &str = "display: flex; flex-direction: row; align-items: center; padding: 2px 0px;";
const FIELD_LABEL_CSS: &str = "font-size: 12px; color: system:secondary-text; width: 92px; \
     user-select: none;";
const METERS_CSS: &str = "display: flex; flex-direction: column; align-items: center; width: 56px; \
     padding: 6px 4px; background-color: system:window-background;";
const METER_ROW_CSS: &str = "display: flex; flex-direction: row; flex-grow: 1; min-height: 40px; \
     margin: 6px 0px;";
const METER_CSS: &str = "width: 8px; margin: 0px 3px;";
const TOOLBAR_CSS: &str = "display: flex; flex-direction: row; align-items: center; flex-wrap: wrap; \
     padding: 4px 8px; background-color: system:window-background; @theme(flora) { \
     background-color: #E9E7E2; border-bottom: 1px solid system:separator; @media \
     (prefers-color-scheme: dark) { background-color: #1F1F1F; } }";
const TIMELINE_HOST_CSS: &str =
    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;";
const DIALOG_BODY_CSS: &str =
    "display: flex; flex-direction: column; min-width: 420px; padding: 4px;";

/// What a pane title or a note says, as a paragraph.
/// A line of text: a key said in the window's language, plain words (a name) as they are.
fn text(words: &str, css: &str) -> Dom {
    Dom::create_p_with_text(t_label(words)).with_css(css)
}

/// A pane: its title over its content.
fn pane(title: &str, content: Vec<Dom>) -> Dom {
    let mut d = Dom::create_div().with_css(PANE_CSS).with_child(text(title, PANE_TITLE_CSS));
    for c in content {
        d.add_child(c);
    }
    d
}

/// A button calling `cb` with the app.
fn button(label: &str, app_ref: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create(t_label(label))
        .with_on_click(app_ref.clone(), cb)
        .dom()
        .with_css(GAP_CSS)
}

/// A primary button calling `cb` with the app.
fn primary(label: &str, app_ref: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::with_type(t_label(label), ButtonType::Primary)
        .with_on_click(app_ref.clone(), cb)
        .dom()
        .with_css(GAP_CSS)
}

/// A row of controls.
fn row(children: Vec<Dom>) -> Dom {
    let mut d = Dom::create_div().with_css(ROW_CSS);
    for c in children {
        d.add_child(c);
    }
    d
}

/// A picture for an image node: the canvas, or an empty one.
fn monitor_image(picture: Option<&Canvas>, marker: AzString, size: (u32, u32)) -> Dom {
    let image = picture
        .and_then(image_of)
        .unwrap_or_else(|| blank_image(size.0, size.1));
    Dom::create_image(image)
        .with_marker(OptionString::Some(marker))
        .with_css(IMAGE_CSS)
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // The window's language for every word this pass says.
    azul_appkit::l10n::begin_layout(&info);
    let mode = info.get_mode();
    let width = info.get_window_width();
    let theme = info.get_theme();
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<VideoCut>() else {
        return Dom::create_body();
    };
    let app = &mut *guard;
    app.window_width = width;
    app.dark = matches!(mode, DarkLightMode::Dark);
    // Flora or a spin of it ("flora:green").
    app.flora = azul_appkit::Theme::parse(theme.as_str()).is_some_and(azul_appkit::Theme::is_flora);
    let settings = kit::settings_open(&app.kit);

    let main = if settings {
        // azul-appkit's settings page: Playback, Export, then Appearance
        // (remembered), Data, Shortcuts, About.
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
            .with_child(kit::title_row("AzVideoCut"))
            .with_child(kit::settings_page(&app.kit, settings_sections(app)))
    } else if app.loading {
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
            .with_child(Titlebar::create("AzVideoCut").without_border_bottom().dom())
            .with_child(
                ShellEmptyState::create(t("azvideocut-opening-the-project"))
                    .with_icon("hourglass_empty")
                    .dom(),
            )
    } else if app.project.is_none() {
        empty_state(&app_ref)
    } else {
        editor(app, &app_ref)
    };
    let mut column = Dom::create_div().with_css(ROOT_CSS).with_child(main);
    if app.project.is_some() && app.export.open && !settings {
        column.add_child(export_dialog(app, &app_ref));
    }
    if app.about_open && !settings {
        column.add_child(about_dialog(&app_ref));
    }
    // The body and the theme scope fill the window: the S3 shell's panes
    // (min-height 0) collapsed to nothing in a body sized by its content.
    Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px; height: 100%;")
        .with_child(
            ShellThemeScope::create(column)
                .with_accent(ShellThemeAccent::Clay)
                .dom()
                .with_css(ROOT_CSS),
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app_ref.clone(),
            on_key,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            app_ref.clone(),
            on_close_requested,
        )
}

/// No project yet: make the sample or start an empty one.
fn empty_state(app_ref: &RefAny) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; align-items: center; justify-content: center;")
        .with_child(Titlebar::create("AzVideoCut").without_border_bottom().dom())
        .with_child(
            ShellEmptyState::create(t("azvideocut-no-project-yet"))
                .with_icon("movie")
                .with_detail(
                    "Make the sample project (generated clips, encoded to MP4 where this \
                     machine can), or start an empty project and import MP4 files.",
                )
                .with_action_label(t("azvideocut-make-the-sample-project"))
                .with_on_action(app_ref.clone(), on_make_sample as ButtonOnClickCallbackType)
                .dom(),
        )
        .with_child(row(vec![button("azvideocut-new-empty-project", app_ref, on_new_project)]))
}

/// The editor: the S3 shell, the menu row, the meters, the status bar.
fn editor(app: &VideoCut, app_ref: &RefAny) -> Dom {
    TimelineShell::create(
        media_pane(app, app_ref),
        source_pane(app, app_ref),
        program_pane(app, app_ref),
        inspector_pane(app, app_ref),
        timeline_pane(app, app_ref),
    )
    .with_menu_bar(menu_row(app, app_ref))
    .with_meters(meters(app))
    .office_shell()
    .with_status_bar(status_bar(app))
    .dom()
}

/// The title row (the window is `NoTitle`) over the tools.
fn menu_row(app: &VideoCut, app_ref: &RefAny) -> Dom {
    let name = app.project.as_ref().map_or("", |p| p.name.as_str());
    let tools: Vec<AzString> = Tool::ALL
        .iter()
        .map(|tool| azul_appkit::l10n::label(tool.label()))
        .collect();
    let undo = app
        .project
        .as_ref()
        .and_then(Project::undo_label)
        .map_or_else(|| t("azvideocut-undo"), |l| said_edit(&format!("Undo {l}")));
    let redo = app
        .project
        .as_ref()
        .and_then(Project::redo_label)
        .map_or_else(|| t("azvideocut-redo"), |l| said_edit(&format!("Redo {l}")));
    let toolbar = Dom::create_div()
        .with_css(TOOLBAR_CSS)
        .with_child(
            Segmented::create(StringVec::from_vec(tools))
                .with_selected_index(app.tool.index())
                .with_on_change(app_ref.clone(), on_tool as SegmentedOnChangeCallbackType)
                .dom()
                .with_css(GAP_CSS),
        )
        .with_child(button(
            if app.snapping {
                "azvideocut-snap-on"
            } else {
                "azvideocut-snap-off"
            },
            app_ref,
            on_snap,
        ))
        .with_child(button(&undo, app_ref, on_undo))
        .with_child(button(&redo, app_ref, on_redo))
        .with_child(button("azvideocut-import", app_ref, on_import))
        .with_child(primary("azvideocut-export-open", app_ref, on_export_open))
        .with_child(button("azvideocut-settings", app_ref, on_settings_open))
        .with_child(button("azvideocut-about", app_ref, on_about_open));
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(
            Titlebar::create(format!("AzVideoCut - {name}"))
                .without_border_bottom()
                .dom(),
        )
        .with_child(toolbar)
}

/// "00:00:04:00 - 640 x 360 - H.264" for the bin.
fn media_line(app: &VideoCut, m: &MediaItem) -> String {
    format!("{} - {} x {} - {}", app.timecode(m.frames), m.width, m.height, m.codec)
}

/// The media bin.
fn media_pane(app: &VideoCut, app_ref: &RefAny) -> Dom {
    let Some(project) = app.project.as_ref() else {
        return pane("azvideocut-project", Vec::new());
    };
    let mut list = Dom::create_div().with_css(LIST_CSS);
    if project.media.is_empty() {
        list.add_child(
            ShellEmptyState::create(t("azvideocut-no-media"))
                .with_icon("perm_media")
                .with_detail(t("azvideocut-import-mp4-files-ctrl-i"))
                .with_action_label(t("azvideocut-import"))
                .with_on_action(app_ref.clone(), on_import as ButtonOnClickCallbackType)
                .dom(),
        );
    }
    for m in &project.media {
        let selected = app.selected_media == Some(m.id);
        let thumb = match app.thumbs.get(&m.id) {
            Some(image) => Dom::create_image(image.clone()).with_css(BIN_THUMB_CSS),
            None => Dom::create_div().with_css(BIN_THUMB_CSS),
        };
        let item_data = RefAny::new(BinItem {
            app: app_ref.clone(),
            media: m.id,
        });
        let css = if selected {
            format!("{BIN_ITEM_CSS} {BIN_ITEM_SELECTED_CSS}")
        } else {
            String::from(BIN_ITEM_CSS)
        };
        list.add_child(
            Dom::create_div()
                .with_css(css.as_str())
                .with_child(thumb)
                .with_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: column; min-width: 0px;")
                        .with_child(text(&m.name, BIN_NAME_CSS))
                        .with_child(text(&media_line(app, m), NOTE_CSS)),
                )
                .clickable_as(&m.name, AccessibilityRole::ListItem, item_data.clone(), on_bin_click)
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::DoubleClick),
                    item_data,
                    on_bin_open,
                ),
        );
    }
    pane(
        &t_args(
            "azvideocut-project-of",
            &[("name", Arg::from(&project.name))],
        ),
        vec![
            list,
            row(vec![
                button("azvideocut-import", app_ref, on_import),
                button("azvideocut-open-in-source", app_ref, on_bin_open_selected),
            ]),
        ],
    )
}

/// The source monitor: the media's picture, its position, its marks.
fn source_pane(app: &VideoCut, app_ref: &RefAny) -> Dom {
    let media = app
        .source
        .and_then(|marks| app.project.as_ref().and_then(|p| p.media(marks.media)));
    let Some((marks, media)) = app.source.zip(media) else {
        return pane(
            "azvideocut-source",
            vec![
                Dom::create_div()
                    .with_css(STAGE_CSS)
                    .with_child(monitor_image(None, SOURCE_IMAGE, (MONITOR_W, MONITOR_H))),
                text("azvideocut-double-click-an-item-of-the", NOTE_CSS),
            ],
        );
    };
    let picture = app
        .source_frame
        .as_ref()
        .filter(|(m, _, _)| *m == marks.media)
        .map(|(_, _, c)| c);
    let mark = |f: Option<Frame>| f.map_or_else(|| String::from("--"), |f| app.timecode(f));
    #[allow(clippy::cast_precision_loss)]
    let slider = Slider::create(marks.position as f32, 0.0, (media.frames - 1).max(1) as f32)
        .with_on_value_change(
            app_ref.clone(),
            on_source_slider as SliderOnValueChangeCallbackType,
        )
        .with_accessibility_name(t("azvideocut-source-position"))
        .dom();
    let active = if app.active == Monitor::Source {
        t("azvideocut-active")
    } else {
        String::new()
    };
    pane(
        &t_args(
            "azvideocut-source-of",
            &[
                ("name", Arg::from(&media.name)),
                ("active", Arg::from(active)),
            ],
        ),
        vec![
            Dom::create_div()
                .with_css(STAGE_CSS)
                .with_child(monitor_image(picture, SOURCE_IMAGE, (MONITOR_W, MONITOR_H)))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseDown),
                    app_ref.clone(),
                    on_activate_source,
                ),
            slider,
            row(vec![text(
                &format!(
                    "{}  In {}  Out {}",
                    app.timecode(marks.position),
                    mark(marks.mark_in),
                    mark(marks.mark_out)
                ),
                TC_CSS,
            )]),
            row(vec![
                button("azvideocut-mark-in-i", app_ref, on_source_in),
                button("azvideocut-mark-out-o", app_ref, on_source_out),
                button("azvideocut-insert", app_ref, on_insert),
                button("azvideocut-overwrite", app_ref, on_overwrite),
            ]),
        ],
    )
}

/// The program monitor: the sequence at the playhead and the transport.
fn program_pane(app: &VideoCut, app_ref: &RefAny) -> Dom {
    let name = app.project.as_ref().map_or("", |p| p.sequence.name.as_str());
    let picture = app.program_frame.as_ref().map(|(_, c)| c);
    let playing = app.playback.is_some();
    let mark = |f: Option<Frame>| f.map_or_else(|| String::from("--"), |f| app.timecode(f));
    let active = if app.active == Monitor::Program {
        t("azvideocut-active")
    } else {
        String::new()
    };
    pane(
        &t_args(
            "azvideocut-program-of",
            &[("name", Arg::from(name)), ("active", Arg::from(active))],
        ),
        vec![
            Dom::create_div()
                .with_css(STAGE_CSS)
                .with_child(monitor_image(picture, PROGRAM_IMAGE, app.monitor_size()))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseDown),
                    app_ref.clone(),
                    on_activate_program,
                ),
            row(vec![
                Dom::create_p_with_text(app.timecode(app.playhead).as_str())
                    .with_css(TC_CSS)
                    .with_marker(OptionString::Some(PROGRAM_TC)),
                text(
                    &format!(
                        "/ {}  In {}  Out {}",
                        app.timecode(app.end()),
                        mark(app.program_in),
                        mark(app.program_out)
                    ),
                    NOTE_CSS,
                ),
            ]),
            row(vec![
                button("|<", app_ref, on_go_start),
                button("azvideocut-frame", app_ref, on_step_back),
                primary(
                    if playing {
                        "azvideocut-pause"
                    } else {
                        "azvideocut-play"
                    },
                    app_ref,
                    on_play_toggle,
                ),
                button("azvideocut-frame-2", app_ref, on_step_forward),
                button(">|", app_ref, on_go_end),
                button("azvideocut-in-i", app_ref, on_program_in),
                button("azvideocut-out-o", app_ref, on_program_out),
            ]),
            text("azvideocut-space-plays-and-pauses-j-k", NOTE_CSS),
        ],
    )
}

/// One numeric effect control.
fn effect_field(app_ref: &RefAny, clip: u64, field: EffectField, value: f32) -> Dom {
    Dom::create_div()
        .with_css(FIELD_CSS)
        .with_child(text(field.label(), FIELD_LABEL_CSS))
        .with_child(
            NumberInput::create(value)
                .with_on_value_change(
                    RefAny::new(EffectInput {
                        app: app_ref.clone(),
                        clip,
                        field,
                    }),
                    on_effect_value as NumberInputOnValueChangeCallbackType,
                )
                .with_accessibility_name(t(field.label()))
                .dom(),
        )
}

/// The effect controls of the selected clip.
fn inspector_pane(app: &VideoCut, app_ref: &RefAny) -> Dom {
    let clip = app.selected.first().and_then(|id| {
        app.project
            .as_ref()
            .and_then(|p| p.sequence.clip(*id).map(|c| (c.clone(), p.media(c.media).map(|m| m.name.clone()))))
    });
    let Some((clip, name)) = clip else {
        return pane(
            "azvideocut-effect-controls",
            vec![ShellEmptyState::create(t("azvideocut-no-clip-selected"))
                .with_icon("tune")
                .with_detail(t("azvideocut-select-a-clip-in-the-timeline"))
                .dom()],
        );
    };
    let e = clip.effects;
    let mut content = vec![text(
        &format!(
            "{} - {} to {}",
            name.unwrap_or_default(),
            app.timecode(clip.start),
            app.timecode(clip.end())
        ),
        BIN_NAME_CSS,
    )];
    for (field, value) in [
        (EffectField::X, e.x),
        (EffectField::Y, e.y),
        (EffectField::Scale, e.scale * 100.0),
        (EffectField::Opacity, e.opacity * 100.0),
        (EffectField::CropLeft, e.crop_left * 100.0),
        (EffectField::CropRight, e.crop_right * 100.0),
        (EffectField::CropTop, e.crop_top * 100.0),
        (EffectField::CropBottom, e.crop_bottom * 100.0),
    ] {
        content.push(effect_field(app_ref, clip.id, field, value));
    }
    let kind = match clip.transition.map(|t| t.kind) {
        None => 0,
        Some(TransitionKind::CrossDissolve) => 1,
        Some(TransitionKind::DipToBlack) => 2,
    };
    #[allow(clippy::cast_precision_loss)]
    let frames = clip.transition.map_or(12, |t| t.frames) as f32;
    content.push(text(
        "azvideocut-transition-at-the-clip-s-head",
        FIELD_LABEL_CSS,
    ));
    content.push(
        Segmented::create(StringVec::from_vec(vec![
            AzString::from(t("azvideocut-transition-none")),
            AzString::from(t("azvideocut-cross-dissolve")),
            AzString::from(t("azvideocut-dip-to-black")),
        ]))
        .with_selected_index(kind)
        .with_on_change(
            app_ref.clone(),
            on_transition_kind as SegmentedOnChangeCallbackType,
        )
        .dom(),
    );
    content.push(effect_field(
        app_ref,
        clip.id,
        EffectField::TransitionFrames,
        frames,
    ));
    content.push(row(vec![
        button(
            if clip.enabled {
                "azvideocut-disable-clip"
            } else {
                "azvideocut-enable-clip"
            },
            app_ref,
            on_toggle_enabled,
        ),
        button("azvideocut-reset-effects", app_ref, on_reset_effects),
    ]));
    pane("azvideocut-effect-controls", content)
}

/// The timeline: azul's Timeline widget over the sequence. The wheel over
/// it is the app's (the widget leaves it to the page): it scrolls the view,
/// with Ctrl / Cmd it zooms.
fn timeline_pane(app: &VideoCut, app_ref: &RefAny) -> Dom {
    let Some(project) = app.project.as_ref() else {
        return Dom::create_div();
    };
    let secs = |f: Frame| app.seconds(f);
    let tracks: Vec<TimelineTrack> = project
        .sequence
        .tracks
        .iter()
        .map(|t| {
            let kind = match t.kind {
                TrackKind::Video => TimelineTrackKind::Video,
                TrackKind::Audio => TimelineTrackKind::Audio,
            };
            let clips: Vec<TimelineClip> = t
                .clips
                .iter()
                .map(|c| {
                    let media = project.media(c.media);
                    let name = media.map_or_else(
                        || azul_appkit::l10n::t("azvideocut-missing-media"),
                        |m| m.name.clone(),
                    );
                    let tint = match (t.kind, media.map(|m| &m.source)) {
                        (TrackKind::Audio, _) => TimelineClipTint::Audio,
                        (
                            _,
                            Some(MediaSource::Generated {
                                pattern: model::Pattern::Matte { .. },
                            }),
                        ) => TimelineClipTint::Title,
                        _ => TimelineClipTint::Video,
                    };
                    let mut clip =
                        TimelineClip::create(c.id, secs(c.start), secs(c.length), name.as_str())
                            .with_selected(app.selected.contains(&c.id))
                            .with_tint(tint)
                            .with_disabled(!c.enabled);
                    if let Some(tr) = c.transition {
                        clip = clip.with_detail(match tr.kind {
                            TransitionKind::CrossDissolve => {
                                azul_appkit::l10n::t("azvideocut-cross-dissolve")
                            }
                            TransitionKind::DipToBlack => {
                                azul_appkit::l10n::t("azvideocut-dip-to-black")
                            }
                        });
                    }
                    if let Some(image) = app.thumbs.get(&c.media) {
                        clip = clip.with_thumbnail(image.clone());
                    }
                    clip
                })
                .collect();
            TimelineTrack::create(t.id, t.name.as_str(), kind)
                .with_clips(TimelineClipVec::from_vec(clips))
                .with_muted(t.hidden)
                .with_locked(t.locked)
        })
        .collect();
    let duration = (app.seconds(project.sequence.end()) + 10.0).max(30.0);
    let timeline = Timeline::create(TimelineTrackVec::from_vec(tracks), duration)
        .with_playhead(app.seconds(app.playhead))
        .with_view(app.view_start, app.pps)
        .with_view_width((app.window_width - 180.0).max(400.0))
        .with_fps(app.fps() as f32)
        .with_snapping(app.snapping)
        .with_on_event(app_ref.clone(), on_timeline as TimelineOnEventCallbackType)
        .dom();
    Dom::create_div()
        .with_css(TIMELINE_HOST_CSS)
        .with_child(timeline)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Scroll),
            app_ref.clone(),
            on_timeline_wheel,
        )
}

/// The audio meters: the left and right level of the clips under the playhead (on the
/// unmuted audio tracks), each a bar on a 60 dB scale - green, then yellow from -18 dB, red
/// from -6 dB - and the louder channel's peak in dB under them. While playing they move in
/// place with every frame shown ([`follow_meters`]).
fn meters(app: &VideoCut) -> Dom {
    let level = app
        .project
        .as_ref()
        .map_or([0.0; 2], |p| levels::reading(p, app.playhead, &app.levels));
    let bar = |level: f32, marker: AzString, name: &str| {
        LevelMeter::create((levels::fill(level) * 100.0).round())
            .with_orientation(LevelMeterOrientation::Vertical)
            .with_accessibility_name(AzString::from(name))
            .dom()
            .with_marker(OptionString::Some(marker))
            .with_css(METER_CSS)
    };
    Dom::create_div()
        .with_css(METERS_CSS)
        .with_child(text("L  R", NOTE_CSS))
        .with_child(
            Dom::create_div()
                .with_css(METER_ROW_CSS)
                .with_child(bar(level[0], METER_LEFT, &t("azvideocut-meter-left")))
                .with_child(bar(level[1], METER_RIGHT, &t("azvideocut-meter-right"))),
        )
        .with_child(
            Dom::create_p().with_css(NOTE_CSS).with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper(AzString::from(
                    peak_text(level),
                ))
                .with_marker(OptionString::Some(METER_PEAK)),
            ),
        )
}

/// The status bar: the last message, the tool, the snapping, the format.
fn status_bar(app: &VideoCut) -> Dom {
    let format = app.project.as_ref().map_or_else(String::new, |p| {
        t_args(
            "azvideocut-format",
            &[
                ("width", Arg::from(p.sequence.width)),
                ("height", Arg::from(p.sequence.height)),
                ("fps", Arg::from(p.sequence.fps.to_string())),
            ],
        )
    });
    StatusBar::create(StatusBarSegmentVec::from_vec(vec![
        StatusBarSegment::create(app.status.as_str()),
        StatusBarSegment::create(tool_status(app.tool).as_str()),
        StatusBarSegment::create(t(if app.snapping {
            "azvideocut-snapping-on"
        } else {
            "azvideocut-snapping-off"
        })),
        StatusBarSegment::create(format.as_str()),
        StatusBarSegment::create(
            t_args(
                "azvideocut-encoder",
                &[("encoder", Arg::from(&app.encoder))],
            )
            .as_str(),
        ),
    ]))
    .dom()
}

/// The export dialog: size, bitrate, range, progress.
fn export_dialog(app: &VideoCut, app_ref: &RefAny) -> Dom {
    let progress = app
        .export
        .shared
        .as_ref()
        .and_then(|s| s.progress.lock().ok().map(|p| p.clone()));
    let running = progress.as_ref().is_some_and(|p| !p.finished);
    let percent = progress.as_ref().map_or(0.0, |p| p.percent());
    let line = progress.as_ref().map_or_else(
        || t("azvideocut-ready"),
        |p| match (&p.error, &p.output, p.finished) {
            (Some(e), _, _) => t_args("azvideocut-failed", &[("why", Arg::from(e))]),
            (None, Some(out), true) => t_args("azvideocut-done", &[("file", Arg::from(out))]),
            _ => t_args(
                "azvideocut-frames-of",
                &[
                    ("done", Arg::from(p.done)),
                    ("total", Arg::from(p.total)),
                    ("how", Arg::from(&p.how)),
                ],
            ),
        },
    );
    if running {
        // While it runs: azul's standard ProgressDialog (status, bar, Cancel).
        let progress = ProgressDialog::create(t("azvideocut-exporting-title"), percent)
            .with_text(line.as_str())
            .with_detail(
                t_args(
                    "azvideocut-h264-by",
                    &[("encoder", Arg::from(&app.encoder))],
                )
                .as_str(),
            )
            .with_cancel(t("azvideocut-cancel-export"), true)
            .with_on_event(
                app_ref.clone(),
                on_export_event as StandardDialogOnEventCallbackType,
            )
            .dom();
        return Dialog::create(progress)
            .with_title(t("azvideocut-export"))
            .with_open(true)
            .with_modal(true)
            .with_close_button(false)
            .dom();
    }
    let labels = |items: &[&str]| {
        StringVec::from_vec(
            items
                .iter()
                .map(|s| azul_appkit::l10n::label(s))
                .collect::<Vec<_>>(),
        )
    };
    let body = Dom::create_div()
        .with_css(DIALOG_BODY_CSS)
        .with_child(text(
            &t_args(
                "azvideocut-encoder-note",
                &[("encoder", Arg::from(&app.encoder))],
            ),
            NOTE_CSS,
        ))
        .with_child(text("azvideocut-size", FIELD_LABEL_CSS))
        .with_child(
            Segmented::create(labels(&["1280 x 720", "854 x 480", "640 x 360"]))
                .with_selected_index(app.export.size)
                .with_on_change(
                    app_ref.clone(),
                    on_export_size as SegmentedOnChangeCallbackType,
                )
                .dom(),
        )
        .with_child(text("azvideocut-bitrate", FIELD_LABEL_CSS))
        .with_child(
            Segmented::create(labels(&["8 Mbit/s", "4 Mbit/s", "1.5 Mbit/s"]))
                .with_selected_index(app.export.bitrate)
                .with_on_change(
                    app_ref.clone(),
                    on_export_bitrate as SegmentedOnChangeCallbackType,
                )
                .dom(),
        )
        .with_child(text("azvideocut-range", FIELD_LABEL_CSS))
        .with_child(
            Segmented::create(labels(&[
                "azvideocut-range-sequence",
                "azvideocut-range-in-out",
                "azvideocut-range-first-second",
            ]))
            .with_selected_index(app.export.range)
            .with_on_change(
                app_ref.clone(),
                on_export_range as SegmentedOnChangeCallbackType,
            )
            .dom(),
        )
        .with_child(text(&line, NOTE_CSS))
        .with_child(row(vec![
            primary("azvideocut-export-now", app_ref, on_export_start),
            button("azvideocut-close", app_ref, on_export_close),
        ]));
    Dialog::create(body)
        .with_title(t("azvideocut-export"))
        .with_open(true)
        .with_modal(true)
        .with_close_button(true)
        .with_on_close(app_ref.clone(), on_dialog_close as DialogOnCloseCallbackType)
        .dom()
}

/// About AzVideoCut: azul's standard AboutDialog.
fn about_dialog(app_ref: &RefAny) -> Dom {
    let body = AboutDialog::create("AzVideoCut", env!("CARGO_PKG_VERSION"))
        .with_icon("movie")
        .with_description(azul_appkit::l10n::app_word(
            ABOUT.name,
            "about-summary",
            ABOUT.summary,
        ))
        .with_copyright(t("azvideocut-mit-license"))
        .with_labels(t("azvideocut-about-credits"), t("kit-button-ok"))
        .with_credit("azul", "MIT")
        .with_on_event(
            app_ref.clone(),
            on_about_event as StandardDialogOnEventCallbackType,
        )
        .dom();
    Dialog::create(body)
        .with_title(t("azvideocut-about-azvideocut"))
        .with_open(true)
        .with_modal(true)
        .with_close_button(true)
        .with_on_close(app_ref.clone(), on_dialog_close as DialogOnCloseCallbackType)
        .dom()
}

// ==== callbacks ====

/// A media bin item's payload.
struct BinItem {
    app: RefAny,
    media: u64,
}

/// Which effect a number field sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectField {
    X,
    Y,
    Scale,
    Opacity,
    CropLeft,
    CropRight,
    CropTop,
    CropBottom,
    TransitionFrames,
}

impl EffectField {
    /// Its words: a key of AzVideoCut's, said in the window's language.
    fn label(self) -> &'static str {
        match self {
            EffectField::X => "azvideocut-field-position-x",
            EffectField::Y => "azvideocut-field-position-y",
            EffectField::Scale => "azvideocut-field-scale",
            EffectField::Opacity => "azvideocut-field-opacity",
            EffectField::CropLeft => "azvideocut-field-crop-left",
            EffectField::CropRight => "azvideocut-field-crop-right",
            EffectField::CropTop => "azvideocut-field-crop-top",
            EffectField::CropBottom => "azvideocut-field-crop-bottom",
            EffectField::TransitionFrames => "azvideocut-field-frames",
        }
    }
}

/// A number field's payload.
struct EffectInput {
    app: RefAny,
    clip: u64,
    field: EffectField,
}

/// Applies `edit`: the model changes, the picture and the file follow.
fn apply_edit(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut, edit: Edit) -> Update {
    let label = edit.label();
    let Some(result) = app.project.as_mut().map(|p| p.edit(edit)) else {
        return Update::DoNothing;
    };
    after_change(info, app_ref, app, label, result)
}

/// The status bar's word on the tool: "Tool: Razor (C)".
fn tool_status(tool: Tool) -> String {
    t_args(
        "azvideocut-tool-status",
        &[("tool", Arg::from(t(tool.label())))],
    )
}

/// An edit's name (the undo history's English one, "Undo " / "Redo " before it for those) as
/// the window says it.
fn said_edit(label: &str) -> String {
    let edit = |name: &str| named(ABOUT.name, "edit", name);
    if let Some(name) = label.strip_prefix("Undo ") {
        t_args("azvideocut-undo-edit", &[("edit", Arg::from(edit(name)))])
    } else if let Some(name) = label.strip_prefix("Redo ") {
        t_args("azvideocut-redo-edit", &[("edit", Arg::from(edit(name)))])
    } else {
        edit(label)
    }
}

/// What every change of the sequence is followed by (an edit, an undo).
fn after_change(
    info: &mut CallbackInfo,
    app_ref: &RefAny,
    app: &mut VideoCut,
    label: &str,
    result: Result<(), model::EditError>,
) -> Update {
    match result {
        Ok(()) => {
            app.revision += 1;
            app.program_frame = None;
            if let Some(p) = app.project.as_ref() {
                app.selected.retain(|id| p.sequence.clip(*id).is_some());
            }
            app.status = said_edit(label);
            announce(&format!(
                "EDIT {} {} {}",
                label.replace(' ', "_"),
                app.clip_count(),
                app.end()
            ));
            save(info, app_ref, app);
            request_program_frame(info, app_ref, app);
        }
        Err(e) => {
            app.status = t(e.message());
        }
    }
    Update::RefreshDom
}

/// Moves the playhead to `f` (kept in view) and shows its picture.
fn seek(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut, f: Frame) -> Update {
    let end = app.end();
    app.playhead = f.clamp(0, end.max(0));
    let t = app.seconds(app.playhead);
    let span = f64::from((app.window_width - 180.0).max(400.0)) / f64::from(app.pps.max(0.05));
    if t < app.view_start || t > app.view_start + span * 0.95 {
        app.view_start = (t - span * 0.1).max(0.0);
    }
    announce(&format!("PLAYHEAD {}", app.playhead));
    request_program_frame(info, app_ref, app);
    Update::RefreshDom
}

/// The first selected clip.
fn selected_clip(app: &VideoCut) -> Option<model::Clip> {
    let id = *app.selected.first()?;
    app.project.as_ref()?.sequence.clip(id).cloned()
}

/// The selected clips lifted (or ripple-deleted with `ripple`).
fn delete_selected(
    info: &mut CallbackInfo,
    app_ref: &RefAny,
    app: &mut VideoCut,
    ripple: bool,
) -> Update {
    let ids = app.selected.clone();
    if ids.is_empty() {
        app.status = t("azvideocut-select-clip-to-delete");
        return Update::RefreshDom;
    }
    let mut update = Update::DoNothing;
    for id in ids {
        let edit = if ripple {
            Edit::RippleDelete { clip: id }
        } else {
            Edit::Lift { clip: id }
        };
        update = apply_edit(info, app_ref, app, edit);
    }
    update
}

/// Opens `media` in the source monitor with the marks of `range` (a clip's
/// in and out), or none.
fn open_in_source(
    info: &mut CallbackInfo,
    app_ref: &RefAny,
    app: &mut VideoCut,
    media: u64,
    range: Option<(Frame, Frame)>,
) -> Update {
    app.source = Some(SourceMarks {
        media,
        mark_in: range.map(|r| r.0),
        mark_out: range.map(|r| r.1),
        position: range.map_or(0, |r| r.0),
    });
    app.source_frame = None;
    app.active = Monitor::Source;
    request_source_frame(info, app_ref, app);
    Update::RefreshDom
}

/// Puts the source monitor's marked range at the playhead: on V1, or for sound alone on the
/// first audio track.
fn source_to_timeline(
    info: &mut CallbackInfo,
    app_ref: &RefAny,
    app: &mut VideoCut,
    insert: bool,
) -> Update {
    let Some(marks) = app.source else {
        app.status = t("azvideocut-open-clip-first");
        return Update::RefreshDom;
    };
    let at = app.playhead;
    let track = app
        .project
        .as_ref()
        .and_then(|p| p.target_track(marks.media))
        .unwrap_or(0);
    let Some(clip) = app.project.as_mut().and_then(|p| p.clip_from_marks(&marks)) else {
        app.status = t("azvideocut-out-before-in");
        return Update::RefreshDom;
    };
    let length = clip.length;
    let edit = if insert {
        Edit::Insert { track, at, clip }
    } else {
        Edit::Overwrite { track, at, clip }
    };
    let update = apply_edit(info, app_ref, app, edit);
    app.playhead = at + length;
    request_program_frame(info, app_ref, app);
    update
}

/// Toggles playback at normal speed.
fn toggle_play(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut) -> Update {
    if app.playback.is_some() {
        stop_playback(app);
        announce(&format!("PLAYHEAD {}", app.playhead));
    } else {
        start_playback(info, app_ref, app, 1);
    }
    Update::RefreshDom
}

/// J / L: faster in their direction, or reversed.
fn shuttle(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut, forward: bool) -> Update {
    let speed = app.playback.as_ref().map_or(0, |p| p.speed);
    let next = match (forward, speed) {
        (true, s) if s > 0 => (s * 2).min(4),
        (true, _) => 1,
        (false, s) if s < 0 => (s * 2).max(-4),
        (false, _) => -1,
    };
    start_playback(info, app_ref, app, next);
    Update::RefreshDom
}

/// Zooms the timeline by `factor` about `anchor` seconds.
fn zoom_about(app: &mut VideoCut, factor: f32, anchor: f64) {
    let pps = (app.pps * factor).clamp(0.5, 2000.0);
    let x = (anchor - app.view_start) * f64::from(app.pps);
    app.view_start = (anchor - x / f64::from(pps)).max(0.0);
    app.pps = pps;
}

/// Whether the focused node carries a class containing `part`.
fn focus_has_class(info: &CallbackInfo, part: &str) -> bool {
    info.get_focused_node()
        .into_option()
        .is_some_and(|node| {
            info.get_node_classes(node)
                .as_slice()
                .iter()
                .any(|c| c.as_str().contains(part))
        })
}

/// The window's keys (Premiere's shortcuts).
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(key) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    let cmd = m.primary_down();
    // A text field keeps its letters, the focused timeline its own keys.
    if focus_has_class(&info, "text-input") || focus_has_class(&info, "number-input") {
        return Update::DoNothing;
    }
    let timeline_focused = focus_has_class(&info, "__azul-native-timeline-lanes");
    // The kit's keys first (Mod+, the settings, F1 the shortcuts, Escape
    // closes them); the settings page takes no editing keys.
    let Some(kit_ref) = data.downcast_ref::<VideoCut>().map(|a| a.kit.clone()) else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    if cmd && matches!(key, VirtualKeyCode::I) {
        return on_import(data, info);
    }
    let Some(mut guard) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    if matches!(key, VirtualKeyCode::Escape) {
        app.about_open = false;
        if app.export.shared.as_ref().map_or(true, |s| s.progress.lock().map_or(true, |p| p.finished)) {
            app.export.open = false;
        }
        return Update::RefreshDom;
    }
    if app.project.is_none() {
        return Update::DoNothing;
    }
    let fps = i64::from(app.fps());
    let update = match key {
        VirtualKeyCode::Z if cmd && m.shift => redo(&mut info, &app_ref, app),
        VirtualKeyCode::Z if cmd => undo(&mut info, &app_ref, app),
        VirtualKeyCode::Y if cmd => redo(&mut info, &app_ref, app),
        VirtualKeyCode::K if cmd => {
            let at = app.playhead;
            apply_edit(&mut info, &app_ref, app, Edit::Razor { track: None, at })
        }
        VirtualKeyCode::E if cmd => {
            app.export.open = true;
            Update::RefreshDom
        }
        _ if cmd => return Update::DoNothing,
        VirtualKeyCode::Space => toggle_play(&mut info, &app_ref, app),
        VirtualKeyCode::K => {
            stop_playback(app);
            Update::RefreshDom
        }
        VirtualKeyCode::L => shuttle(&mut info, &app_ref, app, true),
        VirtualKeyCode::J => shuttle(&mut info, &app_ref, app, false),
        VirtualKeyCode::I | VirtualKeyCode::O => {
            let is_in = matches!(key, VirtualKeyCode::I);
            match app.active {
                Monitor::Source => {
                    if let Some(marks) = app.source.as_mut() {
                        if is_in {
                            marks.mark_in = Some(marks.position);
                        } else {
                            marks.mark_out = Some(marks.position);
                        }
                    }
                }
                Monitor::Program => {
                    if is_in {
                        app.program_in = Some(app.playhead);
                    } else {
                        app.program_out = Some(app.playhead);
                    }
                }
            }
            Update::RefreshDom
        }
        VirtualKeyCode::Comma => source_to_timeline(&mut info, &app_ref, app, true),
        VirtualKeyCode::Period => source_to_timeline(&mut info, &app_ref, app, false),
        VirtualKeyCode::V => set_tool(app, Tool::Select),
        VirtualKeyCode::C => set_tool(app, Tool::Razor),
        VirtualKeyCode::B => set_tool(app, Tool::Ripple),
        VirtualKeyCode::Y => set_tool(app, Tool::Slip),
        VirtualKeyCode::H => set_tool(app, Tool::Hand),
        VirtualKeyCode::Z => set_tool(app, Tool::Zoom),
        VirtualKeyCode::S => {
            app.snapping = !app.snapping;
            Update::RefreshDom
        }
        _ if timeline_focused => return Update::DoNothing,
        VirtualKeyCode::Left => {
            stop_playback(app);
            let f = app.playhead - if m.shift { fps } else { 1 };
            seek(&mut info, &app_ref, app, f)
        }
        VirtualKeyCode::Right => {
            stop_playback(app);
            let f = app.playhead + if m.shift { fps } else { 1 };
            seek(&mut info, &app_ref, app, f)
        }
        VirtualKeyCode::Home => seek(&mut info, &app_ref, app, 0),
        VirtualKeyCode::End => {
            let end = app.end();
            seek(&mut info, &app_ref, app, end)
        }
        VirtualKeyCode::Up | VirtualKeyCode::Down => {
            let points = app.project.as_ref().map(|p| p.sequence.edit_points()).unwrap_or_default();
            let at = app.playhead;
            let target = if matches!(key, VirtualKeyCode::Up) {
                points.iter().rev().find(|p| **p < at).copied().unwrap_or(0)
            } else {
                points.iter().find(|p| **p > at).copied().unwrap_or(at)
            };
            seek(&mut info, &app_ref, app, target)
        }
        VirtualKeyCode::Delete | VirtualKeyCode::Back => delete_selected(&mut info, &app_ref, app, m.shift),
        VirtualKeyCode::Equals => {
            let anchor = app.seconds(app.playhead);
            zoom_about(app, 1.25, anchor);
            Update::RefreshDom
        }
        VirtualKeyCode::Minus => {
            let anchor = app.seconds(app.playhead);
            zoom_about(app, 0.8, anchor);
            Update::RefreshDom
        }
        _ => return Update::DoNothing,
    };
    info.prevent_default();
    update
}

fn set_tool(app: &mut VideoCut, tool: Tool) -> Update {
    app.tool = tool;
    app.status = tool_status(tool);
    Update::RefreshDom
}

fn undo(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut) -> Update {
    let label = app
        .project
        .as_ref()
        .and_then(Project::undo_label)
        .map(str::to_string)
        .unwrap_or_default();
    let done = app.project.as_mut().is_some_and(Project::undo);
    if !done {
        app.status = t("azvideocut-nothing-to-undo");
        return Update::RefreshDom;
    }
    after_change(info, app_ref, app, &format!("Undo {label}"), Ok(()))
}

fn redo(info: &mut CallbackInfo, app_ref: &RefAny, app: &mut VideoCut) -> Update {
    let label = app
        .project
        .as_ref()
        .and_then(Project::redo_label)
        .map(str::to_string)
        .unwrap_or_default();
    let done = app.project.as_mut().is_some_and(Project::redo);
    if !done {
        app.status = t("azvideocut-nothing-to-redo");
        return Update::RefreshDom;
    }
    after_change(info, app_ref, app, &format!("Redo {label}"), Ok(()))
}

/// What the timeline widget asks for.
extern "C" fn on_timeline(mut data: RefAny, mut info: CallbackInfo, event: TimelineEvent) -> Update {
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    let at = app.frame_of(event.time);
    match event.kind {
        TimelineEventKind::Seek => {
            stop_playback(app);
            seek(&mut info, &app_ref, app, at)
        }
        TimelineEventKind::Scroll => {
            app.view_start = event.time.max(0.0);
            Update::RefreshDom
        }
        TimelineEventKind::Zoom => {
            #[allow(clippy::cast_possible_truncation)]
            let pps = event.value as f32;
            app.pps = pps.clamp(0.5, 2000.0);
            app.view_start = event.time.max(0.0);
            Update::RefreshDom
        }
        TimelineEventKind::ZoomToFit => {
            let span = app.seconds(app.end()).max(1.0);
            #[allow(clippy::cast_possible_truncation)]
            let pps = (f64::from((app.window_width - 180.0).max(400.0)) / span) as f32;
            app.pps = pps.clamp(0.5, 2000.0);
            app.view_start = 0.0;
            Update::RefreshDom
        }
        TimelineEventKind::Select => match app.tool {
            Tool::Razor => {
                let track = if event.shift { None } else { Some(event.track) };
                apply_edit(&mut info, &app_ref, app, Edit::Razor { track, at })
            }
            Tool::Zoom => {
                zoom_about(app, if event.shift { 0.5 } else { 2.0 }, event.time);
                Update::RefreshDom
            }
            _ => {
                if event.shift || event.ctrl {
                    if let Some(i) = app.selected.iter().position(|id| *id == event.clip_id) {
                        app.selected.remove(i);
                    } else {
                        app.selected.push(event.clip_id);
                    }
                } else {
                    app.selected = vec![event.clip_id];
                }
                Update::RefreshDom
            }
        },
        TimelineEventKind::Open => {
            let clip = app.project.as_ref().and_then(|p| p.sequence.clip(event.clip_id).cloned());
            match clip {
                Some(c) => open_in_source(
                    &mut info,
                    &app_ref,
                    app,
                    c.media,
                    Some((c.source_in, c.source_in + c.length - 1)),
                ),
                None => Update::DoNothing,
            }
        }
        TimelineEventKind::Move => {
            if app.tool == Tool::Slip {
                let start = app
                    .project
                    .as_ref()
                    .and_then(|p| p.sequence.clip(event.clip_id).map(|c| c.start))
                    .unwrap_or(at);
                apply_edit(
                    &mut info,
                    &app_ref,
                    app,
                    Edit::Slip {
                        clip: event.clip_id,
                        delta: start - at,
                    },
                )
            } else {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let track = event.value.max(0.0) as usize;
                apply_edit(
                    &mut info,
                    &app_ref,
                    app,
                    Edit::Move {
                        clip: event.clip_id,
                        track,
                        start: at,
                    },
                )
            }
        }
        TimelineEventKind::Trim => {
            let edge = if matches!(event.edge, TimelineEdge::Start) {
                Edge::Start
            } else {
                Edge::End
            };
            let edit = if app.tool == Tool::Ripple {
                Edit::RippleTrim {
                    clip: event.clip_id,
                    edge,
                    at,
                }
            } else {
                Edit::Trim {
                    clip: event.clip_id,
                    edge,
                    at,
                }
            };
            apply_edit(&mut info, &app_ref, app, edit)
        }
        TimelineEventKind::LaneClick => {
            app.selected.clear();
            Update::RefreshDom
        }
        TimelineEventKind::Delete => delete_selected(&mut info, &app_ref, app, event.shift),
        TimelineEventKind::ToggleMute => {
            let hidden = app
                .project
                .as_ref()
                .and_then(|p| p.sequence.tracks.get(event.track).map(|t| t.hidden))
                .unwrap_or(false);
            apply_edit(
                &mut info,
                &app_ref,
                app,
                Edit::SetTrackHidden {
                    track: event.track,
                    hidden: !hidden,
                },
            )
        }
        TimelineEventKind::ToggleLock => {
            let locked = app
                .project
                .as_ref()
                .and_then(|p| p.sequence.tracks.get(event.track).map(|t| t.locked))
                .unwrap_or(false);
            apply_edit(
                &mut info,
                &app_ref,
                app,
                Edit::SetTrackLocked {
                    track: event.track,
                    locked: !locked,
                },
            )
        }
    }
}

/// The wheel over the timeline: scrolls the view; with Ctrl / Cmd, zooms
/// about the playhead.
extern "C" fn on_timeline_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let node = NodeId::create(hit.node.into_raw().saturating_sub(1));
    let Some(delta) = info.get_scroll_delta(hit.dom, node).into_option() else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    if delta.x == 0.0 && delta.y == 0.0 {
        return Update::DoNothing;
    }
    info.prevent_default();
    if m.primary_down() {
        let anchor = app.seconds(app.playhead);
        zoom_about(&mut app, if delta.y < 0.0 { 1.15 } else { 1.0 / 1.15 }, anchor);
    } else {
        let d = if delta.x == 0.0 { delta.y } else { delta.x };
        app.view_start = (app.view_start + f64::from(d) / f64::from(app.pps.max(0.05))).max(0.0);
    }
    Update::RefreshDom
}

extern "C" fn on_tool(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    set_tool(&mut app, Tool::ALL[state.selected_index.min(Tool::ALL.len() - 1)])
}

extern "C" fn on_snap(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    app.snapping = !app.snapping;
    Update::RefreshDom
}

extern "C" fn on_undo(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    undo(&mut info, &app_ref, &mut app)
}

extern "C" fn on_redo(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    redo(&mut info, &app_ref, &mut app)
}

extern "C" fn on_import(data: RefAny, _info: CallbackInfo) -> Update {
    let filter = OptionFileTypeList::Some(FileTypeList {
        document_types: StringVec::from_vec(
            ["mp4", "m4v", "mov", "wav", "mp3", "flac", "m4a", "aac", "ogg", "opus"]
                .into_iter()
                .map(AzString::from)
                .collect(),
        ),
        document_descriptor: AzString::from(t("azvideocut-video-files")),
    });
    let _request = FileDialog::open_multiple_files(
        t("azvideocut-import-media"),
        OptionString::None,
        filter,
        data,
        on_import_picked,
    );
    Update::DoNothing
}

extern "C" fn on_import_picked(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenMultiResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let paths: Vec<String> = picked
        .paths
        .as_slice()
        .iter()
        .map(|p: &FilePath| p.inner.as_str().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if paths.is_empty() {
        return Update::DoNothing;
    }
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    if app.project.is_none() {
        return Update::DoNothing;
    }
    app.status = t_args("azvideocut-importing", &[("count", Arg::from(paths.len()))]);
    let fps = app.fps();
    spawn(&mut info, &app_ref, &app, Job::Import { paths, fps });
    Update::RefreshDom
}

extern "C" fn on_bin_click(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app_ref, media)) = data.downcast_ref::<BinItem>().map(|b| (b.app.clone(), b.media)) else {
        return Update::DoNothing;
    };
    let Some(mut app) = app_ref.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    app.selected_media = Some(media);
    Update::RefreshDom
}

extern "C" fn on_bin_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app_ref, media)) = data.downcast_ref::<BinItem>().map(|b| (b.app.clone(), b.media)) else {
        return Update::DoNothing;
    };
    let handle = app_ref.clone();
    let Some(mut app) = app_ref.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    app.selected_media = Some(media);
    open_in_source(&mut info, &handle, &mut app, media, None)
}

extern "C" fn on_bin_open_selected(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let Some(media) = app.selected_media.or_else(|| app.project.as_ref().and_then(|p| p.media.first().map(|m| m.id))) else {
        return Update::DoNothing;
    };
    open_in_source(&mut info, &app_ref, &mut app, media, None)
}

extern "C" fn on_source_slider(mut data: RefAny, mut info: CallbackInfo, state: SliderState) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    #[allow(clippy::cast_possible_truncation)]
    let position = state.value.round() as Frame;
    if let Some(marks) = app.source.as_mut() {
        marks.position = position.max(0);
    }
    app.active = Monitor::Source;
    request_source_frame(&mut info, &app_ref, &mut app);
    Update::RefreshDom
}

extern "C" fn on_activate_source(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    if app.active == Monitor::Source {
        return Update::DoNothing;
    }
    app.active = Monitor::Source;
    Update::RefreshDom
}

extern "C" fn on_activate_program(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    if app.active == Monitor::Program {
        return Update::DoNothing;
    }
    app.active = Monitor::Program;
    Update::RefreshDom
}

/// Sets the source monitor's in (`is_in`) or out mark at its position.
fn source_mark(data: &mut RefAny, is_in: bool) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    if let Some(marks) = app.source.as_mut() {
        if is_in {
            marks.mark_in = Some(marks.position);
        } else {
            marks.mark_out = Some(marks.position);
        }
    }
    app.active = Monitor::Source;
    Update::RefreshDom
}

extern "C" fn on_source_in(mut data: RefAny, _info: CallbackInfo) -> Update {
    source_mark(&mut data, true)
}

extern "C" fn on_source_out(mut data: RefAny, _info: CallbackInfo) -> Update {
    source_mark(&mut data, false)
}

extern "C" fn on_insert(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    source_to_timeline(&mut info, &app_ref, &mut app, true)
}

extern "C" fn on_overwrite(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    source_to_timeline(&mut info, &app_ref, &mut app, false)
}

/// Moves the playhead by `delta` frames, or to `to`.
fn transport(data: &mut RefAny, info: &mut CallbackInfo, delta: Frame, to: Option<Frame>) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    stop_playback(&mut app);
    app.active = Monitor::Program;
    let f = to.unwrap_or(app.playhead + delta);
    seek(info, &app_ref, &mut app, f)
}

extern "C" fn on_go_start(mut data: RefAny, mut info: CallbackInfo) -> Update {
    transport(&mut data, &mut info, 0, Some(0))
}

extern "C" fn on_go_end(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let end = data.downcast_ref::<VideoCut>().map_or(0, |a| a.end());
    transport(&mut data, &mut info, 0, Some(end))
}

extern "C" fn on_step_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    transport(&mut data, &mut info, -1, None)
}

extern "C" fn on_step_forward(mut data: RefAny, mut info: CallbackInfo) -> Update {
    transport(&mut data, &mut info, 1, None)
}

extern "C" fn on_play_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    toggle_play(&mut info, &app_ref, &mut app)
}

/// Sets the program monitor's in (`is_in`) or out mark at the playhead.
fn program_mark(data: &mut RefAny, is_in: bool) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let at = app.playhead;
    if is_in {
        app.program_in = Some(at);
    } else {
        app.program_out = Some(at);
    }
    app.active = Monitor::Program;
    Update::RefreshDom
}

extern "C" fn on_program_in(mut data: RefAny, _info: CallbackInfo) -> Update {
    program_mark(&mut data, true)
}

extern "C" fn on_program_out(mut data: RefAny, _info: CallbackInfo) -> Update {
    program_mark(&mut data, false)
}

/// A number field of the effect controls changed.
extern "C" fn on_effect_value(mut data: RefAny, mut info: CallbackInfo, state: NumberInputState) -> Update {
    let Some((mut app_ref, clip_id, field)) =
        data.downcast_ref::<EffectInput>().map(|e| (e.app.clone(), e.clip, e.field))
    else {
        return Update::DoNothing;
    };
    let handle = app_ref.clone();
    let Some(mut app) = app_ref.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let Some(clip) = app.project.as_ref().and_then(|p| p.sequence.clip(clip_id).cloned()) else {
        return Update::DoNothing;
    };
    let v = state.number;
    let mut e: Effects = clip.effects;
    let edit = match field {
        EffectField::X => {
            e.x = v;
            Edit::SetEffects { clip: clip_id, effects: e }
        }
        EffectField::Y => {
            e.y = v;
            Edit::SetEffects { clip: clip_id, effects: e }
        }
        EffectField::Scale => {
            e.scale = (v / 100.0).max(0.01);
            Edit::SetEffects { clip: clip_id, effects: e }
        }
        EffectField::Opacity => {
            e.opacity = (v / 100.0).clamp(0.0, 1.0);
            Edit::SetEffects { clip: clip_id, effects: e }
        }
        EffectField::CropLeft => {
            e.crop_left = (v / 100.0).clamp(0.0, 1.0);
            Edit::SetEffects { clip: clip_id, effects: e }
        }
        EffectField::CropRight => {
            e.crop_right = (v / 100.0).clamp(0.0, 1.0);
            Edit::SetEffects { clip: clip_id, effects: e }
        }
        EffectField::CropTop => {
            e.crop_top = (v / 100.0).clamp(0.0, 1.0);
            Edit::SetEffects { clip: clip_id, effects: e }
        }
        EffectField::CropBottom => {
            e.crop_bottom = (v / 100.0).clamp(0.0, 1.0);
            Edit::SetEffects { clip: clip_id, effects: e }
        }
        EffectField::TransitionFrames => {
            #[allow(clippy::cast_possible_truncation)]
            let frames = v.round().max(1.0) as Frame;
            let transition = clip.transition.map(|t| Transition { frames, ..t });
            Edit::SetTransition { clip: clip_id, transition }
        }
    };
    apply_edit(&mut info, &handle, &mut app, edit)
}

extern "C" fn on_transition_kind(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let Some(clip) = selected_clip(&app) else {
        return Update::DoNothing;
    };
    let frames = clip.transition.map_or(12, |t| t.frames);
    let transition = match state.selected_index {
        1 => Some(Transition { kind: TransitionKind::CrossDissolve, frames }),
        2 => Some(Transition { kind: TransitionKind::DipToBlack, frames }),
        _ => None,
    };
    apply_edit(&mut info, &app_ref, &mut app, Edit::SetTransition { clip: clip.id, transition })
}

extern "C" fn on_toggle_enabled(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let Some(clip) = selected_clip(&app) else {
        return Update::DoNothing;
    };
    apply_edit(
        &mut info,
        &app_ref,
        &mut app,
        Edit::SetEnabled {
            clip: clip.id,
            enabled: !clip.enabled,
        },
    )
}

extern "C" fn on_reset_effects(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let Some(clip) = selected_clip(&app) else {
        return Update::DoNothing;
    };
    apply_edit(
        &mut info,
        &app_ref,
        &mut app,
        Edit::SetEffects {
            clip: clip.id,
            effects: Effects::default(),
        },
    )
}

extern "C" fn on_export_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    app.export.open = true;
    Update::RefreshDom
}

extern "C" fn on_export_close(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    app.export.open = false;
    Update::RefreshDom
}

/// A dialog's own close button (or Escape in it): the dialogs close; an
/// export runs on.
extern "C" fn on_dialog_close(mut data: RefAny, _info: CallbackInfo, _state: DialogState) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    app.export.open = false;
    app.about_open = false;
    Update::RefreshDom
}

/// One of the export dialog's choices.
fn export_choice(data: &mut RefAny, set: impl FnOnce(&mut ExportState)) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    set(&mut app.export);
    Update::RefreshDom
}

extern "C" fn on_export_size(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    export_choice(&mut data, |e| e.size = state.selected_index.min(EXPORT_SIZES.len() - 1))
}

extern "C" fn on_export_bitrate(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    export_choice(&mut data, |e| e.bitrate = state.selected_index.min(EXPORT_BITRATES.len() - 1))
}

extern "C" fn on_export_range(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    export_choice(&mut data, |e| e.range = state.selected_index.min(2))
}

/// Starts the export the dialog describes, and a timer that shows its
/// progress until it ends.
extern "C" fn on_export_start(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    let Some(project) = app.project.clone() else {
        return Update::DoNothing;
    };
    if app.export.shared.as_ref().is_some_and(|s| s.progress.lock().map_or(false, |p| !p.finished)) {
        return Update::DoNothing;
    }
    let (width, height) = EXPORT_SIZES[app.export.size.min(EXPORT_SIZES.len() - 1)];
    let range = match (app.export.range, app.program_in, app.program_out) {
        (1, Some(from), Some(to)) => ExportRange::Marked { from, to },
        (2, _, _) => ExportRange::First(i64::from(project.sequence.fps)),
        _ => ExportRange::Sequence,
    };
    let settings = ExportSettings {
        width,
        height,
        bitrate_kbps: EXPORT_BITRATES[app.export.bitrate.min(EXPORT_BITRATES.len() - 1)],
        range,
        name: project.name.clone(),
    };
    let shared = Arc::new(ExportShared::default());
    app.export.shared = Some(shared.clone());
    app.export.last_line.clear();
    app.status = t("azvideocut-exporting");
    spawn(
        &mut info,
        &app_ref,
        app,
        Job::Export {
            project,
            settings,
            shared,
        },
    );
    let timer = TimerId::unique();
    let get_time = info.get_system_time_fn();
    info.add_timer(
        timer,
        Timer::create(app_ref.clone(), export_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(200))),
    );
    app.export.timer = Some(timer);
    Update::RefreshDom
}

/// The export's progress on screen, five times a second, until it ends.
extern "C" fn export_tick(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let Some(progress) = app
        .export
        .shared
        .as_ref()
        .and_then(|s| s.progress.lock().ok().map(|p| p.clone()))
    else {
        app.export.timer = None;
        return TimerCallbackReturn::terminate_unchanged();
    };
    let line = format!("EXPORT {} {}", progress.done, progress.total);
    if line != app.export.last_line {
        announce(&line);
        app.export.last_line = line;
    }
    if progress.finished {
        app.export.timer = None;
        TimerCallbackReturn::terminate_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_and_refresh_dom()
    }
}

extern "C" fn on_export_cancel(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    if let Some(s) = app.export.shared.as_ref() {
        s.cancel.store(true, Ordering::Relaxed);
    }
    app.status = t("azvideocut-cancelling");
    Update::RefreshDom
}

extern "C" fn on_settings_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(app) = data.downcast_ref::<VideoCut>() else {
        return Update::DoNothing;
    };
    kit::open_settings(&app.kit, None);
    Update::RefreshDom
}

extern "C" fn on_about_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    kit::close_settings(&app.kit);
    app.about_open = true;
    Update::RefreshDom
}

/// The About dialog's OK closes it.
extern "C" fn on_about_event(mut data: RefAny, _info: CallbackInfo, _event: StandardDialogEvent) -> Update {
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    app.about_open = false;
    Update::RefreshDom
}

/// The progress dialog's Cancel stops the export (the job ends at the next
/// frame and reports it).
extern "C" fn on_export_event(data: RefAny, info: CallbackInfo, event: StandardDialogEvent) -> Update {
    match event.kind {
        StandardDialogEventKind::Cancel => on_export_cancel(data, info),
        _ => Update::DoNothing,
    }
}


/// Makes the sample project (a job: it may encode two clips).
extern "C" fn on_make_sample(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    app.loading = true;
    let id = new_project_id();
    spawn(
        &mut info,
        &app_ref,
        &app,
        Job::Load {
            sample_id: Some(id),
            project: None,
        },
    );
    Update::RefreshDom
}

/// Starts an empty project.
extern "C" fn on_new_project(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    let id = new_project_id();
    app.project = Some(Project::create(id.clone(), String::from("Untitled"), 1280, 720, 25));
    app.revision += 1;
    announce(&format!("PROJECT {id} 0 0"));
    save(&mut info, &app_ref, &mut app);
    Update::RefreshDom
}

// ==== start ====

/// The window is up: open the project (or make the sample) on a worker.
extern "C" fn startup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut app) = data.downcast_mut::<VideoCut>() else {
        return Update::DoNothing;
    };
    kit::on_window_created(&app.kit, &mut info);
    let sample_id = app.args.sample.then(|| new_project_id());
    let project = app.args.project.clone();
    app.loading = true;
    spawn(&mut info, &app_ref, &app, Job::Load { sample_id, project });
    Update::RefreshDom
}

/// What this machine's video stack can do, for the status bar.
fn encoder_line() -> String {
    let encoder = VideoEncoder::open(64, 64, false, 500);
    let decoder = VideoDecoder::open(false);
    let backend = VideoEncoder::backend_name().as_str().to_string();
    match (encoder.is_open(), decoder.is_open()) {
        (true, true) => format!("{backend} H.264"),
        (true, false) => format!("{backend} H.264 encode only"),
        (false, true) => format!("{backend} decode only, exports Y4M"),
        (false, false) => String::from("none (generated media, Y4M export)"),
    }
}

/// What the settings page and About say about AzVideoCut.
pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzVideoCut",
    version: env!("CARGO_PKG_VERSION"),
    summary: "A video editor on azul's video stack: a media bin, source and program monitors, \
              a timeline of tracks, effects, transitions and H.264 export. Part of the Azlin apps.",
    license: "MIT",
    app_folder: store::APP_FOLDER,
};

/// The keyboard shortcuts the settings page lists (`Mod` = Cmd / Ctrl).
pub const SHORTCUTS: [Shortcut; 14] = [
    Shortcut::new(
        "azvideocut-category-playback",
        "Space",
        "azvideocut-shortcut-play-pause",
    ),
    Shortcut::new(
        "azvideocut-category-playback",
        "J  K  L",
        "azvideocut-shortcut-shuttle-back-stop-forward",
    ),
    Shortcut::new(
        "azvideocut-category-playback",
        "Left  Right",
        "azvideocut-shortcut-one-frame-shift-one",
    ),
    Shortcut::new(
        "azvideocut-category-playback",
        "Home  End  Up  Down",
        "azvideocut-shortcut-start-end-previous-next",
    ),
    Shortcut::new(
        "azvideocut-group-marks",
        "I  O",
        "azvideocut-shortcut-mark-in-out-the",
    ),
    Shortcut::new(
        "azvideocut-group-edit",
        ",  .",
        "azvideocut-shortcut-insert-overwrite-the-source",
    ),
    Shortcut::new(
        "azvideocut-group-edit",
        "Mod+K",
        "azvideocut-shortcut-razor-every-track-at",
    ),
    Shortcut::new(
        "azvideocut-group-edit",
        "Delete  Shift+Delete",
        "azvideocut-shortcut-lift-ripple-delete",
    ),
    Shortcut::new(
        "azvideocut-group-edit",
        "Mod+Z  Mod+Shift+Z",
        "azvideocut-shortcut-undo-redo",
    ),
    Shortcut::new(
        "azvideocut-group-tools",
        "V C B Y H Z",
        "azvideocut-shortcut-select-razor-ripple-slip",
    ),
    Shortcut::new(
        "azvideocut-group-timeline",
        "S",
        "azvideocut-shortcut-snapping-on-off",
    ),
    Shortcut::new(
        "azvideocut-group-timeline",
        "=  -",
        "azvideocut-shortcut-zoom-in-out",
    ),
    Shortcut::new(
        "azvideocut-group-file",
        "Mod+I",
        "azvideocut-shortcut-import-media",
    ),
    Shortcut::new(
        "azvideocut-group-file",
        "Mod+E",
        "azvideocut-shortcut-export-into-the-data",
    ),
];

/// The settings page's own categories (before the kit's).
const APP_CATEGORIES: [&str; 2] = ["Playback", "Export"];

/// The settings page's own sections.
fn settings_sections(app: &VideoCut) -> Vec<kit::AppSection> {
    vec![
        kit::AppSection {
            category: 0,
            title: t("azvideocut-category-playback"),
            content: kit::note("azvideocut-playback-note"),
        },
        kit::AppSection {
            category: 1,
            title: t("azvideocut-category-export"),
            content: kit::note(&t_args(
                "azvideocut-export-settings-note",
                &[("encoder", Arg::from(&app.encoder))],
            )),
        },
    ]
}

/// Starts AzVideoCut (`--help` for the switches).
pub fn start() {
    let (app_args, args) = match Args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    // The kit: the data root (--data-dir, $AZLIN_DATA, the user's), the
    // remembered theme and mode, the shortcuts.
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &APP_CATEGORIES, app_args.clone());
    // The words first: everything from here is said in the language chosen (Settings,
    // `--language`; the system's otherwise).
    let mut config = kit::app_config(&kit_ref);
    l10n::register(&mut config);
    let mut kit_now = kit_ref.clone();
    let language = kit_now.downcast_ref::<kit::Kit>().map(|k| k.language());
    if let Some(language) = language.filter(|l| *l != LanguagePref::System) {
        azul_appkit::l10n::set_locale(language.tag());
    }
    let root = {
        let mut k = kit_ref.clone();
        let root = k.downcast_ref::<kit::Kit>().map(|k| k.data_root.clone());
        root.unwrap_or_else(|| PathBuf::from("."))
    };
    if args.screen == Screen::Settings {
        kit::open_settings(&kit_ref, None);
    }
    let drive: Arc<dyn Drive> = Arc::new(LocalDrive::new(&root));
    let encoder = encoder_line();
    eprintln!("[azvideocut] data root {}; video: {encoder}", root.display());
    let state = VideoCut {
        drive,
        drive_root: root.display().to_string(),
        files: Arc::new(MediaFiles::default()),
        project: None,
        revision: 0,
        loading: true,
        status: t("azvideocut-ready"),
        tool: Tool::Select,
        snapping: true,
        playhead: 0,
        view_start: 0.0,
        pps: 40.0,
        selected: Vec::new(),
        selected_media: None,
        source: None,
        program_in: None,
        program_out: None,
        active: Monitor::Program,
        program_frame: None,
        source_frame: None,
        program_job: Coalesce::default(),
        source_job: Coalesce::default(),
        writes: WriteQueue::new(),
        close_hold: CloseHold::default(),
        thumbs: HashMap::new(),
        levels: HashMap::new(),
        levels_asked: HashSet::new(),
        playback: None,
        export: ExportState {
            open: args.screen == Screen::Export,
            size: 2,
            bitrate: 1,
            range: 0,
            shared: None,
            timer: None,
            last_line: String::new(),
        },
        kit: kit_ref.clone(),
        about_open: args.screen == Screen::About,
        window_width: 1280.0,
        dark: false,
        flora: false,
        encoder,
        args: args.clone(),
    };
    let window = kit::window_options(&kit_ref, layout, (1280.0, 800.0), (960.0, 600.0), startup);
    App::create(RefAny::new(state), config).run(window);
}

/// A fresh project id (the project's folder in the drive): random, so no
/// launch reuses another's (`Uuid::v4` is a process-local marker sequence).
#[must_use]
pub fn new_project_id() -> String {
    Uuid::from_seed(azul_storage::ids::random_seed()).as_str().to_string()
}

#[cfg(test)]
mod ids_tests;
