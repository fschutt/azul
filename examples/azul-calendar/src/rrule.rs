//! Repeat rules: the part of iCalendar's RRULE (RFC 5545, 3.3.10) that AzCalendar writes and
//! reads, and the dates a rule makes.
//!
//! An event file keeps its rule as RRULE text (`"repeat": "FREQ=WEEKLY;BYDAY=WE"`), so the file,
//! an .ics export and an .ics import all say the same thing. The subset:
//!
//! - `FREQ` = `DAILY`, `WEEKLY`, `MONTHLY` or `YEARLY`. There is no `SECONDLY` / `MINUTELY` /
//!   `HOURLY`: an event is on a day, at its times.
//! - `INTERVAL`, and `COUNT` or `UNTIL` (a date, or a date-time whose date is taken).
//! - `BYDAY`: weekdays (`MO,WE`) for any rule, or nth weekdays (`2TU`, `-1FR`) in a month
//!   (`MONTHLY`, or `YEARLY` with `BYMONTH`) or in a year (`YEARLY` alone).
//! - `BYMONTHDAY` (1 to 31, or -1 to -31 from the end), `BYMONTH` (1 to 12), `WKST`.
//!
//! Anything else (`BYSETPOS`, `BYWEEKNO`, `BYYEARDAY`, `BYHOUR`, ...) is [`RuleError::Unsupported`]:
//! an import keeps the event's first date and says why.
//!
//! The first date is the event's own day (`DTSTART`), always; the rule makes the ones after it.
//! `COUNT` counts that first date too. Exceptions (`EXDATE`) are taken out afterwards and do not
//! give a date back (RFC 5545, 3.8.5.1). A day a rule names that does not exist (the 31st in a
//! 30-day month, 29 February in a common year) is skipped, not moved.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

/// How often a rule repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// A weekday in `BYDAY`: every such weekday (`nth == 0`), or the nth one (`2TU`, `-1FR`) of the
/// month or the year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByDay {
    pub nth: i8,
    pub weekday: Weekday,
}

impl ByDay {
    /// Every `weekday`.
    #[must_use]
    pub const fn every(weekday: Weekday) -> ByDay {
        todo!()
    }

    /// The `nth` `weekday` (negative: from the end).
    #[must_use]
    pub const fn nth(nth: i8, weekday: Weekday) -> ByDay {
        todo!()
    }
}

/// When a rule stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RepeatEnd {
    Never,
    /// After this many dates, the first one included.
    Count(u32),
    /// On or before this day.
    Until(NaiveDate),
}

/// A repeat rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rule {
    pub freq: Freq,
    /// Every `interval` days / weeks / months / years (at least 1).
    pub interval: u32,
    pub end: RepeatEnd,
    pub by_day: Vec<ByDay>,
    pub by_month_day: Vec<i8>,
    pub by_month: Vec<u32>,
    /// The day a week starts on, for `WEEKLY` with an interval (`WKST`, Monday by default).
    pub week_start: Weekday,
}

/// Why a rule's text is not a rule this AzCalendar keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    /// There is no `FREQ`.
    NoFreq,
    /// A part that does not parse: `INTERVAL=0`, `BYDAY=XX`, `COUNT=many`.
    BadPart(String),
    /// A part this subset does not have: `FREQ=HOURLY`, `BYSETPOS=-1`.
    Unsupported(String),
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleError::NoFreq => write!(f, "the repeat rule has no FREQ"),
            RuleError::BadPart(part) => write!(f, "the repeat rule's {part:?} does not parse"),
            RuleError::Unsupported(part) => {
                write!(f, "the repeat rule's {part:?} is not supported")
            }
        }
    }
}

/// No rule looks further than this many of its periods (a daily rule from the year 1 asked
/// about the year 9999), so a nonsense rule cannot hang the view.
const MAX_PERIODS: i64 = 200_000;

const WEEKDAYS: [(&str, Weekday); 7] = [
    ("MO", Weekday::Mon),
    ("TU", Weekday::Tue),
    ("WE", Weekday::Wed),
    ("TH", Weekday::Thu),
    ("FR", Weekday::Fri),
    ("SA", Weekday::Sat),
    ("SU", Weekday::Sun),
];

