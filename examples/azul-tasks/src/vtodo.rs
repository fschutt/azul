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

use azul_pim::content_line::{fold, parse_line, unfold, ContentLine};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

use crate::model::{Priority, Task};

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
    let _ = (tasks, name, stamp, to_utc);
    todo!()
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
    let _ = (text, list, now, new_id, to_local);
    todo!()
}

/// The export's file name for the list `name`: `Work.ics` (a character a file name cannot
/// hold becomes `_`; no name: `Tasks.ics`).
#[must_use]
pub fn file_name_for(name: &str) -> String {
    let _ = name;
    todo!()
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
