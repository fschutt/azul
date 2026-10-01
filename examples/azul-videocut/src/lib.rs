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

// ==== PIECE B: jobs ====

// ==== PIECE C: pictures and playback ====

// ==== PIECE D: layout ====

// ==== PIECE E: callbacks ====

// ==== PIECE F: start ====
