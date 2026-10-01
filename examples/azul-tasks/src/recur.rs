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
    pub fn name(self) -> &'static str {
        match self {
            Unit::Day => "day",
            Unit::Week => "week",
            Unit::Month => "month",
            Unit::Year => "year",
        }
    }

    /// The unit a task file names.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Unit> {
        Unit::ALL.into_iter().find(|u| u.name() == name)
    }

    /// "day" or "days", for "every 3 days".
    #[must_use]
    pub fn label(self, n: u32) -> String {
        if n == 1 {
            self.name().to_string()
        } else {
            format!("{}s", self.name())
        }
    }
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
    pub fn new(every: u32, unit: Unit) -> Self {
        Repeat {
            every: every.max(1),
            unit,
            weekdays: Vec::new(),
            month_day: None,
            from_completion: false,
        }
    }

    #[must_use]
    pub fn daily() -> Self {
        Repeat::new(1, Unit::Day)
    }

    #[must_use]
    pub fn weekly() -> Self {
        Repeat::new(1, Unit::Week)
    }

    /// Monday to Friday.
    #[must_use]
    pub fn weekdays() -> Self {
        Repeat::new(1, Unit::Week).on_weekdays(&WORK_DAYS)
    }

    #[must_use]
    pub fn monthly() -> Self {
        Repeat::new(1, Unit::Month)
    }

    #[must_use]
    pub fn yearly() -> Self {
        Repeat::new(1, Unit::Year)
    }

    /// A week rule on these days (sorted Monday first, each once).
    #[must_use]
    pub fn on_weekdays(mut self, days: &[Weekday]) -> Self {
        let mut days: Vec<Weekday> = days.to_vec();
        days.sort_by_key(|d| d.num_days_from_monday());
        days.dedup();
        self.weekdays = days;
        self
    }

    /// A month or year rule on this day of the month (1..=31).
    #[must_use]
    pub fn on_month_day(mut self, day: u32) -> Self {
        self.month_day = Some(day.clamp(1, 31));
        self
    }

    /// Count from the completion day.
    #[must_use]
    pub fn counting_from_completion(mut self, yes: bool) -> Self {
        self.from_completion = yes;
        self
    }

    /// Whether this is the Monday-to-Friday rule.
    #[must_use]
    pub fn is_weekdays(&self) -> bool {
        self.unit == Unit::Week && self.every == 1 && self.weekdays == WORK_DAYS
    }

    /// The rule with its day of the month fixed from `due` (month and year rules), so the
    /// occurrences after a short month come back to the day the rule started on.
    #[must_use]
    pub fn anchored(mut self, due: NaiveDate) -> Self {
        if matches!(self.unit, Unit::Month | Unit::Year) && self.month_day.is_none() {
            self.month_day = Some(due.day());
        }
        self
    }

    /// The first occurrence strictly after `date` (which is taken to be an occurrence).
    #[must_use]
    pub fn next_after(&self, date: NaiveDate) -> NaiveDate {
        let every = i64::from(self.every.max(1));
        match self.unit {
            Unit::Day => date + Duration::days(every),
            Unit::Week => {
                if self.weekdays.is_empty() {
                    return date + Duration::days(7 * every);
                }
                let today = date.weekday().num_days_from_monday();
                if let Some(later) = self
                    .weekdays
                    .iter()
                    .map(|d| d.num_days_from_monday())
                    .find(|&d| d > today)
                {
                    return date + Duration::days(i64::from(later - today));
                }
                let monday = date - Duration::days(i64::from(today));
                let first = self.weekdays[0].num_days_from_monday();
                monday + Duration::days(7 * every + i64::from(first))
            }
            Unit::Month => {
                let day = self.month_day.unwrap_or_else(|| date.day());
                add_months(date, i32::try_from(every).unwrap_or(1), day)
            }
            Unit::Year => {
                let day = self.month_day.unwrap_or_else(|| date.day());
                let year = date.year() + i32::try_from(every).unwrap_or(1);
                ymd_clamped(year, date.month(), day)
            }
        }
    }

    /// The first occurrence on or after `date`: where a rule given without a date starts
    /// ("every monday" typed on a Thursday is due next Monday).
    #[must_use]
    pub fn first_on_or_after(&self, date: NaiveDate) -> NaiveDate {
        match self.unit {
            Unit::Day | Unit::Year => date,
            Unit::Week => {
                if self.weekdays.is_empty() {
                    return date;
                }
                (0..7)
                    .map(|n| date + Duration::days(n))
                    .find(|d| self.weekdays.contains(&d.weekday()))
                    .unwrap_or(date)
            }
            Unit::Month => match self.month_day {
                None => date,
                Some(day) => {
                    let this = ymd_clamped(date.year(), date.month(), day);
                    if this >= date {
                        this
                    } else {
                        add_months(date, 1, day)
                    }
                }
            },
        }
    }

    /// The rule as the detail pane and the row show it: "Daily", "Every 3 days", "Weekdays",
    /// "Weekly on Mon, Wed", "Every 2 weeks", "Monthly on the 31st", "Yearly", "... after
    /// completion".
    #[must_use]
    pub fn label(&self) -> String {
        let mut text = if self.is_weekdays() {
            "Weekdays".to_string()
        } else if self.every == 1 {
            match self.unit {
                Unit::Day => "Daily",
                Unit::Week => "Weekly",
                Unit::Month => "Monthly",
                Unit::Year => "Yearly",
            }
            .to_string()
        } else {
            format!("Every {} {}", self.every, self.unit.label(self.every))
        };
        if self.unit == Unit::Week && !self.weekdays.is_empty() && !self.is_weekdays() {
            let days: Vec<&str> = self.weekdays.iter().map(|d| weekday_short(*d)).collect();
            text.push_str(" on ");
            text.push_str(&days.join(", "));
        }
        if self.unit == Unit::Month {
            if let Some(day) = self.month_day {
                text.push_str(" on the ");
                text.push_str(&ordinal(day));
            }
        }
        if self.from_completion {
            text.push_str(" after completion");
        }
        text
    }
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
) -> NaiveDate {
    let rule = repeat.clone().anchored(due);
    if rule.from_completion {
        return rule.next_after(completed_on);
    }
    let mut next = rule.next_after(due);
    // A daily rule ten years overdue is 3650 steps; stop long before anything loops forever.
    for _ in 0..100_000 {
        if next >= today {
            break;
        }
        next = rule.next_after(next);
    }
    next
}

