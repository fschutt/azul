//! One calendar event is one JSON file, `<data dir>/events/<event id>.json`, where the id is a
//! version 4 UUID. The same relative path is the object key in a bucket (`events/<id>.json`), so
//! the folder can move to S3 or R2 as it is. Durable data lives only in these files: the meeting
//! server (the `meet` Worker) mints a meeting's link, and the event file keeps it.
//!
//! The format, version 2 (an event with a title, a day and times, and maybe a meeting):
//!
//! ```json
//! {
//!   "format": "azcalendar.event",
//!   "version": 2,
//!   "id": "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a",
//!   "title": "Team sync",
//!   "date": "2026-09-30",
//!   "start": "09:00",
//!   "end": "10:00",
//!   "meeting": {
//!     "link": "azlin://meet/a2h859hyqkfaa11nhzxfh3gd7f",
//!     "server": "http://127.0.0.1:8787",
//!     "code": "xq4-8kd-2nm",
//!     "expires": "2026-09-30T10:00:00.000Z",
//!     "starts_at": "2026-09-30T07:00:00.000Z",
//!     "ends_at": "2026-09-30T08:00:00.000Z"
//!   }
//! }
//! ```
//!
//! Version 3 is version 2 with what the event editor adds, each field left out while it holds
//! nothing, so an event that uses none of them is still written as version 2, byte for byte:
//!
//! ```json
//! {
//!   "format": "azcalendar.event",
//!   "version": 3,
//!   "id": "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a",
//!   "title": "Planning days",
//!   "date": "2026-09-30",
//!   "all_day": true,
//!   "last_day": "2026-10-02",
//!   "location": "Room 4",
//!   "notes": "Bring the roadmap.",
//!   "attendees": ["ana@example.com"],
//!   "reminder": 15,
//!   "calendar": "9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3",
//!   "repeat": "FREQ=WEEKLY;BYDAY=WE",
//!   "except": ["2026-10-07"],
//!   "uid": "abc123@google.com"
//! }
//! ```
//!
//! - `all_day`: the event has no times (`start` / `end` are left out) and lasts from `date` to
//!   `last_day`, both included (`last_day` left out: one day).
//! - `reminder`: minutes before the start (0: at the start).
//! - `calendar`: the id of the calendar the event is in (`calendars.rs`); left out: the default
//!   calendar.
//! - `repeat`: the repeat rule as iCalendar RRULE text (`rrule.rs`); `except`: the days a
//!   repeating event skips (EXDATE).
//! - `uid`: the iCalendar UID of an imported event, so importing the same file again updates it
//!   instead of adding it twice. Events made here are `<id>@azcalendar`.
//!
//! Times are wall-clock times on the event's day (no time zones in the event itself); an event
//! with times ends on the day it starts. `meeting.starts_at` / `ends_at` are the times the
//! meeting server keeps the room for, in UTC, as it answered them: the event's times read in the
//! zone of the AzCalendar that made the link.
//!
//! AzCalendar makes a meeting's link itself (a room id drawn here, the same shape as the server's),
//! so making one works offline, and registers the room with the meeting server as soon as it
//! answers. Until then the meeting has `"pending": true` (and no `code`, `expires` or times); the
//! registered meeting leaves `pending` out, as every file from before it does.
//!
//! Version 1 is the same as version 2 without `meeting.starts_at` / `ends_at` (links minted before
//! meeting times); it is still read, and written as version 2 when saved again. A file with a
//! higher `version` was written by a newer AzCalendar and is left alone, never guessed at; fields
//! this version does not know are ignored.

use std::path::PathBuf;

use azul_pim::mail_address::is_email;
use azul_storage::Drive;
use chrono::{Duration, NaiveDate, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

use crate::{
    meet_rooms::{self, RoomKey},
    rrule::Rule,
};

/// The `format` of an event file.
pub const FORMAT: &str = "azcalendar.event";
/// The version this AzCalendar writes for an event that uses version 3's fields, and the newest
/// it reads.
pub const VERSION: u64 = 3;
/// The version an event that uses none of version 3's fields is written in: the files of the
/// AzCalendar before the event editor, which that AzCalendar still reads.
pub const PLAIN_VERSION: u64 = 2;
/// The oldest version this AzCalendar reads.
pub const OLDEST_VERSION: u64 = 1;
/// The folder, and the object-key prefix, of the event files.
pub const EVENTS_DIR: &str = "events";
/// The folder in the user's data folder when `AZCAL_DATA` is not set.
pub const APP_DIR: &str = "AzCalendar";

const DATE_FORMAT: &str = "%Y-%m-%d";
const TIME_FORMAT: &str = "%H:%M";

/// One event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// A version 4 UUID in lower case, which names the event's file.
    pub id: String,
    pub title: String,
    /// The day the event is on (its first day; a repeating event's first date).
    pub date: NaiveDate,
    /// Start and end, to the minute. An all-day event runs from 00:00 to the last minute of
    /// the day here; its file has no times.
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub meeting: Option<Meeting>,
    /// The event has no times: it takes whole days, `date` to `last_day`.
    pub all_day: bool,
    /// An all-day event's last day (included); `date` for every other event.
    pub last_day: NaiveDate,
    pub location: String,
    pub notes: String,
    /// The people invited, by e-mail address.
    pub attendees: Vec<String>,
    /// A reminder this many minutes before the start.
    pub reminder: Option<u32>,
    /// The id of the calendar the event is in; empty: the default calendar.
    pub calendar: String,
    /// How the event repeats.
    pub repeat: Option<Rule>,
    /// The days a repeating event skips.
    pub except: Vec<NaiveDate>,
    /// An imported event's iCalendar UID (empty for an event made here).
    pub uid: String,
}

