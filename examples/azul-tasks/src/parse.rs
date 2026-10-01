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
pub fn parse(input: &str, ctx: &Context<'_>) -> Parsed {
    parse_with(input, ctx, &[])
}

/// Parses a quick-add line, taking the words that start at the indices in `ignore` literally
/// (a chip the user clicked away).
#[must_use]
pub fn parse_with(input: &str, ctx: &Context<'_>, ignore: &[usize]) -> Parsed {
    let words = Words::new(input);
    let today = ctx.now.date();
    let n = words.len();
    let mut used = vec![false; n];
    let mut out = Parsed::default();
    let mut i = 0;
    while i < n {
        if ignore.contains(&i) {
            i += 1;
            continue;
        }
        let Some((len, hit)) = match_at(&words, i, ctx, ignore) else {
            i += 1;
            continue;
        };
        // Only the first day, time, repeat, list and priority count.
        let taken = match &hit {
            Hit::Date { .. } => out.due.is_some(),
            Hit::Time(_) => out.time.is_some(),
            Hit::Repeat(_) => out.repeat.is_some(),
            Hit::List { .. } => out.list.is_some(),
            Hit::Priority(_) => out.priority.is_some(),
            Hit::Tag(_) | Hit::Flag => false,
        };
        if taken {
            i += 1;
            continue;
        }
        let text = words.raw[i..i + len].join(" ");
        let (kind, label) = match hit {
            Hit::Date { date, time } => {
                out.due = Some(date);
                if let Some(t) = time {
                    if out.time.is_none() {
                        out.time = Some(t);
                    }
                }
                (PartKind::Date, model::day_label(date, today))
            }
            Hit::Time(t) => {
                out.time = Some(t);
                (PartKind::Time, model::format_time(t))
            }
            Hit::Repeat(r) => {
                let label = r.label();
                out.repeat = Some(r);
                (PartKind::Repeat, label)
            }
            Hit::Tag(tag) => {
                let label = format!("#{tag}");
                if !out.tags.iter().any(|t| t.to_lowercase() == tag.to_lowercase()) {
                    out.tags.push(tag);
                }
                (PartKind::Tag, label)
            }
            Hit::List { id, name } => {
                out.list = Some(id);
                (PartKind::List, name)
            }
            Hit::Priority(p) => {
                out.priority = Some(p);
                (PartKind::Priority, format!("{} priority", p.label()))
            }
            Hit::Flag => {
                out.flagged = true;
                (PartKind::Flag, "Flagged".to_string())
            }
        };
        out.parts.push(Part {
            kind,
            text,
            start: i,
            label,
        });
        for u in used.iter_mut().skip(i).take(len) {
            *u = true;
        }
        i += len;
    }

    let title: Vec<&str> = words
        .raw
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(w, _)| *w)
        .collect();
    out.title = if title.is_empty() {
        input.trim().to_string()
    } else {
        title.join(" ")
    };

    // A time without a day: today while it is still to come, else tomorrow.
    if out.due.is_none() {
        if let Some(t) = out.time {
            out.due = Some(if t > ctx.now.time() {
                today
            } else {
                today + Duration::days(1)
            });
        }
    }
    // A repeat without a day starts on its first day; a month rule keeps its day.
    if let Some(rule) = out.repeat.take() {
        let due = *out.due.get_or_insert_with(|| rule.first_on_or_after(today));
        out.repeat = Some(rule.anchored(due));
    }
    out
}

// ==== Words ====

/// The words of a line: as typed, lower case without trailing `,;:` (`clean`), and that
/// without trailing `.!?` too (`bare`).
struct Words<'a> {
    raw: Vec<&'a str>,
    clean: Vec<String>,
    bare: Vec<String>,
}

impl<'a> Words<'a> {
    fn new(input: &'a str) -> Self {
        let raw: Vec<&str> = input.split_whitespace().collect();
        let clean: Vec<String> = raw
            .iter()
            .map(|w| w.to_lowercase().trim_end_matches([',', ';', ':']).to_string())
            .collect();
        let bare = clean
            .iter()
            .map(|w| w.trim_end_matches(['.', '!', '?']).to_string())
            .collect();
        Words { raw, clean, bare }
    }

    fn len(&self) -> usize {
        self.raw.len()
    }

