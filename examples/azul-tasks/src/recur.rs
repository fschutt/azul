//! Repeat rules: when a repeating task is completed, the next occurrence is a new task with
//! the next due date (`next_occurrence`), the way Microsoft To Do and Apple Reminders do it.
//!
//! A rule is "every N days / weeks / months / years", a week rule may name its days
//! ("weekly on Mon, Wed", "weekdays" = Mon to Fri), a month or year rule keeps the day of the
//! month it started on (`month_day`): "monthly on the 31st" is the 28th or 29th in February
//! and the 31st again in March, "yearly on 29 February" is the 28th in a common year and the
//! 29th in a leap year. A rule counts from the due date, or - `from_completion` - from the
//! day the task was completed ("water the plants 3 days after the last time").
//!
//! Weeks start on Monday for "every 2 weeks on ..." (ISO 8601); the week-start setting only
//! changes what the app shows, never which days a rule falls on.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

/// The step of a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    Day,
    Week,
    Month,
    Year,
}

impl Unit {
    /// Every unit, in the order the repeat editor lists them.
    pub const ALL: [Unit; 4] = [Unit::Day, Unit::Week, Unit::Month, Unit::Year];

    /// The unit's name in a task file: `day`, `week`, `month`, `year`.
    #[must_use]
    pub fn name(self) -> &'static str { todo!() }

    /// The unit a task file names.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Unit> { todo!() }

    /// "day" or "days", for "every 3 days".
    #[must_use]
    pub fn label(self, n: u32) -> String { todo!() }
}

/// The days Monday to Friday, the "weekdays" rule.
pub const WORK_DAYS: [Weekday; 5] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
];

/// A repeat rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repeat {
    /// Every how many units; at least 1.
    pub every: u32,
    pub unit: Unit,
    /// A week rule's days, Monday first, each once; empty = the due date's weekday.
    pub weekdays: Vec<Weekday>,
    /// A month or year rule's day of the month (1..=31), clamped to short months; `None` =
    /// the due date's day (set by [`Repeat::anchored`] when the rule meets its first date).
    pub month_day: Option<u32>,
    /// Count from the completion day instead of the due date.
    pub from_completion: bool,
}

impl Repeat {
    /// Every `every` (at least 1) `unit`s.
    #[must_use]
    pub fn new(every: u32, unit: Unit) -> Self { todo!() }

    #[must_use]
    pub fn daily() -> Self { todo!() }

    #[must_use]
    pub fn weekly() -> Self { todo!() }

    /// Monday to Friday.
    #[must_use]
    pub fn weekdays() -> Self { todo!() }

    #[must_use]
    pub fn monthly() -> Self { todo!() }

    #[must_use]
    pub fn yearly() -> Self { todo!() }

    /// A week rule on these days (sorted Monday first, each once).
    #[must_use]
    pub fn on_weekdays(mut self, days: &[Weekday]) -> Self { todo!() }

    /// A month or year rule on this day of the month (1..=31).
    #[must_use]
    pub fn on_month_day(mut self, day: u32) -> Self { todo!() }

    /// Count from the completion day.
    #[must_use]
    pub fn counting_from_completion(mut self, yes: bool) -> Self { todo!() }

    /// Whether this is the Monday-to-Friday rule.
    #[must_use]
    pub fn is_weekdays(&self) -> bool { todo!() }

    /// The rule with its day of the month fixed from `due` (month and year rules), so the
    /// occurrences after a short month come back to the day the rule started on.
    #[must_use]
    pub fn anchored(mut self, due: NaiveDate) -> Self { todo!() }

    /// The first occurrence strictly after `date` (which is taken to be an occurrence).
    #[must_use]
    pub fn next_after(&self, date: NaiveDate) -> NaiveDate { todo!() }

    /// The first occurrence on or after `date`: where a rule given without a date starts
    /// ("every monday" typed on a Thursday is due next Monday).
    #[must_use]
    pub fn first_on_or_after(&self, date: NaiveDate) -> NaiveDate { todo!() }

    /// The rule as the detail pane and the row show it: "Daily", "Every 3 days", "Weekdays",
    /// "Weekly on Mon, Wed", "Every 2 weeks", "Monthly on the 31st", "Yearly", "... after
    /// completion".
    #[must_use]
    pub fn label(&self) -> String { todo!() }
}

/// The due date of the task a completed repeating task spawns: the first occurrence after
/// the due date that is not in the past (today counts), or - for a rule counting from
/// completion - the first occurrence after the completion day.
#[must_use]
pub fn next_occurrence(
    repeat: &Repeat,
    due: NaiveDate,
    completed_on: NaiveDate,
    today: NaiveDate,
) -> NaiveDate { todo!() }

