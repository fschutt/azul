//! iCalendar files (RFC 5545, `.ics`): reading the events of one into AzCalendar's events, and
//! writing AzCalendar's events as one, for File > Open & Export.
//!
//! Reading:
//! - Lines are unfolded first (a line break followed by a space or a tab continues the line) and
//!   text values unescaped (`\,` `\;` `\\` `\n`).
//! - Every `VEVENT` becomes an [`Imported`] event: `SUMMARY`, `LOCATION`, `DESCRIPTION`,
//!   `DTSTART` / `DTEND` (or `DURATION`), `RRULE` (`rrule.rs`'s subset), `EXDATE`, `ATTENDEE`,
//!   the first `VALARM`'s `TRIGGER` (the reminder), `UID`, and AzCalendar's own
//!   `X-AZCAL-MEETING` / `X-AZCAL-MEETING-SERVER` (an AzMeet link).
//! - Times: a date (`VALUE=DATE`) makes an all-day event (`DTEND` is the day after the last).
//!   A UTC time (`...Z`) and a time with a `TZID` are read as the moment they are and shown in
//!   the reader's zone. The `TZID`'s rules come from the file's own `VTIMEZONE` (its
//!   `STANDARD` / `DAYLIGHT` observances, their onsets made by their `RRULE`s), as RFC 5545
//!   wants. A time without either is "floating": the wall-clock time as written.
//! - An event's occurrence that was moved or cancelled (`RECURRENCE-ID`) becomes an exception
//!   of the repeating event, and, moved, an event of its own.
//! - What could not be kept as it was is said in [`IcsCalendar::notes`]: an unsupported repeat
//!   rule (its first date is kept), an unknown time zone (the time is kept as written), an event
//!   with times that runs past midnight (it ends at 23:59).
//!
//! Writing is the same subset, with times "floating" (AzCalendar's events are wall-clock times),
//! every line folded at 75 octets and ended with CRLF.

use std::collections::BTreeMap;

// The content lines (folding, escaping, parameters) are the format vCard has too:
// azul_pim::content_line (DEDUP_EDITORS B15).
pub use azul_pim::content_line::{
    escape_text, fold, parse_line, unescape_text, unfold, ContentLine,
};
use azul_pim::mail_address::{is_email, parse_mailbox};
use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike};

use crate::{
    event::{Event, EventError, Meeting},
    meet_rooms::{self, RoomKey},
    rrule::Rule,
};

/// What an imported event without a `SUMMARY` is called (as a draft without a title is).
pub const NO_TITLE: &str = "(No title)";
/// AzCalendar's own properties: an event's AzMeet link and the meeting server that has its room.
const MEETING_PROP: &str = "X-AZCAL-MEETING";
const MEETING_SERVER_PROP: &str = "X-AZCAL-MEETING-SERVER";

/// A moment as an .ics file writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum IcsTime {
    Date(NaiveDate),
    Floating(NaiveDateTime),
    Utc(NaiveDateTime),
    Zoned(NaiveDateTime, String),
}

/// `20260930T090000` (seconds may be left out by sloppy writers).
fn parse_date_time(text: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text, "%Y%m%dT%H%M%S")
        .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y%m%dT%H%M"))
        .ok()
}

/// One DATE or DATE-TIME value of a line with `params`.
fn parse_time_value(value: &str, value_date: bool, tzid: Option<&str>) -> Option<IcsTime> {
    let value = value.trim();
    if value_date || (value.len() == 8 && value.bytes().all(|b| b.is_ascii_digit())) {
        return NaiveDate::parse_from_str(value.get(..8)?, "%Y%m%d")
            .ok()
            .map(IcsTime::Date);
    }
    if let Some(utc) = value.strip_suffix('Z').or_else(|| value.strip_suffix('z')) {
        return parse_date_time(utc).map(IcsTime::Utc);
    }
    let local = parse_date_time(value)?;
    Some(match tzid {
        Some(zone) => IcsTime::Zoned(local, zone.to_string()),
        None => IcsTime::Floating(local),
    })
}

/// The DATE / DATE-TIME values of `line` (a comma list for EXDATE).
fn parse_times(line: &ContentLine) -> Vec<IcsTime> {
    let value_date = line
        .param_value("VALUE")
        .is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
    let tzid = line.param_value("TZID");
    line.value
        .split(',')
        .filter_map(|v| parse_time_value(v, value_date, tzid))
        .collect()
}

/// `+0200` / `-0530` / `+023000` as seconds east of UTC.
fn parse_offset(text: &str) -> Option<i32> {
    let text = text.trim();
    let (sign, digits) = match text.as_bytes().first()? {
        b'+' => (1, &text[1..]),
        b'-' => (-1, &text[1..]),
        _ => (1, text),
    };
    if !(digits.len() == 4 || digits.len() == 6) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i32 = digits[..2].parse().ok()?;
    let minutes: i32 = digits[2..4].parse().ok()?;
    let seconds: i32 = if digits.len() == 6 {
        digits[4..6].parse().ok()?
    } else {
        0
    };
    Some(sign * (hours * 3600 + minutes * 60 + seconds))
}