    fn bare(&self, i: usize) -> &str {
        self.bare.get(i).map_or("", String::as_str)
    }

    fn clean(&self, i: usize) -> &str {
        self.clean.get(i).map_or("", String::as_str)
    }
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
fn match_at(words: &Words<'_>, i: usize, ctx: &Context<'_>, ignore: &[usize]) -> Option<(usize, Hit)> {
    let raw = words.raw[i];
    if let Some(tag) = raw.strip_prefix('#') {
        let tag = tag.trim_end_matches([',', ';', ':', '.', '!', '?']);
        if !tag.is_empty() && !tag.chars().all(|c| c.is_ascii_digit()) && !tag.contains('#') {
            return Some((1, Hit::Tag(tag.to_string())));
        }
        return None;
    }
    if let Some(name) = raw.strip_prefix('@') {
        return list_named(name, ctx.lists).map(|(id, name)| (1, Hit::List { id, name }));
    }
    if let Some(hit) = priority_of(words.clean(i)) {
        return Some((1, hit));
    }
    if let Some((len, rule)) = repeat_at(words, i) {
        return Some((len, Hit::Repeat(rule)));
    }
    if let Some((len, date, time)) = date_at(words, i, ctx) {
        return Some((len, Hit::Date { date, time }));
    }
    if let Some((len, time)) = time_at(words, i, None) {
        return Some((len, Hit::Time(time)));
    }
    let filler = words.bare(i);
    if is_filler(filler) && i + 1 < words.len() && !ignore.contains(&(i + 1)) {
        if let Some((len, date, time)) = date_at(words, i + 1, ctx) {
            return Some((len + 1, Hit::Date { date, time }));
        }
        if let Some((len, time)) = time_at(words, i + 1, Some(filler)) {
            return Some((len + 1, Hit::Time(time)));
        }
    }
    None
}

/// Words that lead into a day or a time and belong to it.
fn is_filler(word: &str) -> bool {
    matches!(
        word,
        "on" | "by"
            | "due"
            | "at"
            | "next"
            | "am"
            | "um"
            | "bis"
            | "nächsten"
            | "naechsten"
            | "nächster"
            | "naechster"
            | "nächste"
            | "naechste"
            | "kommenden"
            | "kommender"
    )
}

/// The list `@name` names: its name without spaces and punctuation, any case, or - if only
/// one list starts so - the start of it.
fn list_named(name: &str, lists: &[(String, String)]) -> Option<(String, String)> {
    let squash = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let wanted = squash(name);
    if wanted.is_empty() {
        return None;
    }
    if let Some((id, n)) = lists.iter().find(|(_, n)| squash(n) == wanted) {
        return Some((id.clone(), n.clone()));
    }
    let mut starting = lists.iter().filter(|(_, n)| squash(n).starts_with(&wanted));
    match (starting.next(), starting.next()) {
        (Some((id, n)), None) => Some((id.clone(), n.clone())),
        _ => None,
    }
}

/// `!` (flagged), `!!`, `!!!`, `!low`, `!high`, `p1`, ...
fn priority_of(word: &str) -> Option<Hit> {
    let p = match word {
        "!" | "!flag" | "!flagged" => return Some(Hit::Flag),
        "!!" | "!m" | "!med" | "!medium" | "!mittel" | "p2" => Priority::Medium,
        "!!!" | "!h" | "!hi" | "!high" | "!hoch" | "!important" | "!wichtig" | "p1" => {
            Priority::High
        }
        "!l" | "!lo" | "!low" | "!niedrig" | "p3" => Priority::Low,
        _ => return None,
    };
    Some(Hit::Priority(p))
}

// ==== Repeats ====

fn repeat_at(words: &Words<'_>, i: usize) -> Option<(usize, Repeat)> {
    let w = words.bare(i);
    let single = match w {
        "daily" | "everyday" | "täglich" | "taeglich" => Some(Repeat::daily()),
        "weekly" | "wöchentlich" | "woechentlich" => Some(Repeat::weekly()),
        "monthly" | "monatlich" => Some(Repeat::monthly()),
        "yearly" | "annually" | "jährlich" | "jaehrlich" => Some(Repeat::yearly()),
        "biweekly" | "fortnightly" => Some(Repeat::new(2, Unit::Week)),
        "weekdays" | "werktags" => Some(Repeat::weekdays()),
        _ => None,
    };
    if let Some(rule) = single {
        return Some((1, rule));
    }
    if !matches!(w, "every" | "each" | "jeden" | "jede" | "jedes" | "jeder" | "alle") {
        return None;
    }
    let j = i + 1;
    let next = words.bare(j);
    if next == "other" {
        if let Some(Span::Unit(unit)) = span_of(words.bare(j + 1)) {
            return Some((3, Repeat::new(2, unit)));
        }
        return None;
    }
    if matches!(next, "weekday" | "weekdays" | "werktag") {
        return Some((2, Repeat::weekdays()));
    }
    if let Some(n) = count_of(next) {
        if let Some(Span::Unit(unit)) = span_of(words.bare(j + 1)) {
            return Some((3, Repeat::new(n, unit)));
        }
    }
    if let Some(Span::Unit(unit)) = span_of(next) {
        return Some((2, Repeat::new(1, unit)));
    }
    if let Some(day) = ordinal_day(next) {
        return Some((2, Repeat::monthly().on_month_day(day)));
    }
    // A list of weekdays: "every monday", "every mon and wed", "jeden Montag und Freitag".
    let mut days = Vec::new();
    let mut k = j;
    let mut end = j;
    while k < words.len() {
        let word = words.bare(k);
        if let Some(day) = weekday_of(word) {
            days.push(day);
            k += 1;
            end = k;
        } else if !days.is_empty() && matches!(word, "and" | "und" | "&" | "") {
            k += 1;
        } else {
            break;
        }
    }
    if days.is_empty() {
        return None;
    }
    Some((end - i, Repeat::weekly().on_weekdays(&days)))
}

// ==== Days ====

/// A length of time: a calendar unit, or hours / minutes (for "in 2 hours").
#[derive(Clone, Copy)]
enum Span {
    Unit(Unit),
    Hours,
    Minutes,
}

fn span_of(word: &str) -> Option<Span> {
    Some(match word {
        "day" | "days" | "tag" | "tage" | "tagen" => Span::Unit(Unit::Day),
        "week" | "weeks" | "woche" | "wochen" => Span::Unit(Unit::Week),
        "month" | "months" | "monat" | "monate" | "monaten" => Span::Unit(Unit::Month),
        "year" | "years" | "jahr" | "jahre" | "jahren" => Span::Unit(Unit::Year),
        "hour" | "hours" | "hr" | "hrs" | "h" | "stunde" | "stunden" => Span::Hours,
        "minute" | "minutes" | "min" | "mins" | "minuten" => Span::Minutes,
        _ => return None,
    })
}

/// `3`, `a`, `an`, `one` .. `ten`, `ein` .. `zehn`.
fn count_of(word: &str) -> Option<u32> {
    if !word.is_empty() && word.len() <= 3 && word.chars().all(|c| c.is_ascii_digit()) {
        return word.parse().ok().filter(|n| *n > 0);
    }
    Some(match word {
        "a" | "an" | "one" | "ein" | "eine" | "einem" | "einen" | "einer" => 1,
        "two" | "zwei" => 2,
        "three" | "drei" => 3,
        "four" | "vier" => 4,
        "five" | "fünf" | "fuenf" => 5,
        "six" | "sechs" => 6,
        "seven" | "sieben" => 7,
        "eight" | "acht" => 8,
        "nine" | "neun" => 9,
        "ten" | "zehn" => 10,
        _ => return None,
    })
}

/// A weekday's English or German name (English also abbreviated).
fn weekday_of(word: &str) -> Option<Weekday> {
    Some(match word {
        "mon" | "monday" | "montag" => Weekday::Mon,
        "tue" | "tues" | "tuesday" | "dienstag" => Weekday::Tue,
        "wed" | "weds" | "wednesday" | "mittwoch" => Weekday::Wed,
        "thu" | "thur" | "thurs" | "thursday" | "donnerstag" => Weekday::Thu,
        "fri" | "friday" | "freitag" => Weekday::Fri,
        "sat" | "saturday" | "samstag" | "sonnabend" => Weekday::Sat,
        "sun" | "sunday" | "sonntag" => Weekday::Sun,
        _ => return None,
    })
}

/// A month's English or German name (abbreviated too).
fn month_of(word: &str) -> Option<u32> {
    Some(match word {
        "jan" | "january" | "januar" | "jänner" | "jaenner" => 1,
        "feb" | "february" | "februar" => 2,
        "mar" | "march" | "märz" | "maerz" | "mär" => 3,
        "apr" | "april" => 4,
        "may" | "mai" => 5,
        "jun" | "june" | "juni" => 6,
        "jul" | "july" | "juli" => 7,
        "aug" | "august" => 8,
        "sep" | "sept" | "september" => 9,
        "oct" | "october" | "okt" | "oktober" => 10,
        "nov" | "november" => 11,
        "dec" | "december" | "dez" | "dezember" => 12,
        _ => return None,
    })
}

/// A day of the month: `2`, `02`, `2nd`, `2.` (1..=31).
fn day_number(word: &str) -> Option<u32> {
    let digits = word
        .trim_end_matches('.')
        .trim_end_matches("st")
        .trim_end_matches("nd")
        .trim_end_matches("rd")
        .trim_end_matches("th");
    if digits.is_empty() || digits.len() > 2 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|d| (1..=31).contains(d))
}

