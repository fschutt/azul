//! CalDAV's events: AzCalendar's files, read and written with AzCalendar's own code
//! (azul-calendar-core) - one `events/<id>.json` per event and one `calendars/<id>.json` per
//! calendar in AzCalendar's data folder.
//!
//! - Each calendar is a collection, `/calendars/<id>/` (the default calendar, whose id is empty,
//!   is `/calendars/default/`); an event is in the collection of its calendar, an event whose
//!   calendar is gone in the default one (as AzCalendar shows it).
//! - GET is AzCalendar's export of the one event (`ics::write`: floating times, the `UID`, the
//!   repeat rule, the exceptions, the attendees, the reminder, an AzMeet link), stamped with the
//!   file's time, without the export's `METHOD` (RFC 4791 4.1).
//! - PUT is AzCalendar's import (`ics::parse`, times in this computer's zone): the resource's
//!   event becomes the file, keeping what iCalendar does not carry (an AzMeet meeting's code,
//!   times and state when its link is the same). An occurrence the program moved
//!   (`RECURRENCE-ID`) becomes an event of its own, as AzCalendar's File > Open makes it, with an
//!   id made from the event's, so sending the event again replaces it; deleting the event deletes
//!   them too.

use std::collections::{HashMap, HashSet};

use azcal_core::{
    calendars::{self, Calendar},
    event::{self, Event},
    ics::{self, Imported},
};
use azul_storage::{ops, DriveError, ObjectInfo};
use chrono::{Duration, NaiveDateTime};

use super::{dav_error, precondition, version_of, Item, Kind, Pim, DEFAULT_SEGMENT};
use crate::{
    dates, digest,
    http::{Head, Response, Status},
};

/// What an event is served as.
pub const CONTENT_TYPE: &str = "text/calendar; charset=utf-8; component=vevent";

/// A calendar's URL segment: [`DEFAULT_SEGMENT`] for the default calendar (its id is empty),
/// else its id.
#[must_use]
pub fn segment_of(id: &str) -> &str {
    if id.is_empty() {
        DEFAULT_SEGMENT
    } else {
        id
    }
}

/// The calendar id a URL segment can name (the default calendar's is empty).
#[must_use]
pub fn calendar_id_of(segment: &str) -> Option<&str> {
    if segment == DEFAULT_SEGMENT {
        Some("")
    } else {
        event::is_event_id(segment).then_some(segment)
    }
}

/// The segment of the calendar `event` is in.
fn segment_in(calendars: &[Calendar], event: &Event) -> String {
    calendars::calendar_of(calendars, &event.calendar)
        .map_or(DEFAULT_SEGMENT, |calendar| segment_of(&calendar.id))
        .to_string()
}

/// A calendar's colour as Apple's programs write it (`#RRGGBBAA`): its swatch.
#[must_use]
pub fn colour_of(calendar: &Calendar) -> String {
    format!("{}FF", calendar.colour.paint().light_edge.to_ascii_uppercase())
}

/// One event as iCalendar: AzCalendar's export of it, stamped with its file's time, without the
/// export's `METHOD` (a calendar's resource has none).
#[must_use]
pub fn ics_of(event: &Event, calendar_name: &str, info: &ObjectInfo) -> String {
    let secs = i64::try_from(info.modified.unwrap_or(0)).unwrap_or(0);
    let stamp = chrono::DateTime::from_timestamp(secs, 0)
        .map(|time| time.naive_utc())
        .unwrap_or_default();
    ics::write(&[event], calendar_name, stamp).replace("METHOD:PUBLISH\r\n", "")
}

