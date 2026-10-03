//! To-dos as an iCalendar file (RFC 5545 VTODO) and back - what Outlook, Apple Reminders,
//! Thunderbird and Nextcloud Tasks exchange to-dos as. The lines are azul-pim's content lines
//! (folding, escaping, parameters), the repeat its RRULE.
//!
//! Written per task: `UID` (the task id), `DTSTAMP`, `SUMMARY`, `DESCRIPTION` (the notes),
//! `DUE` (a date, or a floating local date-time with the due time), `RRULE` (a repeat RRULE
//! can say - not one counting from the completion), `PRIORITY` (1 high, 5 medium, 9 low),
//! `CATEGORIES` (the tags), `STATUS` and `COMPLETED`. Steps, the flag, reminders and files
//! stay in AzTasks.
//!
//! Read: every VTODO with a SUMMARY becomes a new task of the list it is imported into (a
//! new id); `DUE` (else `DTSTART`) as a date or a date-time (a `TZID` time as its wall time,
//! a UTC time in local time), `PRIORITY` 1-4 high, 5 medium, 6-9 low, `STATUS:COMPLETED` /
//! `COMPLETED`, `CATEGORIES`, `RRULE` when a to-do's repeat can hold it. What cannot be read
//! is said, one sentence each; the rest of the file is still read.

use azul_pim::{
    content_line::{escape_text, fold, parse_line, unfold, ContentLine},
    rrule::Rule,
};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

use crate::{
    model::{Priority, Task},
    recur::Repeat,
};

/// The `PRODID` AzTasks writes.
pub const PRODID: &str = "-//Azlin//AzTasks//EN";

/// What an import read.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The new tasks, in the file's order.
    pub tasks: Vec<Task>,
    /// What could not be read, one sentence each.
    pub problems: Vec<String>,
}

/// `tasks` as an iCalendar file named `name` (`X-WR-CALNAME`): one VTODO each, CRLF lines
/// folded at 75 octets. `stamp` is now in UTC (`DTSTAMP`); `to_utc` turns a local moment
/// into UTC (`COMPLETED` is UTC by the standard).
#[must_use]
pub fn write(
    tasks: &[&Task],
    name: &str,
    stamp: NaiveDateTime,
    to_utc: &dyn Fn(NaiveDateTime) -> NaiveDateTime,
) -> String {
    let mut out = String::new();
    let mut push = |l: ContentLine| {
        out.push_str(&fold(&l.to_line()));
        out.push_str("\r\n");
    };
    let raw = ContentLine::new;
    push(raw("BEGIN", "VCALENDAR"));
    push(raw("VERSION", "2.0"));
    push(raw("PRODID", PRODID));
    if !name.trim().is_empty() {
        push(ContentLine::text_value("X-WR-CALNAME", name.trim()));
    }
    for t in tasks {
        push(raw("BEGIN", "VTODO"));
        push(raw("UID", &t.id));
        push(raw("DTSTAMP", &utc_moment(stamp)));
        push(ContentLine::text_value("SUMMARY", &t.title));
        if !t.notes.is_empty() {
            push(ContentLine::text_value("DESCRIPTION", &t.notes));
        }
        if let Some(due) = t.due {
            match t.due_time {
                // A floating local time, as the task's own.
                Some(time) => push(raw("DUE", &local_moment(due.and_time(time)))),
                None => push(raw("DUE", &basic_date(due)).with_param("VALUE", &["DATE"])),
            }
            // A repeat RRULE cannot say (counting from the completion) stays in AzTasks.
            if let Some(rule) = t.repeat.as_ref().and_then(|r| r.to_rule(due)) {
                push(raw("RRULE", &rule.to_rrule(t.due_time.is_none())));
            }
        }
        if let Some(p) = priority_number(t.priority) {
            push(raw("PRIORITY", &p.to_string()));
        }
        if !t.tags.is_empty() {
            let tags: Vec<String> = t.tags.iter().map(|tag| escape_text(tag)).collect();
            push(raw("CATEGORIES", &tags.join(",")));
        }
        match t.completed {
            Some(done) => {
                push(raw("STATUS", "COMPLETED"));
                push(raw("COMPLETED", &utc_moment(to_utc(done))));
            }
            None => push(raw("STATUS", "NEEDS-ACTION")),
        }
        push(raw("END", "VTODO"));
    }
    push(raw("END", "VCALENDAR"));
    out
}

