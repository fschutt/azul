//! A to-do's repeat: when a repeating task is completed, the next occurrence is a new task with
//! the next due date (`next_occurrence`), the way Microsoft To Do and Apple Reminders do it.
//! Moved here from AzTasks' `recur.rs` (scripts/DEDUP_EDITORS_2026_10_02.md, B6).
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

pub use crate::dates::WORK_DAYS;
use crate::dates::{
    add_months_clamped, days_in_month, ordinal_suffix, weekday_short, weekday_short_message_id,
    ymd_clamped,
};
use crate::rrule::{ByDay, Freq, RepeatEnd, Rule};
use crate::said::{Said, SaidArg};

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
                add_months_clamped(date, i32::try_from(every).unwrap_or(1), day)
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
                        add_months_clamped(date, 1, day)
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
                text.push_str(&ordinal_suffix(day));
            }
        }
        if self.from_completion {
            text.push_str(" after completion");
        }
        text
    }

    /// What the rule does as messages of azul-appkit's resources an app says in the window's
    /// language (`l10n::t_said`): "Weekdays", "Every 3 days", "Weekly on Mon and Wed", "Monthly
    /// on the 31st", "... after completion" ([`Repeat::label`]'s English is for logs).
    #[must_use]
    pub fn description(&self) -> Said {
        let every = Said::new(match self.unit {
            Unit::Day => "kit-rule-daily",
            Unit::Week => "kit-rule-weekly",
            Unit::Month => "kit-rule-monthly",
            Unit::Year => "kit-rule-yearly",
        })
        .arg("n", SaidArg::Number(i64::from(self.every.max(1))));
        let on = |every: Said, on: SaidArg| {
            Said::new("kit-rule-on")
                .arg("every", SaidArg::Said(every))
                .arg("on", on)
        };
        let rule = if self.is_weekdays() {
            Said::new("kit-repeat-weekdays")
        } else if self.unit == Unit::Week && !self.weekdays.is_empty() {
            let days = self
                .weekdays
                .iter()
                .map(|d| Said::new(weekday_short_message_id(*d)))
                .collect();
            on(every, SaidArg::List(days))
        } else if let (Unit::Month, Some(day)) = (self.unit, self.month_day) {
            let day = Said::new("kit-repeat-month-day").arg("day", SaidArg::Number(i64::from(day)));
            on(every, SaidArg::Said(day))
        } else {
            every
        };
        if self.from_completion {
            Said::new("kit-repeat-after-completion").arg("rule", SaidArg::Said(rule))
        } else {
            rule
        }
    }
}

impl Repeat {
    /// The iCalendar rule (RRULE) that makes the same dates from `due` as this repeat - what a
    /// VTODO or an event carries. `None` where RRULE cannot say it: a repeat counting from the
    /// completion, a month repeat on the 29th or 30th (a to-do clamps it to February's last day,
    /// RRULE skips February), or a `due` date that is not one of the repeat's own days. A month
    /// repeat on the 31st is every month's last day (`BYMONTHDAY=-1`), a year repeat on a day
    /// its month does not always have (29 February) that month's last day.
    #[must_use]
    pub fn to_rule(&self, due: NaiveDate) -> Option<Rule> {
        if self.from_completion {
            return None;
        }
        let repeat = self.clone().anchored(due);
        let every = repeat.every.max(1);
        let rule = match repeat.unit {
            Unit::Day => Rule::new(Freq::Daily).with_interval(every),
            Unit::Week => Rule::new(Freq::Weekly)
                .with_interval(every)
                .with_by_day(repeat.weekdays.iter().copied().map(ByDay::every).collect()),
            Unit::Month => {
                let day = repeat.month_day.unwrap_or_else(|| due.day());
                if ymd_clamped(due.year(), due.month(), day) != due {
                    return None;
                }
                let by: i8 = match day {
                    1..=28 => i8::try_from(day).ok()?,
                    31 => -1,
                    _ => return None,
                };
                Rule::new(Freq::Monthly)
                    .with_interval(every)
                    .with_by_month_day(vec![by])
            }
            Unit::Year => {
                let day = repeat.month_day.unwrap_or_else(|| due.day());
                if ymd_clamped(due.year(), due.month(), day) != due {
                    return None;
                }
                // The fewest days the month has in any year (February: 28).
                let shortest = if due.month() == 2 {
                    28
                } else {
                    days_in_month(due.year(), due.month())
                };
                let by: i8 = if day <= shortest {
                    i8::try_from(day).ok()?
                } else {
                    -1
                };
                Rule::new(Freq::Yearly)
                    .with_interval(every)
                    .with_by_month(vec![due.month()])
                    .with_by_month_day(vec![by])
            }
        };
        Some(rule)
    }