/// A duration as iCalendar writes it (`PT15M`, `-P1D`, `P1W`, `-PT1H30M`), in seconds.
#[must_use]
pub fn parse_duration(text: &str) -> Option<i64> {
    let text = text.trim();
    let (sign, rest) = match text.as_bytes().first()? {
        b'-' => (-1, &text[1..]),
        b'+' => (1, &text[1..]),
        _ => (1, text),
    };
    let rest = rest.strip_prefix('P').or_else(|| rest.strip_prefix('p'))?;
    let mut total: i64 = 0;
    let mut number = String::new();
    let mut in_time = false;
    let mut any = false;
    for c in rest.chars() {
        match c.to_ascii_uppercase() {
            'T' => in_time = true,
            d if d.is_ascii_digit() => number.push(d),
            unit => {
                let n: i64 = number.parse().ok()?;
                number.clear();
                any = true;
                total += n * match (unit, in_time) {
                    ('W', false) => 7 * 86_400,
                    ('D', false) => 86_400,
                    ('H', true) => 3_600,
                    ('M', true) => 60,
                    ('S', true) => 1,
                    _ => return None,
                };
            }
        }
    }
    (any && number.is_empty()).then_some(sign * total)
}

/// One observance of a `VTIMEZONE` (`STANDARD` or `DAYLIGHT`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Observance {
    /// The first onset, in the local time it replaces.
    start: NaiveDateTime,
    offset_from: i32,
    offset_to: i32,
    rule: Option<Rule>,
    rdates: Vec<NaiveDateTime>,
}

impl Observance {
    /// The latest onset at or before the local time `at`.
    fn last_onset(&self, at: NaiveDateTime) -> Option<NaiveDateTime> {
        if self.start > at {
            return None;
        }
        let time = self.start.time();
        let mut best = Some(self.start);
        if let Some(rule) = &self.rule {
            if let Some(day) = rule
                .dates(self.start.date(), &[], self.start.date(), at.date())
                .into_iter()
                .map(|d| d.and_time(time))
                .filter(|onset| *onset <= at)
                .last()
            {
                best = best.max(Some(day));
            }
        }
        for rdate in &self.rdates {
            if *rdate <= at {
                best = best.max(Some(*rdate));
            }
        }
        best
    }
}

/// A `VTIMEZONE`'s rules: which UTC offset is in force at a local time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZoneRules {
    observances: Vec<Observance>,
}

impl ZoneRules {
    /// The UTC offset (seconds east) in force at the local time `at`: the observance with the
    /// latest onset at or before it; before every onset, the earliest one's offset before it.
    fn offset_at(&self, at: NaiveDateTime) -> Option<i32> {
        let latest = self
            .observances
            .iter()
            .filter_map(|o| o.last_onset(at).map(|onset| (onset, o.offset_to)))
            .max_by_key(|(onset, _)| *onset);
        match latest {
            Some((_, offset)) => Some(offset),
            None => self
                .observances
                .iter()
                .min_by_key(|o| o.start)
                .map(|o| o.offset_from),
        }
    }
}

/// An event read from an .ics file, before it is an AzCalendar event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    pub uid: String,
    pub title: String,
    pub location: String,
    pub notes: String,
    pub date: NaiveDate,
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub all_day: bool,
    pub last_day: NaiveDate,
    pub repeat: Option<Rule>,
    pub except: Vec<NaiveDate>,
    pub attendees: Vec<String>,
    pub reminder: Option<u32>,
    pub meeting: Option<Meeting>,
}

impl Imported {
    /// The AzCalendar event: id `id`, in the calendar `calendar`.
    pub fn to_event(&self, id: &str, calendar: &str) -> Result<Event, EventError> {
        Event {
            id: id.to_string(),
            title: self.title.clone(),
            date: self.date,
            start: self.start,
            end: self.end,
            meeting: self.meeting.clone(),
            all_day: self.all_day,
            last_day: self.last_day,
            location: self.location.clone(),
            notes: self.notes.clone(),
            attendees: self.attendees.clone(),
            reminder: self.reminder,
            calendar: calendar.to_string(),
            repeat: self.repeat.clone(),
            except: self.except.clone(),
            uid: self.uid.clone(),
        }
        .check()
    }
}

/// What an .ics file holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IcsCalendar {
    /// The calendar's name (`X-WR-CALNAME`).
    pub name: Option<String>,
    pub events: Vec<Imported>,
    /// What could not be kept as it was, for the user, one sentence each.
    pub notes: Vec<String>,
}

/// A component's lines, as read.
#[derive(Debug, Default)]
struct Component {
    name: String,
    lines: Vec<ContentLine>,
    children: Vec<Component>,
}

impl Component {
    fn first(&self, name: &str) -> Option<&ContentLine> {
        self.lines.iter().find(|l| l.name == name)
    }

    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a ContentLine> + 'a {
        self.lines.iter().filter(move |l| l.name == name)
    }
}

/// The components of the unfolded `lines`, nested by BEGIN / END.
fn components(lines: &[String]) -> Vec<Component> {
    let mut stack: Vec<Component> = vec![Component::default()];
    for raw in lines {
        let Ok(line) = parse_line(raw) else {
            continue;
        };
        match line.name.as_str() {
            "BEGIN" => stack.push(Component {
                name: line.value.trim().to_ascii_uppercase(),
                ..Component::default()
            }),
            "END" if stack.len() > 1 => {
                if let Some(done) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(done);
                    }
                }
            }
            _ => {
                if let Some(top) = stack.last_mut() {
                    top.lines.push(line);
                }
            }
        }
    }
    // An unclosed component still counts.
    while stack.len() > 1 {
        if let Some(done) = stack.pop() {
            if let Some(parent) = stack.last_mut() {
                parent.children.push(done);
            }
        }
    }
    stack.pop().map(|root| root.children).unwrap_or_default()
}