/// `20261014`.
fn basic_date(d: NaiveDate) -> String {
    d.format("%Y%m%d").to_string()
}

/// `20261014T093000`: a floating local moment.
fn local_moment(at: NaiveDateTime) -> String {
    at.format("%Y%m%dT%H%M%S").to_string()
}

/// `20261014T093000Z`: a moment in UTC.
fn utc_moment(at: NaiveDateTime) -> String {
    format!("{}Z", local_moment(at))
}

/// The iCalendar PRIORITY of a priority (none: no PRIORITY line).
fn priority_number(p: Priority) -> Option<u8> {
    match p {
        Priority::High => Some(1),
        Priority::Medium => Some(5),
        Priority::Low => Some(9),
        Priority::None => None,
    }
}

/// The priority of an iCalendar PRIORITY: 1-4 high, 5 medium, 6-9 low, else none.
fn priority_of(number: &str) -> Priority {
    match number.trim().parse::<u8>() {
        Ok(1..=4) => Priority::High,
        Ok(5) => Priority::Medium,
        Ok(6..=9) => Priority::Low,
        _ => Priority::None,
    }
}

/// A DUE / DTSTART / COMPLETED value: a date (`VALUE=DATE` or 8 digits), or a date and time -
/// floating or with a TZID as its wall time, in UTC (`Z`) turned local by `to_local`.
fn moment(
    l: &ContentLine,
    to_local: &dyn Fn(NaiveDateTime) -> NaiveDateTime,
) -> Option<(NaiveDate, Option<NaiveTime>)> {
    let v = l.value.trim();
    let date_only = l
        .param_value("VALUE")
        .is_some_and(|value| value.eq_ignore_ascii_case("DATE"))
        || v.len() == 8;
    if date_only {
        return NaiveDate::parse_from_str(v, "%Y%m%d").ok().map(|d| (d, None));
    }
    let (body, utc) = match v.strip_suffix(['Z', 'z']) {
        Some(body) => (body, true),
        None => (v, false),
    };
    let at = NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M%S").ok()?;
    let at = if utc { to_local(at) } else { at };
    Some((at.date(), Some(at.time())))
}

/// One VTODO's lines as a task of `list` (`None`, and why in `problems`, without a title).
fn to_task(
    lines: &[ContentLine],
    list: &str,
    now: NaiveDateTime,
    new_id: &mut dyn FnMut() -> String,
    to_local: &dyn Fn(NaiveDateTime) -> NaiveDateTime,
    problems: &mut Vec<String>,
) -> Option<Task> {
    let get = |name: &str| lines.iter().find(|l| l.name == name);
    let title = get("SUMMARY")
        .map(|l| l.text().trim().to_string())
        .unwrap_or_default();
    if title.is_empty() {
        let uid = get("UID").map(|l| format!(" ({})", l.text())).unwrap_or_default();
        problems.push(format!("A to-do with no title{uid} was left out."));
        return None;
    }
    let mut t = Task::new(new_id(), list.to_string(), title, now);
    t.notes = get("DESCRIPTION").map(ContentLine::text).unwrap_or_default();
    if let Some(l) = get("DUE").or_else(|| get("DTSTART")) {
        match moment(l, to_local) {
            Some((date, time)) => {
                t.due = Some(date);
                t.due_time = time;
            }
            None => problems.push(format!(
                "{:?}: the date {:?} could not be read; it was imported without a date.",
                t.title, l.value
            )),
        }
    }
    if let Some(l) = get("PRIORITY") {
        t.priority = priority_of(&l.value);
    }
    for l in lines.iter().filter(|l| l.name == "CATEGORIES") {
        for tag in l.list() {
            let tag = tag.trim_start_matches('#').trim().to_string();
            if !tag.is_empty() && !t.tags.iter().any(|x| x.eq_ignore_ascii_case(&tag)) {
                t.tags.push(tag);
            }
        }
    }
    let status_done =
        get("STATUS").is_some_and(|l| l.value.trim().eq_ignore_ascii_case("COMPLETED"));
    let completed = get("COMPLETED")
        .and_then(|l| moment(l, to_local))
        .map(|(d, time)| d.and_time(time.unwrap_or(NaiveTime::MIN)));
    if status_done || completed.is_some() {
        t.completed = Some(completed.unwrap_or(now));
    }
    if let Some(l) = get("RRULE") {
        match t.due {
            None => problems.push(format!(
                "{:?} repeats but has no date; it was imported without its repeat.",
                t.title
            )),
            Some(due) => match Rule::parse(&l.value)
                .ok()
                .and_then(|rule| Repeat::from_rule(&rule, due))
            {
                Some(repeat) => t.repeat = Some(repeat),
                None => problems.push(format!(
                    "{:?} repeats in a way a to-do cannot ({}); it was imported without its \
                     repeat.",
                    t.title, l.value
                )),
            },
        }
    }
    Some(t)
}