/// An event's AzMeet meeting, as the meeting server minted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meeting {
    /// `azlin://meet/<room id>`: what AzMeet joins (`AZMEET_JOIN`).
    pub link: String,
    /// The meeting server that minted the room, which a joining AzMeet must ask.
    pub server: String,
    /// The room's short code, if the server sent one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
    /// When the server forgets the room (ISO 8601, as the server sent it).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub expires: String,
    /// The meeting's start and end the server keeps the room for (RFC 3339, UTC, as the server
    /// sent them); empty when it keeps the room without times (a server, or a file, from before
    /// meeting times).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub starts_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ends_at: String,
    /// The link was made here and its room is not registered with `server` yet (made offline, or
    /// the registration is on its way): AzCalendar sends it again until the server has it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pending: bool,
}

/// For `skip_serializing_if`: a `false` flag is left out of the file.
#[allow(clippy::trivially_copy_pass_by_ref)] // serde passes a reference
fn is_false(flag: &bool) -> bool {
    !*flag
}

/// Why an event cannot be made or read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError {
    /// The file is not JSON.
    NotJson(String),
    /// JSON, but not an AzCalendar event: `format` is missing or different.
    NotAnEvent,
    /// Written by a newer AzCalendar.
    NewerVersion(u64),
    /// A field is missing or of the wrong type.
    Malformed(String),
    /// The id is not a UUID in lower case.
    BadId(String),
    /// A version, date or time that does not parse.
    BadField {
        field: &'static str,
        value: String,
    },
    EmptyTitle,
    /// The event ends at or before its start.
    EndNotAfterStart,
    /// The meeting link does not name an AzMeet room.
    BadMeetingLink(String),
    /// An all-day event's last day is before its first.
    LastDayBeforeFirst,
    /// An attendee that is not an e-mail address.
    BadAttendee(String),
    /// The calendar id is not a calendar's (`calendars.rs`).
    BadCalendar(String),
    /// The repeat rule does not parse, or is outside what AzCalendar keeps (`rrule.rs`).
    BadRepeat(String),
}

impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventError::NotJson(e) => write!(f, "not JSON ({e})"),
            EventError::NotAnEvent => {
                write!(f, "not an AzCalendar event (no \"format\": \"{FORMAT}\")")
            }
            EventError::NewerVersion(v) => {
                write!(
                    f,
                    "written by a newer AzCalendar (version {v}; this one reads up to {VERSION})"
                )
            }
            EventError::Malformed(e) => write!(f, "malformed ({e})"),
            EventError::BadId(id) => write!(f, "the id {id:?} is not a UUID in lower case"),
            EventError::BadField { field, value } => {
                write!(f, "the {field} {value:?} does not parse")
            }
            EventError::EmptyTitle => write!(f, "the event has no title"),
            EventError::EndNotAfterStart => write!(f, "the event ends before it starts"),
            EventError::BadMeetingLink(link) => {
                write!(f, "the meeting link {link:?} does not name an AzMeet room")
            }
            EventError::LastDayBeforeFirst => write!(f, "the event ends before its first day"),
            EventError::BadAttendee(who) => write!(f, "{who:?} is not an e-mail address"),
            EventError::BadCalendar(id) => write!(f, "the calendar id {id:?} is not a calendar's"),
            EventError::BadRepeat(why) => write!(f, "{why}"),
        }
    }
}

/// The last minute of a day: where an all-day event ends, and the latest an event can end.
fn last_minute() -> NaiveTime {
    NaiveTime::from_hms_opt(23, 59, 0).unwrap_or(NaiveTime::MIN)
}

impl Event {
    /// A checked event with times on one day: `id` is a UUID in lower case, the title (trimmed)
    /// is not empty, the end is after the start, and a meeting's link names an AzMeet room by
    /// its id. Times are cut to the minute. Everything else is empty: set it on the event and
    /// [`Event::check`] it.
    pub fn create(
        id: &str,
        title: &str,
        date: NaiveDate,
        start: NaiveTime,
        end: NaiveTime,
        meeting: Option<Meeting>,
    ) -> Result<Event, EventError> {
        Event {
            id: id.to_string(),
            title: title.to_string(),
            date,
            start,
            end,
            meeting,
            all_day: false,
            last_day: date,
            location: String::new(),
            notes: String::new(),
            attendees: Vec::new(),
            reminder: None,
            calendar: String::new(),
            repeat: None,
            except: Vec::new(),
            uid: String::new(),
        }
        .check()
    }

    /// A checked all-day event from `date` to `last_day` (both included).
    pub fn create_all_day(
        id: &str,
        title: &str,
        date: NaiveDate,
        last_day: NaiveDate,
    ) -> Result<Event, EventError> {
        let mut event = Event::create(id, title, date, NaiveTime::MIN, last_minute(), None)?;
        event.all_day = true;
        event.last_day = last_day;
        event.check()
    }