/// A day of the month written as an ordinal: `1st`, `22nd`, `3rd`, `15th`.
fn ordinal_day(word: &str) -> Option<u32> {
    let has_suffix = ["st", "nd", "rd", "th"].iter().any(|s| word.ends_with(s));
    if has_suffix {
        day_number(word)
    } else {
        None
    }
}

/// A four-digit year from 1970 to 2200.
fn year_of(word: &str) -> Option<i32> {
    if word.len() != 4 || !word.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    word.parse().ok().filter(|y| (1970..=2200).contains(y))
}

/// The next `day` after `today` (a week ahead when `today` is that day).
fn next_weekday(today: NaiveDate, day: Weekday) -> NaiveDate {
    let ahead = (7 + day.num_days_from_monday() - today.weekday().num_days_from_monday()) % 7;
    today + Duration::days(i64::from(if ahead == 0 { 7 } else { ahead }))
}

/// `day` this week: today when today is that day, else the next one.
fn this_weekday(today: NaiveDate, day: Weekday) -> NaiveDate {
    let ahead = (7 + day.num_days_from_monday() - today.weekday().num_days_from_monday()) % 7;
    today + Duration::days(i64::from(ahead))
}

/// The first day of the week after this one.
fn next_week_start(today: NaiveDate, week_start: Weekday) -> NaiveDate {
    let back =
        (7 + today.weekday().num_days_from_monday() - week_start.num_days_from_monday()) % 7;
    today - Duration::days(i64::from(back)) + Duration::days(7)
}