/// The VTODOs of `text` as new tasks of the list `list`, each with an id from `new_id`, made
/// `now`; `to_local` turns a UTC moment (`...Z`) into local time.
pub fn read(
    text: &str,
    list: &str,
    now: NaiveDateTime,
    new_id: &mut dyn FnMut() -> String,
    to_local: &dyn Fn(NaiveDateTime) -> NaiveDateTime,
) -> Imported {
    let mut out = Imported::default();
    // The lines of the VTODO being read, and how deep inside it a component (a VALARM) is.
    let mut current: Option<Vec<ContentLine>> = None;
    let mut nested = 0usize;
    let mut seen = 0usize;
    for raw in unfold(text) {
        let line = match parse_line(&raw) {
            Ok(line) => line,
            Err(e) => {
                out.problems
                    .push(format!("A line could not be read ({e}); it was left out."));
                continue;
            }
        };
        let value = line.value.trim().to_ascii_uppercase();
        let name = line.name.clone();
        let open = current.is_some();
        match name.as_str() {
            "BEGIN" if !open && value == "VTODO" => {
                current = Some(Vec::new());
                nested = 0;
                seen += 1;
            }
            "BEGIN" if open => nested += 1,
            "END" if open && nested > 0 => nested -= 1,
            "END" if open && value == "VTODO" => {
                if let Some(lines) = current.take() {
                    if let Some(task) =
                        to_task(&lines, list, now, new_id, to_local, &mut out.problems)
                    {
                        out.tasks.push(task);
                    }
                }
            }
            _ if open && nested == 0 => {
                if let Some(lines) = current.as_mut() {
                    lines.push(line);
                }
            }
            _ => {}
        }
    }
    if seen == 0 {
        out.problems
            .push(String::from("There are no to-dos (VTODO) in the file."));
    }
    out
}