    /// The event as it may be saved, or why not: everything [`Event::create`] checks, an
    /// all-day event's last day is not before its first (and it runs from 00:00 to the day's
    /// last minute), any other event's last day is its day, the attendees are e-mail addresses
    /// (trimmed, each once), the calendar is a calendar's id, and the exceptions are sorted
    /// and each once. Text fields are trimmed.
    pub fn check(mut self) -> Result<Event, EventError> {
        if !is_event_id(&self.id) {
            return Err(EventError::BadId(self.id));
        }
        self.title = self.title.trim().to_string();
        if self.title.is_empty() {
            return Err(EventError::EmptyTitle);
        }
        if self.all_day {
            self.start = NaiveTime::MIN;
            self.end = last_minute();
            if self.last_day < self.date {
                return Err(EventError::LastDayBeforeFirst);
            }
        } else {
            self.last_day = self.date;
        }
        let (start, end) = (to_the_minute(self.start), to_the_minute(self.end));
        if end <= start {
            return Err(EventError::EndNotAfterStart);
        }
        self.start = start;
        self.end = end;
        if let Some(m) = &self.meeting {
            if !matches!(meet_rooms::parse_room_link(&m.link), Some(RoomKey::Id(_))) {
                return Err(EventError::BadMeetingLink(m.link.clone()));
            }
        }
        self.location = self.location.trim().to_string();
        self.notes = self.notes.trim_end().to_string();
        let mut attendees: Vec<String> = Vec::with_capacity(self.attendees.len());
        for who in &self.attendees {
            let who = who.trim();
            if !is_email(who) {
                return Err(EventError::BadAttendee(who.to_string()));
            }
            if !attendees.iter().any(|a| a.eq_ignore_ascii_case(who)) {
                attendees.push(who.to_string());
            }
        }
        self.attendees = attendees;
        if !is_calendar_id(&self.calendar) {
            return Err(EventError::BadCalendar(self.calendar));
        }
        self.except.sort();
        self.except.dedup();
        self.uid = self.uid.trim().to_string();
        Ok(self)
    }

    /// The event uses a field version 2 does not have.
    #[must_use]
    pub fn needs_version_3(&self) -> bool {
        self.all_day
            || self.last_day != self.date
            || !self.location.is_empty()
            || !self.notes.is_empty()
            || !self.attendees.is_empty()
            || self.reminder.is_some()
            || !self.calendar.is_empty()
            || self.repeat.is_some()
            || !self.except.is_empty()
            || !self.uid.is_empty()
    }

    /// How many days after its first day an occurrence of the event ends (0: the same day).
    #[must_use]
    pub fn span_days(&self) -> i64 {
        (self.last_day - self.date).num_days().max(0)
    }

    /// The first days of the event's occurrences that are on any day from `from` to `to` (both
    /// included), in order: an all-day event of several days that began before `from` is one.
    #[must_use]
    pub fn starts_between(&self, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        if to < from {
            return Vec::new();
        }
        let earliest = from - Duration::days(self.span_days());
        match &self.repeat {
            None => {
                if self.date >= earliest && self.date <= to {
                    vec![self.date]
                } else {
                    Vec::new()
                }
            }
            Some(rule) => rule.dates(self.date, &self.except, earliest, to),
        }
    }

    /// The event's iCalendar UID: the imported one, else `<id>@azcalendar`.
    #[must_use]
    pub fn ical_uid(&self) -> String {
        if self.uid.is_empty() {
            format!("{}@azcalendar", self.id)
        } else {
            self.uid.clone()
        }
    }
}

fn to_the_minute(t: NaiveTime) -> NaiveTime {
    NaiveTime::from_hms_opt(t.hour(), t.minute(), 0).unwrap_or(t)
}

/// The file on disk: version 3, version 2 (without the editor's fields) and version 1 (the same
/// without a meeting's times). A field that holds nothing is left out, in that order, so a
/// version 2 event is written exactly as before.
#[derive(Serialize, Deserialize)]
struct EventFile {
    format: String,
    version: u64,
    id: String,
    title: String,
    date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    end: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    all_day: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_day: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    location: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    attendees: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reminder: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    calendar: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    repeat: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    except: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    uid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    meeting: Option<Meeting>,
}

/// The version `event`'s file is written in: 2 unless it uses version 3's fields.
#[must_use]
pub fn file_version(event: &Event) -> u64 {
    if event.needs_version_3() {
        VERSION
    } else {
        PLAIN_VERSION
    }
}

/// The event's file contents (pretty JSON, ending in a newline).
pub fn to_json(event: &Event) -> String {
    let date = |d: NaiveDate| d.format(DATE_FORMAT).to_string();
    let time = |t: NaiveTime| Some(t.format(TIME_FORMAT).to_string());
    let file = EventFile {
        format: FORMAT.to_string(),
        version: file_version(event),
        id: event.id.clone(),
        title: event.title.clone(),
        date: date(event.date),
        start: if event.all_day {
            None
        } else {
            time(event.start)
        },
        end: if event.all_day { None } else { time(event.end) },
        all_day: event.all_day,
        last_day: (event.all_day && event.last_day != event.date).then(|| date(event.last_day)),
        location: event.location.clone(),
        notes: event.notes.clone(),
        attendees: event.attendees.clone(),
        reminder: event.reminder,
        calendar: event.calendar.clone(),
        repeat: event.repeat.as_ref().map(|r| r.to_rrule(event.all_day)),
        except: event.except.iter().map(|d| date(*d)).collect(),
        uid: event.uid.clone(),
        meeting: event.meeting.clone(),
    };
    // Strings and numbers only: serializing cannot fail.
    let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
    text.push('\n');
    text
}