    /// The repeat that makes the same dates from `first` as `rule` - an imported VTODO's or an
    /// event's. `None` for a rule a repeat cannot hold: one that ends (`COUNT`, `UNTIL`), nth
    /// weekdays, several months or month days, weeks that do not start on Monday, a day the
    /// rule skips in short months or common years (where a repeat clamps), or a `first` date
    /// that is not one of the rule's own days.
    #[must_use]
    pub fn from_rule(rule: &Rule, first: NaiveDate) -> Option<Repeat> {
        if rule.end != RepeatEnd::Never || rule.by_day.iter().any(|d| d.nth != 0) {
            return None;
        }
        let every = rule.interval.max(1);
        let days: Vec<Weekday> = rule.by_day.iter().map(|d| d.weekday).collect();
        let no_month_parts = rule.by_month_day.is_empty() && rule.by_month.is_empty();
        let is_last_day = first.day() == days_in_month(first.year(), first.month());
        // The day of the month a month or year rule names, from `first`: `None` when `first`
        // is not on it.
        let month_day = || -> Option<u32> {
            match rule.by_month_day.as_slice() {
                [] => Some(first.day()),
                [-1] => is_last_day.then_some(first.day()),
                [d] => (u32::try_from(*d).ok()? == first.day()).then_some(first.day()),
                _ => None,
            }
        };
        match rule.freq {
            Freq::Daily if no_month_parts => {
                if days.is_empty() {
                    Some(Repeat::new(every, Unit::Day))
                } else if every == 1 {
                    Some(Repeat::new(1, Unit::Week).on_weekdays(&days))
                } else {
                    None
                }
            }
            Freq::Weekly if no_month_parts && (every == 1 || rule.week_start == Weekday::Mon) => {
                Some(Repeat::new(every, Unit::Week).on_weekdays(&days))
            }
            Freq::Monthly if days.is_empty() && rule.by_month.is_empty() => {
                let day = month_day()?;
                if matches!(rule.by_month_day.as_slice(), [-1]) {
                    // Every month's last day: the 31st, clamped.
                    Some(Repeat::new(every, Unit::Month).on_month_day(31))
                } else if day <= 28 {
                    Some(Repeat::new(every, Unit::Month).on_month_day(day))
                } else {
                    None
                }
            }
            Freq::Yearly if days.is_empty() => {
                if !matches!(rule.by_month.as_slice(), [] | [_]) {
                    return None;
                }
                if rule.by_month.first().is_some_and(|&m| m != first.month()) {
                    return None;
                }
                let day = month_day()?;
                let last_day = matches!(rule.by_month_day.as_slice(), [-1]);
                // 29 February named by its number is skipped in common years.
                if first.month() == 2 && day == 29 && !last_day {
                    return None;
                }
                Some(Repeat::new(every, Unit::Year).on_month_day(day))
            }
            _ => None,
        }
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
        assert_eq!(
            Repeat::daily().next_after(day(2026, 9, 30)),
            day(2026, 10, 1)
        );
        assert_eq!(
            Repeat::new(3, Unit::Day).next_after(day(2026, 9, 30)),
            day(2026, 10, 3)
        );
        assert_eq!(
            Repeat::daily().next_after(day(2026, 12, 31)),
            day(2027, 1, 1)
        );
    }

