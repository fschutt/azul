//! One calendar event is one JSON file, `<data dir>/events/<event id>.json`, where the id is a
//! version 4 UUID. The same relative path is the object key in a bucket (`events/<id>.json`), so
//! the folder can move to S3 or R2 as it is. Durable data lives only in these files: the meeting
//! server (the `meet` Worker) mints a meeting's link, and the event file keeps it.
//!
//! The format, version 1:
//!
//! ```json
//! {
//!   "format": "azcalendar.event",
//!   "version": 1,
//!   "id": "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a",
//!   "title": "Team sync",
//!   "date": "2026-09-30",
//!   "start": "09:00",
//!   "end": "10:00",
//!   "meeting": {
//!     "link": "azlin://meet/a2h859hyqkfaa11nhzxfh3gd7f",
//!     "server": "http://127.0.0.1:8787",
//!     "code": "xq4-8kd-2nm",
//!     "expires": "2026-10-01T09:00:00.000Z"
//!   }
//! }
//! ```
//!
//! Times are wall-clock times on the event's day (no time zones in this version); an event ends
//! on the day it starts. A file with a higher `version` was written by a newer AzCalendar and is
//! left alone, never guessed at; fields this version does not know are ignored.

use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

use crate::meet_rooms::{self, RoomKey};

/// The `format` of an event file.
pub const FORMAT: &str = "azcalendar.event";
/// The version this AzCalendar writes and reads.
pub const VERSION: u64 = 1;
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
    pub date: NaiveDate,
    /// Start and end, to the minute.
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub meeting: Option<Meeting>,
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
                    "written by a newer AzCalendar (version {v}; this one reads {VERSION})"
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
        }
    }
}

impl Event {
    /// A checked event: `id` is a UUID in lower case, the title (trimmed) is not empty, the end
    /// is after the start, and a meeting's link names an AzMeet room by its id. Times are cut to
    /// the minute.
    pub fn create(
        id: &str,
        title: &str,
        date: NaiveDate,
        start: NaiveTime,
        end: NaiveTime,
        meeting: Option<Meeting>,
    ) -> Result<Event, EventError> {
        if !is_event_id(id) {
            return Err(EventError::BadId(id.to_string()));
        }
        let title = title.trim();
        if title.is_empty() {
            return Err(EventError::EmptyTitle);
        }
        let (start, end) = (to_the_minute(start), to_the_minute(end));
        if end <= start {
            return Err(EventError::EndNotAfterStart);
        }
        if let Some(m) = &meeting {
            if !matches!(meet_rooms::parse_room_link(&m.link), Some(RoomKey::Id(_))) {
                return Err(EventError::BadMeetingLink(m.link.clone()));
            }
        }
        Ok(Event {
            id: id.to_string(),
            title: title.to_string(),
            date,
            start,
            end,
            meeting,
        })
    }
}

fn to_the_minute(t: NaiveTime) -> NaiveTime {
    NaiveTime::from_hms_opt(t.hour(), t.minute(), 0).unwrap_or(t)
}

/// The file on disk, version 1.
#[derive(Serialize, Deserialize)]
struct FileV1 {
    format: String,
    version: u64,
    id: String,
    title: String,
    date: String,
    start: String,
    end: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    meeting: Option<Meeting>,
}

/// The event's file contents (pretty JSON, ending in a newline).
pub fn to_json(event: &Event) -> String {
    let file = FileV1 {
        format: FORMAT.to_string(),
        version: VERSION,
        id: event.id.clone(),
        title: event.title.clone(),
        date: event.date.format(DATE_FORMAT).to_string(),
        start: event.start.format(TIME_FORMAT).to_string(),
        end: event.end.format(TIME_FORMAT).to_string(),
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
        Some(VERSION) => {}
        Some(newer) if newer > VERSION => return Err(EventError::NewerVersion(newer)),
        _ => {
            return Err(EventError::BadField {
                field: "version",
                value: version.map(|v| v.to_string()).unwrap_or_default(),
            })
        }
    }
    let file: FileV1 =
        serde_json::from_value(value).map_err(|e| EventError::Malformed(e.to_string()))?;
    let date =
        NaiveDate::parse_from_str(&file.date, DATE_FORMAT).map_err(|_| EventError::BadField {
            field: "date",
            value: file.date.clone(),
        })?;
    let time = |field: &'static str, value: &str| {
        NaiveTime::parse_from_str(value, TIME_FORMAT).map_err(|_| EventError::BadField {
            field,
            value: value.to_string(),
        })
    };
    let start = time("start", &file.start)?;
    let end = time("end", &file.end)?;
    Event::create(&file.id, &file.title, date, start, end, file.meeting)
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

/// Where the event with `id` is stored under `data_dir`.
pub fn event_path(data_dir: &Path, id: &str) -> PathBuf {
    data_dir.join(EVENTS_DIR).join(file_name(id))
}

/// The data folder: `setting` (`AZCAL_DATA`), else `AzCalendar` in the user's data folder, else
/// `AzCalendar` in the current folder.
pub fn data_dir(setting: Option<&str>, user_data: Option<PathBuf>) -> PathBuf {
    match setting.map(str::trim).filter(|s| !s.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => user_data.unwrap_or_default().join(APP_DIR),
    }
}

