//! The recorder's settings, as pure functions: the settings file's values in, the numbers the
//! recorder runs with out, every value outside what a person can mean capped (or replaced by its
//! default) with a word about it. Nothing is read from the environment here; the caller passes the
//! values it was given (the rule of this tree: values come from the settings carriers).

use std::{collections::BTreeMap, path::PathBuf};

/// The settings file's value: where the recordings go (a folder; empty: the data root's).
pub const KEY_FOLDER: &str = "recorder-folder";
/// How much of the disk the recorder leaves free, in GB.
pub const KEY_MIN_FREE_GB: &str = "recorder-min-free-gb";
/// How long a segment file runs, minutes.
pub const KEY_SEGMENT_MIN: &str = "recorder-segment-min";
/// Minutes without input, speech or change after which the recorder pauses itself (0: never).
pub const KEY_IDLE_MIN: &str = "recorder-idle-min";
/// Whether the screen is recorded (`on` / `off`).
pub const KEY_SCREEN: &str = "recorder-screen";
/// Whether the camera is recorded.
pub const KEY_CAMERA: &str = "recorder-camera";
/// Whether the microphone is recorded.
pub const KEY_MICROPHONE: &str = "recorder-microphone";
/// The picture quality (`low`, `standard`, `high`): the encoders' targets.
pub const KEY_QUALITY: &str = "recorder-quality";
/// The most frames a second the screen track takes (the screen is recorded when it changes).
pub const KEY_SCREEN_FPS: &str = "recorder-screen-fps";
/// The longest a session may run, hours.
pub const KEY_MAX_HOURS: &str = "recorder-max-hours";
/// The ffmpeg the export runs (a path).
pub const KEY_FFMPEG: &str = "recorder-ffmpeg";
/// Where the screen comes from: `display` (the whole display, the default), `window` (one window,
/// AzReview's own, from the system's capture) or `own` (AzReview's own presented frames).
pub const KEY_SCREEN_SOURCE: &str = "recorder-screen-source";
/// The display to record, by the system's id (`CGDirectDisplayID`); 0: the display AzReview's window is on.
pub const KEY_SCREEN_DISPLAY: &str = "recorder-screen-display";
/// The widest the screen track's picture is, pixels: a display larger than this is scaled down to it.
pub const KEY_SCREEN_WIDTH: &str = "recorder-screen-width";
/// Apps whose windows are left out of the screen capture: bundle ids, separated by commas.
pub const KEY_SCREEN_EXCLUDE: &str = "recorder-screen-exclude";
/// The length of a chunk of the camera track, seconds: a keyframe and a fragment every chunk.
pub const KEY_CAMERA_CHUNK_SECS: &str = "recorder-camera-chunk-secs";
/// Whether a locked screen pauses the recording (`on` / `off`).
pub const KEY_PAUSE_ON_LOCK: &str = "recorder-pause-on-lock";
/// How many times a second the pointer's place on the display is written to the events (at most).
pub const KEY_POINTER_HZ: &str = "recorder-pointer-hz";

/// The disk the recorder leaves free unless told otherwise: 10 GB. This Mac has about 20 GB free
/// of 228; two weeks of recording would not fit, so the default is conservative and the folder can
/// be an external disk.
pub const DEFAULT_MIN_FREE_BYTES: u64 = 10 * GB;
/// One gigabyte, in bytes (decimal, as a disk is sold and a file manager shows it).
pub const GB: u64 = 1_000_000_000;
/// The least free space a person may ask the recorder to keep, GB.
pub static MIN_FREE_GB_RANGE: (u64, u64) = (1, 2_000);
/// The default and the range of a segment's length, minutes.
pub const DEFAULT_SEGMENT_MIN: u64 = 5;
pub static SEGMENT_MIN_RANGE: (u64, u64) = (1, 30);
/// The default and the range of the idle pause, minutes (0 turns it off).
pub const DEFAULT_IDLE_MIN: u64 = 5;
pub const IDLE_MIN_MAX: u64 = 240;
/// The default and the range of the screen's frame-rate cap.
pub const DEFAULT_SCREEN_FPS: u32 = 10;
pub static SCREEN_FPS_RANGE: (u32, u32) = (1, 30);
/// The default and the range of a session's length, hours.
pub const DEFAULT_MAX_HOURS: u64 = 24;
pub static MAX_HOURS_RANGE: (u64, u64) = (1, 168);
/// Where the export finds ffmpeg unless told otherwise.
pub const DEFAULT_FFMPEG: &str = "/opt/homebrew/bin/ffmpeg";
/// The default and the range of the widest screen picture, pixels. 1920 keeps the text of a review
/// legible and the files small (the video is the supplement; the review's data is the record).
pub const DEFAULT_SCREEN_WIDTH: u32 = 1_920;
pub static SCREEN_WIDTH_RANGE: (u32, u32) = (640, 7_680);
/// The default and the range of the camera's chunk, seconds.
pub const DEFAULT_CAMERA_CHUNK_SECS: u64 = 5;
pub static CAMERA_CHUNK_SECS_RANGE: (u64, u64) = (1, 60);
/// The default and the range of the pointer's rate, samples a second.
pub const DEFAULT_POINTER_HZ: u32 = 10;
pub static POINTER_HZ_RANGE: (u32, u32) = (1, 20);
/// The apps left out of the capture unless the person says otherwise: the system's notifications,
/// which show other people's messages over the review.
pub static DEFAULT_SCREEN_EXCLUDE: [&str; 1] = ["com.apple.notificationcenterui"];
/// The most apps in the exclusion list, and the longest bundle id.
pub const SCREEN_EXCLUDE_MAX: usize = 64;
pub const BUNDLE_ID_MAX: usize = 256;