    #[test]
    fn a_weekly_rule_without_days_keeps_the_weekday() {
        assert_eq!(
            Repeat::weekly().next_after(day(2026, 10, 1)),
            day(2026, 10, 8)
        );
        assert_eq!(
            Repeat::new(2, Unit::Week).next_after(day(2026, 10, 1)),
            day(2026, 10, 15)
        );
    }

    #[test]
    fn a_weekly_rule_on_monday_wednesday_friday_walks_the_days_then_wraps() {
        let rule = Repeat::weekly().on_weekdays(&[Weekday::Fri, Weekday::Mon, Weekday::Wed]);
        assert_eq!(
            rule.weekdays,
            vec![Weekday::Mon, Weekday::Wed, Weekday::Fri]
        );
        assert_eq!(
            walk(&rule, day(2026, 9, 30), 4),
            vec![
                day(2026, 10, 2),
                day(2026, 10, 5),
                day(2026, 10, 7),
                day(2026, 10, 9)
            ]
        );
    }

    #[test]
    fn every_two_weeks_on_monday_and_wednesday_skips_a_week() {
        let rule = Repeat::new(2, Unit::Week).on_weekdays(&[Weekday::Mon, Weekday::Wed]);
        assert_eq!(
            walk(&rule, day(2026, 9, 28), 4),
            vec![
                day(2026, 9, 30),
                day(2026, 10, 12),
                day(2026, 10, 14),
                day(2026, 10, 26)
            ]
        );
    }

    #[test]
    fn the_weekdays_rule_skips_the_weekend() {
        let rule = Repeat::weekdays();
        assert!(rule.is_weekdays());
        assert_eq!(rule.next_after(day(2026, 10, 1)), day(2026, 10, 2));
        assert_eq!(
            rule.next_after(day(2026, 10, 2)),
            day(2026, 10, 5),
            "Friday -> Monday"
        );
    }

    #[test]
    fn monthly_on_the_31st_clamps_to_short_months_and_returns_to_the_31st() {
        assert_eq!(
            walk(&Repeat::monthly(), day(2027, 1, 31), 4),
            vec![
                day(2027, 2, 28),
                day(2027, 3, 31),
                day(2027, 4, 30),
                day(2027, 5, 31)
            ]
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
            vec![
                day(2029, 2, 28),
                day(2030, 2, 28),
                day(2031, 2, 28),
                day(2032, 2, 29)
            ]
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
        assert_eq!(
            Repeat::monthly().next_after(day(2026, 12, 15)),
            day(2027, 1, 15)
        );
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
            next_occurrence(
                &Repeat::monthly(),
                day(2027, 1, 31),
                day(2027, 3, 2),
                day(2027, 3, 2)
            ),
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
            Repeat::monthly()
                .on_month_day(1)
                .first_on_or_after(day(2026, 10, 2)),
            day(2026, 11, 1)
        );
        assert_eq!(
            Repeat::monthly()
                .on_month_day(1)
                .first_on_or_after(day(2026, 10, 1)),
            day(2026, 10, 1)
        );
        assert_eq!(Repeat::daily().first_on_or_after(thursday), thursday);
    }

