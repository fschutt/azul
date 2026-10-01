//! Quick add: one line becomes a task.
//!
//! "Pay rent tomorrow 9am #home !high" is the task "Pay rent", due tomorrow at 09:00, tagged
//! `home`, high priority. What the parser recognised is returned as [`Part`]s, which the
//! quick-add line shows as chips before the task is made; a click on a chip parses the line
//! again with that part taken literally ([`parse_with`]'s `ignore`), so a wrong guess
//! ("Fix the sun icon" is not due on Sunday) is one click away from right.
//!
//! What it knows, in English and German:
//!
//! - days: `today`, `tonight`, `tomorrow`, `day after tomorrow`, a weekday (`fri`, `friday`,
//!   `next monday`, `this friday`), `this weekend`, `next week` / `month` / `year`,
//!   `in 3 days` / `2 weeks` / `an hour`, `oct 2`, `2 october 2027`, `2nd of october`,
//!   `the 1st`, `2026-10-02`, `2.10.` / `2.10.2026`; `heute`, `morgen`, `übermorgen`,
//!   `freitag`, `nächsten montag`, `in 3 Tagen`, `2. Oktober`;
//! - times: `9am`, `9:30pm`, `9 pm`, `15:00`, `at 3` (3 pm), `noon`, `9 Uhr`, `um 9`;
//! - repeats: `daily`, `weekly`, `monthly`, `yearly`, `weekdays`, `biweekly`, `every day`,
//!   `every 2 weeks`, `every other week`, `every monday and wednesday`, `every 1st`;
//!   `täglich`, `jeden Dienstag`, `alle 2 Wochen`;
//! - `#tag`, `@list` (a list's name, spaces left out, or the start of it), priorities
//!   `!low` / `!medium` / `!high` (`!l`, `!m`, `!h`, `!!`, `!!!`, `p3`, `p2`, `p1`) and a lone
//!   `!` for "flagged".
//!
//! Filler words in front of a day or a time belong to it: "on the 1st", "by friday",
//! "at 3pm", "am Montag", "um 9". A plain weekday is the next such day after today ("monday"
//! said on a Monday is next week's); "this monday" said on a Monday is today. A time without
//! a day is today when it is still to come, else tomorrow; a repeat without a day starts on
//! its first day. Only the first day, time, repeat, list and priority count: a second one
//! stays in the title.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};

use crate::{
    model::{self, Priority},
    recur::{self, Repeat, Unit},
};

/// What the parser needs to know besides the line.
#[derive(Debug, Clone)]
pub struct Context<'a> {
    /// The moment the line is parsed (the user's wall clock).
    pub now: NaiveDateTime,
    /// Where "next week" starts.
    pub week_start: Weekday,
    /// `(id, name)` of the lists `@name` can name.
    pub lists: &'a [(String, String)],
}

/// The kind of a recognised part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    Date,
    Time,
    Repeat,
    Tag,
    List,
    Priority,
    Flag,
}

/// A recognised part of the line, for its chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub kind: PartKind,
    /// The words as typed.
    pub text: String,
    /// The index of its first word (what [`parse_with`] ignores).
    pub start: usize,
    /// What the chip says: "Tomorrow", "09:00", "Every 2 weeks", "#home", "Work",
    /// "High priority", "Flagged".
    pub label: String,
}

/// The task a line describes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parsed {
    pub title: String,
    pub due: Option<NaiveDate>,
    pub time: Option<NaiveTime>,
    pub repeat: Option<Repeat>,
    pub tags: Vec<String>,
    /// The id of the list `@name` named.
    pub list: Option<String>,
    pub priority: Option<Priority>,
    pub flagged: bool,
    pub parts: Vec<Part>,
}