/// Where the screen track comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenKind {
    /// The whole display (the system's capture, the cursor in it): what the user sees, whatever app
    /// is in front.
    #[default]
    Display,
    /// One window, AzReview's own, from the system's capture.
    Window,
    /// AzReview's own presented frames (the CPU renderer's frame tap): no permission of the system,
    /// nothing but the review on it.
    OwnFrames,
}

impl ScreenKind {
    /// The word in the settings file.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Window => "window",
            Self::OwnFrames => "own",
        }
    }

    /// What the manifest calls the track's `source`.
    #[must_use]
    pub const fn source(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Window => "window",
            Self::OwnFrames => "frame-tap",
        }
    }

    /// The kind called `name` in the settings file.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "display" => Some(Self::Display),
            "window" => Some(Self::Window),
            "own" => Some(Self::OwnFrames),
            _ => None,
        }
    }
}

/// How good the picture is kept: the encoders' targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    Low,
    #[default]
    Standard,
    High,
}

impl Quality {
    /// The word in the settings file.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Standard => "standard",
            Self::High => "high",
        }
    }

    /// The quality called `name`.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "standard" => Some(Self::Standard),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    /// The screen encoder's target in kbit/s for a picture of `width` x `height` pixels: the
    /// reference (a 1920 x 1200 picture at 2.5 Mbit/s for `Standard`: the supplement a review's video is)
    /// scaled by the pixel count and kept between a quarter and double of it, so a small window is not
    /// given a big window's budget and a huge one is not starved.
    #[must_use]
    pub fn screen_kbps(self, width: u32, height: u32) -> u32 {
        let reference = match self {
            Self::Low => 1_250_u64,
            Self::Standard => 2_500,
            Self::High => 5_000,
        };
        let pixels = u64::from(width) * u64::from(height);
        let scaled = reference * pixels / (1_920 * 1_200);
        let scaled = scaled.clamp(reference / 4, reference * 2).clamp(300, 20_000);
        u32::try_from(scaled).unwrap_or(20_000)
    }

    /// The camera's size, its frame rate and its target in kbit/s.
    #[must_use]
    pub const fn camera(self) -> CameraPreset {
        match self {
            Self::Low => CameraPreset { width: 640, height: 360, fps: 10, kbps: 400 },
            Self::Standard => CameraPreset { width: 1_280, height: 720, fps: 15, kbps: 800 },
            Self::High => CameraPreset { width: 1_920, height: 1_080, fps: 15, kbps: 2_000 },
        }
    }
}

/// What the camera is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraPreset {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub kbps: u32,
}

/// The Opus encoder's target, kbit/s: a voice, mono.
pub const AUDIO_KBPS: u32 = 32;

/// What the recorder runs with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecorderSettings {
    /// Where the recordings go; `None`: `review/recordings` in the data root.
    pub folder: Option<PathBuf>,
    /// The free space the recorder keeps on the disk, bytes.
    pub min_free_bytes: u64,
    /// How long a segment file runs, microseconds.
    pub segment_us: u64,
    /// Without input, speech or change for this long the recorder pauses itself; `None`: never.
    pub idle_pause_us: Option<u64>,
    pub screen: bool,
    pub camera: bool,
    pub microphone: bool,
    pub quality: Quality,
    /// The most frames a second the screen track takes.
    pub screen_fps: u32,
    /// The longest a session may run, microseconds.
    pub max_session_us: u64,
    /// The ffmpeg the export runs.
    pub ffmpeg: PathBuf,
    /// Where the screen comes from.
    pub screen_source: ScreenKind,
    /// The display to record, by the system's id; 0: the one AzReview's window is on.
    pub screen_display: u32,
    /// The widest the screen picture is, pixels.
    pub screen_width: u32,
    /// Apps whose windows are left out of the capture (bundle ids).
    pub screen_exclude: Vec<String>,
    /// The camera's chunk, microseconds: a keyframe and a fragment every chunk.
    pub camera_chunk_us: u64,
    /// A locked screen pauses the recording.
    pub pause_on_lock: bool,
    /// The most samples a second of the pointer's place on the display.
    pub pointer_hz: u32,
}