fn weekday_code(weekday: Weekday) -> &'static str {
    WEEKDAYS
        .iter()
        .find(|(_, w)| *w == weekday)
        .map_or("MO", |(code, _)| code)
}

fn parse_weekday(code: &str) -> Option<Weekday> {
    WEEKDAYS
        .iter()
        .find(|(c, _)| c.eq_ignore_ascii_case(code))
        .map(|(_, w)| *w)
}

/// `2TU`, `-1FR`, `MO`.
fn parse_by_day(text: &str) -> Option<ByDay> {
    let text = text.trim();
    let split = text.len().checked_sub(2)?;
    let (number, code) = text.split_at(split);
    let weekday = parse_weekday(code)?;
    let nth = if number.is_empty() {
        0
    } else {
        let n: i8 = number.trim_start_matches('+').parse().ok()?;
        if n == 0 || !(-53..=53).contains(&n) {
            return None;
        }
        n
    };
    Some(ByDay { nth, weekday })
}

/// `20261231`, or the date of `20261231T235959` / `20261231T235959Z`.
pub(crate) fn parse_basic_date(text: &str) -> Option<NaiveDate> {
    todo!()
}

/// The first day of the week `weekday_start` begins that holds `day`.
fn start_of_week(day: NaiveDate, week_start: Weekday) -> NaiveDate {
    day - Duration::days(i64::from(day.weekday().days_since(week_start)))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|d| d.pred_opt())
        .map_or(28, |d| d.day())
}

/// `(year, month)` `months` months after `(year, month)`; `None` past chrono's range.
fn add_months(year: i32, month: u32, months: i64) -> Option<(i32, u32)> {
    let index = i64::from(year) * 12 + i64::from(month) - 1 + months;
    let year = i32::try_from(index.div_euclid(12)).ok()?;
    let month = u32::try_from(index.rem_euclid(12)).ok()? + 1;
    (1..=9999).contains(&year).then_some((year, month))
}

/// The days of `weekday` from `from` to `to` (inclusive), in order.
fn weekdays_between(from: NaiveDate, to: NaiveDate, weekday: Weekday) -> Vec<NaiveDate> {
    let mut day = from + Duration::days(i64::from(weekday.days_since(from.weekday())));
    let mut out = Vec::new();
    while day <= to {
        out.push(day);
        day += Duration::days(7);
    }
    out
}

/// The `nth` of `days` (negative: from the end), as BYDAY counts.
fn pick_nth(days: &[NaiveDate], nth: i8) -> Option<NaiveDate> {
    if nth > 0 {
        days.get(usize::try_from(nth - 1).ok()?).copied()
    } else {
        let back = usize::try_from(-i16::from(nth)).ok()?;
        days.len()
            .checked_sub(back)
            .and_then(|i| days.get(i))
            .copied()
    }
}

impl Rule {
    /// A rule repeating every `freq` forever, from its event's own day.
    #[must_use]
    pub fn new(freq: Freq) -> Rule {
        todo!()
    }

    /// Every `interval` periods.
    #[must_use]
    pub fn with_interval(mut self, interval: u32) -> Rule {
        todo!()
    }

    #[must_use]
    pub fn with_end(mut self, end: RepeatEnd) -> Rule {
        todo!()
    }

    #[must_use]
    pub fn with_by_day(mut self, by_day: Vec<ByDay>) -> Rule {
        todo!()
    }

    #[must_use]
    pub fn with_by_month_day(mut self, days: Vec<i8>) -> Rule {
        todo!()
    }

    #[must_use]
    pub fn with_by_month(mut self, months: Vec<u32>) -> Rule {
        todo!()
    }

    /// Reads RRULE text (`FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE`), with or without a leading
    /// `RRULE:`; part names are read in any case and order.
    pub fn parse(text: &str) -> Result<Rule, RuleError> {
        todo!()
    }

    /// The rule as RRULE text, its parts in one order: `FREQ`, `INTERVAL` (when not 1),
    /// `COUNT` / `UNTIL`, `BYMONTH`, `BYMONTHDAY`, `BYDAY`, `WKST` (when not Monday). `UNTIL` is
    /// a date for an all-day event and the end of that day (floating, as the event's own times
    /// are) for one with times, as RFC 5545 wants it to match `DTSTART`.
    #[must_use]
    pub fn to_rrule(&self, all_day: bool) -> String {
        todo!()
    }