    /// What a repeat does, as messages of azul-appkit's resources (azul-appkit's
    /// `l10n_switch_tests` say them in English and German).
    #[test]
    fn a_repeat_says_what_it_does_as_messages() {
        use crate::said::{Said, SaidArg};
        let every = |id: &'static str, n: i64| Said::new(id).arg("n", SaidArg::Number(n));
        assert_eq!(Repeat::daily().description(), every("kit-rule-daily", 1));
        assert_eq!(
            Repeat::new(3, Unit::Day).description(),
            every("kit-rule-daily", 3)
        );
        assert_eq!(
            Repeat::weekdays().description(),
            Said::new("kit-repeat-weekdays")
        );
        assert_eq!(
            Repeat::weekly()
                .on_weekdays(&[Weekday::Wed, Weekday::Mon])
                .description(),
            Said::new("kit-rule-on")
                .arg("every", SaidArg::Said(every("kit-rule-weekly", 1)))
                .arg(
                    "on",
                    SaidArg::List(vec![
                        Said::new("kit-weekday-short-mon"),
                        Said::new("kit-weekday-short-wed"),
                    ])
                )
        );
        assert_eq!(
            Repeat::monthly().on_month_day(31).description(),
            Said::new("kit-rule-on")
                .arg("every", SaidArg::Said(every("kit-rule-monthly", 1)))
                .arg(
                    "on",
                    SaidArg::Said(
                        Said::new("kit-repeat-month-day").arg("day", SaidArg::Number(31))
                    )
                )
        );
        assert_eq!(
            Repeat::daily().counting_from_completion(true).description(),
            Said::new("kit-repeat-after-completion")
                .arg("rule", SaidArg::Said(every("kit-rule-daily", 1)))
        );
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
        assert_eq!(
            Repeat::monthly().on_month_day(31).label(),
            "Monthly on the 31st"
        );
        assert_eq!(Repeat::yearly().label(), "Yearly");
        assert_eq!(
            Repeat::daily().counting_from_completion(true).label(),
            "Daily after completion"
        );
    }

    #[test]
    fn units_have_stable_names_and_plural_labels() {
        for unit in Unit::ALL {
            assert_eq!(Unit::from_name(unit.name()), Some(unit));
        }
        assert_eq!(Unit::from_name("month"), Some(Unit::Month));
        assert_eq!(Unit::from_name("fortnight"), None);
        assert_eq!(Unit::Week.label(2), "weeks");
        assert_eq!(Unit::Day.label(1), "day");
    }

    /// `due` and the occurrences after it up to `until`, by walking the repeat.
    fn walk_until(rule: &Repeat, due: NaiveDate, until: NaiveDate) -> Vec<NaiveDate> {
        let rule = rule.clone().anchored(due);
        let mut out = vec![due];
        let mut at = due;
        loop {
            at = rule.next_after(at);
            if at > until {
                return out;
            }
            out.push(at);
        }
    }

    #[test]
    fn a_repeat_is_the_rrule_that_makes_the_same_dates() {
        // DEDUP_EDITORS B6: a task's repeat as RRULE (VTODO, the calendar's To-Do bar).
        let thursday = day(2026, 10, 1);
        let text = |r: &Repeat, due: NaiveDate| r.to_rule(due).map(|rule| rule.to_rrule(true));
        let cases: Vec<(Repeat, NaiveDate, &str)> = vec![
            (Repeat::daily(), thursday, "FREQ=DAILY"),
            (Repeat::new(3, Unit::Day), thursday, "FREQ=DAILY;INTERVAL=3"),
            (Repeat::weekly(), thursday, "FREQ=WEEKLY"),
            (
                Repeat::weekdays(),
                thursday,
                "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR",
            ),
            (
                Repeat::new(2, Unit::Week).on_weekdays(&[Weekday::Mon, Weekday::Wed]),
                thursday,
                "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE",
            ),
            (Repeat::monthly(), thursday, "FREQ=MONTHLY;BYMONTHDAY=1"),
            (
                Repeat::monthly().on_month_day(31),
                day(2027, 1, 31),
                "FREQ=MONTHLY;BYMONTHDAY=-1",
            ),
            (
                Repeat::monthly().on_month_day(31),
                day(2027, 2, 28),
                "FREQ=MONTHLY;BYMONTHDAY=-1",
            ),
            (
                Repeat::yearly(),
                thursday,
                "FREQ=YEARLY;BYMONTH=10;BYMONTHDAY=1",
            ),
            (
                Repeat::yearly(),
                day(2028, 2, 29),
                "FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=-1",
            ),
        ];
        for (repeat, due, rrule) in cases {
            assert_eq!(
                text(&repeat, due).as_deref(),
                Some(rrule),
                "{repeat:?} from {due}"
            );
            let until = due + Duration::days(3 * 366);
            let rule = repeat.to_rule(due).unwrap();
            assert_eq!(
                rule.dates(due, &[], due, until),
                walk_until(&repeat, due, until),
                "{rrule} makes the repeat's dates"
            );
        }
    }

