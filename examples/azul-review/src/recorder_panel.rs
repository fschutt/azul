//! The recorder's panel as words and numbers: what the always-visible indicator, the panel and the list of
//! recordings show, from what the recorder reports. Plain Rust, no window and no azul: each message is a
//! [`Phrase`] (a key of `resources/en.ftl` and `de.ftl` and its arguments) that the window says in its
//! language, and the tests check keys and arguments, not a language. The panel's DOM is a thin skin on this.
//!
//! Numbers that are not whole counts (a size, a time) are text made here, with the language's decimal mark
//! passed in: Fluent's arguments are strings and counts.

use std::collections::BTreeMap;

use azul_appkit::phrase::{Arg, Phrase, Text};

use crate::{
    recorder::{Indicator, Notice, PauseReason, Perm, StartError, State, TrackName},
    recorder_export_job::{JobState, VIDEO_FILE},
    recorder_fs::RecordingInfo,
    recorder_manifest::SessionState,
    recorder_policy::DiskState,
    recorder_settings::{RecorderSettings, ScreenKind},
};

/// The most hours the disk is said to last: a disk that lasts longer is not worth the digits.
const MOST_HOURS: f64 = 9_999.0;

/// A time recorded as `m:ss`, or `h:mm:ss` from an hour on: whole seconds, never rounded up.
#[must_use]
pub fn duration_text(us: u64) -> String {
    let secs = us / 1_000_000;
    let (hours, minutes, seconds) = (secs / 3_600, secs / 60 % 60, secs % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// A size in the decimal units the Finder uses: kB and MB whole, GB with one decimal (`decimal` is the language's
/// mark). Never rounded up into the next unit.
#[must_use]
pub fn size_text(bytes: u64, decimal: char) -> String {
    if bytes < 1_000_000 {
        format!("{} kB", bytes / 1_000)
    } else if bytes < 1_000_000_000 {
        format!("{} MB", bytes / 1_000_000)
    } else {
        format!("{}{decimal}{} GB", bytes / 1_000_000_000, bytes % 1_000_000_000 / 100_000_000)
    }
}

/// How long the disk lasts at the recording's rate: minutes under an hour, whole hours over it (the count is
/// an argument, so that the language picks its plural). `None` for what is no time: not a number, negative.
#[must_use]
// The counts are cut to what is said (under an hour's minutes, at most `MOST_HOURS` hours) before they become whole numbers.
#[allow(clippy::cast_possible_truncation)]
pub fn hours_phrase(hours: f64) -> Option<Phrase> {
    if hours.is_nan() || hours < 0.0 {
        return None;
    }
    if hours < 1.0 {
        return Some(Phrase::new("azreview-rec-minutes-left").arg("minutes", (hours * 60.0).floor() as i64));
    }
    Some(Phrase::new("azreview-rec-hours-left").arg("hours", hours.min(MOST_HOURS).floor() as i64))
}

/// The dot of the always-visible indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dot {
    /// Nothing is recording.
    Idle,
    /// Recording.
    Recording,
    /// Paused.
    Paused,
}

/// What a button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Do {
    Start,
    Stop,
    Pause,
    Resume,
}

/// A button: what it does, its words, and what a person who does not see it is told (the pause button names
/// the key that does the same).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Button {
    pub what: Do,
    pub label: Phrase,
    pub described: Phrase,
}

/// How much a note matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Something to know.
    Info,
    /// Something the person should act on, or that costs them part of what they asked for.
    Warning,
}

/// A line of words under the buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub severity: Severity,
    pub text: Text,
}

/// What the always-visible indicator and the panel's head show.
#[derive(Debug, Clone, PartialEq)]
pub struct Hud {
    pub dot: Dot,
    /// The time recorded, `m:ss`.
    pub elapsed: String,
    /// The state in words.
    pub status: Phrase,
    /// The indicator as a sentence for a person who does not see the dot (its accessibility name).
    pub described: Phrase,
    /// The buttons, in order.
    pub buttons: Vec<Button>,
    /// The voice's level, 0 to 1.
    pub level: f32,
    /// The bytes written so far, as text; `None` before a recording.
    pub size: Option<String>,
    /// Problems, the disk, dropped pictures.
    pub notes: Vec<Note>,
}

/// The key and the English words of a pause's reason.
#[must_use]
pub const fn pause_word(why: PauseReason) -> (&'static str, &'static str) {
    match why {
        PauseReason::User => ("azreview-rec-status-paused-user", "Paused by you"),
        PauseReason::Idle => ("azreview-rec-status-paused-idle", "Paused: nothing happened for a while"),
        PauseReason::Quota => ("azreview-rec-status-paused-quota", "Paused: the disk is nearly full"),
        PauseReason::Locked => ("azreview-rec-status-paused-locked", "Paused: the screen is locked"),
    }
}