    /// The first day of the `k`th period after the one `first` is in (each period `interval`
    /// days, weeks, months or years on); `None` past the calendar.
    fn period_start(&self, first: NaiveDate, k: i64) -> Option<NaiveDate> {
        let step = k.checked_mul(i64::from(self.interval.max(1)))?;
        match self.freq {
            Freq::Daily => first.checked_add_signed(Duration::try_days(step)?),
            Freq::Weekly => start_of_week(first, self.week_start)
                .checked_add_signed(Duration::try_days(step.checked_mul(7)?)?),
            Freq::Monthly => {
                let (y, m) = add_months(first.year(), first.month(), step)?;
                NaiveDate::from_ymd_opt(y, m, 1)
            }
            Freq::Yearly => {
                let year = i32::try_from(i64::from(first.year()).checked_add(step)?).ok()?;
                (1..=9999)
                    .contains(&year)
                    .then(|| NaiveDate::from_ymd_opt(year, 1, 1))
                    .flatten()
            }
        }
    }

    /// How many whole periods can be stepped over before `from` without missing a date in it.
    fn periods_before(&self, first: NaiveDate, from: NaiveDate) -> i64 {
        if from <= first {
            return 0;
        }
        let interval = i64::from(self.interval.max(1));
        let units = match self.freq {
            Freq::Daily => (from - first).num_days(),
            Freq::Weekly => {
                (start_of_week(from, self.week_start) - start_of_week(first, self.week_start))
                    .num_days()
                    / 7
            }
            Freq::Monthly => {
                (i64::from(from.year()) * 12 + i64::from(from.month()))
                    - (i64::from(first.year()) * 12 + i64::from(first.month()))
            }
            Freq::Yearly => i64::from(from.year() - first.year()),
        };
        (units / interval - 1).max(0)
    }

    /// The days of month `(year, month)` this rule names, in order.
    fn month_days(&self, first: NaiveDate, year: i32, month: u32) -> Vec<NaiveDate> {
        let dim = days_in_month(year, month);
        let mut days: Vec<NaiveDate> = if !self.by_month_day.is_empty() {
            self.by_month_day
                .iter()
                .filter_map(|&md| {
                    let day = if md > 0 {
                        u32::try_from(md).ok()?
                    } else {
                        dim.checked_add_signed(i32::from(md) + 1)?
                    };
                    if day == 0 || day > dim {
                        return None;
                    }
                    NaiveDate::from_ymd_opt(year, month, day)
                })
                .filter(|d| {
                    self.by_day.is_empty() || self.by_day.iter().any(|b| b.weekday == d.weekday())
                })
                .collect()
        } else if !self.by_day.is_empty() {
            let (Some(from), Some(to)) = (
                NaiveDate::from_ymd_opt(year, month, 1),
                NaiveDate::from_ymd_opt(year, month, dim),
            ) else {
                return Vec::new();
            };
            self.by_day
                .iter()
                .flat_map(|b| {
                    let all = weekdays_between(from, to, b.weekday);
                    match b.nth {
                        0 => all,
                        n => pick_nth(&all, n).into_iter().collect(),
                    }
                })
                .collect()
        } else {
            NaiveDate::from_ymd_opt(year, month, first.day())
                .into_iter()
                .collect()
        };
        days.sort();
        days.dedup();
        days
    }