    #[test]
    fn a_repeat_an_rrule_cannot_say_has_no_rule() {
        let due = day(2026, 10, 1);
        assert_eq!(
            Repeat::daily().counting_from_completion(true).to_rule(due),
            None,
            "RRULE has no 'after completion'"
        );
        assert_eq!(
            Repeat::monthly().on_month_day(30).to_rule(day(2026, 9, 30)),
            None,
            "the 30th clamps to the 28th in February, RRULE skips February"
        );
        assert_eq!(
            Repeat::monthly().on_month_day(15).to_rule(due),
            None,
            "a due date off the repeat's own day"
        );
    }

    #[test]
    fn an_rrule_a_repeat_can_hold_comes_back_as_that_repeat() {
        let thursday = day(2026, 10, 1);
        let read =
            |text: &str, first: NaiveDate| Repeat::from_rule(&Rule::parse(text).unwrap(), first);
        assert_eq!(
            read("FREQ=DAILY;INTERVAL=3", thursday),
            Some(Repeat::new(3, Unit::Day))
        );
        assert_eq!(
            read("FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR", thursday),
            Some(Repeat::weekdays())
        );
        assert_eq!(
            read("FREQ=WEEKLY;INTERVAL=2;BYDAY=WE,MO", thursday),
            Some(Repeat::new(2, Unit::Week).on_weekdays(&[Weekday::Mon, Weekday::Wed]))
        );
        assert_eq!(
            read("FREQ=MONTHLY", thursday),
            Some(Repeat::monthly().on_month_day(1))
        );
        assert_eq!(
            read("FREQ=MONTHLY;BYMONTHDAY=-1", day(2026, 10, 31)),
            Some(Repeat::monthly().on_month_day(31))
        );
        assert_eq!(
            read("FREQ=YEARLY", thursday),
            Some(Repeat::yearly().on_month_day(1))
        );
        for (repeat, due) in [
            (Repeat::daily(), thursday),
            (Repeat::weekdays(), thursday),
            (
                Repeat::new(2, Unit::Week).on_weekdays(&[Weekday::Fri]),
                thursday,
            ),
            (Repeat::monthly(), day(2026, 10, 12)),
            (Repeat::monthly().on_month_day(31), day(2027, 4, 30)),
            (Repeat::new(4, Unit::Year), day(2028, 2, 29)),
        ] {
            let rule = repeat.to_rule(due).unwrap();
            assert_eq!(
                Repeat::from_rule(&rule, due),
                Some(repeat.clone().anchored(due)),
                "{}",
                rule.to_rrule(true)
            );
        }
        for (text, first) in [
            ("FREQ=WEEKLY;COUNT=3", thursday),
            ("FREQ=MONTHLY;BYDAY=-1FR", thursday),
            ("FREQ=MONTHLY;BYMONTHDAY=30", day(2026, 9, 30)),
            ("FREQ=MONTHLY", day(2026, 10, 31)),
            ("FREQ=MONTHLY;BYMONTHDAY=15", thursday),
            ("FREQ=WEEKLY;INTERVAL=2;WKST=SU;BYDAY=MO", thursday),
            ("FREQ=YEARLY;BYMONTH=3", thursday),
            ("FREQ=YEARLY", day(2028, 2, 29)),
        ] {
            assert_eq!(read(text, first), None, "{text} from {first}");
        }
    }
}
