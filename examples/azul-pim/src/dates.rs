//! Dates and month math: the helpers AzCalendar's repeat rules (`rrule.rs`), AzTasks' repeats
//! (`recur.rs`) and AzMail's date groups (`listing.rs`) each had a copy of
//! (scripts/DEDUP_EDITORS_2026_10_02.md, B5).
//!
//! Two month steps on purpose: an RRULE skips a day a month does not have ([`shift_month`] steps
//! whole months and the rule asks for its days), a to-do clamps it ([`add_months_clamped`]:
//! "monthly on the 31st" is the 28th or 29th in February and the 31st again in March).
//!
//! Names are English, as every app's labels are today; the localized names come later through
//! azul's ICU formatter (B7).

use chrono::{Datelike, Duration, NaiveDate, Weekday};

/// Every weekday, Monday first (ISO 8601).
pub const WEEKDAYS: [Weekday; 7] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
    Weekday::Sat,
    Weekday::Sun,
];

/// Monday to Friday.
pub const WORK_DAYS: [Weekday; 5] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
];

const MONTH_NAMES: [&str; 12] = [
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
];

/// Whether `year` has a 29 February (Gregorian: every 4th year, but not a century unless it
/// divides by 400).
#[must_use]
pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The days of `month` (1 to 12) in `year`; a month past 12 counts as December.
#[must_use]
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// `year-month-day`, the month clamped to 1..=12 and the day to the month's last day (31 in
/// February is the 28th or the 29th). `NaiveDate::MIN` past chrono's range.
#[must_use]
pub fn ymd_clamped(year: i32, month: u32, day: u32) -> NaiveDate {
    let month = month.clamp(1, 12);
    let day = day.clamp(1, days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or(NaiveDate::MIN)
}

/// `months` months after `date` (negative: before), on `day` of that month, clamped to its
/// length: the step of a to-do that repeats monthly.
#[must_use]
pub fn add_months_clamped(date: NaiveDate, months: i32, day: u32) -> NaiveDate {
    let index = date.year() * 12 + i32::try_from(date.month0()).unwrap_or(0) + months;
    let year = index.div_euclid(12);
    let month = u32::try_from(index.rem_euclid(12)).unwrap_or(0) + 1;
    ymd_clamped(year, month, day)
}

/// The `(year, month)` `months` months after `(year, month)` (negative: before); `None` outside
/// the years 1 to 9999. The step of an RRULE, which then asks the month for its days.
#[must_use]
pub fn shift_month(year: i32, month: u32, months: i64) -> Option<(i32, u32)> {
    let index = i64::from(year) * 12 + i64::from(month) - 1 + months;
    let year = i32::try_from(index.div_euclid(12)).ok()?;
    let month = u32::try_from(index.rem_euclid(12)).ok()? + 1;
    (1..=9999).contains(&year).then_some((year, month))
}

/// The first day of the week that holds `day`, for weeks starting on `week_start`.
#[must_use]
pub fn start_of_week(day: NaiveDate, week_start: Weekday) -> NaiveDate {
    day - Duration::days(i64::from(day.weekday().days_since(week_start)))
}

/// Which `nth` weekday of its month `day` is, counted from the start (1 to 5), and whether it is
/// also the month's last such weekday.
#[must_use]
pub fn nth_weekday_of_month(day: NaiveDate) -> (i8, bool) {
    let nth = i8::try_from((day.day() - 1) / 7 + 1).unwrap_or(1);
    let last = day.day() + 7 > days_in_month(day.year(), day.month());
    (nth, last)
}

/// "Monday".
#[must_use]
pub fn weekday_name(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

/// "Mon", as a row or a button shows a weekday.
#[must_use]
pub fn weekday_short(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

/// `mon`, `tue`, ... as a task or settings file writes a weekday.
#[must_use]
pub fn weekday_key(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    }
}

/// The weekday a file names (`mon` .. `sun`, exactly).
#[must_use]
pub fn weekday_from_key(key: &str) -> Option<Weekday> {
    WEEKDAYS.into_iter().find(|d| weekday_key(*d) == key)
}

/// `MO`, `TU`, ... as an RRULE writes a weekday (`BYDAY`, `WKST`).
#[must_use]
pub fn weekday_code(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Mon => "MO",
        Weekday::Tue => "TU",
        Weekday::Wed => "WE",
        Weekday::Thu => "TH",
        Weekday::Fri => "FR",
        Weekday::Sat => "SA",
        Weekday::Sun => "SU",
    }
}

/// The weekday an RRULE code names, in any case (`MO`, `we`).
#[must_use]
pub fn weekday_from_code(code: &str) -> Option<Weekday> {
    WEEKDAYS
        .into_iter()
        .find(|d| weekday_code(*d).eq_ignore_ascii_case(code))
}

/// "January" for 1 .. "December" for 12; "January" for anything else.
#[must_use]
pub fn month_name(month: u32) -> &'static str {
    MONTH_NAMES
        .get(month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or(MONTH_NAMES[0])
}

/// "first", "second", ... "fifth", "6th"; "last", "second to last", "3th to last": the words of
/// an RRULE's nth weekday ("the last Friday").
#[must_use]
pub fn ordinal_word(n: i32) -> String {
    match n {
        -1 => String::from("last"),
        -2 => String::from("second to last"),
        1 => String::from("first"),
        2 => String::from("second"),
        3 => String::from("third"),
        4 => String::from("fourth"),
        5 => String::from("fifth"),
        n if n < 0 => format!("{}th to last", -n),
        n => format!("{n}th"),
    }
}

/// `1st`, `2nd`, `3rd`, `4th`, `11th`, `21st`, `31st`: a day of the month.
#[must_use]
pub fn ordinal_suffix(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// "a", "a and b", "a, b and c".
#[must_use]
pub fn join_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn month_lengths_know_leap_years() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28, "a century is not a leap year");
        assert_eq!(days_in_month(2000, 2), 29, "unless it divides by 400");
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(
            days_in_month(2026, 13),
            31,
            "past December counts as December"
        );
        for (year, month) in [(2026, 1), (2026, 2), (2028, 2), (2026, 6), (2026, 12)] {
            let next = shift_month(year, month, 1).unwrap();
            let last = NaiveDate::from_ymd_opt(next.0, next.1, 1)
                .unwrap()
                .pred_opt()
                .unwrap();
            assert_eq!(days_in_month(year, month), last.day(), "{year}-{month}");
        }
    }

    #[test]
    fn a_clamped_month_step_keeps_the_day_where_the_month_has_it() {
        assert_eq!(
            add_months_clamped(day(2027, 1, 31), 1, 31),
            day(2027, 2, 28)
        );
        assert_eq!(
            add_months_clamped(day(2027, 2, 28), 1, 31),
            day(2027, 3, 31)
        );
        assert_eq!(
            add_months_clamped(day(2028, 1, 31), 1, 31),
            day(2028, 2, 29)
        );
        assert_eq!(
            add_months_clamped(day(2026, 12, 15), 1, 15),
            day(2027, 1, 15)
        );
        assert_eq!(
            add_months_clamped(day(2026, 1, 15), -1, 15),
            day(2025, 12, 15)
        );
        assert_eq!(ymd_clamped(2026, 2, 31), day(2026, 2, 28));
        assert_eq!(ymd_clamped(2026, 0, 0), day(2026, 1, 1));
    }

    #[test]
    fn a_strict_month_step_gives_the_month_and_stops_at_the_calendars_end() {
        assert_eq!(shift_month(2026, 11, 3), Some((2027, 2)));
        assert_eq!(shift_month(2026, 1, -1), Some((2025, 12)));
        assert_eq!(shift_month(2026, 1, 24), Some((2028, 1)));
        assert_eq!(shift_month(9999, 12, 1), None);
        assert_eq!(shift_month(1, 1, -1), None);
    }

    #[test]
    fn a_week_starts_on_the_day_asked_for() {
        // Thursday 1 October 2026
        assert_eq!(
            start_of_week(day(2026, 10, 1), Weekday::Mon),
            day(2026, 9, 28)
        );
        assert_eq!(
            start_of_week(day(2026, 10, 1), Weekday::Sun),
            day(2026, 9, 27)
        );
        assert_eq!(
            start_of_week(day(2026, 10, 1), Weekday::Sat),
            day(2026, 9, 26)
        );
        assert_eq!(
            start_of_week(day(2026, 9, 28), Weekday::Mon),
            day(2026, 9, 28)
        );
    }

    #[test]
    fn the_nth_weekday_of_a_day_and_whether_it_is_the_last() {
        assert_eq!(nth_weekday_of_month(day(2026, 10, 13)), (2, false));
        assert_eq!(nth_weekday_of_month(day(2026, 10, 30)), (5, true));
        assert_eq!(nth_weekday_of_month(day(2026, 10, 25)), (4, true));
        assert_eq!(ordinal_word(-1), "last");
        assert_eq!(ordinal_word(2), "second");
        assert_eq!(ordinal_word(-3), "3th to last");
    }

    #[test]
    fn ordinals_and_weekday_names() {
        let ords: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 31]
            .into_iter()
            .map(ordinal_suffix)
            .collect();
        assert_eq!(
            ords,
            vec![
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "23rd", "31st"
            ]
        );
        assert_eq!(weekday_from_key("wed"), Some(Weekday::Wed));
        assert_eq!(weekday_from_key("xyz"), None);
        assert_eq!(
            weekday_from_key("Wed"),
            None,
            "a file writes them in lower case"
        );
        assert_eq!(weekday_from_code("we"), Some(Weekday::Wed));
        assert_eq!(weekday_from_code("XX"), None);
        for d in WEEKDAYS {
            assert_eq!(weekday_from_key(weekday_key(d)), Some(d));
            assert_eq!(weekday_from_code(weekday_code(d)), Some(d));
            assert!(weekday_name(d).starts_with(weekday_short(d)));
        }
        assert_eq!(month_name(9), "September");
        assert_eq!(month_name(0), "January");
        assert_eq!(month_name(13), "January");
    }

    #[test]
    fn lists_join_with_commas_and_a_final_and() {
        let s = |items: &[&str]| join_and(&items.iter().map(|i| i.to_string()).collect::<Vec<_>>());
        assert_eq!(s(&[]), "");
        assert_eq!(s(&["a"]), "a");
        assert_eq!(s(&["a", "b"]), "a and b");
        assert_eq!(s(&["a", "b", "c"]), "a, b and c");
    }
}