    /// The days the `k`th period holds by this rule, in order.
    fn candidates(&self, first: NaiveDate, k: i64) -> Vec<NaiveDate> {
        let Some(start) = self.period_start(first, k) else {
            return Vec::new();
        };
        let in_months =
            |d: &NaiveDate| self.by_month.is_empty() || self.by_month.contains(&d.month());
        let mut days: Vec<NaiveDate> = match self.freq {
            Freq::Daily => {
                let day_ok = |d: &NaiveDate| {
                    (self.by_day.is_empty() || self.by_day.iter().any(|b| b.weekday == d.weekday()))
                        && (self.by_month_day.is_empty()
                            || self.month_days(first, d.year(), d.month()).contains(d))
                };
                Some(start).into_iter().filter(day_ok).collect()
            }
            Freq::Weekly => {
                let weekdays: Vec<Weekday> = if self.by_day.is_empty() {
                    vec![first.weekday()]
                } else {
                    self.by_day.iter().map(|b| b.weekday).collect()
                };
                weekdays
                    .into_iter()
                    .map(|w| start + Duration::days(i64::from(w.days_since(self.week_start))))
                    .collect()
            }
            Freq::Monthly => self.month_days(first, start.year(), start.month()),
            Freq::Yearly => {
                let year = start.year();
                if self.by_month.is_empty()
                    && self.by_month_day.is_empty()
                    && !self.by_day.is_empty()
                {
                    // nth weekdays of the whole year
                    let (Some(from), Some(to)) = (
                        NaiveDate::from_ymd_opt(year, 1, 1),
                        NaiveDate::from_ymd_opt(year, 12, 31),
                    ) else {
                        return Vec::new();
                    };
                    self.by_day
                        .iter()
                        .flat_map(|b| {
                            let all = weekdays_between(from, to, b.weekday);
                            match b.nth {
                                0 => all,
                                n => pick_nth(&all, n).into_iter().collect(),
                            }
                        })
                        .collect()
                } else {
                    let months = if self.by_month.is_empty() {
                        vec![first.month()]
                    } else {
                        self.by_month.clone()
                    };
                    months
                        .into_iter()
                        .flat_map(|m| self.month_days(first, year, m))
                        .collect()
                }
            }
        };
        days.retain(in_months);
        days.sort();
        days.dedup();
        days
    }

    /// The dates of an event that starts on `first` and repeats by this rule, from `from` to
    /// `to` (both included), without the `except` days, in order. `first` is always the first
    /// date.
    #[must_use]
    pub fn dates(
        &self,
        first: NaiveDate,
        except: &[NaiveDate],
        from: NaiveDate,
        to: NaiveDate,
    ) -> Vec<NaiveDate> {
        todo!()
    }

    /// Whether the rule makes any date after `first` at all (an `UNTIL` before the second date,
    /// or `COUNT=1`, makes none).
    #[must_use]
    pub fn repeats(&self, first: NaiveDate) -> bool {
        todo!()
    }

    /// What the rule says, for people: "Weekly on Wednesday", "Every 2 weeks on Monday and
    /// Friday, 10 times", "Monthly on the last Friday, until 31 December 2026".
    #[must_use]
    pub fn describe(&self, first: NaiveDate) -> String {
        todo!()
    }
}

/// Monday to Friday, each once, nothing else.
fn is_weekdays(days: &[ByDay]) -> bool {
    let mut weekdays: Vec<u32> = days
        .iter()
        .filter(|d| d.nth == 0)
        .map(|d| d.weekday.num_days_from_monday())
        .collect();
    weekdays.sort_unstable();
    days.len() == 5 && weekdays == [0, 1, 2, 3, 4]
}

/// "Monday"
#[must_use]
pub fn weekday_name(weekday: Weekday) -> &'static str {
    todo!()
}

fn month_name(month: u32) -> &'static str {
    [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ]
    .get(month.saturating_sub(1) as usize)
    .copied()
    .unwrap_or("January")
}

/// "first", "second", ... "fifth", "last", "second to last".
#[must_use]
pub fn ordinal(n: i32) -> String {
    todo!()
}