/// Whether `event` may fall into the time range `from` to `to` (UTC, either end open). An
/// event's times are wall-clock times of no zone, so a day more on each side; a repeating event
/// always may (the program expands it).
#[must_use]
pub fn may_overlap(event: &Event, from: Option<NaiveDateTime>, to: Option<NaiveDateTime>) -> bool {
    if event.repeat.is_some() {
        return true;
    }
    let start = event.date.and_time(event.start);
    let end = if event.all_day {
        (event.last_day + Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap_or(start)
    } else {
        event.last_day.and_time(event.end)
    };
    let margin = Duration::days(1);
    from.is_none_or(|from| end + margin > from) && to.is_none_or(|to| start - margin < to)
}

/// The id of an occurrence the program moved (its uid `<uid>#<day>`) of the event `id`: the same
/// every time the event is sent again, shaped as AzCalendar's ids are (a version-4 UUID in
/// lower case).
#[must_use]
pub fn occurrence_id(id: &str, uid: &str) -> String {
    let hex = digest::md5_hex(&format!("{id}/{uid}"));
    let variant = ["8", "9", "a", "b"][usize::from(hex.as_bytes()[16]) % 4];
    format!(
        "{}-{}-4{}-{variant}{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        &hex[17..20],
        &hex[20..32]
    )
}

/// An event file and the name its href has.
#[derive(Debug, Clone)]
pub(crate) struct EventFile {
    pub name: String,
    pub info: ObjectInfo,
    pub event: Event,
}

impl Pim {
    fn lock_events(&self) -> std::sync::MutexGuard<'_, HashMap<String, (String, Event)>> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Every calendar, the default one first.
    pub(crate) fn calendars(&self) -> Vec<Calendar> {
        calendars::load(&*self.calendar)
    }

    /// The event of the file `info` (named for `id`), read through the cache: a file is read
    /// again only when its version changed. `None` for a file AzCalendar cannot read either (it
    /// reports those).
    fn read_event(&self, info: &ObjectInfo, id: &str) -> Option<Event> {
        let version = version_of(info);
        if let Some((cached, event)) = self.lock_events().get(&info.key) {
            if *cached == version {
                return Some(event.clone());
            }
        }
        let event = self
            .calendar
            .get(&info.key)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|text| event::from_json(&text).ok())
            .filter(|event| event.id == id)?;
        self.lock_events()
            .insert(info.key.clone(), (version, event.clone()));
        Some(event)
    }

    /// Every event file, under the names the programs gave them (else their ids).
    pub(crate) fn event_files(&self) -> Result<Vec<EventFile>, DriveError> {
        let prefix = format!("{}/", event::EVENTS_DIR);
        let mut out = Vec::new();
        let mut ids = HashSet::new();
        let mut keys = HashSet::new();
        for info in ops::list_all(&*self.calendar, &prefix)? {
            let Some(id) = info
                .key
                .strip_prefix(prefix.as_str())
                .and_then(event::id_of_file_name)
                .map(str::to_string)
            else {
                continue;
            };
            let Some(event) = self.read_event(&info, &id) else {
                continue;
            };
            keys.insert(info.key.clone());
            let name = self
                .names
                .name_of(Kind::Event, &id)
                .unwrap_or_else(|| id.clone());
            ids.insert(id);
            out.push(EventFile { name, info, event });
        }
        self.lock_events().retain(|key, _| keys.contains(key));
        self.names.keep_only(Kind::Event, &ids);
        Ok(out)
    }

    /// Every calendar as a collection, with its CTag: a hash of its name, its colour, and its
    /// events' names and versions.
    pub(crate) fn calendar_items(&self) -> Result<Vec<Item>, DriveError> {
        let calendars = self.calendars();
        let files = self.event_files()?;
        let placed: Vec<String> = files
            .iter()
            .map(|file| segment_in(&calendars, &file.event))
            .collect();
        let mut items = Vec::new();
        for calendar in &calendars {
            let segment = segment_of(&calendar.id).to_string();
            let mut text = format!("{}\n{}\n", calendar.name, calendar.colour.name());
            for (file, place) in files.iter().zip(&placed) {
                if *place == segment {
                    text.push_str(&format!("{}\n{}\n", file.name, version_of(&file.info)));
                }
            }
            items.push(Item::Calendar {
                segment,
                calendar: calendar.clone(),
                ctag: super::ctag_of(&text),
            });
        }
        Ok(items)
    }

    /// The events of the calendar `segment`.
    pub(crate) fn event_items(&self, segment: &str) -> Result<Vec<Item>, DriveError> {
        let calendars = self.calendars();
        let Some(calendar_name) = calendar_id_of(segment)
            .and_then(|id| calendars.iter().find(|calendar| calendar.id == id))
            .map(|calendar| calendar.name.clone())
        else {
            return Ok(Vec::new());
        };
        Ok(self
            .event_files()?
            .into_iter()
            .filter(|file| segment_in(&calendars, &file.event) == segment)
            .map(|file| Item::Event {
                segment: segment.to_string(),
                calendar_name: calendar_name.clone(),
                name: file.name,
                info: file.info,
                event: file.event,
            })
            .collect())
    }

    /// The event a program's `name` stands for: the id kept for it, else the name itself when it
    /// is an event id.
    fn event_id(&self, name: &str) -> Option<String> {
        self.names
            .id_of(Kind::Event, name)
            .or_else(|| event::is_event_id(name).then(|| name.to_string()))
    }

    /// The event `name` of the calendar `segment`, if it is there and in that calendar.
    pub(crate) fn event_item(&self, segment: &str, name: &str) -> Result<Option<Item>, DriveError> {
        let Some(id) = self.event_id(name) else {
            return Ok(None);
        };
        let info = match self.calendar.head(&event::object_key(&id)) {
            Ok(info) => info,
            Err(DriveError::NotFound { .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        let Some(event) = self.read_event(&info, &id) else {
            return Ok(None);
        };
        let calendars = self.calendars();
        if segment_in(&calendars, &event) != segment {
            return Ok(None);
        }
        let calendar_name = calendars::calendar_of(&calendars, &event.calendar)
            .map(|calendar| calendar.name.clone())
            .unwrap_or_default();
        Ok(Some(Item::Event {
            segment: segment.to_string(),
            calendar_name,
            name: name.to_string(),
            info,
            event,
        }))
    }

    pub(crate) fn get_event(&self, segment: &str, name: &str) -> Result<Response, DriveError> {
        let Some(Item::Event {
            calendar_name,
            info,
            event,
            ..
        }) = self.event_item(segment, name)?
        else {
            return Ok(Response::text(Status::NOT_FOUND, "Not there."));
        };
        let mut response = Response::new(Status::OK)
            .with_header("ETag", format!("\"{}\"", version_of(&info)))
            .with_body(CONTENT_TYPE, ics_of(&event, &calendar_name, &info).into_bytes());
        if let Some(modified) = info.modified {
            response = response.with_header("Last-Modified", dates::http_date(i64::try_from(modified).unwrap_or(0)));
        }
        Ok(response)
    }

    pub(crate) fn put_event(&self, head: &Head, segment: &str, name: &str, body: &[u8]) -> Result<Response, DriveError> {
        let calendars = self.calendars();
        let Some(calendar_id) = calendar_id_of(segment)
            .filter(|id| calendars.iter().any(|calendar| calendar.id == *id))
        else {
            return Ok(Response::text(Status::CONFLICT, "No calendar has this path."));
        };
        let parsed = std::str::from_utf8(body)
            .map_err(|e| e.to_string())
            .and_then(|text| ics::parse(text, &chrono::Local));
        let Ok(parsed) = parsed else {
            return Ok(dav_error(Status::FORBIDDEN, "<C:valid-calendar-data/>"));
        };
        // The resource's event first, then the occurrences it moved (`<uid>#<day>`).
        let (moved, masters): (Vec<Imported>, Vec<Imported>) =
            parsed.events.into_iter().partition(|imported| imported.uid.contains('#'));
        let mut all = masters.into_iter().chain(moved);
        let Some(master) = all.next() else {
            return Ok(dav_error(Status::FORBIDDEN, "<C:supported-calendar-component/>"));
        };
        let moved: Vec<Imported> = all.collect();
        let id = self.event_id(name).unwrap_or_else(event::new_event_id);
        let key = event::object_key(&id);
        let current = match self.calendar.head(&key) {
            Ok(info) => Some(info),
            Err(DriveError::NotFound { .. }) => None,
            Err(e) => return Err(e),
        };
        if let Some(refusal) = precondition(head, current.as_ref().map(version_of).as_deref()) {
            return Ok(refusal);
        }
        let Ok(mut new) = master.to_event(&id, calendar_id) else {
            return Ok(dav_error(Status::FORBIDDEN, "<C:valid-calendar-data/>"));
        };
        // What iCalendar does not carry stays: the meeting's code, times and state, same link.
        if let Some(old) = current.as_ref().and_then(|info| self.read_event(info, &id)) {
            if let (Some(kept), Some(meeting)) = (old.meeting, new.meeting.as_mut()) {
                if kept.link == meeting.link {
                    *meeting = kept;
                }
            }
        }
        self.calendar.put(&key, event::to_json(&new).as_bytes())?;
        for occurrence in &moved {
            let moved_id = occurrence_id(&id, &occurrence.uid);
            if let Ok(moved_event) = occurrence.to_event(&moved_id, calendar_id) {
                self.calendar
                    .put(&event::object_key(&moved_id), event::to_json(&moved_event).as_bytes())?;
            }
        }
        if id != name {
            self.names.set(Kind::Event, name, &id);
        }
        // No ETag: the file is AzCalendar's, not the program's text - it fetches what it became.
        Ok(Response::new(if current.is_some() { Status::NO_CONTENT } else { Status::CREATED }))
    }

    pub(crate) fn delete_event(&self, head: &Head, segment: &str, name: &str) -> Result<Response, DriveError> {
        let Some(Item::Event { info, event, .. }) = self.event_item(segment, name)? else {
            return Ok(Response::text(Status::NOT_FOUND, "Not there."));
        };
        if let Some(refusal) = precondition(head, Some(version_of(&info).as_str())) {
            return Ok(refusal);
        }
        self.calendar.delete(&info.key)?;
        self.names.forget(Kind::Event, name);
        // The occurrences it moved go with it.
        let moved = format!("{}#", event.ical_uid());
        for file in self.event_files()? {
            if file.event.uid.starts_with(moved.as_str()) {
                self.calendar.delete(&file.info.key)?;
            }
        }
        Ok(Response::new(Status::NO_CONTENT))
    }
}