impl Default for RecorderSettings {
    fn default() -> Self {
        Self {
            folder: None,
            min_free_bytes: DEFAULT_MIN_FREE_BYTES,
            segment_us: DEFAULT_SEGMENT_MIN * 60_000_000,
            idle_pause_us: Some(DEFAULT_IDLE_MIN * 60_000_000),
            screen: true,
            camera: true,
            microphone: true,
            quality: Quality::Standard,
            screen_fps: DEFAULT_SCREEN_FPS,
            max_session_us: DEFAULT_MAX_HOURS * 3_600_000_000,
            ffmpeg: PathBuf::from(DEFAULT_FFMPEG),
            screen_source: ScreenKind::Display,
            screen_display: 0,
            screen_width: DEFAULT_SCREEN_WIDTH,
            screen_exclude: DEFAULT_SCREEN_EXCLUDE.iter().map(|s| (*s).to_string()).collect(),
            camera_chunk_us: DEFAULT_CAMERA_CHUNK_SECS * 1_000_000,
            pause_on_lock: true,
            pointer_hz: DEFAULT_POINTER_HZ,
        }
    }
}

/// What the settings resolved to, and what was changed on the way, for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub settings: RecorderSettings,
    pub notes: Vec<String>,
}

/// A whole number under `key`, kept inside `range` (a value outside it is capped, a value that is no
/// number is the default), with a note for each.
fn number(
    values: &BTreeMap<String, String>,
    key: &str,
    default: u64,
    range: (u64, u64),
    unit: &str,
    notes: &mut Vec<String>,
) -> u64 {
    let Some(raw) = values.get(key) else {
        return default;
    };
    let Ok(n) = raw.trim().parse::<u64>() else {
        notes.push(format!("the setting {key} is {raw:?}, no whole number of {unit}: using {default}"));
        return default;
    };
    if n < range.0 {
        notes.push(format!("the setting {key} is {n} {unit}, under the least of {}: using {}", range.0, range.0));
        range.0
    } else if n > range.1 {
        notes.push(format!("the setting {key} is {n} {unit}, over the most of {}: using {}", range.1, range.1));
        range.1
    } else {
        n
    }
}

/// `on` / `off` (also `true` / `false`, `1` / `0`, `yes` / `no`) under `key`.
fn switch(values: &BTreeMap<String, String>, key: &str, default: bool, notes: &mut Vec<String>) -> bool {
    let Some(raw) = values.get(key) else {
        return default;
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => true,
        "off" | "false" | "0" | "no" => false,
        _ => {
            notes.push(format!("the setting {key} is {raw:?}, not on or off: using {}", if default { "on" } else { "off" }));
            default
        }
    }
}

/// Whether `id` can be an app's bundle id: letters, digits, dots, hyphens and underscores.
fn is_bundle_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// The apps of the exclusion list under `key`: bundle ids separated by commas. A missing key is the
/// default list, a blank one nobody. What is no bundle id is left out and said, a repeat is left out
/// without a word, and no more than [`SCREEN_EXCLUDE_MAX`] are kept.
fn exclusions(values: &BTreeMap<String, String>, notes: &mut Vec<String>) -> Vec<String> {
    let Some(raw) = values.get(KEY_SCREEN_EXCLUDE) else {
        return DEFAULT_SCREEN_EXCLUDE.iter().map(|s| (*s).to_string()).collect();
    };
    let mut list: Vec<String> = Vec::new();
    for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        if entry.len() > BUNDLE_ID_MAX {
            notes.push(format!("the setting {KEY_SCREEN_EXCLUDE} has a name of {} characters, over the most of {BUNDLE_ID_MAX}: left out", entry.len()));
        } else if !is_bundle_id(entry) {
            notes.push(format!("the setting {KEY_SCREEN_EXCLUDE} has {entry:?}, which is no bundle id (letters, digits, dots, hyphens): left out"));
        } else if !list.iter().any(|known| known == entry) {
            list.push(entry.to_string());
        }
    }
    if list.len() > SCREEN_EXCLUDE_MAX {
        notes.push(format!("the setting {KEY_SCREEN_EXCLUDE} names {} apps, over the most of {SCREEN_EXCLUDE_MAX}: the rest are left out", list.len()));
        list.truncate(SCREEN_EXCLUDE_MAX);
    }
    list
}