/// `month`/`day` of `year`, or of the next year when that is past.
fn upcoming(today: NaiveDate, year: Option<i32>, month: u32, day: u32) -> Option<NaiveDate> {
    match year {
        Some(y) => NaiveDate::from_ymd_opt(y, month, day),
        None => {
            let this = NaiveDate::from_ymd_opt(today.year(), month, day);
            match this {
                Some(d) if d >= today => Some(d),
                _ => NaiveDate::from_ymd_opt(today.year() + 1, month, day),
            }
        }
    }
}

/// The next date (from today on) whose day of the month is `day`.
fn next_month_day(today: NaiveDate, day: u32) -> Option<NaiveDate> {
    (0..13).find_map(|m| {
        let first = recur::add_months(today.with_day(1)?, m, 1);
        NaiveDate::from_ymd_opt(first.year(), first.month(), day).filter(|d| *d >= today)
    })
}

/// `2026-10-02`, `2.10.`, `2.10.2026`, `02.10.26`.
fn numeric_date(word: &str, today: NaiveDate) -> Option<NaiveDate> {
    if let Ok(d) = NaiveDate::parse_from_str(word, "%Y-%m-%d") {
        return Some(d);
    }
    let parts: Vec<&str> = word.split('.').collect();
    let (day, month, year) = match parts.as_slice() {
        [d, m, ""] => (*d, *m, None),
        [d, m, y] if !y.is_empty() => (*d, *m, Some(*y)),
        _ => return None,
    };
    let all_digits = |s: &str| !s.is_empty() && s.len() <= 4 && s.chars().all(|c| c.is_ascii_digit());
    if !all_digits(day) || !all_digits(month) || day.len() > 2 || month.len() > 2 {
        return None;
    }
    let (day, month): (u32, u32) = (day.parse().ok()?, month.parse().ok()?);
    let year = match year {
        None => None,
        Some(y) if all_digits(y) && y.len() == 4 => Some(y.parse().ok()?),
        Some(y) if all_digits(y) && y.len() == 2 => Some(2000 + y.parse::<i32>().ok()?),
        Some(_) => return None,
    };
    upcoming(today, year, month, day)
}