fn plain_button(what: Do, key: &str) -> Button {
    let label = Phrase::new(key);
    Button { what, described: label.clone(), label }
}

fn hotkey_button(what: Do, key: &str, described_key: &str, hotkey: &str) -> Button {
    Button { what, label: Phrase::new(key), described: Phrase::new(described_key).arg("hotkey", hotkey) }
}

/// The indicator, the buttons and the notes for what the recorder reports. `hotkey` is the pause key as the
/// person reads it (`Cmd+Shift+P`); `decimal` is the language's decimal mark.
#[must_use]
pub fn hud(ind: &Indicator, hotkey: &str, decimal: char) -> Hud {
    let elapsed = duration_text(ind.recorded_us);
    let (dot, status, described, buttons) = match ind.state {
        State::Idle => (Dot::Idle, Phrase::new("azreview-rec-status-idle"), Phrase::new("azreview-rec-described-idle"), vec![plain_button(Do::Start, "azreview-rec-start")]),
        State::Recording => (
            Dot::Recording,
            Phrase::new("azreview-rec-status-recording"),
            Phrase::new("azreview-rec-described-recording").arg("time", elapsed.as_str()),
            vec![hotkey_button(Do::Pause, "azreview-rec-pause", "azreview-rec-pause-described", hotkey), plain_button(Do::Stop, "azreview-rec-stop")],
        ),
        State::Paused(why) => {
            let (key, english) = pause_word(why);
            (
                Dot::Paused,
                Phrase::new(key),
                Phrase::new("azreview-rec-described-paused").arg("time", elapsed.as_str()).arg("why", Arg::word(key, english)),
                vec![hotkey_button(Do::Resume, "azreview-rec-resume", "azreview-rec-resume-described", hotkey), plain_button(Do::Stop, "azreview-rec-stop")],
            )
        }
    };

    let mut notes = Vec::new();
    for (track, why) in &ind.problems {
        let name = Arg::word(&format!("azreview-rec-track-{track}"), track);
        notes.push(Note { severity: Severity::Warning, text: Text::from(Phrase::new("azreview-rec-problem").arg("track", name).arg("why", why.as_str())) });
    }
    if ind.state != State::Idle {
        notes.push(disk_note(ind, decimal));
    }
    if ind.dropped_frames > 0 {
        notes.push(Note { severity: Severity::Warning, text: Text::from(Phrase::new("azreview-rec-dropped").arg("count", ind.dropped_frames)) });
    }

    Hud {
        dot,
        elapsed,
        status,
        described,
        buttons,
        level: if ind.level.is_nan() { 0.0 } else { ind.level.clamp(0.0, 1.0) },
        size: (ind.state != State::Idle).then(|| size_text(ind.media_bytes, decimal)),
        notes,
    }
}

/// What the disk says while a recording runs: the free space and how long it lasts, a warning when it gets
/// low, a stronger one when the recording paused for it, a word when the disk cannot be asked.
fn disk_note(ind: &Indicator, decimal: char) -> Note {
    let hours = ind.hours_left.and_then(hours_phrase);
    let with_hours = |text: Text| match hours.clone() {
        Some(hours) => text.then(Text::plain(" ")).then(hours),
        None => text,
    };
    let Some(free) = ind.free_bytes.map(|bytes| size_text(bytes, decimal)) else {
        return Note { severity: Severity::Info, text: Text::key("azreview-rec-disk-unknown") };
    };
    match ind.disk {
        DiskState::Unknown => Note { severity: Severity::Info, text: Text::key("azreview-rec-disk-unknown") },
        DiskState::Fine => Note { severity: Severity::Info, text: with_hours(Text::from(Phrase::new("azreview-rec-disk-free").arg("free", free))) },
        DiskState::Low => Note { severity: Severity::Warning, text: with_hours(Text::from(Phrase::new("azreview-rec-disk-low").arg("free", free))) },
        DiskState::Full => Note { severity: Severity::Warning, text: Text::from(Phrase::new("azreview-rec-disk-full").arg("free", free)) },
    }
}

