//! Date calculation: the difference between two dates, and a date plus or
//! minus years, months and days (proleptic Gregorian calendar).
//!
//! Months are calendar months: Jan 31 plus one month is Feb 28 (29 in a
//! leap year), as every calendar app does; the difference counts whole
//! years, then whole months, then the days left.

use std::fmt;

/// A calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Whether `year` has a February 29.
#[must_use]
pub fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Days in a month (1..=12).
#[must_use]
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

impl Date {
    /// A valid date, or `None` (Feb 30, month 13).
    #[must_use]
    pub fn new(year: i32, month: u32, day: u32) -> Option<Date> {
        ((1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month) && (1..=9999).contains(&year))
            .then_some(Date { year, month, day })
    }

    /// Days since 1970-01-01 (Howard Hinnant's days_from_civil).
    #[must_use]
    pub fn days(self) -> i64 {
        todo!("RED: days")
    }

    /// The date `days` after 1970-01-01.
    #[must_use]
    pub fn from_days(days: i64) -> Date {
        todo!("RED: from_days")
    }

    /// Monday = 0 .. Sunday = 6.
    #[must_use]
    pub fn weekday(self) -> u32 {
        // 1970-01-01 was a Thursday (3).
        (self.days() + 3).rem_euclid(7) as u32
    }

    /// The weekday's name.
    #[must_use]
    pub fn weekday_name(self) -> &'static str {
        ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
            [self.weekday() as usize]
    }

    /// This date plus `months` calendar months, the day clamped to the
    /// month's length (Jan 31 + 1 month = Feb 28 / 29). `None` outside 1..=9999.
    #[must_use]
    pub fn add_months(self, months: i64) -> Option<Date> {
        todo!("RED: add_months")
    }

    /// This date plus `days` days. `None` outside 1..=9999.
    #[must_use]
    pub fn add_days(self, days: i64) -> Option<Date> {
        let d = Date::from_days(self.days().checked_add(days)?);
        Date::new(d.year, d.month, d.day)
    }
}

/// `2026-10-01`, `2026/10/1`, `1.10.2026` (day first) or `10/01/2026`
/// is not accepted (ambiguous): ISO with `-` or `/`, or day.month.year.
#[must_use]
pub fn parse_date(text: &str) -> Option<Date> {
        todo!("RED: parse_date")
    }

/// The difference between two dates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Difference {
    pub years: i64,
    pub months: i64,
    pub days: i64,
    /// Whole days between them.
    pub total_days: i64,
}

impl Difference {
    /// `1 year, 2 months, 3 days`; `Same dates` for none.
    #[must_use]
    pub fn describe(&self) -> String {
        if self.total_days == 0 {
            return "Same dates".to_string();
        }
        let mut parts = Vec::new();
        for (n, one, many) in [
            (self.years, "year", "years"),
            (self.months, "month", "months"),
            (self.days, "day", "days"),
        ] {
            if n != 0 {
                parts.push(format!("{n} {}", if n == 1 { one } else { many }));
            }
        }
        parts.join(", ")
    }

    /// `61 weeks, 2 days` (or `3 weeks`, `5 days`).
    #[must_use]
    pub fn in_weeks(&self) -> String {
        let (w, d) = (self.total_days / 7, self.total_days % 7);
        let weeks = if w == 1 { "1 week".to_string() } else { format!("{w} weeks") };
        let days = if d == 1 { "1 day".to_string() } else { format!("{d} days") };
        match (w, d) {
            (0, _) => days,
            (_, 0) => weeks,
            _ => format!("{weeks}, {days}"),
        }
    }

    /// `429 days`.
    #[must_use]
    pub fn in_days(&self) -> String {
        if self.total_days == 1 {
            "1 day".to_string()
        } else {
            format!("{} days", grouped(self.total_days))
        }
    }
}

fn grouped(n: i64) -> String {
    crate::num::group_thousands(&n.to_string())
}

/// From the earlier to the later date (the order of the arguments does not
/// matter): whole years, whole months, the days left, and the total.
#[must_use]
pub fn difference(a: Date, b: Date) -> Difference {
        todo!("RED: difference")
    }