/// A `VTIMEZONE`'s rules.
fn zone_rules(zone: &Component) -> ZoneRules {
    let observances = zone
        .children
        .iter()
        .filter(|c| c.name == "STANDARD" || c.name == "DAYLIGHT")
        .filter_map(|c| {
            let start = match parse_times(c.first("DTSTART")?).into_iter().next()? {
                IcsTime::Floating(t) | IcsTime::Utc(t) | IcsTime::Zoned(t, _) => t,
                IcsTime::Date(d) => d.and_time(NaiveTime::MIN),
            };
            let offset_from = parse_offset(&c.first("TZOFFSETFROM")?.value)?;
            let offset_to = parse_offset(&c.first("TZOFFSETTO")?.value)?;
            let rule = c.first("RRULE").and_then(|l| Rule::parse(&l.value).ok());
            let rdates = c
                .all("RDATE")
                .flat_map(parse_times)
                .filter_map(|t| match t {
                    IcsTime::Floating(t) | IcsTime::Utc(t) | IcsTime::Zoned(t, _) => Some(t),
                    IcsTime::Date(d) => Some(d.and_time(NaiveTime::MIN)),
                })
                .collect();
            Some(Observance {
                start,
                offset_from,
                offset_to,
                rule,
                rdates,
            })
        })
        .collect();
    ZoneRules { observances }
}

/// The zones every reader knows without a `VTIMEZONE`.
fn is_utc_name(tzid: &str) -> bool {
    matches!(
        tzid.trim().to_ascii_uppercase().as_str(),
        "UTC" | "Z" | "GMT" | "ETC/UTC" | "ETC/GMT" | "ZULU"
    )
}

/// Reads times into the reader's zone.
struct Clock<'a, Tz: TimeZone> {
    zone: &'a Tz,
    zones: &'a BTreeMap<String, ZoneRules>,
}

/// A time read into the reader's zone: a day (all day) or a local date and time; and the
/// unknown time zone, when the time was kept as written for want of its rules.
enum Read {
    Day(NaiveDate),
    At(NaiveDateTime),
}

impl<Tz: TimeZone> Clock<'_, Tz> {
    fn local_of_utc(&self, utc: NaiveDateTime) -> NaiveDateTime {
        self.zone.from_utc_datetime(&utc).naive_local()
    }

    /// `time` in the reader's zone; `Err(tzid)` alongside the time as written for a zone whose
    /// rules are unknown.
    fn read(&self, time: &IcsTime) -> (Read, Option<String>) {
        match time {
            IcsTime::Date(d) => (Read::Day(*d), None),
            IcsTime::Floating(t) => (Read::At(*t), None),
            IcsTime::Utc(t) => (Read::At(self.local_of_utc(*t)), None),
            IcsTime::Zoned(t, tzid) => {
                if is_utc_name(tzid) {
                    return (Read::At(self.local_of_utc(*t)), None);
                }
                match self.zones.get(tzid).and_then(|z| z.offset_at(*t)) {
                    Some(offset) => (
                        Read::At(self.local_of_utc(*t - Duration::seconds(i64::from(offset)))),
                        None,
                    ),
                    None => (Read::At(*t), Some(tzid.clone())),
                }
            }
        }
    }
}

/// The last minute of a day: where an event that runs past midnight ends here.
fn last_minute() -> NaiveTime {
    NaiveTime::from_hms_opt(23, 59, 0).unwrap_or(NaiveTime::MIN)
}

/// The AzMeet meeting an event's link line names, if it names a room by its id.
fn meeting_of(event: &Component) -> Option<Meeting> {
    let link = event
        .first(MEETING_PROP)
        .or_else(|| event.first("URL"))?
        .value
        .trim()
        .to_string();
    if !matches!(meet_rooms::parse_room_link(&link), Some(RoomKey::Id(_))) {
        return None;
    }
    let server = event
        .first(MEETING_SERVER_PROP)
        .map(|l| unescape_text(l.value.trim()))
        .unwrap_or_default();
    Some(Meeting {
        link,
        server,
        code: String::new(),
        expires: String::new(),
        starts_at: String::new(),
        ends_at: String::new(),
        // Who made it registered it, or will: this copy does not register it again.
        pending: false,
    })
}

/// The reminder of the event's first alarm that is relative to its start, in minutes before.
fn reminder_of(event: &Component) -> Option<u32> {
    event
        .children
        .iter()
        .filter(|c| c.name == "VALARM")
        .find_map(|alarm| {
            let trigger = alarm.first("TRIGGER")?;
            if trigger
                .param_value("RELATED")
                .is_some_and(|r| r.eq_ignore_ascii_case("END"))
                || trigger
                    .param_value("VALUE")
                    .is_some_and(|v| v.eq_ignore_ascii_case("DATE-TIME"))
            {
                return None;
            }
            let seconds = parse_duration(&trigger.value)?;
            (seconds <= 0).then(|| u32::try_from(-seconds / 60).unwrap_or(u32::MAX))
        })
}

/// The day of a RECURRENCE-ID / EXDATE time in the reader's zone.
fn day_of(read: Read) -> NaiveDate {
    match read {
        Read::Day(d) => d,
        Read::At(t) => t.date(),
    }
}