/// A day that starts at word `i`: `(words used, day, a time it implies)`.
fn date_at(words: &Words<'_>, i: usize, ctx: &Context<'_>) -> Option<(usize, NaiveDate, Option<NaiveTime>)> {
    let today = ctx.now.date();
    let w = words.bare(i);
    let next = words.bare(i + 1);
    let plain = |d: NaiveDate| Some((1, d, None));
    match w {
        "today" | "tod" | "heute" => {
            if next == "abend" {
                return Some((2, today, NaiveTime::from_hms_opt(20, 0, 0)));
            }
            return plain(today);
        }
        "tonight" => return Some((1, today, NaiveTime::from_hms_opt(20, 0, 0))),
        "tomorrow" | "tmr" | "tmrw" | "morgen" => return plain(today + Duration::days(1)),
        "übermorgen" | "uebermorgen" => return plain(today + Duration::days(2)),
        "day" if next == "after" && words.bare(i + 2) == "tomorrow" => {
            return Some((3, today + Duration::days(2), None));
        }
        _ => {}
    }
    if let Some(day) = weekday_of(w) {
        return plain(next_weekday(today, day));
    }
    // "this friday", "this weekend", "next week", "nächste Woche".
    if matches!(w, "this" | "diesen" | "diese" | "dieses") {
        if let Some(day) = weekday_of(next) {
            return Some((2, this_weekday(today, day), None));
        }
        if matches!(next, "weekend" | "wochenende") {
            return Some((2, weekend(today), None));
        }
    }
    if matches!(
        w,
        "next" | "nächste" | "naechste" | "nächsten" | "naechsten" | "nächstes" | "naechstes"
    ) {
        match next {
            "week" | "woche" => return Some((2, next_week_start(today, ctx.week_start), None)),
            "month" | "monat" => {
                return Some((2, recur::add_months(today, 1, 1), None));
            }
            "year" | "jahr" => {
                return NaiveDate::from_ymd_opt(today.year() + 1, 1, 1).map(|d| (2, d, None));
            }
            "weekend" | "wochenende" => {
                return Some((2, weekend(today) + Duration::days(7), None));
            }
            _ => {}
        }
    }
    // "in 3 days", "in an hour", "in 2 Wochen".
    if w == "in" {
        if let (Some(n), Some(span)) = (count_of(next), span_of(words.bare(i + 2))) {
            let n64 = i64::from(n);
            return Some(match span {
                Span::Unit(Unit::Day) => (3, today + Duration::days(n64), None),
                Span::Unit(Unit::Week) => (3, today + Duration::days(7 * n64), None),
                Span::Unit(Unit::Month) => (
                    3,
                    recur::add_months(today, i32::try_from(n).unwrap_or(1), today.day()),
                    None,
                ),
                Span::Unit(Unit::Year) => (
                    3,
                    recur::ymd_clamped(today.year() + i32::try_from(n).unwrap_or(1), today.month(), today.day()),
                    None,
                ),
                Span::Hours | Span::Minutes => {
                    let minutes = if matches!(span, Span::Hours) { 60 * n64 } else { n64 };
                    let at = ctx.now + Duration::minutes(minutes);
                    (3, at.date(), NaiveTime::from_hms_opt(at.hour(), at.minute(), 0))
                }
            });
        }
    }
    // "the 1st", "the 15th".
    if w == "the" {
        if let Some(day) = ordinal_day(next) {
            if weekday_of(words.bare(i + 2)).is_none() && month_of(words.bare(i + 2)).is_none() {
                return next_month_day(today, day).map(|d| (2, d, None));
            }
        }
    }
    // "oct 2", "october 2nd 2027".
    if let Some(month) = month_of(w) {
        if let Some(day) = day_number(next) {
            let year = year_of(words.bare(i + 2));
            let len = if year.is_some() { 3 } else { 2 };
            return upcoming(today, year, month, day).map(|d| (len, d, None));
        }
    }
    // "2 oct", "2. Oktober 2027", "2nd of october".
    if let Some(day) = day_number(w) {
        let (month_at, used) = if next == "of" { (i + 2, 3) } else { (i + 1, 2) };
        if let Some(month) = month_of(words.bare(month_at)) {
            let year = year_of(words.bare(month_at + 1));
            let len = if year.is_some() { used + 1 } else { used };
            return upcoming(today, year, month, day).map(|d| (len, d, None));
        }
    }
    // "1st" alone.
    if let Some(day) = ordinal_day(w) {
        return next_month_day(today, day).map(|d| (1, d, None));
    }
    numeric_date(words.clean(i), today).map(|d| (1, d, None))
}