/// `date` plus (or minus, with `subtract`) years, months and days, in that order.
#[must_use]
pub fn add(date: Date, years: i64, months: i64, days: i64, subtract: bool) -> Option<Date> {
    let sign = if subtract { -1 } else { 1 };
    date.add_months(sign * (years * 12 + months))?.add_days(sign * days)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(text: &str) -> Date {
        parse_date(text).unwrap()
    }

    #[test]
    fn days_since_the_epoch_round_trip() {
        assert_eq!(d("1970-01-01").days(), 0);
        assert_eq!(d("2000-03-01").days(), 11_017);
        assert_eq!(d("1969-12-31").days(), -1);
        for n in [-800_000, -1, 0, 1, 59, 60, 11_016, 20_000, 2_900_000] {
            assert_eq!(Date::from_days(n).days(), n);
        }
    }

    #[test]
    fn leap_years_follow_the_gregorian_rules() {
        assert!(is_leap(2024) && is_leap(2000));
        assert!(!is_leap(1900) && !is_leap(2026));
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2026, 2), 28);
        assert!(Date::new(2026, 2, 29).is_none());
        assert!(Date::new(2024, 2, 29).is_some());
    }

    #[test]
    fn dates_parse_in_iso_and_day_first_form() {
        assert_eq!(d("2026-10-01"), Date { year: 2026, month: 10, day: 1 });
        assert_eq!(d("2026/10/1"), Date { year: 2026, month: 10, day: 1 });
        assert_eq!(d("14.03.1987"), Date { year: 1987, month: 3, day: 14 });
        assert!(parse_date("10/01/2026").is_none(), "month-first is ambiguous");
        assert!(parse_date("2026-13-01").is_none());
        assert!(parse_date("yesterday").is_none());
        assert_eq!(d("2026-10-01").to_string(), "2026-10-01");
    }

    #[test]
    fn the_weekday_of_a_date() {
        assert_eq!(d("2026-10-01").weekday_name(), "Thursday");
        assert_eq!(d("2000-01-01").weekday_name(), "Saturday");
        assert_eq!(d("1970-01-01").weekday(), 3);
    }

    #[test]
    fn a_month_later_clamps_to_the_months_length() {
        assert_eq!(d("2026-01-31").add_months(1), Some(d("2026-02-28")));
        assert_eq!(d("2024-01-31").add_months(1), Some(d("2024-02-29")));
        assert_eq!(d("2026-03-31").add_months(-1), Some(d("2026-02-28")));
        assert_eq!(d("2026-11-15").add_months(2), Some(d("2027-01-15")));
        assert_eq!(d("9999-12-31").add_days(1), None);
    }

    #[test]
    fn the_difference_counts_years_months_then_days() {
        let diff = difference(d("2025-08-01"), d("2026-10-04"));
        assert_eq!((diff.years, diff.months, diff.days), (1, 2, 3));
        assert_eq!(diff.describe(), "1 year, 2 months, 3 days");
        assert_eq!(diff.total_days, 429);
        assert_eq!(diff.in_days(), "429 days");
        assert_eq!(diff.in_weeks(), "61 weeks, 2 days");
        assert_eq!(difference(d("2026-10-04"), d("2025-08-01")), diff, "order does not matter");
    }

    #[test]
    fn the_difference_across_a_short_month() {
        let diff = difference(d("2026-01-31"), d("2026-03-01"));
        assert_eq!((diff.years, diff.months, diff.days), (0, 1, 1));
        assert_eq!(diff.total_days, 29);
        assert_eq!(difference(d("2026-10-01"), d("2026-10-01")).describe(), "Same dates");
        assert_eq!(difference(d("2026-10-01"), d("2026-10-08")).in_weeks(), "1 week");
        assert_eq!(difference(d("2026-10-01"), d("2026-10-02")).in_days(), "1 day");
    }

    #[test]
    fn adding_and_subtracting_years_months_and_days() {
        assert_eq!(add(d("2026-10-01"), 0, 0, 100, false), Some(d("2027-01-09")));
        assert_eq!(add(d("2026-10-01"), 1, 2, 3, false), Some(d("2027-12-04")));
        assert_eq!(add(d("2026-10-01"), 0, 0, 1, true), Some(d("2026-09-30")));
        assert_eq!(add(d("2024-02-29"), 1, 0, 0, false), Some(d("2025-02-28")));
    }
}
