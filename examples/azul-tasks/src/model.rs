//! The tasks, the lists and the settings, and their files: the one task store of the Azlin
//! apps, `azul_pim::task` (moved there from here so AzCalendar's and AzMail's To-Do bars read
//! and write the same files - scripts/DEDUP_EDITORS_2026_10_02.md, B12). Re-exported, so every
//! `crate::model::...` path of AzTasks stays; what is AzTasks' own (how a day is named in its
//! rows and headings) is here.

use azul_appkit::l10n::{date_text, t, DateStyle};
use chrono::{Datelike, NaiveDate};

pub use azul_pim::task::*;

/// A day as the app names it next to `today`: "Today", "Tomorrow", "Yesterday", else
/// "Fri 2 Oct" (with the year when it is not this year's: "Fri 1 Jan 2027").
#[must_use]
pub fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => t("kit-date-today"),
        1 => t("kit-date-tomorrow"),
        -1 => t("kit-date-yesterday"),
        _ if date.year() == today.year() => said(DateStyle::ShortDate, date),
        _ => said(DateStyle::ShortDateYear, date),
    }
}

/// `date` in `style`, in the window's language.
#[must_use]
pub fn said(style: DateStyle, date: NaiveDate) -> String {
    date_text(
        style,
        date.year(),
        date.month(),
        date.day(),
        date.weekday().num_days_from_monday(),
    )
}

/// A day as a section heading names it: "Today", "Tomorrow", else "Saturday 3 October"
/// (with the year when it is not this year's).
#[must_use]
pub fn day_heading(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => t("kit-date-today"),
        1 => t("kit-date-tomorrow"),
        -1 => t("kit-date-yesterday"),
        _ if date.year() == today.year() => said(DateStyle::WeekdayDayMonth, date),
        _ => said(DateStyle::WeekdayDate, date),
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
