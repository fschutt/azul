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
//! The project is files on a Drive (`store`): `videocut/<uuid>/project.json`
//! and `media/`, `exports/`, saved after every edit by a job.
//!
//! On stdout, for scripts (`scripts/azvideocut_e2e.py`): `AZVIDEOCUT_PROJECT
//! <id> <clips> <frames>`, `AZVIDEOCUT_SAMPLE <encoded|generated>`,
//! `AZVIDEOCUT_EDIT <edit> <clips> <frames>`, `AZVIDEOCUT_PLAYHEAD <frame>`,
//! `AZVIDEOCUT_FRAME <frame>` (a program frame shown), `AZVIDEOCUT_SAVED
//! <id>`, `AZVIDEOCUT_EXPORT <done> <total>`, `AZVIDEOCUT_EXPORTED <key>
//! <bytes> <format>`, `AZVIDEOCUT_ERROR <message>`.

pub mod args;
pub mod decode;
pub mod export;
pub mod model;
pub mod render;
pub mod sample;
pub mod store;

use std::{
    collections::{HashMap, VecDeque},
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
        SegmentedOnChangeCallbackType, SliderOnValueChangeCallbackType, TimelineOnEventCallbackType,
        UpdateImageType,
    },
    css::DarkLightMode,
    dialog::{FileDialog, FileOpenMultiResult},
    dom::{NodeId, VirtualKeyCode},
    file::{FilePath, FileTypeList},
    image::{ImageRef, RawImage, RawImageData, RawImageFormat},
    option::{OptionDarkLightMode, OptionFileTypeList, OptionString},
    prelude::*,
    shells::{
        ShellEmptyState, ShellSettingsLayout, ShellSettingsSection, ShellThemeAccent,
        ShellThemeScope, TimelineShell,
    },
    str::String as AzString,
    time::SystemTimeDiff,
    uuid::Uuid,
    vec::{StringVec, TimelineClipVec, TimelineTrackVec, U8Vec, U8VecRef},
    video::{VideoDecoder, VideoEncoder},
    widgets::{
        ButtonType, Dialog, DialogState, NumberInputState, ProgressBar, Segmented,
        SegmentedState, Slider, SliderState, StatusBar, StatusBarSegment, Timeline, TimelineClip,
        TimelineClipTint, TimelineEdge, TimelineEvent, TimelineEventKind, TimelineTrack,
        TimelineTrackKind, Titlebar,
    },
    window::WindowDecorations,
};
use azul_storage::{Drive, LocalDrive};

use crate::{
    args::{Args, Mode, Screen},
    decode::{Library, MediaFiles},
    export::{ExportRange, ExportSettings, ExportShared, OutputFormat},
    model::{
        Edge, Edit, Effects, Frame, MediaItem, MediaSource, Project, SourceMarks, TrackKind,
        Transition, TransitionKind,
    },
    render::{compose, fit_within, scale_to, Canvas, FrameSource, Generated},
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
const PROGRAM_IMAGE: &str = "vc-program-image";
const PROGRAM_TC: &str = "vc-program-tc";
const SOURCE_IMAGE: &str = "vc-source-image";
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
    const ALL: [Tool; 6] = [Tool::Select, Tool::Razor, Tool::Ripple, Tool::Slip, Tool::Hand, Tool::Zoom];

    fn label(self) -> &'static str {
        match self {
            Tool::Select => "Select (V)",
            Tool::Razor => "Razor (C)",
            Tool::Ripple => "Ripple (B)",
            Tool::Slip => "Slip (Y)",
            Tool::Hand => "Hand (H)",
            Tool::Zoom => "Zoom (Z)",
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
pub struct App {
    drive: Arc<dyn Drive>,
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
    /// The bin's thumbnails and the clips' heads.
    thumbs: HashMap<u64, ImageRef>,
    playback: Option<Playback>,
    export: ExportState,
    settings_open: bool,
    about_open: bool,
    window_width: f32,
    dark: bool,
    flora: bool,
    encoder: String,
}

impl App {
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
    /// Write `project.json`.
    Save { project: Project },
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
    Saved {
        id: String,
        result: Result<(), String>,
    },
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
            let (w, h) = fit_within(picture.width, picture.height, THUMB_W, THUMB_H);
            Some((m.id, scale_to(&picture, w, h)))
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
        Job::Save { project } => Outcome::Saved {
            id: project.id.clone(),
            result: store::save_project(drive.as_ref(), &project),
        },
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
            let picture = library.picture(&media, frame, width, height).map(|p| {
                let (w, h) = fit_within(p.width, p.height, width, height);
                if (w, h) == (p.width, p.height) {
                    p
                } else {
                    scale_to(&p, w, h)
                }
            });
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
                match probed {
                    Ok(probe) => {
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
                                has_audio: false,
                            },
                            probe.thumbnail,
                        ));
                    }
                    Err(e) => errors.push(e),
                }
            }
            Outcome::Imported { items, errors }
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

/// Starts `job` on a worker.
fn spawn(info: &mut CallbackInfo, app_ref: &RefAny, app: &App, job: Job) {
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

/// Saves the project (after an edit).
fn save(info: &mut CallbackInfo, app_ref: &RefAny, app: &App) {
    if let Some(project) = app.project.clone() {
        spawn(info, app_ref, app, Job::Save { project });
    }
}

/// A job is done (UI thread).
extern "C" fn on_job_done(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(outcome) = msg.downcast_mut::<Done>().and_then(|mut d| d.outcome.take()) else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<App>() else {
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
                    app.status = String::from("Ready.");
                    let id = app.project.as_ref().map_or_else(String::new, |p| p.id.clone());
                    announce(&format!("PROJECT {id} {} {}", app.clip_count(), app.end()));
                    if let Some(encoded) = sample_encoded {
                        announce(if encoded { "SAMPLE encoded" } else { "SAMPLE generated" });
                    }
                    request_program_frame(&mut info, &app_ref, app);
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
        Outcome::Saved { id, result } => {
            match result {
                Ok(()) => announce(&format!("SAVED {id}")),
                Err(e) => {
                    app.status = e.clone();
                    announce(&format!("ERROR {e}"));
                }
            }
            Update::DoNothing
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
                format!("Imported {added} file(s).")
            } else {
                format!("Imported {added} file(s); {}", errors.join("; "))
            };
            save(&mut info, &app_ref, app);
            Update::RefreshDom
        }
        Outcome::Exported { result } => {
            match result {
                Ok((key, bytes, format)) => {
                    app.status = format!("Exported {key} ({bytes} bytes).");
                    announce(&format!("EXPORTED {key} {bytes} {}", format.extension()));
                }
                Err(e) => {
                    app.status = format!("Export failed: {e}");
                    announce(&format!("ERROR export {e}"));
                }
            }
            Update::RefreshDom
        }
    }
}

// ==== PIECE C: pictures and playback ====

// ==== PIECE D: layout ====

// ==== PIECE E: callbacks ====

// ==== PIECE F: start ====