/// Parses a quick-add line.
#[must_use]
pub fn parse(input: &str, ctx: &Context<'_>) -> Parsed { todo!() }

/// Parses a quick-add line, taking the words that start at the indices in `ignore` literally
/// (a chip the user clicked away).
#[must_use]
pub fn parse_with(input: &str, ctx: &Context<'_>, ignore: &[usize]) -> Parsed { todo!() }

// ==== Words ====

/// The words of a line: as typed, lower case without trailing `,;:` (`clean`), and that
/// without trailing `.!?` too (`bare`).
struct Words<'a> {
    raw: Vec<&'a str>,
    clean: Vec<String>,
    bare: Vec<String>,
}

impl<'a> Words<'a> {
    fn new(input: &'a str) -> Self { todo!() }

    fn len(&self) -> usize { todo!() }

    fn bare(&self, i: usize) -> &str { todo!() }

    fn clean(&self, i: usize) -> &str { todo!() }
}

/// What a match found.
enum Hit {
    Date {
        date: NaiveDate,
        time: Option<NaiveTime>,
    },
    Time(NaiveTime),
    Repeat(Repeat),
    Tag(String),
    List {
        id: String,
        name: String,
    },
    Priority(Priority),
    Flag,
}

/// The longest thing that starts at word `i`: `(words used, what)`.
fn match_at(words: &Words<'_>, i: usize, ctx: &Context<'_>, ignore: &[usize]) -> Option<(usize, Hit)> { todo!() }

/// Words that lead into a day or a time and belong to it.
fn is_filler(word: &str) -> bool { todo!() }

/// The list `@name` names: its name without spaces and punctuation, any case, or - if only
/// one list starts so - the start of it.
fn list_named(name: &str, lists: &[(String, String)]) -> Option<(String, String)> { todo!() }

/// `!` (flagged), `!!`, `!!!`, `!low`, `!high`, `p1`, ...
fn priority_of(word: &str) -> Option<Hit> { todo!() }

// ==== Repeats ====

fn repeat_at(words: &Words<'_>, i: usize) -> Option<(usize, Repeat)> { todo!() }

// ==== Days ====

/// A length of time: a calendar unit, or hours / minutes (for "in 2 hours").
#[derive(Clone, Copy)]
enum Span {
    Unit(Unit),
    Hours,
    Minutes,
}

fn span_of(word: &str) -> Option<Span> { todo!() }

/// `3`, `a`, `an`, `one` .. `ten`, `ein` .. `zehn`.
fn count_of(word: &str) -> Option<u32> { todo!() }

/// A weekday's English or German name (English also abbreviated).
fn weekday_of(word: &str) -> Option<Weekday> { todo!() }

/// A month's English or German name (abbreviated too).
fn month_of(word: &str) -> Option<u32> { todo!() }

/// A day of the month: `2`, `02`, `2nd`, `2.` (1..=31).
fn day_number(word: &str) -> Option<u32> { todo!() }

/// A day of the month written as an ordinal: `1st`, `22nd`, `3rd`, `15th`.
fn ordinal_day(word: &str) -> Option<u32> { todo!() }

/// A four-digit year from 1970 to 2200.
fn year_of(word: &str) -> Option<i32> { todo!() }

/// The next `day` after `today` (a week ahead when `today` is that day).
fn next_weekday(today: NaiveDate, day: Weekday) -> NaiveDate { todo!() }

/// `day` this week: today when today is that day, else the next one.
fn this_weekday(today: NaiveDate, day: Weekday) -> NaiveDate { todo!() }

/// The first day of the week after this one.
fn next_week_start(today: NaiveDate, week_start: Weekday) -> NaiveDate { todo!() }

/// `month`/`day` of `year`, or of the next year when that is past.
fn upcoming(today: NaiveDate, year: Option<i32>, month: u32, day: u32) -> Option<NaiveDate> { todo!() }

/// The next date (from today on) whose day of the month is `day`.
fn next_month_day(today: NaiveDate, day: u32) -> Option<NaiveDate> { todo!() }

/// `2026-10-02`, `2.10.`, `2.10.2026`, `02.10.26`.
fn numeric_date(word: &str, today: NaiveDate) -> Option<NaiveDate> { todo!() }

/// A day that starts at word `i`: `(words used, day, a time it implies)`.
fn date_at(words: &Words<'_>, i: usize, ctx: &Context<'_>) -> Option<(usize, NaiveDate, Option<NaiveTime>)> { todo!() }

/// The coming Saturday (today on a Saturday or a Sunday).
fn weekend(today: NaiveDate) -> NaiveDate { todo!() }

// ==== Times ====

/// `9`, `9:30`, `15:00` as hour and minute (24-hour range).
fn clock(text: &str) -> Option<(u32, u32)> { todo!() }

/// `9`, `9:30` with `am` / `pm`.
fn twelve_hour(text: &str, pm: bool) -> Option<NaiveTime> { todo!() }

/// A time that starts at word `i`; `filler` is the word before it when that leads into it
/// ("at", "um"), which lets a bare hour count ("at 3" is 15:00, "um 9" is 09:00).
fn time_at(words: &Words<'_>, i: usize, filler: Option<&str>) -> Option<(usize, NaiveTime)> { todo!() }

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn time(h: u32, m: u32) -> Option<NaiveTime> {
        NaiveTime::from_hms_opt(h, m, 0)
    }

    fn lists() -> Vec<(String, String)> {
        vec![
            ("l-work".into(), "Work".into()),
            ("l-home".into(), "Household".into()),
            ("l-azl".into(), "Azlin launch".into()),
            ("l-azd".into(), "Azlin design".into()),
        ]
    }

    /// Thursday 1 October 2026, 10:00.
    fn at_thursday(input: &str) -> Parsed {
        at(input, day(2026, 10, 1), 10, 0)
    }

    fn at(input: &str, date: NaiveDate, h: u32, m: u32) -> Parsed {
        let lists = lists();
        let ctx = Context {
            now: date.and_hms_opt(h, m, 0).unwrap(),
            week_start: Weekday::Mon,
            lists: &lists,
        };
        parse(input, &ctx)
    }

    #[test]
    fn pay_rent_tomorrow_9am_home_high_is_a_task_due_tomorrow_at_nine() {
        let p = at_thursday("Pay rent tomorrow 9am #home !high");
        assert_eq!(p.title, "Pay rent");
        assert_eq!(p.due, Some(day(2026, 10, 2)));
        assert_eq!(p.time, time(9, 0));
        assert_eq!(p.tags, vec!["home"]);
        assert_eq!(p.priority, Some(Priority::High));
        let kinds: Vec<PartKind> = p.parts.iter().map(|x| x.kind).collect();
        assert_eq!(
            kinds,
            vec![PartKind::Date, PartKind::Time, PartKind::Tag, PartKind::Priority]
        );
        let labels: Vec<&str> = p.parts.iter().map(|x| x.label.as_str()).collect();
        assert_eq!(labels, vec!["Tomorrow", "09:00", "#home", "High priority"]);
    }

    #[test]
    fn the_plans_quick_add_line_is_due_friday_at_three_flagged_and_repeating() {
        let p = at_thursday("Call Kai about the venue fri 3pm #work !  every 2 weeks");
        assert_eq!(p.title, "Call Kai about the venue");
        assert_eq!(p.due, Some(day(2026, 10, 2)));
        assert_eq!(p.time, time(15, 0));
        assert!(p.flagged);
        assert_eq!(p.tags, vec!["work"]);
        assert_eq!(p.repeat, Some(Repeat::new(2, Unit::Week)));
    }

    #[test]
    fn a_repeat_without_a_day_starts_today_or_on_its_first_day() {
        let p = at_thursday("every 2 weeks water plants");
        assert_eq!(p.title, "water plants");
        assert_eq!(p.repeat, Some(Repeat::new(2, Unit::Week)));
        assert_eq!(p.due, Some(day(2026, 10, 1)));
        let p = at_thursday("every monday and wednesday standup");
        assert_eq!(p.title, "standup");
        assert_eq!(
            p.repeat,
            Some(Repeat::weekly().on_weekdays(&[Weekday::Mon, Weekday::Wed]))
        );
        assert_eq!(p.due, Some(day(2026, 10, 5)));
        let p = at_thursday("Pay rent every 1st");
        assert_eq!(p.repeat, Some(Repeat::monthly().on_month_day(1)));
        assert_eq!(p.due, Some(day(2026, 10, 1)));
    }

    #[test]
    fn german_phrases_are_understood() {
        let p = at_thursday("morgen 9 Uhr Zahnarzt");
        assert_eq!(p.title, "Zahnarzt");
        assert_eq!(p.due, Some(day(2026, 10, 2)));
        assert_eq!(p.time, time(9, 0));
        let p = at_thursday("Meeting nächsten Freitag um 14 Uhr");
        assert_eq!(p.title, "Meeting");
        assert_eq!(p.due, Some(day(2026, 10, 2)));
        assert_eq!(p.time, time(14, 0));
        let p = at_thursday("Müll rausbringen jeden Dienstag");
        assert_eq!(p.title, "Müll rausbringen");
        assert_eq!(p.repeat, Some(Repeat::weekly().on_weekdays(&[Weekday::Tue])));
        assert_eq!(p.due, Some(day(2026, 10, 6)));
        let p = at_thursday("Steuer in 3 Tagen");
        assert_eq!(p.due, Some(day(2026, 10, 4)));
        let p = at_thursday("Termin am 2. Oktober");
        assert_eq!(p.title, "Termin");
        assert_eq!(p.due, Some(day(2026, 10, 2)));
        let p = at_thursday("Gießen alle 2 Wochen");
        assert_eq!(p.repeat, Some(Repeat::new(2, Unit::Week)));
    }

    #[test]
    fn the_1st_is_the_next_first_of_a_month() {
        let p = at("pay rent on the 1st", day(2026, 10, 2), 10, 0);
        assert_eq!(p.title, "pay rent");
        assert_eq!(p.due, Some(day(2026, 11, 1)));
        assert_eq!(at_thursday("pay rent on the 1st").due, Some(day(2026, 10, 1)));
        assert_eq!(
            at("rent the 31st", day(2026, 9, 5), 10, 0).due,
            Some(day(2026, 10, 31)),
            "September has no 31st"
        );
    }

    #[test]
    fn next_monday_said_on_a_monday_is_a_week_away_and_this_monday_is_today() {
        let monday = day(2026, 10, 5);
        assert_eq!(at("Standup next monday", monday, 9, 0).due, Some(day(2026, 10, 12)));
        assert_eq!(at("Standup monday", monday, 9, 0).due, Some(day(2026, 10, 12)));
        assert_eq!(at("Standup this monday", monday, 9, 0).due, Some(monday));
        assert_eq!(at_thursday("Standup next monday").due, Some(monday));
        assert_eq!(at_thursday("Plan next week").due, Some(day(2026, 10, 5)));
        assert_eq!(at_thursday("Plan next month").due, Some(day(2026, 11, 1)));
        assert_eq!(at_thursday("Hike this weekend").due, Some(day(2026, 10, 3)));
    }

    #[test]
    fn relative_days_and_hours_count_from_now() {
        assert_eq!(at_thursday("Report in 3 days").due, Some(day(2026, 10, 4)));
        assert_eq!(at_thursday("Report in 2 weeks").due, Some(day(2026, 10, 15)));
        assert_eq!(at_thursday("Report in a month").due, Some(day(2026, 11, 1)));
        let p = at_thursday("Call back in 2 hours");
        assert_eq!(p.title, "Call back");
        assert_eq!((p.due, p.time), (Some(day(2026, 10, 1)), time(12, 0)));
        assert_eq!(at_thursday("Go day after tomorrow").due, Some(day(2026, 10, 3)));
        let p = at_thursday("Movie tonight");
        assert_eq!((p.due, p.time), (Some(day(2026, 10, 1)), time(20, 0)));
    }

    #[test]
    fn calendar_dates_in_words_and_numbers() {
        let p = at_thursday("Dentist Oct 14 at 3");
        assert_eq!(p.title, "Dentist");
        assert_eq!((p.due, p.time), (Some(day(2026, 10, 14)), time(15, 0)));
        assert_eq!(at_thursday("Dentist 14 October 2027").due, Some(day(2027, 10, 14)));
        assert_eq!(at_thursday("Dentist 2nd of november").due, Some(day(2026, 11, 2)));
        let p = at_thursday("Flight 2026-12-24 07:30");
        assert_eq!((p.due, p.time), (Some(day(2026, 12, 24)), time(7, 30)));
        assert_eq!(at_thursday("Party 24.12.").due, Some(day(2026, 12, 24)));
        assert_eq!(
            at_thursday("Steuer 31.5.").due,
            Some(day(2027, 5, 31)),
            "a day already past this year is next year's"
        );
        assert_eq!(at_thursday("Steuer 31.5.2026").due, Some(day(2026, 5, 31)));
        assert_eq!(at_thursday("Nothing 31.2.").due, None, "no 31 February");
    }

    #[test]
    fn a_time_without_a_day_is_today_while_it_is_to_come_else_tomorrow() {
        let p = at_thursday("Call 3pm");
        assert_eq!((p.due, p.time), (Some(day(2026, 10, 1)), time(15, 0)));
        let p = at_thursday("Call 9am");
        assert_eq!((p.due, p.time), (Some(day(2026, 10, 2)), time(9, 0)));
        let p = at_thursday("Lunch at noon");
        assert_eq!((p.title.as_str(), p.time), ("Lunch", time(12, 0)));
        let p = at_thursday("Gym 18:30");
        assert_eq!(p.time, time(18, 30));
        assert_eq!(at_thursday("Buy 3 apples").time, None, "a bare number is not a time");
        assert_eq!(at_thursday("Buy 3 apples").title, "Buy 3 apples");
    }

    #[test]
    fn lists_priorities_and_flags() {
        let p = at_thursday("Review @work notes p2");
        assert_eq!(p.title, "Review notes");
        assert_eq!(p.list.as_deref(), Some("l-work"));
        assert_eq!(p.priority, Some(Priority::Medium));
        assert_eq!(
            at_thursday("Tweet @azlinlaunch").list.as_deref(),
            Some("l-azl"),
            "spaces left out"
        );
        assert_eq!(at_thursday("Tweet @house").list.as_deref(), Some("l-home"), "a unique start");
        let p = at_thursday("Tweet @azlin");
        assert_eq!(p.list, None, "two lists start so");
        assert_eq!(p.title, "Tweet @azlin");
        assert_eq!(at_thursday("x !low").priority, Some(Priority::Low));
        assert_eq!(at_thursday("x !!!").priority, Some(Priority::High));
        assert!(at_thursday("x !").flagged);
        assert_eq!(at_thursday("Fix issue #12").title, "Fix issue #12", "a number is no tag");
    }

    #[test]
    fn a_clicked_away_chip_keeps_its_words_in_the_title() {
        let p = at_thursday("Fix the sun icon");
        assert_eq!(p.due, Some(day(2026, 10, 4)), "sun reads as Sunday");
        assert_eq!(p.parts[0].start, 2);
        let lists = lists();
        let ctx = Context {
            now: day(2026, 10, 1).and_hms_opt(10, 0, 0).unwrap(),
            week_start: Weekday::Mon,
            lists: &lists,
        };
        let p = parse_with("Fix the sun icon", &ctx, &[2]);
        assert_eq!(p.title, "Fix the sun icon");
        assert_eq!(p.due, None);
        assert!(p.parts.is_empty());
    }

    #[test]
    fn only_the_first_day_counts_and_a_line_of_only_a_day_keeps_its_words() {
        let p = at_thursday("Move monday meeting to friday");
        assert_eq!(p.due, Some(day(2026, 10, 5)));
        assert_eq!(p.title, "Move meeting to friday");
        let p = at_thursday("tomorrow");
        assert_eq!(p.title, "tomorrow");
        assert_eq!(p.due, Some(day(2026, 10, 2)));
    }

    #[test]
    fn repeats_in_words() {
        assert_eq!(at_thursday("Pills daily").repeat, Some(Repeat::daily()));
        assert_eq!(at_thursday("Pills every day").repeat, Some(Repeat::daily()));
        assert_eq!(at_thursday("Pills everyday").repeat, Some(Repeat::daily()));
        assert_eq!(at_thursday("Plants every other week").repeat, Some(Repeat::new(2, Unit::Week)));
        assert_eq!(at_thursday("Standup weekdays 9am").repeat, Some(Repeat::weekdays()));
        assert_eq!(at_thursday("Review every 3 months").repeat.map(|r| (r.every, r.unit)), Some((3, Unit::Month)));
        let p = at_thursday("Birthday yearly Oct 14");
        assert_eq!(p.due, Some(day(2026, 10, 14)));
        assert_eq!(p.repeat, Some(Repeat::yearly().on_month_day(14)));
    }
}