/// A notice of the recorder as a line of words with its weight: what needs the person's action is a warning, what
/// only happened is information.
#[must_use]
pub fn notice_note(notice: &Notice, decimal: char) -> Note {
    let problem = |track: Arg, why: &str| Text::from(Phrase::new("azreview-rec-problem").arg("track", track).arg("why", why));
    let track_arg = |track: &str| Arg::word(&format!("azreview-rec-track-{track}"), track);
    match notice {
        Notice::DiskLow { free, min_free } => Note {
            severity: Severity::Warning,
            text: Text::from(Phrase::new("azreview-rec-notice-disk-low").arg("free", size_text(*free, decimal)).arg("min", size_text(*min_free, decimal))),
        },
        Notice::DiskUnknown => Note { severity: Severity::Info, text: Text::key("azreview-rec-disk-unknown") },
        Notice::Paused(why) => Note { severity: if *why == PauseReason::Quota { Severity::Warning } else { Severity::Info }, text: Text::key(pause_word(*why).0) },
        Notice::Resumed(_) => Note { severity: Severity::Info, text: Text::key("azreview-rec-notice-resumed") },
        Notice::Ending { left_us } => Note { severity: Severity::Warning, text: Text::from(Phrase::new("azreview-rec-notice-ending").arg("time", duration_text(*left_us))) },
        Notice::LengthReached => Note { severity: Severity::Warning, text: Text::key("azreview-rec-notice-length-reached") },
        Notice::TrackFailed { track, why } => Note { severity: Severity::Warning, text: problem(track_arg(track), why) },
        Notice::TrackOff { track, why } => Note { severity: Severity::Warning, text: problem(track_arg(track), why) },
        Notice::ScreenGap { why } => Note { severity: Severity::Info, text: Text::from(Phrase::new("azreview-rec-notice-screen-gap").arg("why", why.as_str())) },
        Notice::ManifestError(why) => Note { severity: Severity::Warning, text: Text::from(Phrase::new("azreview-rec-notice-manifest-error").arg("why", why.as_str())) },
    }
}

/// Why a recording did not start, in words that say what to do.
#[must_use]
pub fn start_error_text(error: &StartError, decimal: char) -> Text {
    match error {
        StartError::Busy => Text::key("azreview-rec-error-busy"),
        StartError::NothingToRecord => Text::key("azreview-rec-error-nothing"),
        StartError::DiskFull { free, min_free } => {
            Text::from(Phrase::new("azreview-rec-error-disk-full").arg("free", size_text(*free, decimal)).arg("min", size_text(*min_free, decimal)))
        }
        StartError::Io(why) => Text::from(Phrase::new("azreview-rec-error-io").arg("why", why.as_str())),
    }
}

/// What will be recorded, said before the first press: the screen in the words of its source, the camera and the
/// voice when they are on, and the review's own events (the pen, the pointer, the files opened) always.
#[must_use]
pub fn will_record(settings: &RecorderSettings) -> Vec<Phrase> {
    let mut said = Vec::new();
    if settings.screen {
        said.push(Phrase::new(match settings.screen_source {
            ScreenKind::Display => "azreview-rec-will-display",
            ScreenKind::Window => "azreview-rec-will-window",
            ScreenKind::OwnFrames => "azreview-rec-will-own",
        }));
    }
    if settings.camera {
        said.push(Phrase::new("azreview-rec-will-camera"));
    }
    if settings.microphone {
        said.push(Phrase::new("azreview-rec-will-voice"));
    }
    said.push(Phrase::new("azreview-rec-will-events"));
    said
}

/// The folder and the manifest's name of a track.
const fn track_word(track: TrackName) -> &'static str {
    match track {
        TrackName::Screen => "screen",
        TrackName::Camera => "camera",
        TrackName::Microphone => "audio",
    }
}

/// A permission the system refused: which track, whom the system holds responsible (`who`: [`attribution`]) and
/// the words the system gave.
#[must_use]
pub fn denied(track: TrackName, why: &str, who: Arg) -> Text {
    let name = track_word(track);
    let track = Arg::word(&format!("azreview-rec-track-{name}"), name);
    Text::from(Phrase::new("azreview-rec-denied").arg("track", track).arg("who", who).arg("why", why))
}

/// The program the system holds responsible for a permission: the app, by the bundle's own name, when the
/// binary runs inside an app bundle (`exe` is its path); else the terminal it was started from
/// (`term_program`, the terminal's own `TERM_PROGRAM`); else a general word.
#[must_use]
pub fn attribution(exe: &str, term_program: Option<&str>) -> Arg {
    if let Some(end) = exe.find(".app/Contents/MacOS/") {
        let start = exe[..end].rfind('/').map_or(0, |at| at + 1);
        if end > start {
            return Arg::from(&exe[start..end]);
        }
    }
    match term_program.map(str::trim).filter(|name| !name.is_empty()) {
        Some("Apple_Terminal") => Arg::from("Terminal"),
        Some("iTerm.app") => Arg::from("iTerm"),
        Some("vscode") => Arg::from("Visual Studio Code"),
        Some(other) => Arg::from(other),
        None => Arg::word("azreview-rec-who-unknown", "the program that started AzReview"),
    }
}

