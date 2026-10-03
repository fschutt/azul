//! The tasks, the lists and the settings, and their files: the one task store of the Azlin
//! apps, `azul_pim::task` (moved there from here so AzCalendar's and AzMail's To-Do bars read
//! and write the same files - scripts/DEDUP_EDITORS_2026_10_02.md, B12). Re-exported, so every
//! `crate::model::...` path of AzTasks stays; what is AzTasks' own (how a day is named in its
//! rows and headings) is here.

use chrono::{Datelike, NaiveDate};

pub use azul_pim::task::*;

/// A day as the app names it next to `today`: "Today", "Tomorrow", "Yesterday", else
/// "Fri 2 Oct" (with the year when it is not this year's: "Fri 1 Jan 2027").
#[must_use]
pub fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        _ if date.year() == today.year() => date.format("%a %-d %b").to_string(),
        _ => date.format("%a %-d %b %Y").to_string(),
    }
}

/// A day as a section heading names it: "Today", "Tomorrow", else "Saturday 3 October"
/// (with the year when it is not this year's).
#[must_use]
pub fn day_heading(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => "Today".to_string(),
        1 => "Tomorrow".to_string(),
        -1 => "Yesterday".to_string(),
        _ if date.year() == today.year() => date.format("%A %-d %B").to_string(),
        _ => date.format("%A %-d %B %Y").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn days_are_named_from_today() {
        let today = day(2026, 10, 1);
        assert_eq!(day_label(today, today), "Today");
        assert_eq!(day_label(day(2026, 10, 2), today), "Tomorrow");
        assert_eq!(day_label(day(2026, 9, 30), today), "Yesterday");
        assert_eq!(day_label(day(2026, 10, 9), today), "Fri 9 Oct");
        assert_eq!(day_label(day(2027, 1, 1), today), "Fri 1 Jan 2027");
        assert_eq!(day_heading(day(2026, 10, 3), today), "Saturday 3 October");
    }
}