/// The days of `month` in `year`.
#[must_use]
pub fn days_in_month(year: i32, month: u32) -> u32 { todo!() }

/// `year-month-day`, the day clamped to the month's last day (31 in February is the 28th or
/// the 29th).
#[must_use]
pub fn ymd_clamped(year: i32, month: u32, day: u32) -> NaiveDate { todo!() }

/// `months` months after `date`, on `day` of that month (clamped to its length).
#[must_use]
pub fn add_months(date: NaiveDate, months: i32, day: u32) -> NaiveDate { todo!() }

/// `mon`, `tue`, ... as a task file writes a weekday.
#[must_use]
pub fn weekday_name(day: Weekday) -> &'static str { todo!() }

/// `Mon`, `Tue`, ... as the app shows a weekday.
#[must_use]
pub fn weekday_short(day: Weekday) -> &'static str { todo!() }

/// The weekday a task file names (`mon` .. `sun`).
#[must_use]
pub fn weekday_from_name(name: &str) -> Option<Weekday> { todo!() }

/// `1st`, `2nd`, `3rd`, `4th`, `11th`, `21st`, `31st`.
#[must_use]
pub fn ordinal(n: u32) -> String { todo!() }

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// The occurrences after `start`, `n` of them.
    fn walk(rule: &Repeat, start: NaiveDate, n: usize) -> Vec<NaiveDate> {
        let rule = rule.clone().anchored(start);
        let mut out = Vec::new();
        let mut at = start;
        for _ in 0..n {
            at = rule.next_after(at);
            out.push(at);
        }
        out
    }

    #[test]
    fn a_daily_rule_moves_one_day_and_every_three_days_moves_three() {
        assert_eq!(Repeat::daily().next_after(day(2026, 9, 30)), day(2026, 10, 1));
        assert_eq!(
            Repeat::new(3, Unit::Day).next_after(day(2026, 9, 30)),
            day(2026, 10, 3)
        );
        assert_eq!(Repeat::daily().next_after(day(2026, 12, 31)), day(2027, 1, 1));
    }

    #[test]
    fn a_weekly_rule_without_days_keeps_the_weekday() {
        assert_eq!(Repeat::weekly().next_after(day(2026, 10, 1)), day(2026, 10, 8));
        assert_eq!(
            Repeat::new(2, Unit::Week).next_after(day(2026, 10, 1)),
            day(2026, 10, 15)
        );
    }

    #[test]
    fn a_weekly_rule_on_monday_wednesday_friday_walks_the_days_then_wraps() {
        let rule = Repeat::weekly().on_weekdays(&[Weekday::Fri, Weekday::Mon, Weekday::Wed]);
        assert_eq!(rule.weekdays, vec![Weekday::Mon, Weekday::Wed, Weekday::Fri]);
        assert_eq!(
            walk(&rule, day(2026, 9, 30), 4),
            vec![day(2026, 10, 2), day(2026, 10, 5), day(2026, 10, 7), day(2026, 10, 9)]
        );
    }

    #[test]
    fn every_two_weeks_on_monday_and_wednesday_skips_a_week() {
        let rule = Repeat::new(2, Unit::Week).on_weekdays(&[Weekday::Mon, Weekday::Wed]);
        assert_eq!(
            walk(&rule, day(2026, 9, 28), 4),
            vec![day(2026, 9, 30), day(2026, 10, 12), day(2026, 10, 14), day(2026, 10, 26)]
        );
    }

    #[test]
    fn the_weekdays_rule_skips_the_weekend() {
        let rule = Repeat::weekdays();
        assert!(rule.is_weekdays());
        assert_eq!(rule.next_after(day(2026, 10, 1)), day(2026, 10, 2));
        assert_eq!(rule.next_after(day(2026, 10, 2)), day(2026, 10, 5), "Friday -> Monday");
    }

    #[test]
    fn monthly_on_the_31st_clamps_to_short_months_and_returns_to_the_31st() {
        assert_eq!(
            walk(&Repeat::monthly(), day(2027, 1, 31), 4),
            vec![day(2027, 2, 28), day(2027, 3, 31), day(2027, 4, 30), day(2027, 5, 31)]
        );
    }

    #[test]
    fn monthly_on_the_31st_lands_on_february_29_in_a_leap_year() {
        assert_eq!(
            walk(&Repeat::monthly(), day(2028, 1, 31), 2),
            vec![day(2028, 2, 29), day(2028, 3, 31)]
        );
        assert_eq!(
            walk(&Repeat::monthly(), day(2027, 12, 30), 3),
            vec![day(2028, 1, 30), day(2028, 2, 29), day(2028, 3, 30)]
        );
    }

    #[test]
    fn yearly_on_february_29_is_february_28_in_common_years_and_29_in_leap_years() {
        assert_eq!(
            walk(&Repeat::yearly(), day(2028, 2, 29), 4),
            vec![day(2029, 2, 28), day(2030, 2, 28), day(2031, 2, 28), day(2032, 2, 29)]
        );
    }

    #[test]
    fn every_four_years_from_a_leap_day_stays_on_the_leap_day() {
        assert_eq!(
            walk(&Repeat::new(4, Unit::Year), day(2028, 2, 29), 2),
            vec![day(2032, 2, 29), day(2036, 2, 29)]
        );
    }

    #[test]
    fn a_monthly_rule_in_december_rolls_into_january_of_the_next_year() {
        assert_eq!(Repeat::monthly().next_after(day(2026, 12, 15)), day(2027, 1, 15));
        assert_eq!(
            Repeat::new(3, Unit::Month).next_after(day(2026, 11, 30)),
            day(2027, 2, 28)
        );
    }

    #[test]
    fn an_overdue_repeat_spawns_the_first_occurrence_that_is_not_in_the_past() {
        let today = day(2026, 10, 1);
        assert_eq!(
            next_occurrence(&Repeat::daily(), day(2026, 9, 25), today, today),
            today,
            "a daily task six days overdue is due today again"
        );
        assert_eq!(
            next_occurrence(&Repeat::weekly(), day(2026, 9, 21), today, today),
            day(2026, 10, 5),
            "a Monday task is due next Monday"
        );
        assert_eq!(
            next_occurrence(&Repeat::daily(), today, today, today),
            day(2026, 10, 2),
            "completed on its day, the next one is tomorrow"
        );
    }

    #[test]
    fn a_rule_counting_from_completion_counts_from_the_completion_day() {
        let rule = Repeat::new(3, Unit::Day).counting_from_completion(true);
        assert_eq!(
            next_occurrence(&rule, day(2026, 9, 20), day(2026, 10, 1), day(2026, 10, 1)),
            day(2026, 10, 4)
        );
    }

    #[test]
    fn a_monthly_task_completed_late_keeps_its_day_of_the_month() {
        // Due 31 Jan, completed in March: Feb 28 is past, the next one is 31 March.
        assert_eq!(
            next_occurrence(&Repeat::monthly(), day(2027, 1, 31), day(2027, 3, 2), day(2027, 3, 2)),
            day(2027, 3, 31)
        );
    }

    #[test]
    fn a_rule_given_without_a_date_starts_on_its_first_day() {
        let thursday = day(2026, 10, 1);
        assert_eq!(
            Repeat::weekly()
                .on_weekdays(&[Weekday::Mon])
                .first_on_or_after(thursday),
            day(2026, 10, 5)
        );
        assert_eq!(
            Repeat::monthly().on_month_day(1).first_on_or_after(day(2026, 10, 2)),
            day(2026, 11, 1)
        );
        assert_eq!(
            Repeat::monthly().on_month_day(1).first_on_or_after(day(2026, 10, 1)),
            day(2026, 10, 1)
        );
        assert_eq!(Repeat::daily().first_on_or_after(thursday), thursday);
    }

    #[test]
    fn a_label_reads_like_a_sentence() {
        assert_eq!(Repeat::daily().label(), "Daily");
        assert_eq!(Repeat::new(3, Unit::Day).label(), "Every 3 days");
        assert_eq!(Repeat::weekdays().label(), "Weekdays");
        assert_eq!(
            Repeat::weekly()
                .on_weekdays(&[Weekday::Wed, Weekday::Mon])
                .label(),
            "Weekly on Mon, Wed"
        );
        assert_eq!(Repeat::new(2, Unit::Week).label(), "Every 2 weeks");
        assert_eq!(Repeat::monthly().on_month_day(31).label(), "Monthly on the 31st");
        assert_eq!(Repeat::yearly().label(), "Yearly");
        assert_eq!(
            Repeat::daily().counting_from_completion(true).label(),
            "Daily after completion"
        );
    }

    #[test]
    fn month_lengths_know_leap_years() {
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2028, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28, "a century is not a leap year");
        assert_eq!(days_in_month(2000, 2), 29, "unless it divides by 400");
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(days_in_month(2026, 4), 30);
    }

    #[test]
    fn ordinals_and_weekday_names() {
        let ords: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 23, 31]
            .into_iter()
            .map(ordinal)
            .collect();
        assert_eq!(
            ords,
            vec!["1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "23rd", "31st"]
        );
        assert_eq!(weekday_from_name("wed"), Some(Weekday::Wed));
        assert_eq!(weekday_from_name("xyz"), None);
        assert_eq!(Unit::from_name("month"), Some(Unit::Month));
        assert_eq!(Unit::Week.label(2), "weeks");
    }
}