/// What the system says of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Allowed.
    Allowed,
    /// Not asked yet: the system asks when the device opens.
    NotAsked,
    /// The person refused it.
    Denied,
    /// A profile or parental controls forbid it: the person cannot switch it on.
    Restricted,
}

/// What the panel says of a device before a recording starts.
#[must_use]
pub fn access_phrase(track: TrackName, access: Access, who: &Arg) -> Phrase {
    let name = track_word(track);
    let track = Arg::word(&format!("azreview-rec-track-{name}"), name);
    match access {
        Access::Allowed => Phrase::new("azreview-rec-access-allowed").arg("track", track),
        Access::NotAsked => Phrase::new("azreview-rec-access-not-asked").arg("track", track),
        Access::Denied => Phrase::new("azreview-rec-access-denied").arg("track", track).arg("who", who.clone()),
        Access::Restricted => Phrase::new("azreview-rec-access-restricted").arg("track", track),
    }
}

/// What the recorder is told of a device: allowed (or not yet asked) is no refusal; a refusal is the words the
/// manifest keeps (in English: they are data of the recording), naming the track and the program `who`.
#[must_use]
pub fn access_perm(track: TrackName, access: Access, who: &str) -> Perm {
    let (what, pane) = match track {
        TrackName::Screen => ("Screen Recording", "Screen Recording"),
        TrackName::Camera => ("Camera access", "Camera"),
        TrackName::Microphone => ("Microphone access", "Microphone"),
    };
    match access {
        Access::Allowed | Access::NotAsked => Perm::Granted,
        Access::Denied => Perm::Denied(format!(
            "{what} is off for {who}: switch {who} on in the system's privacy settings (a Mac: System Settings > Privacy & Security > {pane}), then start again"
        )),
        Access::Restricted => Perm::Denied(format!("{what} is restricted on this computer (a profile or parental controls): whoever manages the computer has to allow it")),
    }
}

/// What a row's button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowDo {
    /// Show the recording's folder.
    Reveal,
    Delete,
    /// Make a video of the recording.
    Export,
    CancelExport,
    /// Open the video that was made.
    OpenVideo,
}

/// A button of a row: what it does and its words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowButton {
    pub what: RowDo,
    pub label: Phrase,
}

/// What a row says of the export of its recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportLine {
    /// Nothing: no video was asked for, and none is in the folder.
    None,
    /// Being made, in whole percents (never 100: that is for a video that exists).
    Running { percent: u8 },
    /// Made: the video's file name and size, and what did not work as asked on the way.
    Done { name: String, size: String, notes: Vec<String> },
    /// Could not be made, in the words of the system or of ffmpeg.
    Failed(String),
    Cancelled,
}

/// The words of an export line, or none for a recording nothing was done with.
#[must_use]
pub fn export_text(line: &ExportLine) -> Option<Text> {
    Some(match line {
        ExportLine::None => return None,
        ExportLine::Running { percent } => Text::from(Phrase::new("azreview-rec-export-running").arg("percent", u32::from(*percent))),
        ExportLine::Done { name, size, .. } => Text::from(Phrase::new("azreview-rec-export-done").arg("name", name.as_str()).arg("size", size.as_str())),
        ExportLine::Failed(why) => Text::from(Phrase::new("azreview-rec-export-failed").arg("why", why.as_str())),
        ExportLine::Cancelled => Text::key("azreview-rec-export-cancelled"),
    })
}

/// What did not work as asked when the video was made, one line each (under the line that names the video).
#[must_use]
pub fn export_notes(line: &ExportLine) -> Vec<Text> {
    match line {
        ExportLine::Done { notes, .. } => notes.iter().map(|note| Text::from(Phrase::new("azreview-rec-export-note").arg("note", note.as_str()))).collect(),
        _ => Vec::new(),
    }
}

/// One recording of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The folder's name: what Reveal and Delete are given.
    pub name: String,
    /// When it began, `2026-11-26 10:00:00` (the folder's name is the local time).
    pub when: String,
    pub length: String,
    pub size: String,
    /// The tracks it has: `screen`, `camera`, `audio`.
    pub tracks: Vec<&'static str>,
    pub state: Phrase,
    /// What is wrong with it, when something is.
    pub problem: Option<Text>,
    /// What is known of its video.
    pub export: ExportLine,
    /// The buttons it has, in order.
    pub actions: Vec<RowButton>,
}