/// The days of `month` in `year`.
#[must_use]
pub fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = if month >= 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .and_then(|first| first.pred_opt())
        .map_or(28, |last| last.day())
}

/// `year-month-day`, the day clamped to the month's last day (31 in February is the 28th or
/// the 29th).
#[must_use]
pub fn ymd_clamped(year: i32, month: u32, day: u32) -> NaiveDate {
    let month = month.clamp(1, 12);
    let day = day.clamp(1, days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or(NaiveDate::MIN)
}

/// `months` months after `date`, on `day` of that month (clamped to its length).
#[must_use]
pub fn add_months(date: NaiveDate, months: i32, day: u32) -> NaiveDate {
    let index = date.year() * 12 + i32::try_from(date.month0()).unwrap_or(0) + months;
    let year = index.div_euclid(12);
    let month = u32::try_from(index.rem_euclid(12)).unwrap_or(0) + 1;
    ymd_clamped(year, month, day)
}

/// `mon`, `tue`, ... as a task file writes a weekday.
#[must_use]
pub fn weekday_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    }
}

/// `Mon`, `Tue`, ... as the app shows a weekday.
#[must_use]
pub fn weekday_short(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    }
}

/// The weekday a task file names (`mon` .. `sun`).
#[must_use]
pub fn weekday_from_name(name: &str) -> Option<Weekday> {
    [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ]
    .into_iter()
    .find(|d| weekday_name(*d) == name)
}

/// `1st`, `2nd`, `3rd`, `4th`, `11th`, `21st`, `31st`.
#[must_use]
pub fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

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
