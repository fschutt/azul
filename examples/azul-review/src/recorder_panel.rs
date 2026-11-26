//! The recorder's panel as words and numbers: what the always-visible indicator, the panel and the list of
//! recordings show, from what the recorder reports. Plain Rust, no window and no azul: each message is a
//! [`Phrase`] (a key of `resources/en.ftl` and `de.ftl` and its arguments) that the window says in its
//! language, and the tests check keys and arguments, not a language. The panel's DOM is a thin skin on this.
//!
//! Numbers that are not whole counts (a size, a time) are text made here, with the language's decimal mark
//! passed in: Fluent's arguments are strings and counts.

use azul_appkit::phrase::{Arg, Phrase, Text};

use crate::{
    recorder::{Indicator, PauseReason, State, TrackName},
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
}

/// The rows of the list, in the order given (the newest first).
#[must_use]
pub fn rows(list: &[RecordingInfo], decimal: char) -> Vec<Row> {
    list.iter()
        .map(|r| Row {
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
