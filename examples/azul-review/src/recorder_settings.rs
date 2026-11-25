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
    /// reference (a 2880 x 1800 window at 3 Mbit/s for `Standard`) scaled by the pixel count and
    /// kept between a quarter and double of it, so a small window is not given a big window's
    /// budget and a huge one is not starved. A static text frame is 41 dB at the reference.
    #[must_use]
    pub fn screen_kbps(self, width: u32, height: u32) -> u32 {
        let reference = match self {
            Self::Low => 1_500_u64,
            Self::Standard => 3_000,
            Self::High => 6_000,
        };
        let pixels = u64::from(width) * u64::from(height);
        let scaled = reference * pixels / (2_880 * 1_800);
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
        },
        notes,
    }
}

#[cfg(test)]
#[path = "recorder_settings_test.rs"]
mod recorder_settings_test;