/// What a recording's export is: what the job says when one was started in this run of the app (it knows more than
/// the folder), else the video an earlier run left in the folder.
fn export_line(job: Option<&JobState>, video_bytes: Option<u64>, decimal: char) -> ExportLine {
    match (job, video_bytes) {
        (Some(JobState::Running { fraction }), _) => ExportLine::Running { percent: (fraction.clamp(0.0, 1.0) * 100.0).floor().min(99.0) as u8 },
        (Some(JobState::Done(made)), _) => ExportLine::Done { name: VIDEO_FILE.to_string(), size: size_text(made.bytes, decimal), notes: made.notes.clone() },
        (Some(JobState::Failed(why)), _) => ExportLine::Failed(why.clone()),
        (Some(JobState::Cancelled), _) => ExportLine::Cancelled,
        (None, Some(bytes)) => ExportLine::Done { name: VIDEO_FILE.to_string(), size: size_text(bytes, decimal), notes: Vec::new() },
        (None, None) => ExportLine::None,
    }
}

/// The buttons of a row: a finished recording that has a screen can be exported (or the export cancelled while it
/// runs), a video that exists can be opened, the folder can be shown, and the recording deleted unless it is being
/// exported.
fn actions(exportable: bool, export: &ExportLine) -> Vec<RowButton> {
    let running = matches!(export, ExportLine::Running { .. });
    let button = |what: RowDo, key: &str| RowButton { what, label: Phrase::new(key) };
    let mut out = Vec::new();
    if running {
        out.push(button(RowDo::CancelExport, "azreview-rec-export-cancel"));
    } else if exportable {
        out.push(button(RowDo::Export, "azreview-rec-export"));
    }
    if matches!(export, ExportLine::Done { .. }) {
        out.push(button(RowDo::OpenVideo, "azreview-rec-export-open"));
    }
    out.push(button(RowDo::Reveal, "azreview-rec-reveal"));
    if !running {
        out.push(button(RowDo::Delete, "azreview-rec-delete"));
    }
    out
}

/// The rows of the list, in the order given (the newest first); `exports` are the jobs of this run of the app by the
/// recording's name.
#[must_use]
pub fn rows(list: &[RecordingInfo], decimal: char, exports: &BTreeMap<String, JobState>) -> Vec<Row> {
    list.iter()
        .map(|r| {
            let export = export_line(exports.get(&r.name), r.video_bytes, decimal);
            let exportable = matches!(r.state, Some(SessionState::Complete | SessionState::Recovered)) && r.tracks.contains(&"screen");
            Row {
                name: r.name.clone(),
                when: when_text(&r.name),
                length: duration_text(r.recorded_us),
                size: size_text(r.bytes, decimal),
                tracks: r.tracks.clone(),
                state: Phrase::new(match r.state {
                    Some(SessionState::Complete) => "azreview-rec-state-complete",
                    Some(SessionState::Recovered) => "azreview-rec-state-recovered",
                    Some(SessionState::Recording) => "azreview-rec-state-unfinished",
                    None => "azreview-rec-state-damaged",
                }),
                problem: r.problem.as_deref().map(|why| Text::from(Phrase::new("azreview-rec-row-problem").arg("why", why))),
                actions: actions(exportable, &export),
                export,
            }
        })
        .collect()
}

/// A session folder's name (`2026-11-26_10-00-00`, with `-2`, `-3` for a second session of the same second) as
/// `2026-11-26 10:00:00 (2)`; a name that is not a time stays as it is.
fn when_text(name: &str) -> String {
    let (Some(stamp), Some(rest)) = (name.get(..19), name.get(19..)) else {
        return name.to_string();
    };
    let b = stamp.as_bytes();
    let digits = |from: usize, to: usize| -> Option<u32> {
        let part = stamp.get(from..to)?;
        if part.bytes().all(|c| c.is_ascii_digit()) {
            part.parse().ok()
        } else {
            None
        }
    };
    let ok = b[4] == b'-' && b[7] == b'-' && b[10] == b'_' && b[13] == b'-' && b[16] == b'-';
    let (Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (digits(5, 7), digits(8, 10), digits(11, 13), digits(14, 16), digits(17, 19)) else {
        return name.to_string();
    };
    if !ok || digits(0, 4).is_none() || !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 59 {
        return name.to_string();
    }
    let time = format!("{} {}", &stamp[..10], stamp[11..].replace('-', ":"));
    match rest {
        "" => time,
        _ => match rest.strip_prefix('-').filter(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit())) {
            Some(n) => format!("{time} ({n})"),
            None => name.to_string(),
        },
    }
}

#[cfg(test)]
#[path = "recorder_panel_test.rs"]
mod recorder_panel_test;