/// "a", "a and b", "a, b and c".
fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Which `nth` weekday of its month `day` is, counted from the start (1 to 5) - and -1 when it is
/// also the month's last such weekday.
#[must_use]
pub fn nth_weekday_of_month(day: NaiveDate) -> (i8, bool) {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use Weekday::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn dates_in(rule: &str, first: NaiveDate, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        Rule::parse(rule).unwrap().dates(first, &[], from, to)
    }

    #[test]
    fn a_weekly_rule_repeats_on_the_events_weekday() {
        // Wednesday 30 September 2026
        let got = dates_in(
            "FREQ=WEEKLY",
            d(2026, 9, 30),
            d(2026, 9, 28),
            d(2026, 10, 25),
        );
        assert_eq!(
            got,
            vec![
                d(2026, 9, 30),
                d(2026, 10, 7),
                d(2026, 10, 14),
                d(2026, 10, 21)
            ]
        );
    }

    #[test]
    fn nothing_comes_before_the_first_date() {
        let got = dates_in("FREQ=DAILY", d(2026, 9, 30), d(2026, 9, 1), d(2026, 10, 2));
        assert_eq!(got, vec![d(2026, 9, 30), d(2026, 10, 1), d(2026, 10, 2)]);
        assert!(dates_in("FREQ=DAILY", d(2026, 9, 30), d(2026, 9, 1), d(2026, 9, 29)).is_empty());
    }

    #[test]
    fn a_weekly_rule_on_several_days_and_an_interval_skips_whole_weeks() {
        // every second week on Monday and Thursday, from Thursday 1 October 2026
        let got = dates_in(
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TH",
            d(2026, 10, 1),
            d(2026, 9, 28),
            d(2026, 10, 31),
        );
        assert_eq!(
            got,
            vec![
                d(2026, 10, 1),
                d(2026, 10, 12),
                d(2026, 10, 15),
                d(2026, 10, 26),
                d(2026, 10, 29)
            ]
        );
    }

    #[test]
    fn every_weekday_skips_the_weekend() {
        let got = dates_in(
            "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR",
            d(2026, 10, 2),
            d(2026, 10, 1),
            d(2026, 10, 8),
        );
        assert_eq!(
            got,
            vec![
                d(2026, 10, 2),
                d(2026, 10, 5),
                d(2026, 10, 6),
                d(2026, 10, 7),
                d(2026, 10, 8)
            ]
        );
    }

    #[test]
    fn count_counts_the_first_date_and_until_includes_its_day() {
        let got = dates_in(
            "FREQ=DAILY;COUNT=3",
            d(2026, 9, 30),
            d(2026, 9, 1),
            d(2026, 12, 31),
        );
        assert_eq!(got, vec![d(2026, 9, 30), d(2026, 10, 1), d(2026, 10, 2)]);
        let got = dates_in(
            "FREQ=WEEKLY;UNTIL=20261014",
            d(2026, 9, 30),
            d(2026, 9, 1),
            d(2026, 12, 31),
        );
        assert_eq!(got, vec![d(2026, 9, 30), d(2026, 10, 7), d(2026, 10, 14)]);
        // a date-time UNTIL is read by its date
        let got = dates_in(
            "FREQ=WEEKLY;UNTIL=20261014T235959Z",
            d(2026, 9, 30),
            d(2026, 9, 1),
            d(2026, 12, 31),
        );
        assert_eq!(got.last(), Some(&d(2026, 10, 14)));
    }

    #[test]
    fn a_count_is_counted_from_the_first_date_even_when_the_view_starts_later() {
        // 5 dates: 30 Sep, 7, 14, 21, 28 Oct; a view of November shows none
        let rule = "FREQ=WEEKLY;COUNT=5";
        assert!(dates_in(rule, d(2026, 9, 30), d(2026, 11, 1), d(2026, 11, 30)).is_empty());
        assert_eq!(
            dates_in(rule, d(2026, 9, 30), d(2026, 10, 20), d(2026, 10, 31)),
            vec![d(2026, 10, 21), d(2026, 10, 28)]
        );
    }

    #[test]
    fn exceptions_are_taken_out_and_give_no_date_back() {
        let rule = Rule::parse("FREQ=WEEKLY;COUNT=3").unwrap();
        let got = rule.dates(
            d(2026, 9, 30),
            &[d(2026, 10, 7)],
            d(2026, 9, 1),
            d(2026, 12, 31),
        );
        assert_eq!(got, vec![d(2026, 9, 30), d(2026, 10, 14)]);
        // the first date can be an exception too
        let got = rule.dates(
            d(2026, 9, 30),
            &[d(2026, 9, 30)],
            d(2026, 9, 1),
            d(2026, 12, 31),
        );
        assert_eq!(got, vec![d(2026, 10, 7), d(2026, 10, 14)]);
    }

    #[test]
    fn a_monthly_rule_skips_months_without_the_day() {
        let got = dates_in(
            "FREQ=MONTHLY",
            d(2026, 1, 31),
            d(2026, 1, 1),
            d(2026, 6, 30),
        );
        assert_eq!(got, vec![d(2026, 1, 31), d(2026, 3, 31), d(2026, 5, 31)]);
    }

    #[test]
    fn a_monthly_rule_on_the_nth_or_last_weekday() {
        let got = dates_in(
            "FREQ=MONTHLY;BYDAY=2TU",
            d(2026, 10, 13),
            d(2026, 10, 1),
            d(2026, 12, 31),
        );
        assert_eq!(got, vec![d(2026, 10, 13), d(2026, 11, 10), d(2026, 12, 8)]);
        let got = dates_in(
            "FREQ=MONTHLY;BYDAY=-1FR",
            d(2026, 10, 30),
            d(2026, 10, 1),
            d(2027, 1, 31),
        );
        assert_eq!(
            got,
            vec![
                d(2026, 10, 30),
                d(2026, 11, 27),
                d(2026, 12, 25),
                d(2027, 1, 29)
            ]
        );
    }

    #[test]
    fn a_monthly_rule_on_month_days_counts_negative_days_from_the_end() {
        let got = dates_in(
            "FREQ=MONTHLY;BYMONTHDAY=1,-1",
            d(2026, 1, 1),
            d(2026, 1, 1),
            d(2026, 3, 1),
        );
        assert_eq!(
            got,
            vec![
                d(2026, 1, 1),
                d(2026, 1, 31),
                d(2026, 2, 1),
                d(2026, 2, 28),
                d(2026, 3, 1)
            ]
        );
    }

    #[test]
    fn a_yearly_rule_keeps_29_february_for_leap_years() {
        let got = dates_in(
            "FREQ=YEARLY",
            d(2024, 2, 29),
            d(2024, 1, 1),
            d(2032, 12, 31),
        );
        assert_eq!(got, vec![d(2024, 2, 29), d(2028, 2, 29), d(2032, 2, 29)]);
    }

    #[test]
    fn a_yearly_rule_by_month_and_nth_weekday_is_a_time_zone_change() {
        // the European summer time starts on the last Sunday of March
        let got = dates_in(
            "FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU",
            d(1996, 3, 31),
            d(2025, 1, 1),
            d(2027, 12, 31),
        );
        assert_eq!(got, vec![d(2025, 3, 30), d(2026, 3, 29), d(2027, 3, 28)]);
        // and the US one on the second Sunday of March
        let got = dates_in(
            "FREQ=YEARLY;BYMONTH=3;BYDAY=2SU",
            d(2007, 3, 11),
            d(2026, 1, 1),
            d(2026, 12, 31),
        );
        assert_eq!(got, vec![d(2026, 3, 8)]);
    }

    #[test]
    fn a_view_far_after_the_first_date_steps_over_the_periods_before_it() {
        let got = dates_in(
            "FREQ=DAILY;INTERVAL=3",
            d(2000, 1, 1),
            d(2026, 10, 1),
            d(2026, 10, 7),
        );
        // 2000-01-01 + 3k days: 9770 days to 2026-10-01 (9770 % 3 = 2), so 2 Oct and 5 Oct
        assert_eq!(got, vec![d(2026, 10, 2), d(2026, 10, 5)]);
        let got = dates_in(
            "FREQ=WEEKLY;INTERVAL=2",
            d(2026, 1, 7),
            d(2026, 9, 28),
            d(2026, 10, 11),
        );
        // every second Wednesday from 7 January: 30 Sep is week 38 (even), 14 Oct the next
        assert_eq!(got, vec![d(2026, 9, 30)]);
    }

    #[test]
    fn a_rule_reads_back_what_it_writes() {
        for text in [
            "FREQ=DAILY",
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TH",
            "FREQ=WEEKLY;COUNT=10;BYDAY=MO,TU,WE,TH,FR",
            "FREQ=MONTHLY;BYMONTHDAY=1,-1",
            "FREQ=MONTHLY;BYDAY=-1FR",
            "FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU",
            "FREQ=WEEKLY;BYDAY=SU;WKST=SU",
        ] {
            let rule = Rule::parse(text).unwrap();
            assert_eq!(rule.to_rrule(false), text, "{text}");
            assert_eq!(Rule::parse(&rule.to_rrule(false)), Ok(rule));
        }
        let until = Rule::new(Freq::Weekly).with_end(RepeatEnd::Until(d(2026, 12, 31)));
        assert_eq!(until.to_rrule(true), "FREQ=WEEKLY;UNTIL=20261231");
        assert_eq!(until.to_rrule(false), "FREQ=WEEKLY;UNTIL=20261231T235959");
        assert_eq!(Rule::parse(&until.to_rrule(false)), Ok(until.clone()));
        assert_eq!(Rule::parse(&until.to_rrule(true)), Ok(until));
    }

    #[test]
    fn a_rule_is_read_in_any_case_and_order_with_or_without_its_name() {
        let rule = Rule::parse("RRULE:byday=we;freq=weekly;interval=1").unwrap();
        assert_eq!(
            rule,
            Rule::new(Freq::Weekly).with_by_day(vec![ByDay::every(Wed)])
        );
    }

    #[test]
    fn a_rule_outside_the_subset_says_which_part() {
        assert_eq!(
            Rule::parse("FREQ=HOURLY"),
            Err(RuleError::Unsupported(String::from("FREQ=HOURLY")))
        );
        assert_eq!(
            Rule::parse("FREQ=MONTHLY;BYDAY=MO;BYSETPOS=-1"),
            Err(RuleError::Unsupported(String::from("BYSETPOS=-1")))
        );
        assert_eq!(Rule::parse("INTERVAL=2"), Err(RuleError::NoFreq));
        assert!(matches!(
            Rule::parse("FREQ=WEEKLY;INTERVAL=0"),
            Err(RuleError::BadPart(_))
        ));
        assert!(matches!(
            Rule::parse("FREQ=WEEKLY;BYDAY=XX"),
            Err(RuleError::BadPart(_))
        ));
        assert!(matches!(
            Rule::parse("FREQ=WEEKLY;COUNT=2;UNTIL=20261231"),
            Err(RuleError::BadPart(_))
        ));
        assert!(matches!(
            Rule::parse("FREQ=WEEKLY;BYDAY=2MO"),
            Err(RuleError::BadPart(_))
        ));
    }

    #[test]
    fn a_rule_says_what_it_does() {
        let first = d(2026, 9, 30);
        let said = |text: &str| Rule::parse(text).unwrap().describe(first);
        assert_eq!(said("FREQ=WEEKLY"), "Weekly on Wednesday");
        assert_eq!(said("FREQ=DAILY"), "Daily");
        assert_eq!(said("FREQ=DAILY;INTERVAL=3"), "Every 3 days");
        assert_eq!(said("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"), "Every weekday");
        assert_eq!(
            said("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,FR;COUNT=10"),
            "Every 2 weeks on Monday and Friday, 10 times"
        );
        assert_eq!(said("FREQ=MONTHLY"), "Monthly on day 30");
        assert_eq!(
            said("FREQ=MONTHLY;BYDAY=-1FR;UNTIL=20261231"),
            "Monthly on the last Friday, until 31 December 2026"
        );
        assert_eq!(said("FREQ=YEARLY"), "Yearly on 30 September");
    }

    #[test]
    fn a_rule_repeats_unless_it_ends_at_its_first_date() {
        let first = d(2026, 9, 30);
        assert!(Rule::new(Freq::Weekly).repeats(first));
        assert!(!Rule::new(Freq::Weekly)
            .with_end(RepeatEnd::Count(1))
            .repeats(first));
        assert!(!Rule::new(Freq::Weekly)
            .with_end(RepeatEnd::Until(d(2026, 10, 6)))
            .repeats(first));
    }

    #[test]
    fn the_nth_weekday_of_a_day_and_whether_it_is_the_last() {
        assert_eq!(nth_weekday_of_month(d(2026, 10, 13)), (2, false));
        assert_eq!(nth_weekday_of_month(d(2026, 10, 30)), (5, true));
        assert_eq!(nth_weekday_of_month(d(2026, 10, 25)), (4, true));
        assert_eq!(ordinal(-1), "last");
        assert_eq!(ordinal(2), "second");
    }
}