/// The export's file name for the list `name`: `Work.ics` (a character a file name cannot
/// hold becomes `_`; no name: `Tasks.ics`).
#[must_use]
pub fn file_name_for(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return String::from("Tasks.ics");
    }
    let safe: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    format!("{safe}.ics")
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Weekday};

    use super::*;
    use crate::recur::Repeat;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    const ID: &str = "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a";
    const LIST: &str = "9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3";

    fn ids() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("00000000-0000-4000-8000-{n:012}")
        }
    }

    fn same(d: NaiveDateTime) -> NaiveDateTime {
        d
    }

    fn ferns() -> Task {
        let mut t = Task::new(
            ID.to_string(),
            LIST.to_string(),
            "Water the ferns, all of them".to_string(),
            at(2026, 10, 1, 9, 0),
        );
        t.notes = "Two cans;\nthe big one".to_string();
        t.due = Some(day(2026, 10, 14));
        t.priority = Priority::High;
        t.tags = vec!["home".to_string(), "garden".to_string()];
        t.repeat = Some(Repeat::weekly().on_weekdays(&[Weekday::Wed]));
        t
    }

    #[test]
    fn a_task_is_written_as_a_vtodo_with_its_dates_priority_tags_and_repeat() {
        let t = ferns();
        let mut timed = ferns();
        timed.due_time = NaiveTime::from_hms_opt(9, 30, 0);
        timed.repeat = None;
        timed.priority = Priority::Low;
        let mut done = ferns();
        done.completed = Some(at(2026, 10, 2, 19, 0));
        let text = write(&[&t, &timed, &done], "Home", at(2026, 10, 3, 8, 0), &|d| {
            d - Duration::hours(2)
        });
        assert!(text.starts_with("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n"), "{text}");
        assert!(text.ends_with("END:VCALENDAR\r\n"), "{text}");
        for want in [
            "PRODID:-//Azlin//AzTasks//EN",
            "X-WR-CALNAME:Home",
            "BEGIN:VTODO",
            &format!("UID:{ID}"),
            "DTSTAMP:20261003T080000Z",
            "SUMMARY:Water the ferns\\, all of them",
            "DESCRIPTION:Two cans\\;\\nthe big one",
            "DUE;VALUE=DATE:20261014",
            "RRULE:FREQ=WEEKLY;BYDAY=WE",
            "PRIORITY:1",
            "CATEGORIES:home,garden",
            "STATUS:NEEDS-ACTION",
            "DUE:20261014T093000",
            "PRIORITY:9",
            "STATUS:COMPLETED",
            "COMPLETED:20261002T170000Z",
            "END:VTODO",
        ] {
            assert!(text.contains(&format!("{want}\r\n")), "{want:?} is missing:\n{text}");
        }
        assert_eq!(text.matches("BEGIN:VTODO").count(), 3);
        assert!(
            text.split("\r\n").all(|l| l.len() <= 75),
            "every line is folded at 75 octets"
        );
    }

    #[test]
    fn what_aztasks_writes_reads_back_as_the_same_tasks() {
        let mut timed = ferns();
        timed.title = "Call the plumber ".repeat(8).trim().to_string();
        timed.due_time = NaiveTime::from_hms_opt(9, 30, 0);
        timed.priority = Priority::Medium;
        timed.repeat = Some(Repeat::monthly().on_month_day(14));
        let mut done = ferns();
        done.completed = Some(at(2026, 10, 2, 19, 0));
        done.priority = Priority::None;
        done.tags.clear();
        let tasks = [ferns(), timed, done];
        let refs: Vec<&Task> = tasks.iter().collect();
        let text = write(&refs, "Home", at(2026, 10, 3, 8, 0), &same);
        let back = read(&text, LIST, at(2026, 10, 3, 10, 0), &mut ids(), &same);
        assert_eq!(back.problems, Vec::<String>::new());
        assert_eq!(back.tasks.len(), 3);
        for (was, is) in tasks.iter().zip(&back.tasks) {
            assert_eq!(is.title, was.title);
            assert_eq!(is.notes, was.notes);
            assert_eq!((is.due, is.due_time), (was.due, was.due_time));
            assert_eq!(is.priority, was.priority);
            assert_eq!(is.tags, was.tags);
            assert_eq!(is.repeat, was.repeat, "{}", was.title);
            assert_eq!(is.completed, was.completed);
            assert_eq!(is.list, LIST);
            assert_ne!(is.id, was.id, "an imported task is a new task");
        }
    }

    #[test]
    fn other_apps_vtodos_are_read_as_they_mean() {
        let text = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Other//EN\r\n\
            BEGIN:VEVENT\r\nSUMMARY:A meeting, not a to-do\r\nDTSTART:20261014T090000\r\nEND:VEVENT\r\n\
            BEGIN:VTODO\r\nUID:a@x\r\nSUMMARY:Pay the rent before the fir\r\n st of the month\r\n\
            DUE;TZID=Europe/Berlin:20261031T180000\r\nPRIORITY:5\r\nEND:VTODO\r\n\
            BEGIN:VTODO\r\nUID:b@x\r\nSUMMARY:Started\r\nDTSTART;VALUE=DATE:20261020\r\n\
            PRIORITY:7\r\nSTATUS:COMPLETED\r\nEND:VTODO\r\n\
            BEGIN:VTODO\r\nUID:c@x\r\nSUMMARY:Done in UTC\r\nPRIORITY:0\r\n\
            COMPLETED:20261002T170000Z\r\nCATEGORIES:a,b\r\nCATEGORIES:c\r\nEND:VTODO\r\n\
            END:VCALENDAR\r\n";
        let now = at(2026, 10, 3, 10, 0);
        let back = read(text, LIST, now, &mut ids(), &|d| d + Duration::hours(2));
        assert_eq!(back.problems, Vec::<String>::new());
        let titles: Vec<&str> = back.tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Pay the rent before the first of the month", "Started", "Done in UTC"]
        );
        let rent = &back.tasks[0];
        assert_eq!(rent.due, Some(day(2026, 10, 31)));
        assert_eq!(rent.due_time, NaiveTime::from_hms_opt(18, 0, 0), "a TZID time: its wall time");
        assert_eq!(rent.priority, Priority::Medium);
        let started = &back.tasks[1];
        assert_eq!((started.due, started.due_time), (Some(day(2026, 10, 20)), None), "DTSTART");
        assert_eq!(started.priority, Priority::Low);
        assert_eq!(started.completed, Some(now), "completed, no time given: now");
        let utc = &back.tasks[2];
        assert_eq!(utc.priority, Priority::None);
        assert_eq!(utc.completed, Some(at(2026, 10, 2, 19, 0)), "a UTC moment in local time");
        assert_eq!(utc.tags, ["a", "b", "c"]);
        assert_eq!(utc.created, now);
    }

    #[test]
    fn what_cannot_be_read_is_said_and_the_rest_kept() {
        let text = "BEGIN:VCALENDAR\r\n\
            BEGIN:VTODO\r\nUID:a\r\nDUE;VALUE=DATE:20261014\r\nEND:VTODO\r\n\
            BEGIN:VTODO\r\nSUMMARY:Ten times\r\nDUE;VALUE=DATE:20261014\r\n\
            RRULE:FREQ=WEEKLY;COUNT=10\r\nEND:VTODO\r\n\
            BEGIN:VTODO\r\nSUMMARY:Weekly without a date\r\nRRULE:FREQ=WEEKLY\r\nEND:VTODO\r\n\
            this line has no colon\r\n\
            BEGIN:VTODO\r\nSUMMARY:Broken date\r\nDUE:2026-10-14\r\nEND:VTODO\r\n\
            END:VCALENDAR\r\n";
        let back = read(text, LIST, at(2026, 10, 3, 10, 0), &mut ids(), &same);
        let titles: Vec<&str> = back.tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Ten times", "Weekly without a date", "Broken date"]);
        assert!(back.tasks.iter().all(|t| t.repeat.is_none()));
        assert_eq!(back.tasks[2].due, None);
        assert_eq!(back.problems.len(), 5, "{:#?}", back.problems);
        assert!(back.problems[0].contains("no title"), "{:?}", back.problems[0]);
        assert!(back.problems[1].contains("Ten times"), "{:?}", back.problems[1]);
        assert!(back.problems[2].contains("Weekly without a date"), "{:?}", back.problems[2]);
        assert!(back.problems[3].contains("no colon"), "{:?}", back.problems[3]);
        assert!(back.problems[4].contains("Broken date"), "{:?}", back.problems[4]);

        let none = read("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n", LIST, at(2026, 10, 3, 10, 0), &mut ids(), &same);
        assert!(none.tasks.is_empty());
        assert_eq!(none.problems.len(), 1, "a file without to-dos says so");
    }

    #[test]
    fn an_export_file_is_named_after_its_list() {
        assert_eq!(file_name_for("Work"), "Work.ics");
        assert_eq!(file_name_for("Home/Garden: 2026"), "Home_Garden_ 2026.ics");
        assert_eq!(file_name_for("  "), "Tasks.ics");
    }
}