/// Reads an event file.
pub fn from_json(text: &str) -> Result<Event, EventError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| EventError::NotJson(e.to_string()))?;
    if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(EventError::NotAnEvent);
    }
    let version = value.get("version");
    match version.and_then(|v| v.as_u64()) {
        Some(v) if (OLDEST_VERSION..=VERSION).contains(&v) => {}
        Some(newer) if newer > VERSION => return Err(EventError::NewerVersion(newer)),
        _ => {
            return Err(EventError::BadField {
                field: "version",
                value: version.map(|v| v.to_string()).unwrap_or_default(),
            })
        }
    }
    let file: EventFile =
        serde_json::from_value(value).map_err(|e| EventError::Malformed(e.to_string()))?;
    let day = |field: &'static str, value: &str| {
        NaiveDate::parse_from_str(value, DATE_FORMAT).map_err(|_| EventError::BadField {
            field,
            value: value.to_string(),
        })
    };
    let date = day("date", &file.date)?;
    let time = |field: &'static str, value: Option<&String>| {
        let value = value.map(String::as_str).unwrap_or_default();
        NaiveTime::parse_from_str(value, TIME_FORMAT).map_err(|_| EventError::BadField {
            field,
            value: value.to_string(),
        })
    };
    let (start, end) = if file.all_day {
        (NaiveTime::MIN, last_minute())
    } else {
        (
            time("start", file.start.as_ref())?,
            time("end", file.end.as_ref())?,
        )
    };
    let last_day = match &file.last_day {
        Some(text) => day("last_day", text)?,
        None => date,
    };
    let repeat = match &file.repeat {
        Some(text) => Some(Rule::parse(text).map_err(|e| EventError::BadRepeat(e.to_string()))?),
        None => None,
    };
    let except = file
        .except
        .iter()
        .map(|d| day("except", d))
        .collect::<Result<Vec<_>, _>>()?;
    Event {
        id: file.id,
        title: file.title,
        date,
        start,
        end,
        meeting: file.meeting,
        all_day: file.all_day,
        last_day,
        location: file.location,
        notes: file.notes,
        attendees: file.attendees,
        reminder: file.reminder,
        calendar: file.calendar,
        repeat,
        except,
        uid: file.uid,
    }
    .check()
}

/// Whether `s` is a UUID in its canonical lower-case form (8-4-4-4-12 hex digits), which is
/// what an event id must be: it names a file, so nothing else may reach the file system.
pub fn is_event_id(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8usize, 4, 4, 4, 12])
            .all(|(group, len)| {
                group.len() == len
                    && group
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
}

/// Whether `s` names a calendar an event can be in: empty (the default calendar) or a calendar's
/// id, a UUID in lower case like an event's (it names the calendar's file, `calendars.rs`).
pub fn is_calendar_id(s: &str) -> bool {
    s.is_empty() || is_event_id(s)
}

/// `<id>.json`
pub fn file_name(id: &str) -> String {
    format!("{id}.json")
}

/// `events/<id>.json`: the event's path under the data folder, and its key in a bucket.
pub fn object_key(id: &str) -> String {
    format!("{EVENTS_DIR}/{}", file_name(id))
}

/// The event id a file in the events folder is named by, if it is an event file.
pub fn id_of_file_name(name: &str) -> Option<&str> {
    let id = name.strip_suffix(".json")?;
    is_event_id(id).then_some(id)
}

/// The data folder: `setting` (`AZCAL_DATA`), else `AzCalendar` in the user's data folder, else
/// `AzCalendar` in the current folder.
pub fn data_dir(setting: Option<&str>, user_data: Option<PathBuf>) -> PathBuf {
    match setting.map(str::trim).filter(|s| !s.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => user_data.unwrap_or_default().join(APP_DIR),
    }
}

/// A file in the events folder that is named like an event but was not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// Its key in the data folder's drive (`events/<id>.json`).
    pub key: String,
    pub reason: String,
}

/// Every event the drive keeps (`events/<id>.json`; the drive is the data folder's - a
/// `LocalDrive` today, the user's bucket later), in order of date, start, end and title, and the
/// event files that could not be read, with why. Keys not named `events/<uuid>.json` are not
/// events and are left out silently; a drive without events is an empty calendar.
pub fn load(drive: &dyn Drive) -> (Vec<Event>, Vec<Skipped>) {
    let mut events = Vec::new();
    let mut skipped = Vec::new();
    let prefix = format!("{EVENTS_DIR}/");
    let Ok(objects) = azul_storage::ops::list_all(drive, &prefix) else {
        return (events, skipped);
    };
    for object in &objects {
        let Some(id) = object.key.strip_prefix(&prefix).and_then(id_of_file_name) else {
            continue;
        };
        let read = drive
            .get(&object.key)
            .map_err(|e| e.to_string())
            .and_then(|bytes| String::from_utf8(bytes).map_err(|e| e.to_string()))
            .and_then(|text| from_json(&text).map_err(|e| e.to_string()));
        match read {
            Ok(event) if event.id == id => events.push(event),
            Ok(event) => skipped.push(Skipped {
                key: object.key.clone(),
                reason: format!("it holds the event {}, not {id}", event.id),
            }),
            Err(reason) => skipped.push(Skipped {
                key: object.key.clone(),
                reason,
            }),
        }
    }
    events.sort_by(|a, b| {
        (a.date, a.start, a.end, &a.title, &a.id).cmp(&(b.date, b.start, b.end, &b.title, &b.id))
    });
    (events, skipped)
}