/// The coming Saturday (today on a Saturday or a Sunday).
fn weekend(today: NaiveDate) -> NaiveDate {
    match today.weekday() {
        Weekday::Sat | Weekday::Sun => today,
        _ => this_weekday(today, Weekday::Sat),
    }
}

// ==== Times ====

/// `9`, `9:30`, `15:00` as hour and minute (24-hour range).
fn clock(text: &str) -> Option<(u32, u32)> {
    let (h, m) = match text.split_once(':') {
        Some((h, m)) => (h, m),
        None => (text, "0"),
    };
    let digits = |s: &str| !s.is_empty() && s.len() <= 2 && s.chars().all(|c| c.is_ascii_digit());
    if !digits(h) || !digits(m) || (text.contains(':') && m.len() != 2) {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h <= 23 && m <= 59).then_some((h, m))
}

/// `9`, `9:30` with `am` / `pm`.
fn twelve_hour(text: &str, pm: bool) -> Option<NaiveTime> {
    let (h, m) = clock(&text.replace('.', ":"))?;
    if !(1..=12).contains(&h) {
        return None;
    }
    let h = match (h, pm) {
        (12, false) => 0,
        (12, true) => 12,
        (h, true) => h + 12,
        (h, false) => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

/// A time that starts at word `i`; `filler` is the word before it when that leads into it
/// ("at", "um"), which lets a bare hour count ("at 3" is 15:00, "um 9" is 09:00).
fn time_at(words: &Words<'_>, i: usize, filler: Option<&str>) -> Option<(usize, NaiveTime)> {
    let w = words.bare(i);
    let next = words.bare(i + 1);
    match w {
        "noon" | "midday" | "mittag" | "mittags" => return NaiveTime::from_hms_opt(12, 0, 0).map(|t| (1, t)),
        "midnight" | "mitternacht" => return NaiveTime::from_hms_opt(0, 0, 0).map(|t| (1, t)),
        _ => {}
    }
    // "9am", "9:30pm", "9.30pm", "9a.m".
    for (suffix, pm) in [("am", false), ("pm", true), ("a.m", false), ("p.m", true)] {
        if let Some(number) = w.strip_suffix(suffix) {
            if let Some(t) = twelve_hour(number, pm) {
                return Some((1, t));
            }
        }
    }
    let is_clock_word = clock(w).is_some();
    if is_clock_word {
        // "9 pm", "9 Uhr", "9:30 Uhr".
        match next {
            "am" | "a.m" => return twelve_hour(w, false).map(|t| (2, t)),
            "pm" | "p.m" => return twelve_hour(w, true).map(|t| (2, t)),
            "uhr" | "h" => {
                let (h, m) = clock(w)?;
                return NaiveTime::from_hms_opt(h, m, 0).map(|t| (2, t));
            }
            _ => {}
        }
        // "15:00": a clock time with minutes is a time on its own.
        if w.contains(':') {
            let (h, m) = clock(w)?;
            return NaiveTime::from_hms_opt(h, m, 0).map(|t| (1, t));
        }
        // A bare hour only after "at" / "um".
        let (h, m) = clock(w)?;
        return match filler {
            Some("at") => {
                let h = if (1..=7).contains(&h) { h + 12 } else { h };
                NaiveTime::from_hms_opt(h, m, 0).map(|t| (1, t))
            }
            Some("um") => NaiveTime::from_hms_opt(h, m, 0).map(|t| (1, t)),
            _ => None,
        };
    }
    None
}

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