/// One VEVENT, or why it is left out; notes go to `notes`.
fn imported<Tz: TimeZone>(
    event: &Component,
    clock: &Clock<'_, Tz>,
    notes: &mut Vec<String>,
) -> Option<(Imported, Option<NaiveDate>, bool)> {
    let title = event
        .first("SUMMARY")
        .map(|l| unescape_text(&l.value).trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| String::from(NO_TITLE));
    let note_zone = |tzid: Option<String>, notes: &mut Vec<String>| {
        if let Some(tzid) = tzid {
            let note = format!(
                "The time zone {tzid:?} has no rules in the file: its times are kept as written."
            );
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
    };
    let Some(start) = event
        .first("DTSTART")
        .and_then(|l| parse_times(l).into_iter().next())
    else {
        notes.push(format!("{title:?} has no start: it is left out."));
        return None;
    };
    let (start, zone) = clock.read(&start);
    note_zone(zone, notes);
    let end = event
        .first("DTEND")
        .and_then(|l| parse_times(l).into_iter().next())
        .map(|t| {
            let (read, zone) = clock.read(&t);
            note_zone(zone, notes);
            read
        });
    let duration = event
        .first("DURATION")
        .and_then(|l| parse_duration(&l.value));
    let (date, start_time, end_time, all_day, last_day) = match start {
        Read::Day(first) => {
            let after = match end {
                Some(Read::Day(d)) => d,
                Some(Read::At(t)) => t.date() + Duration::days(1),
                None => first + Duration::days(duration.map_or(1, |s| (s / 86_400).max(1))),
            };
            let last = (after - Duration::days(1)).max(first);
            (first, NaiveTime::MIN, last_minute(), true, last)
        }
        Read::At(at) => {
            let end_at = match end {
                Some(Read::At(t)) => t,
                Some(Read::Day(d)) => d.and_time(NaiveTime::MIN),
                None => at + Duration::seconds(duration.unwrap_or(3_600)),
            };
            let start_time = NaiveTime::from_hms_opt(at.hour(), at.minute(), 0)
                .unwrap_or(NaiveTime::MIN)
                .min(NaiveTime::from_hms_opt(23, 58, 0).unwrap_or(NaiveTime::MIN));
            let mut end_time = if end_at.date() > at.date() {
                notes.push(format!(
                    "{title:?} runs past midnight: here it ends at 23:59 on its first day."
                ));
                last_minute()
            } else {
                NaiveTime::from_hms_opt(end_at.hour(), end_at.minute(), 0).unwrap_or(last_minute())
            };
            if end_time <= start_time {
                end_time = (start_time + Duration::hours(1)).min(last_minute());
                if end_time <= start_time {
                    end_time = last_minute();
                }
            }
            (at.date(), start_time, end_time, false, at.date())
        }
    };
    let repeat = match event.first("RRULE") {
        Some(line) => match Rule::parse(&line.value) {
            Ok(rule) => Some(rule),
            Err(e) => {
                notes.push(format!("{title:?}: {e}; only its first date is imported."));
                None
            }
        },
        None => None,
    };
    let mut except: Vec<NaiveDate> = event
        .all("EXDATE")
        .flat_map(parse_times)
        .map(|t| {
            let (read, zone) = clock.read(&t);
            note_zone(zone, notes);
            day_of(read)
        })
        .collect();
    except.sort();
    except.dedup();
    let attendees = event
        .all("ATTENDEE")
        .filter_map(|l| {
            // `mailto:ana@example.com`: the address after its scheme.
            let address = parse_mailbox(&l.value).address;
            is_email(&address).then_some(address)
        })
        .fold(Vec::<String>::new(), |mut all, a| {
            if !all.iter().any(|x| x.eq_ignore_ascii_case(&a)) {
                all.push(a);
            }
            all
        });
    let recurrence = event
        .first("RECURRENCE-ID")
        .and_then(|l| parse_times(l).into_iter().next())
        .map(|t| day_of(clock.read(&t).0));
    let cancelled = event
        .first("STATUS")
        .is_some_and(|s| s.value.trim().eq_ignore_ascii_case("CANCELLED"));
    let event = Imported {
        uid: event
            .first("UID")
            .map(|l| unescape_text(l.value.trim()))
            .unwrap_or_default(),
        title,
        location: event
            .first("LOCATION")
            .map(|l| unescape_text(&l.value).trim().to_string())
            .unwrap_or_default(),
        notes: event
            .first("DESCRIPTION")
            .map(|l| unescape_text(&l.value).trim_end().to_string())
            .unwrap_or_default(),
        date,
        start: start_time,
        end: end_time,
        all_day,
        last_day,
        repeat,
        except,
        attendees,
        reminder: reminder_of(event),
        meeting: meeting_of(event),
    };
    Some((event, recurrence, cancelled))
}

/// Reads an .ics file's events, their times in `zone` (the reader's: `chrono::Local` in the
/// app). `Err` when the text is no iCalendar file.
pub fn parse<Tz: TimeZone>(text: &str, zone: &Tz) -> Result<IcsCalendar, String> {
    let lines = unfold(text.strip_prefix('\u{feff}').unwrap_or(text));
    let roots = components(&lines);
    let calendars: Vec<&Component> = roots.iter().filter(|c| c.name == "VCALENDAR").collect();
    if calendars.is_empty() {
        return Err(String::from(
            "this is not an iCalendar file (no BEGIN:VCALENDAR)",
        ));
    }
    let mut out = IcsCalendar::default();
    let mut zones: BTreeMap<String, ZoneRules> = BTreeMap::new();
    for cal in &calendars {
        for zone in cal.children.iter().filter(|c| c.name == "VTIMEZONE") {
            if let Some(tzid) = zone.first("TZID") {
                zones.insert(tzid.value.trim().to_string(), zone_rules(zone));
            }
        }
        if out.name.is_none() {
            out.name = cal
                .first("X-WR-CALNAME")
                .map(|l| unescape_text(&l.value).trim().to_string())
                .filter(|n| !n.is_empty());
        }
    }
    let clock = Clock {
        zone,
        zones: &zones,
    };
    let mut masters: Vec<Imported> = Vec::new();
    let mut overrides: Vec<(Imported, NaiveDate, bool)> = Vec::new();
    let mut other = 0usize;
    for cal in &calendars {
        for child in &cal.children {
            match child.name.as_str() {
                "VEVENT" => {
                    let Some((event, recurrence, cancelled)) =
                        imported(child, &clock, &mut out.notes)
                    else {
                        continue;
                    };
                    match recurrence {
                        Some(day) => overrides.push((event, day, cancelled)),
                        None if cancelled => out
                            .notes
                            .push(format!("{:?} is cancelled: it is left out.", event.title)),
                        None => masters.push(event),
                    }
                }
                "VTIMEZONE" => {}
                _ => other += 1,
            }
        }
    }
    for (mut moved, day, cancelled) in overrides {
        if let Some(master) = masters
            .iter_mut()
            .find(|m| !moved.uid.is_empty() && m.uid == moved.uid)
        {
            if !master.except.contains(&day) {
                master.except.push(day);
                master.except.sort();
            }
        }
        if !cancelled {
            moved.uid = format!("{}#{}", moved.uid, day.format("%Y%m%d"));
            moved.repeat = None;
            moved.except.clear();
            masters.push(moved);
        }
    }
    if other > 0 {
        out.notes.push(format!(
            "{other} item(s) that are no events (tasks, journal entries) are left out."
        ));
    }
    out.events = masters;
    Ok(out)
}

/// `YYYYMMDD`
fn basic_date(d: NaiveDate) -> String {
    d.format("%Y%m%d").to_string()
}

/// `YYYYMMDDTHHMMSS`
fn basic_date_time(d: NaiveDate, t: NaiveTime) -> String {
    format!("{}T{}", basic_date(d), t.format("%H%M%S"))
}

/// `-PT15M`, `-PT1H`, `-P1D`, `PT0S`: a reminder's trigger.
fn trigger_of(minutes: u32) -> String {
    match minutes {
        0 => String::from("PT0S"),
        m if m % 1440 == 0 => format!("-P{}D", m / 1440),
        m if m % 60 == 0 => format!("-PT{}H", m / 60),
        m => format!("-PT{m}M"),
    }
}

/// The events as an .ics file named `name`, stamped `stamp` (UTC, the export's time): every
/// line folded at 75 octets and ended with CRLF.
#[must_use]
pub fn write(events: &[&Event], name: &str, stamp: NaiveDateTime) -> String {
    let mut lines: Vec<String> = vec![
        String::from("BEGIN:VCALENDAR"),
        String::from("VERSION:2.0"),
        String::from("PRODID:-//azul//AzCalendar//EN"),
        String::from("CALSCALE:GREGORIAN"),
        String::from("METHOD:PUBLISH"),
        format!("X-WR-CALNAME:{}", escape_text(name)),
    ];
    let stamp = format!("{}Z", basic_date_time(stamp.date(), stamp.time()));
    for e in events {
        lines.push(String::from("BEGIN:VEVENT"));
        lines.push(format!("UID:{}", escape_text(&e.ical_uid())));
        lines.push(format!("DTSTAMP:{stamp}"));
        if e.all_day {
            lines.push(format!("DTSTART;VALUE=DATE:{}", basic_date(e.date)));
            lines.push(format!(
                "DTEND;VALUE=DATE:{}",
                basic_date(e.last_day + Duration::days(1))
            ));
        } else {
            lines.push(format!("DTSTART:{}", basic_date_time(e.date, e.start)));
            lines.push(format!("DTEND:{}", basic_date_time(e.date, e.end)));
        }
        lines.push(format!("SUMMARY:{}", escape_text(&e.title)));
        if !e.location.is_empty() {
            lines.push(format!("LOCATION:{}", escape_text(&e.location)));
        }
        if !e.notes.is_empty() {
            lines.push(format!("DESCRIPTION:{}", escape_text(&e.notes)));
        }
        if let Some(rule) = &e.repeat {
            lines.push(format!("RRULE:{}", rule.to_rrule(e.all_day)));
        }
        if !e.except.is_empty() {
            let days: Vec<String> = e
                .except
                .iter()
                .map(|d| {
                    if e.all_day {
                        basic_date(*d)
                    } else {
                        basic_date_time(*d, e.start)
                    }
                })
                .collect();
            let param = if e.all_day { ";VALUE=DATE" } else { "" };
            lines.push(format!("EXDATE{param}:{}", days.join(",")));
        }
        for who in &e.attendees {
            lines.push(format!("ATTENDEE;RSVP=TRUE:mailto:{who}"));
        }
        if let Some(m) = &e.meeting {
            lines.push(format!("{MEETING_PROP}:{}", m.link));
            lines.push(format!("{MEETING_SERVER_PROP}:{}", escape_text(&m.server)));
        }
        if let Some(minutes) = e.reminder {
            lines.push(String::from("BEGIN:VALARM"));
            lines.push(String::from("ACTION:DISPLAY"));
            lines.push(format!("DESCRIPTION:{}", escape_text(&e.title)));
            lines.push(format!("TRIGGER:{}", trigger_of(minutes)));
            lines.push(String::from("END:VALARM"));
        }
        lines.push(String::from("END:VEVENT"));
    }
    lines.push(String::from("END:VCALENDAR"));
    let mut out = String::new();
    for line in lines {
        out.push_str(&fold(&line));
        out.push_str("\r\n");
    }
    out
}

/// A file name for an export of the calendar `name`: its letters, digits and dashes, `.ics`.
#[must_use]
pub fn file_name_for(name: &str) -> String {
    let stem: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim_matches('-');
    if stem.is_empty() {
        String::from("calendar.ics")
    } else {
        format!("{stem}.ics")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rrule::Freq;
    use chrono::FixedOffset;

    const ID: &str = "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a";
    const ROOM: &str = "a2h859hyqkfaa11nhzxfh3gd7f";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    fn calendar(body: &str) -> String {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//test//EN\r\n{body}END:VCALENDAR\r\n")
    }

    const BERLIN: &str = "BEGIN:VTIMEZONE\r\nTZID:Europe/Berlin\r\nBEGIN:DAYLIGHT\r\n\
        TZOFFSETFROM:+0100\r\nTZOFFSETTO:+0200\r\nTZNAME:CEST\r\nDTSTART:19700329T020000\r\n\
        RRULE:FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU\r\nEND:DAYLIGHT\r\nBEGIN:STANDARD\r\n\
        TZOFFSETFROM:+0200\r\nTZOFFSETTO:+0100\r\nTZNAME:CET\r\nDTSTART:19701025T030000\r\n\
        RRULE:FREQ=YEARLY;BYMONTH=10;BYDAY=-1SU\r\nEND:STANDARD\r\nEND:VTIMEZONE\r\n";

    fn one(body: &str, zone: &FixedOffset) -> (Imported, Vec<String>) {
        let cal = parse(&calendar(body), zone).unwrap();
        assert_eq!(cal.events.len(), 1, "{:?}", cal.events);
        (cal.events[0].clone(), cal.notes)
    }

    #[test]
    fn durations_read_weeks_days_hours_minutes_and_seconds() {
        assert_eq!(parse_duration("-PT15M"), Some(-900));
        assert_eq!(parse_duration("PT1H30M"), Some(5400));
        assert_eq!(parse_duration("-P1D"), Some(-86_400));
        assert_eq!(parse_duration("P1W"), Some(604_800));
        assert_eq!(parse_duration("PT0S"), Some(0));
        assert_eq!(parse_duration("P"), None);
        assert_eq!(parse_duration("PT5"), None);
        assert_eq!(parse_duration("15M"), None);
    }

    #[test]
    fn a_tzid_time_is_read_through_the_files_vtimezone() {
        let body = |when: &str| {
            format!(
                "{BERLIN}BEGIN:VEVENT\r\nUID:x\r\nSUMMARY:Sync\r\nDTSTART;TZID=Europe/Berlin:\
                 {when}T090000\r\nDTEND;TZID=Europe/Berlin:{when}T100000\r\nEND:VEVENT\r\n"
            )
        };
        // summer: Berlin is UTC+2, so 09:00 there is 07:00 UTC
        let (summer, notes) = one(&body("20260715"), &utc());
        assert_eq!(
            (summer.date, summer.start, summer.end),
            (d(2026, 7, 15), at(7, 0), at(8, 0))
        );
        assert!(notes.is_empty(), "{notes:?}");
        // winter: UTC+1
        let (winter, _) = one(&body("20261215"), &utc());
        assert_eq!((winter.start, winter.end), (at(8, 0), at(9, 0)));
        // a reader in UTC+2 sees the summer meeting at 09:00
        let (there, _) = one(&body("20260715"), &FixedOffset::east_opt(7200).unwrap());
        assert_eq!(there.start, at(9, 0));
        // the change itself: 29 March 2026 is the last Sunday of March
        let (before, _) = one(&body("20260328"), &utc());
        assert_eq!(before.start, at(8, 0));
        let (after, _) = one(&body("20260330"), &utc());
        assert_eq!(after.start, at(7, 0));
    }

    #[test]
    fn a_utc_time_is_shown_in_the_readers_zone_and_a_floating_one_as_written() {
        let body = "BEGIN:VEVENT\r\nSUMMARY:Call\r\nDTSTART:20260930T070000Z\r\n\
                    DTEND:20260930T073000Z\r\nEND:VEVENT\r\n";
        let (call, _) = one(body, &FixedOffset::east_opt(7200).unwrap());
        assert_eq!((call.start, call.end), (at(9, 0), at(9, 30)));
        let floating = "BEGIN:VEVENT\r\nSUMMARY:Lunch\r\nDTSTART:20260930T123000\r\n\
                        DURATION:PT45M\r\nEND:VEVENT\r\n";
        let (lunch, _) = one(floating, &FixedOffset::east_opt(7200).unwrap());
        assert_eq!((lunch.start, lunch.end), (at(12, 30), at(13, 15)));
    }

    #[test]
    fn a_tzid_without_rules_is_kept_as_written_and_said() {
        let body = "BEGIN:VEVENT\r\nSUMMARY:Sync\r\nDTSTART;TZID=Mars/Olympus:20260930T090000\r\n\
                    DTEND;TZID=Mars/Olympus:20260930T100000\r\nEND:VEVENT\r\n";
        let (sync, notes) = one(body, &utc());
        assert_eq!(sync.start, at(9, 0));
        assert_eq!(
            notes.len(),
            1,
            "one note for the zone, not one per time: {notes:?}"
        );
        assert!(notes[0].contains("Mars/Olympus"));
        // UTC by name needs no rules
        let body = body.replace("Mars/Olympus", "UTC");
        let (sync, notes) = one(&body, &FixedOffset::east_opt(3600).unwrap());
        assert_eq!(sync.start, at(10, 0));
        assert!(notes.is_empty());
    }

    #[test]
    fn a_date_is_an_all_day_event_whose_end_is_the_day_after() {
        let body = "BEGIN:VEVENT\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20261009\r\n\
                    DTEND;VALUE=DATE:20261012\r\nEND:VEVENT\r\n";
        let (holiday, _) = one(body, &utc());
        assert!(holiday.all_day);
        assert_eq!(
            (holiday.date, holiday.last_day),
            (d(2026, 10, 9), d(2026, 10, 11))
        );
        let single =
            "BEGIN:VEVENT\r\nSUMMARY:Day off\r\nDTSTART;VALUE=DATE:20261009\r\nEND:VEVENT\r\n";
        let (off, _) = one(single, &utc());
        assert_eq!(off.last_day, d(2026, 10, 9));
    }

    #[test]
    fn a_repeat_rule_and_its_exceptions_are_read_in_the_readers_zone() {
        let body = format!(
            "{BERLIN}BEGIN:VEVENT\r\nUID:standup@example.com\r\nSUMMARY:Standup\r\n\
             DTSTART;TZID=Europe/Berlin:20260930T090000\r\nDURATION:PT15M\r\n\
             RRULE:FREQ=WEEKLY;BYDAY=WE\r\nEXDATE;TZID=Europe/Berlin:20261007T090000,\
             20261014T090000\r\nEXDATE;VALUE=DATE:20261021\r\nEND:VEVENT\r\n"
        );
        let (standup, _) = one(&body, &utc());
        assert_eq!(
            standup.repeat,
            Some(Rule::parse("FREQ=WEEKLY;BYDAY=WE").unwrap())
        );
        assert_eq!(
            standup.except,
            vec![d(2026, 10, 7), d(2026, 10, 14), d(2026, 10, 21)]
        );
        assert_eq!(standup.uid, "standup@example.com");
        let event = standup.to_event(ID, "").unwrap();
        assert_eq!(
            event.starts_between(d(2026, 9, 28), d(2026, 11, 1)),
            vec![d(2026, 9, 30), d(2026, 10, 28)]
        );
    }

    #[test]
    fn a_rule_outside_the_subset_keeps_the_first_date_and_says_so() {
        let body = "BEGIN:VEVENT\r\nSUMMARY:Pills\r\nDTSTART:20260930T080000\r\n\
                    RRULE:FREQ=HOURLY;INTERVAL=8\r\nEND:VEVENT\r\n";
        let (pills, notes) = one(body, &utc());
        assert_eq!(pills.repeat, None);
        assert!(
            notes
                .iter()
                .any(|n| n.contains("Pills") && n.contains("first date")),
            "{notes:?}"
        );
    }

    #[test]
    fn a_moved_occurrence_is_an_exception_of_its_series_and_an_event_of_its_own() {
        let body = "BEGIN:VEVENT\r\nUID:s1\r\nSUMMARY:Review\r\nDTSTART:20260930T140000\r\n\
                    DTEND:20260930T150000\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\n\
                    BEGIN:VEVENT\r\nUID:s1\r\nRECURRENCE-ID:20261007T140000\r\nSUMMARY:Review (moved)\r\n\
                    DTSTART:20261008T100000\r\nDTEND:20261008T110000\r\nEND:VEVENT\r\n\
                    BEGIN:VEVENT\r\nUID:s1\r\nRECURRENCE-ID:20261014T140000\r\nSTATUS:CANCELLED\r\n\
                    DTSTART:20261014T140000\r\nEND:VEVENT\r\n";
        let cal = parse(&calendar(body), &utc()).unwrap();
        assert_eq!(cal.events.len(), 2);
        assert_eq!(cal.events[0].except, vec![d(2026, 10, 7), d(2026, 10, 14)]);
        assert_eq!(cal.events[1].title, "Review (moved)");
        assert_eq!(cal.events[1].uid, "s1#20261007");
        assert_eq!(cal.events[1].date, d(2026, 10, 8));
    }

    #[test]
    fn the_first_alarm_before_the_start_is_the_reminder() {
        let body = |trigger: &str| {
            format!(
                "BEGIN:VEVENT\r\nSUMMARY:x\r\nDTSTART:20260930T090000\r\nBEGIN:VALARM\r\n\
                 ACTION:DISPLAY\r\nTRIGGER{trigger}\r\nEND:VALARM\r\nEND:VEVENT\r\n"
            )
        };
        assert_eq!(one(&body(":-PT15M"), &utc()).0.reminder, Some(15));
        assert_eq!(one(&body(":-P1D"), &utc()).0.reminder, Some(1440));
        assert_eq!(one(&body(":PT0S"), &utc()).0.reminder, Some(0));
        assert_eq!(one(&body(";RELATED=END:-PT5M"), &utc()).0.reminder, None);
        assert_eq!(one(&body(":PT10M"), &utc()).0.reminder, None);
    }

    #[test]
    fn attendees_meeting_links_and_a_missing_title_are_read() {
        let body = format!(
            "BEGIN:VEVENT\r\nDTSTART:20260930T090000\r\nDTEND:20260930T100000\r\n\
             ATTENDEE;CN=Ana:MAILTO:ana@example.com\r\nATTENDEE:mailto:ANA@example.com\r\n\
             ATTENDEE:urn:uuid:123\r\nX-AZCAL-MEETING:azlin://meet/{ROOM}\r\n\
             X-AZCAL-MEETING-SERVER:https://meet.example.com\r\nEND:VEVENT\r\n"
        );
        let (e, _) = one(&body, &utc());
        assert_eq!(e.title, NO_TITLE);
        assert_eq!(e.attendees, vec!["ana@example.com"]);
        let m = e.meeting.unwrap();
        assert_eq!(m.link, format!("azlin://meet/{ROOM}"));
        assert_eq!(m.server, "https://meet.example.com");
        assert!(!m.pending);
    }

    #[test]
    fn an_event_past_midnight_ends_at_23_59_and_says_so() {
        let body = "BEGIN:VEVENT\r\nSUMMARY:Party\r\nDTSTART:20260930T220000\r\n\
                    DTEND:20261001T020000\r\nEND:VEVENT\r\n";
        let (party, notes) = one(body, &utc());
        assert_eq!((party.start, party.end), (at(22, 0), at(23, 59)));
        assert!(notes.iter().any(|n| n.contains("Party")));
    }

    #[test]
    fn text_that_is_no_calendar_is_refused_and_tasks_are_left_out() {
        assert!(parse("hello", &utc()).is_err());
        let body = "BEGIN:VTODO\r\nSUMMARY:Buy milk\r\nEND:VTODO\r\n";
        let cal = parse(&calendar(body), &utc()).unwrap();
        assert!(cal.events.is_empty());
        assert_eq!(cal.notes.len(), 1);
    }

    /// Every field AzCalendar keeps survives a write and a read: text with commas, semicolons,
    /// backslashes and lines, a long note that folds, an all-day event of several days, a rule
    /// and its exceptions, attendees, a reminder and an AzMeet link.
    #[test]
    fn events_written_and_read_back_are_the_same_events() {
        let mut a = Event::create(
            ID,
            "Plan; review, \\ ship",
            d(2026, 9, 30),
            at(9, 0),
            at(10, 30),
            None,
        )
        .unwrap();
        a.location = String::from("Room 4, floor 2");
        a.notes = format!("Agenda:\n{}", "Discuss the roadmap. ".repeat(10));
        a.attendees = vec![
            String::from("ana@example.com"),
            String::from("bo@example.org"),
        ];
        a.reminder = Some(30);
        a.repeat = Some(Rule::new(Freq::Weekly).with_interval(2));
        a.except = vec![d(2026, 10, 14)];
        a.meeting = Some(crate::meeting::pending_meeting(
            "https://meet.example.com",
            ROOM,
        ));
        let a = a.check().unwrap();
        let b = Event::create_all_day(
            "9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3",
            "Holiday",
            d(2026, 10, 9),
            d(2026, 10, 11),
        )
        .unwrap();
        let stamp = d(2026, 10, 1).and_time(at(6, 0));
        let text = write(&[&a, &b], "Work, mostly", stamp);
        assert!(text.ends_with("END:VCALENDAR\r\n"));
        for line in text.split("\r\n") {
            assert!(line.len() <= 75, "{} octets: {line:?}", line.len());
        }
        assert!(text.contains("DTSTAMP:20261001T060000Z"));
        let cal = parse(&text, &utc()).unwrap();
        assert_eq!(cal.name.as_deref(), Some("Work, mostly"));
        assert!(cal.notes.is_empty(), "{:?}", cal.notes);
        assert_eq!(cal.events.len(), 2);
        let ra = cal.events[0].to_event(ID, "").unwrap();
        assert_eq!(ra.uid, format!("{ID}@azcalendar"));
        // what was written, the meeting as a registered copy and the uid as written
        let expected = Event {
            uid: format!("{ID}@azcalendar"),
            meeting: a.meeting.clone().map(|m| Meeting {
                pending: false,
                ..m
            }),
            ..a.clone()
        };
        assert_eq!(ra, expected);
        let rb = cal.events[1]
            .to_event("9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3", "")
            .unwrap();
        assert!(rb.all_day);
        assert_eq!((rb.date, rb.last_day), (d(2026, 10, 9), d(2026, 10, 11)));
        assert_eq!(rb.title, "Holiday");
    }

    #[test]
    fn an_export_is_named_after_its_calendar() {
        assert_eq!(file_name_for("Work"), "Work.ics");
        assert_eq!(file_name_for("My cal/../x"), "My-cal----x.ics");
        assert_eq!(file_name_for("  "), "calendar.ics");
    }
}