/// A new event's id: a random version-4 UUID (lower case, hyphenated), azul's
/// `Uuid::from_seed` of a random seed.
///
/// Not azul's `Uuid::v4`: that is a deterministic marker mint (the same
/// sequence in every process), fine for DOM markers and wrong for a file
/// name that other devices and an S3 bucket share. `Uuid::from_seed` is a
/// pure function of its seed, so the id is exactly as random as the seed.
///
/// The seed is `azul_storage::ids::random_seed`, the one every Azlin app mints file ids from
/// (this file had its own copy, as AzTasks had - DEDUP_EDITORS B1).
#[must_use]
pub fn new_event_id() -> String {
    azul::uuid::Uuid::from_seed(azul_storage::ids::random_seed())
        .as_str()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_dir::TempDir;

    const ID: &str = "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a";
    const ID2: &str = "9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3";
    const ROOM: &str = "a2h859hyqkfaa11nhzxfh3gd7f";

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn meeting() -> Meeting {
        Meeting {
            link: format!("azlin://meet/{ROOM}"),
            server: String::from("http://127.0.0.1:8787"),
            code: String::from("xq4-8kd-2nm"),
            expires: String::from("2026-09-30T10:00:00.000Z"),
            starts_at: String::from("2026-09-30T07:00:00.000Z"),
            ends_at: String::from("2026-09-30T08:00:00.000Z"),
            pending: false,
        }
    }

    /// The meeting as a server from before meeting times minted it: no times.
    fn without_times(m: Meeting) -> Meeting {
        Meeting {
            starts_at: String::new(),
            ends_at: String::new(),
            ..m
        }
    }

    /// `"version": <the version a plain event is written in>`, as `to_json` writes it.
    fn this_version() -> String {
        format!("\"version\": {PLAIN_VERSION}")
    }

    fn sync(meeting: Option<Meeting>) -> Event {
        Event::create(
            ID,
            "Team sync",
            day(2026, 9, 30),
            at(9, 0),
            at(10, 0),
            meeting,
        )
        .unwrap()
    }

    #[test]
    fn an_event_file_round_trips_with_and_without_a_meeting() {
        for event in [sync(None), sync(Some(meeting()))] {
            assert_eq!(from_json(&to_json(&event)), Ok(event.clone()));
        }
    }

    #[test]
    fn an_event_file_says_what_it_is_and_which_version_it_is() {
        let text = to_json(&sync(Some(meeting())));
        assert!(text.ends_with('\n'));
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["format"], "azcalendar.event");
        assert_eq!(json["version"], 2);
        assert_eq!(json["id"], ID);
        assert_eq!(json["title"], "Team sync");
        assert_eq!(json["date"], "2026-09-30");
        assert_eq!(json["start"], "09:00");
        assert_eq!(json["end"], "10:00");
        assert_eq!(json["meeting"]["link"], format!("azlin://meet/{ROOM}"));
        assert_eq!(json["meeting"]["server"], "http://127.0.0.1:8787");
        assert_eq!(json["meeting"]["code"], "xq4-8kd-2nm");
        assert_eq!(json["meeting"]["expires"], "2026-09-30T10:00:00.000Z");
        assert_eq!(json["meeting"]["starts_at"], "2026-09-30T07:00:00.000Z");
        assert_eq!(json["meeting"]["ends_at"], "2026-09-30T08:00:00.000Z");

        let plain: serde_json::Value = serde_json::from_str(&to_json(&sync(None))).unwrap();
        assert!(plain.get("meeting").is_none(), "{plain}");
    }

    #[test]
    fn a_file_from_a_newer_azcalendar_is_refused_not_guessed_at() {
        let newer = format!("\"version\": {}", VERSION + 1);
        let text = to_json(&sync(None)).replace(&this_version(), &newer);
        assert_eq!(from_json(&text), Err(EventError::NewerVersion(VERSION + 1)));
    }

    /// Version 1 (AzCalendar before meeting times) is the same file without
    /// `meeting.starts_at` / `meeting.ends_at`: it still reads, and is written
    /// in this version when saved again.
    #[test]
    fn a_version_1_file_from_before_meeting_times_still_reads() {
        let v1 = format!(
            r#"{{
  "format": "azcalendar.event",
  "version": 1,
  "id": "{ID}",
  "title": "Team sync",
  "date": "2026-09-30",
  "start": "09:00",
  "end": "10:00",
  "meeting": {{
    "link": "azlin://meet/{ROOM}",
    "server": "http://127.0.0.1:8787",
    "code": "xq4-8kd-2nm",
    "expires": "2026-10-01T09:00:00.000Z"
  }}
}}
"#
        );
        let event = from_json(&v1).unwrap();
        assert_eq!(
            event,
            sync(Some(Meeting {
                expires: String::from("2026-10-01T09:00:00.000Z"),
                ..without_times(meeting())
            }))
        );
        let json: serde_json::Value = serde_json::from_str(&to_json(&event)).unwrap();
        assert_eq!(json["version"], PLAIN_VERSION);
    }

    #[test]
    fn a_meetings_times_round_trip_through_the_event_file_and_are_left_out_when_unknown() {
        let timed = sync(Some(meeting()));
        assert_eq!(from_json(&to_json(&timed)), Ok(timed));
        let untimed = sync(Some(without_times(meeting())));
        let text = to_json(&untimed);
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(json["meeting"].get("starts_at").is_none(), "{json}");
        assert!(json["meeting"].get("ends_at").is_none(), "{json}");
        assert_eq!(from_json(&text), Ok(untimed));
    }

    /// A link made while the meeting server could not be reached is in the file at once, marked
    /// `pending` until the server has registered its room; a registered one leaves the mark
    /// out, and a file without it (every file from before) is registered.
    #[test]
    fn a_link_made_offline_is_pending_in_the_file_until_the_server_has_its_room() {
        let pending = Meeting {
            code: String::new(),
            expires: String::new(),
            pending: true,
            ..without_times(meeting())
        };
        let event = sync(Some(pending));
        let text = to_json(&event);
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["meeting"]["pending"], true);
        assert_eq!(json["meeting"]["link"], format!("azlin://meet/{ROOM}"));
        assert_eq!(from_json(&text), Ok(event));

        let registered = to_json(&sync(Some(meeting())));
        let json: serde_json::Value = serde_json::from_str(&registered).unwrap();
        assert!(json["meeting"].get("pending").is_none(), "{json}");
        assert!(!from_json(&registered).unwrap().meeting.unwrap().pending);
    }

    #[test]
    fn fields_this_version_does_not_know_are_ignored() {
        let text = to_json(&sync(None)).replacen('{', "{\n  \"colour\": \"blue\",", 1);
        assert_eq!(from_json(&text), Ok(sync(None)));
    }

    #[test]
    fn a_file_that_is_not_an_event_is_refused() {
        assert!(matches!(from_json("{"), Err(EventError::NotJson(_))));
        assert!(matches!(from_json(""), Err(EventError::NotJson(_))));
        assert_eq!(from_json("[]"), Err(EventError::NotAnEvent));
        assert_eq!(
            from_json(r#"{"format": "azcontacts.contact", "version": 1}"#),
            Err(EventError::NotAnEvent)
        );
        let good = to_json(&sync(None));
        assert!(matches!(
            from_json(&good.replace(&this_version(), "\"version\": \"one\"")),
            Err(EventError::BadField {
                field: "version",
                ..
            })
        ));
        assert!(matches!(
            from_json(&good.replace(&this_version(), "\"version\": 0")),
            Err(EventError::BadField {
                field: "version",
                ..
            })
        ));
        assert!(matches!(
            from_json(&good.replace("\"title\"", "\"name\"")),
            Err(EventError::Malformed(_))
        ));
    }

    #[test]
    fn a_date_or_time_that_does_not_exist_is_refused() {
        let good = to_json(&sync(None));
        assert_eq!(
            from_json(&good.replace("2026-09-30", "2026-02-30")),
            Err(EventError::BadField {
                field: "date",
                value: String::from("2026-02-30")
            })
        );
        assert_eq!(
            from_json(&good.replace("\"09:00\"", "\"25:00\"")),
            Err(EventError::BadField {
                field: "start",
                value: String::from("25:00")
            })
        );
        assert!(matches!(
            from_json(&good.replace("\"10:00\"", "\"ten\"")),
            Err(EventError::BadField { field: "end", .. })
        ));
    }

    #[test]
    fn an_event_needs_a_title_and_an_end_after_its_start() {
        let d = day(2026, 9, 30);
        assert_eq!(
            Event::create(ID, "  ", d, at(9, 0), at(10, 0), None),
            Err(EventError::EmptyTitle)
        );
        assert_eq!(
            Event::create(ID, "Sync", d, at(10, 0), at(10, 0), None),
            Err(EventError::EndNotAfterStart)
        );
        assert_eq!(
            Event::create(ID, "Sync", d, at(10, 0), at(9, 0), None),
            Err(EventError::EndNotAfterStart)
        );
        let trimmed = Event::create(ID, "  Sync \n", d, at(9, 0), at(10, 0), None).unwrap();
        assert_eq!(trimmed.title, "Sync");
    }

    #[test]
    fn times_are_kept_to_the_minute() {
        let d = day(2026, 9, 30);
        let start = NaiveTime::from_hms_opt(9, 0, 42).unwrap();
        let event = Event::create(ID, "Sync", d, start, at(10, 0), None).unwrap();
        assert_eq!(event.start, at(9, 0));
        assert_eq!(from_json(&to_json(&event)), Ok(event));
    }

    #[test]
    fn a_meeting_link_must_name_an_azmeet_room_by_its_id() {
        let d = day(2026, 9, 30);
        for link in [
            "https://example.com/",
            "",
            // A code is looked up on the server; an event keeps the room id itself.
            "azlin://meet/xq4-8kd-2nm",
        ] {
            let m = Meeting {
                link: link.to_string(),
                ..meeting()
            };
            assert_eq!(
                Event::create(ID, "Sync", d, at(9, 0), at(10, 0), Some(m)),
                Err(EventError::BadMeetingLink(link.to_string())),
                "{link:?}"
            );
        }
        let text = to_json(&sync(Some(meeting()))).replace(ROOM, "not-a-room");
        assert!(matches!(
            from_json(&text),
            Err(EventError::BadMeetingLink(_))
        ));
    }

    /// An event id names a FILE that other devices (and an S3 bucket) share,
    /// so it must not depend on how many ids this process minted before:
    /// azul's `Uuid::v4` is a deterministic marker mint (its first id in
    /// every process is `00000000-0000-4000-...`), and two runs overwrote
    /// each other's first event.
    #[test]
    fn a_new_event_id_is_random_not_the_process_local_marker_sequence() {
        let ids: Vec<String> = (0..256).map(|_| new_event_id()).collect();
        for id in &ids {
            assert!(is_event_id(id), "{id}");
            assert!(!id.starts_with("00000000-0000"), "{id}");
            // Version 4, variant 0b10 (RFC 4122).
            assert_eq!(&id[14..15], "4", "{id}");
            assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"), "{id}");
        }
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "256 ids, all distinct");
        // Independently seeded mints (what two processes are) disagree.
        assert_ne!(new_event_id(), new_event_id());
    }

    #[test]
    fn an_event_id_is_a_lower_case_uuid() {
        assert!(is_event_id(ID));
        assert!(is_event_id(ID2));
        for bad in [
            "",
            "0B0F6F2E-5B8E-4C43-9A57-3F1F0D6F4B1A",
            "0b0f6f2e5b8e4c439a573f1f0d6f4b1a",
            "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1",
            "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1aa",
            "0b0f6f2e-5b8e-4c43-9a57_3f1f0d6f4b1a",
            "0b0f6f2g-5b8e-4c43-9a57-3f1f0d6f4b1a",
            "../../../../etc/passwd",
            "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f/b1a",
        ] {
            assert!(!is_event_id(bad), "{bad:?}");
        }
        assert_eq!(
            Event::create("x", "Sync", day(2026, 9, 30), at(9, 0), at(10, 0), None),
            Err(EventError::BadId(String::from("x")))
        );
    }

    #[test]
    fn an_event_file_is_named_by_its_id_under_events() {
        assert_eq!(file_name(ID), format!("{ID}.json"));
        assert_eq!(object_key(ID), format!("events/{ID}.json"));
        assert_eq!(id_of_file_name(&format!("{ID}.json")), Some(ID));
        for other in [
            format!(".{ID}.json.tmp"),
            format!("{ID}.JSON"),
            format!("{ID}.json.bak"),
            ID.to_string(),
            String::from("notes.txt"),
            String::from(".json"),
        ] {
            assert_eq!(id_of_file_name(&other), None, "{other:?}");
        }
    }

    #[test]
    fn the_data_folder_is_azcal_data_else_the_users_data_folder() {
        assert_eq!(
            data_dir(
                Some("/tmp/cal"),
                Some(PathBuf::from("/home/ada/.local/share"))
            ),
            PathBuf::from("/tmp/cal")
        );
        assert_eq!(
            data_dir(Some("  "), Some(PathBuf::from("/home/ada/.local/share"))),
            PathBuf::from("/home/ada/.local/share/AzCalendar")
        );
        assert_eq!(
            data_dir(None, Some(PathBuf::from("/home/ada/.local/share"))),
            PathBuf::from("/home/ada/.local/share/AzCalendar")
        );
        assert_eq!(data_dir(None, None), PathBuf::from("AzCalendar"));
    }

    /// The drive of a data folder on disk, for the storage tests.
    fn local(dir: &TempDir) -> azul_storage::LocalDrive {
        azul_storage::LocalDrive::new(&dir.0)
    }

    fn put(drive: &dyn Drive, event: &Event) {
        drive.put(&object_key(&event.id), to_json(event).as_bytes()).unwrap();
    }

    #[test]
    fn a_put_event_is_one_file_that_reads_back_and_putting_again_replaces_it() {
        let dir = TempDir::create();
        let drive = local(&dir);
        put(&drive, &sync(None));
        let later = sync(Some(meeting()));
        put(&drive, &later);
        assert!(dir.0.join("events").join(format!("{ID}.json")).is_file());
        let (events, skipped) = load(&drive);
        assert_eq!(events, vec![later]);
        assert!(skipped.is_empty(), "{skipped:?}");
        drive.delete(&object_key(ID)).unwrap();
        assert!(load(&drive).0.is_empty());
    }

    #[test]
    fn events_are_read_in_order_and_other_files_are_left_out() {
        let dir = TempDir::create();
        let drive = local(&dir);
        let d = day(2026, 9, 30);
        let late = Event::create(ID, "Late", d, at(15, 0), at(16, 0), None).unwrap();
        let early = Event::create(ID2, "Early", d, at(8, 0), at(9, 0), None).unwrap();
        put(&drive, &late);
        put(&drive, &early);
        drive.put("events/readme.txt", b"not an event").unwrap();
        drive.put(&format!("events/.{ID}.json.tmp"), b"{").unwrap();
        let broken = "events/11111111-2222-4333-8444-555555555555.json";
        drive.put(broken, b"{").unwrap();
        // A file whose id is not its name was copied or renamed: it is not read as that event.
        let misnamed = "events/22222222-2222-4333-8444-555555555555.json";
        drive.put(misnamed, to_json(&early).as_bytes()).unwrap();

        let (events, skipped) = load(&drive);
        assert_eq!(events, vec![early, late]);
        let mut keys: Vec<&str> = skipped.iter().map(|s| s.key.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec![broken, misnamed]);
        assert!(skipped.iter().all(|s| !s.reason.is_empty()));
    }

    /// The start reads the events through the data folder's drive: wherever it keeps them (here a
    /// grant of `calendar/` in a bigger tree), not with `std::fs` on a folder.
    #[test]
    fn the_events_are_read_through_the_drive_wherever_it_keeps_them() {
        use azul_storage::{LocalDrive, ScopedDrive};
        let root = TempDir::create();
        let tree = LocalDrive::new(&root.0);
        let d = day(2026, 9, 30);
        let late = Event::create(ID, "Late", d, at(15, 0), at(16, 0), None).unwrap();
        let early = Event::create(ID2, "Early", d, at(8, 0), at(9, 0), None).unwrap();
        tree.put(&format!("calendar/{}", object_key(ID)), to_json(&late).as_bytes()).unwrap();
        tree.put(&format!("calendar/{}", object_key(ID2)), to_json(&early).as_bytes()).unwrap();
        tree.put("calendar/events/11111111-2222-4333-8444-555555555555.json", b"{").unwrap();
        tree.put("calendar/events/readme.txt", b"not an event").unwrap();
        tree.put("calendar/events/old/22222222-2222-4333-8444-555555555555.json", b"{").unwrap();
        let drive = ScopedDrive::new(LocalDrive::new(&root.0), "calendar/", false).unwrap();
        let (events, skipped) = load(&drive);
        assert_eq!(events, vec![early, late]);
        assert_eq!(skipped.len(), 1, "the broken file: {skipped:?}");
        let empty = ScopedDrive::new(LocalDrive::new(&root.0), "nothing-here/", false).unwrap();
        assert_eq!(load(&empty), (Vec::new(), Vec::new()));
    }

    #[test]
    fn a_missing_data_folder_is_an_empty_calendar() {
        let dir = TempDir::create();
        let (events, skipped) = load(&azul_storage::LocalDrive::new(&dir.0.join("nothing here")));
        assert!(events.is_empty());
        assert!(skipped.is_empty());
    }

    // ==== version 3: the event editor's fields ====

    const CAL: &str = "9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3";

    /// An event that uses every field of version 3.
    fn planning() -> Event {
        let mut e =
            Event::create_all_day(ID, "Planning days", day(2026, 9, 30), day(2026, 10, 2)).unwrap();
        e.location = String::from("Room 4");
        e.notes = String::from("Bring the roadmap.");
        e.attendees = vec![String::from("ana@example.com")];
        e.reminder = Some(15);
        e.calendar = String::from(CAL);
        e.repeat = Some(Rule::parse("FREQ=WEEKLY;BYDAY=WE").unwrap());
        e.except = vec![day(2026, 10, 7)];
        e.uid = String::from("abc123@google.com");
        e.check().unwrap()
    }

    /// The event editor added fields; an event that uses none of them is still the file the
    /// AzCalendar before it wrote (and reads), byte for byte.
    #[test]
    fn an_event_that_uses_none_of_the_editors_fields_is_written_as_version_2_byte_for_byte() {
        let expected = format!(
            "{{\n  \"format\": \"azcalendar.event\",\n  \"version\": 2,\n  \"id\": \"{ID}\",\n  \
             \"title\": \"Team sync\",\n  \"date\": \"2026-09-30\",\n  \"start\": \"09:00\",\n  \
             \"end\": \"10:00\"\n}}\n"
        );
        assert_eq!(to_json(&sync(None)), expected);
        assert_eq!(file_version(&sync(Some(meeting()))), PLAIN_VERSION);
    }

    #[test]
    fn the_editors_fields_round_trip_through_a_version_3_file() {
        let event = planning();
        let text = to_json(&event);
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["version"], 3);
        assert_eq!(json["all_day"], true);
        assert_eq!(json["last_day"], "2026-10-02");
        assert!(
            json.get("start").is_none() && json.get("end").is_none(),
            "{json}"
        );
        assert_eq!(json["location"], "Room 4");
        assert_eq!(json["notes"], "Bring the roadmap.");
        assert_eq!(json["attendees"][0], "ana@example.com");
        assert_eq!(json["reminder"], 15);
        assert_eq!(json["calendar"], CAL);
        assert_eq!(json["repeat"], "FREQ=WEEKLY;BYDAY=WE");
        assert_eq!(json["except"][0], "2026-10-07");
        assert_eq!(json["uid"], "abc123@google.com");
        assert_eq!(from_json(&text), Ok(event));
        // One field is enough for version 3.
        let mut located = sync(None);
        located.location = String::from("Room 4");
        assert_eq!(file_version(&located), VERSION);
    }

    #[test]
    fn an_all_day_event_runs_the_whole_of_its_days() {
        let e = Event::create_all_day(ID, "Holiday", day(2026, 10, 9), day(2026, 10, 11)).unwrap();
        assert!(e.all_day);
        assert_eq!((e.start, e.end), (at(0, 0), at(23, 59)));
        assert_eq!(e.span_days(), 2);
        assert_eq!(
            Event::create_all_day(ID, "Holiday", day(2026, 10, 9), day(2026, 10, 8)),
            Err(EventError::LastDayBeforeFirst)
        );
        // An event with times ends on its own day, whatever `last_day` said.
        let mut timed = sync(None);
        timed.last_day = day(2026, 10, 3);
        assert_eq!(timed.check().unwrap().last_day, day(2026, 9, 30));
    }

    #[test]
    fn attendees_are_e_mail_addresses_each_once_and_the_calendar_is_a_calendars_id() {
        let mut e = sync(None);
        e.attendees = vec![
            String::from(" ana@example.com "),
            String::from("ANA@example.com"),
            String::from("bo@example.org"),
        ];
        assert_eq!(
            e.clone().check().unwrap().attendees,
            vec![
                String::from("ana@example.com"),
                String::from("bo@example.org")
            ]
        );
        e.attendees.push(String::from("not an address"));
        assert_eq!(
            e.check(),
            Err(EventError::BadAttendee(String::from("not an address")))
        );
        let mut e = sync(None);
        e.calendar = String::from("../work");
        assert_eq!(
            e.check(),
            Err(EventError::BadCalendar(String::from("../work")))
        );
        // What an address is: azul_pim::mail_address::is_email and its tests.
    }

    #[test]
    fn a_repeating_event_is_on_the_dates_its_rule_makes_without_its_exceptions() {
        let mut e = sync(None);
        e.repeat = Some(Rule::parse("FREQ=WEEKLY").unwrap());
        e.except = vec![day(2026, 10, 14)];
        let e = e.check().unwrap();
        assert_eq!(
            e.starts_between(day(2026, 9, 28), day(2026, 10, 25)),
            vec![day(2026, 9, 30), day(2026, 10, 7), day(2026, 10, 21)]
        );
        // The next week shows the next one.
        assert_eq!(
            e.starts_between(day(2026, 10, 5), day(2026, 10, 11)),
            vec![day(2026, 10, 7)]
        );
        // An event that does not repeat is on its day only.
        let once = sync(None);
        assert_eq!(
            once.starts_between(day(2026, 9, 28), day(2026, 10, 4)),
            vec![day(2026, 9, 30)]
        );
        assert!(once
            .starts_between(day(2026, 10, 5), day(2026, 10, 11))
            .is_empty());
    }

    #[test]
    fn a_several_day_event_that_began_before_a_view_is_in_it() {
        let e = Event::create_all_day(ID, "Holiday", day(2026, 10, 9), day(2026, 10, 12)).unwrap();
        assert_eq!(
            e.starts_between(day(2026, 10, 12), day(2026, 10, 18)),
            vec![day(2026, 10, 9)]
        );
        assert!(e
            .starts_between(day(2026, 10, 13), day(2026, 10, 18))
            .is_empty());
    }

    #[test]
    fn a_file_with_a_repeat_rule_outside_the_subset_is_refused_with_the_reason() {
        let text = to_json(&planning()).replace("FREQ=WEEKLY;BYDAY=WE", "FREQ=HOURLY");
        assert!(
            matches!(from_json(&text), Err(EventError::BadRepeat(why)) if why.contains("HOURLY"))
        );
    }

    #[test]
    fn an_event_made_here_has_an_icalendar_uid_of_its_own_id() {
        assert_eq!(sync(None).ical_uid(), format!("{ID}@azcalendar"));
        assert_eq!(planning().ical_uid(), "abc123@google.com");
    }
}