/// The recorder's settings from the settings file's values.
#[must_use]
pub fn resolve(values: &BTreeMap<String, String>) -> Resolved {
    let mut notes = Vec::new();
    let d = RecorderSettings::default();
    let folder = values
        .get(KEY_FOLDER)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);
    let min_free_gb = number(values, KEY_MIN_FREE_GB, DEFAULT_MIN_FREE_BYTES / GB, MIN_FREE_GB_RANGE, "GB", &mut notes);
    let segment_min = number(values, KEY_SEGMENT_MIN, DEFAULT_SEGMENT_MIN, SEGMENT_MIN_RANGE, "minutes", &mut notes);
    let idle_min = number(values, KEY_IDLE_MIN, DEFAULT_IDLE_MIN, (0, IDLE_MIN_MAX), "minutes", &mut notes);
    let fps = number(
        values,
        KEY_SCREEN_FPS,
        u64::from(DEFAULT_SCREEN_FPS),
        (u64::from(SCREEN_FPS_RANGE.0), u64::from(SCREEN_FPS_RANGE.1)),
        "frames a second",
        &mut notes,
    );
    let hours = number(values, KEY_MAX_HOURS, DEFAULT_MAX_HOURS, MAX_HOURS_RANGE, "hours", &mut notes);
    let quality = match values.get(KEY_QUALITY) {
        None => d.quality,
        Some(raw) => Quality::parse(raw).unwrap_or_else(|| {
            notes.push(format!("the setting {KEY_QUALITY} is {raw:?}, not low, standard or high: using standard"));
            Quality::Standard
        }),
    };
    let ffmpeg = values
        .get(KEY_FFMPEG)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map_or(d.ffmpeg, PathBuf::from);
    let screen_source = match values.get(KEY_SCREEN_SOURCE) {
        None => d.screen_source,
        Some(raw) => ScreenKind::parse(raw).unwrap_or_else(|| {
            notes.push(format!("the setting {KEY_SCREEN_SOURCE} is {raw:?}, not display, window or own: using display"));
            ScreenKind::Display
        }),
    };
    let screen_display = number(values, KEY_SCREEN_DISPLAY, 0, (0, u64::from(u32::MAX)), "a display id", &mut notes);
    let screen_width = number(
        values,
        KEY_SCREEN_WIDTH,
        u64::from(DEFAULT_SCREEN_WIDTH),
        (u64::from(SCREEN_WIDTH_RANGE.0), u64::from(SCREEN_WIDTH_RANGE.1)),
        "pixels",
        &mut notes,
    );
    let chunk = number(values, KEY_CAMERA_CHUNK_SECS, DEFAULT_CAMERA_CHUNK_SECS, CAMERA_CHUNK_SECS_RANGE, "seconds", &mut notes);
    let pointer_hz = number(
        values,
        KEY_POINTER_HZ,
        u64::from(DEFAULT_POINTER_HZ),
        (u64::from(POINTER_HZ_RANGE.0), u64::from(POINTER_HZ_RANGE.1)),
        "samples a second",
        &mut notes,
    );
    let screen_exclude = exclusions(values, &mut notes);
    Resolved {
        settings: RecorderSettings {
            folder,
            min_free_bytes: min_free_gb * GB,
            segment_us: segment_min * 60_000_000,
            idle_pause_us: (idle_min > 0).then_some(idle_min * 60_000_000),
            screen: switch(values, KEY_SCREEN, d.screen, &mut notes),
            camera: switch(values, KEY_CAMERA, d.camera, &mut notes),
            microphone: switch(values, KEY_MICROPHONE, d.microphone, &mut notes),
            quality,
            screen_fps: u32::try_from(fps).unwrap_or(DEFAULT_SCREEN_FPS),
            max_session_us: hours * 3_600_000_000,
            ffmpeg,
            screen_source,
            screen_display: u32::try_from(screen_display).unwrap_or(0),
            screen_width: u32::try_from(screen_width).unwrap_or(DEFAULT_SCREEN_WIDTH),
            screen_exclude,
            camera_chunk_us: chunk * 1_000_000,
            pause_on_lock: switch(values, KEY_PAUSE_ON_LOCK, d.pause_on_lock, &mut notes),
            pointer_hz: u32::try_from(pointer_hz).unwrap_or(DEFAULT_POINTER_HZ),
        },
        notes,
    }
}

#[cfg(test)]
#[path = "recorder_settings_test.rs"]
mod recorder_settings_test;