/// Writes `event` to its file, atomically (a temporary file next to it, then a rename), and
/// returns the file's path.
pub fn save(data_dir: &Path, event: &Event) -> std::io::Result<PathBuf> {
    let path = event_path(data_dir, &event.id);
    let dir = data_dir.join(EVENTS_DIR);
    std::fs::create_dir_all(&dir)?;
    // A dot name that `id_of_file_name` never reads as an event.
    let temp = dir.join(format!(".{}.tmp", file_name(&event.id)));
    std::fs::write(&temp, to_json(event))?;
    if let Err(e) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    Ok(path)
}

/// A file in the events folder that is named like an event but was not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: String,
}

/// Every event in the data folder, in order of date, start, end and title, and the event files
/// that could not be read, with why. Files not named `<uuid>.json` are not events and are left
/// out silently; a missing folder is an empty calendar.
pub fn load_all(data_dir: &Path) -> (Vec<Event>, Vec<Skipped>) {
    let mut events = Vec::new();
    let mut skipped = Vec::new();
    let Ok(entries) = std::fs::read_dir(data_dir.join(EVENTS_DIR)) else {
        return (events, skipped);
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(id) = name.to_str().and_then(id_of_file_name) else {
            continue;
        };
        let path = entry.path();
        let read = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| from_json(&text).map_err(|e| e.to_string()));
        match read {
            Ok(event) if event.id == id => events.push(event),
            Ok(event) => skipped.push(Skipped {
                path,
                reason: format!("it holds the event {}, not {id}", event.id),
            }),
            Err(reason) => skipped.push(Skipped { path, reason }),
        }
    }
    events.sort_by(|a, b| {
        (a.date, a.start, a.end, &a.title, &a.id).cmp(&(b.date, b.start, b.end, &b.title, &b.id))
    });
    (events, skipped)
}

/// A new event's id: a random version-4 UUID (lower case, hyphenated).
///
/// NOT azul's `Uuid::v4`: that is a deterministic marker mint (the same
/// sequence in every process), fine for DOM markers and wrong for a file
/// name that other devices and an S3 bucket share. The 128 bits come from
/// `std`'s `RandomState` - keys the OS seeds per process - hashed with the
/// time, the process id and a counter; no extra dependency.
#[must_use]
pub fn new_event_id() -> String {
    use std::{
        collections::hash_map::RandomState,
        hash::{BuildHasher, Hasher},
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    static MINTED: AtomicU64 = AtomicU64::new(0);
    let n = MINTED.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let word = |salt: u64| {
        let mut h = RandomState::new().build_hasher();
        h.write_u64(salt);
        h.write_u64(n);
        h.write_u128(nanos);
        h.write_u32(std::process::id());
        h.finish()
    };
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&word(1).to_be_bytes());
    b[8..].copy_from_slice(&word(2).to_be_bytes());
    // RFC 4122: version 4, variant 0b10.
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

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

    /// `"version": <this version>`, as `to_json` writes it.
    fn this_version() -> String {
        format!("\"version\": {VERSION}")
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

    /// A folder of its own under the system's temporary folder, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "azcalendar-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
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
        assert_eq!(json["version"], VERSION);
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
        let data = Path::new("/data/cal");
        assert_eq!(
            event_path(data, ID),
            data.join("events").join(format!("{ID}.json"))
        );
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

    #[test]
    fn a_saved_event_is_one_file_that_reads_back() {
        let dir = TempDir::new();
        let event = sync(Some(meeting()));
        let path = save(&dir.0, &event).unwrap();
        assert_eq!(path, event_path(&dir.0, ID));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), to_json(&event));
        let names: Vec<String> = std::fs::read_dir(dir.0.join("events"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            vec![format!("{ID}.json")],
            "no temporary file is left"
        );
        let (events, skipped) = load_all(&dir.0);
        assert_eq!(events, vec![event]);
        assert!(skipped.is_empty(), "{skipped:?}");
    }

    #[test]
    fn saving_again_replaces_the_file() {
        let dir = TempDir::new();
        save(&dir.0, &sync(None)).unwrap();
        let later = sync(Some(meeting()));
        save(&dir.0, &later).unwrap();
        assert_eq!(load_all(&dir.0).0, vec![later]);
    }

    #[test]
    fn events_are_read_in_order_and_other_files_are_left_out() {
        let dir = TempDir::new();
        let d = day(2026, 9, 30);
        let late = Event::create(ID, "Late", d, at(15, 0), at(16, 0), None).unwrap();
        let early = Event::create(ID2, "Early", d, at(8, 0), at(9, 0), None).unwrap();
        save(&dir.0, &late).unwrap();
        save(&dir.0, &early).unwrap();
        let events_dir = dir.0.join("events");
        std::fs::write(events_dir.join("readme.txt"), "not an event").unwrap();
        std::fs::write(events_dir.join(format!(".{ID}.json.tmp")), "{").unwrap();
        let broken = events_dir.join("11111111-2222-4333-8444-555555555555.json");
        std::fs::write(&broken, "{").unwrap();
        // A file whose id is not its name was copied or renamed: it is not read as that event.
        let misnamed = events_dir.join("22222222-2222-4333-8444-555555555555.json");
        std::fs::write(&misnamed, to_json(&early)).unwrap();

        let (events, skipped) = load_all(&dir.0);
        assert_eq!(events, vec![early, late]);
        let mut paths: Vec<PathBuf> = skipped.iter().map(|s| s.path.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec![broken, misnamed]);
        assert!(skipped.iter().all(|s| !s.reason.is_empty()));
    }

    #[test]
    fn a_missing_data_folder_is_an_empty_calendar() {
        let dir = TempDir::new();
        let (events, skipped) = load_all(&dir.0.join("nothing here"));
        assert!(events.is_empty());
        assert!(skipped.is_empty());
    }
}
