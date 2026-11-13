//! A to-do's repeat (`azul_pim::repeat::Repeat`) as azul's `DateRepeatPicker` shows it, and
//! back: the task detail's "Custom..." repeat is the toolkit's date repeat picker (the same one
//! AzCalendar's event editor uses - DEDUP_EDITORS C5), with "from completion" and without an
//! end or a month's nth weekday (a to-do's repeat has neither).

use azul::widgets::{
    DatePickerState, DatePickerWeekStart, DateRepeatFrequency, DateRepeatRule,
};
use chrono::{Datelike, NaiveDate, Weekday};

use crate::recur::{Repeat, Unit};

/// The week, Monday first: bit `n` of a rule's weekdays is `WEEK[n]`.
const WEEK: [Weekday; 7] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
    Weekday::Sat,
    Weekday::Sun,
];

/// The day a date picker shows for `date`.
#[must_use]
pub fn picker_day(date: NaiveDate) -> DatePickerState {
    DatePickerState {
        year: u32::try_from(date.year()).unwrap_or(1970),
        month: date.month(),
        day: date.day(),
    }
}

/// The week start of the date pickers and the editor's weekday toggles for the week start
/// setting (a picker starts on Monday or Sunday: a Saturday week starts its rows on Sunday).
#[must_use]
pub fn picker_week_start(week_start: Weekday) -> DatePickerWeekStart {
    if week_start == Weekday::Mon {
        DatePickerWeekStart::Monday
    } else {
        DatePickerWeekStart::Sunday
    }
}

/// The rule the editor shows for a task due on `due` that repeats by `repeat` (`None`: it
/// does not repeat).
#[must_use]
pub fn rule_of(repeat: Option<&Repeat>, due: NaiveDate) -> DateRepeatRule {
    let mut rule = DateRepeatRule::create(picker_day(due));
    let Some(r) = repeat else {
        return rule;
    };
    rule.frequency = match r.unit {
        Unit::Day => DateRepeatFrequency::Daily,
        Unit::Week => DateRepeatFrequency::Weekly,
        Unit::Month => DateRepeatFrequency::Monthly,
        Unit::Year => DateRepeatFrequency::Yearly,
    };
    rule.interval = r.every.clamp(1, 999);
    // No days: the due date's weekday, as in the editor.
    rule.weekdays = r
        .weekdays
        .iter()
        .fold(0u8, |bits, day| bits | 1 << day.num_days_from_monday());
    rule.from_completion = r.from_completion;
    rule
}

/// The repeat the editor's `rule` makes for a task due on `due` that repeated by `before`
/// (whose day of the month a month or year repeat keeps); `None` for "Never".
#[must_use]
pub fn repeat_of(rule: &DateRepeatRule, before: Option<&Repeat>, due: NaiveDate) -> Option<Repeat> {
    let unit = match rule.frequency {
        DateRepeatFrequency::Never => return None,
        DateRepeatFrequency::Daily => Unit::Day,
        DateRepeatFrequency::Weekly => Unit::Week,
        DateRepeatFrequency::Monthly => Unit::Month,
        DateRepeatFrequency::Yearly => Unit::Year,
    };
    let mut repeat = Repeat::new(rule.interval.clamp(1, 999), unit);
    if unit == Unit::Week {
        let days: Vec<Weekday> = (0..7)
            .filter(|n| rule.weekdays & (1 << n) != 0)
            .map(|n| WEEK[n])
            .collect();
        repeat = repeat.on_weekdays(&days);
    }
    // A month or year repeat keeps the day it had (the 31st stays the 31st in November).
    repeat.month_day = before
        .filter(|b| b.unit == unit && matches!(unit, Unit::Month | Unit::Year))
        .and_then(|b| b.month_day);
    Some(
        repeat
            .counting_from_completion(rule.from_completion)
            .anchored(due),
    )
}

/// Only the "every N" number changed: the detail is not rebuilt (the number field keeps its
/// caret while the user types).
#[must_use]
pub fn only_the_number_changed(before: Option<&Repeat>, after: Option<&Repeat>) -> bool {
    match (before, after) {
        (Some(b), Some(a)) => {
            b.every != a.every
                && Repeat {
                    every: a.every,
                    ..b.clone()
                } == *a
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detail::{preset_repeat, REPEATS};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn every_preset_comes_back_from_the_editor_as_itself() {
        let due = d(2026, 10, 14);
        for index in 0..REPEATS.len() - 1 {
            let preset = preset_repeat(index, due);
            let rule = rule_of(preset.as_ref(), due);
            assert_eq!(
                repeat_of(&rule, preset.as_ref(), due),
                preset,
                "{}",
                REPEATS[index]
            );
        }
    }

    #[test]
    fn the_editor_shows_a_repeat_as_its_frequency_interval_days_and_completion() {
        let due = d(2026, 10, 14); // a Wednesday
        let none = rule_of(None, due);
        assert!(matches!(none.frequency, DateRepeatFrequency::Never));
        assert_eq!((none.start.year, none.start.month, none.start.day), (2026, 10, 14));
        let r = Repeat::new(3, Unit::Week)
            .on_weekdays(&[Weekday::Mon, Weekday::Fri])
            .counting_from_completion(true);
        let rule = rule_of(Some(&r), due);
        assert!(matches!(rule.frequency, DateRepeatFrequency::Weekly));
        assert_eq!(rule.interval, 3);
        assert_eq!(rule.weekdays, 0b001_0001);
        assert!(rule.from_completion);
        assert_eq!(repeat_of(&rule, Some(&r), due), Some(r));
    }

    #[test]
    fn a_month_repeat_keeps_its_day_and_a_new_one_takes_the_due_day() {
        // Monthly on the 31st, due on 30 November: still the 31st (the last day) after an edit.
        let due = d(2026, 11, 30);
        let r = Repeat::monthly().on_month_day(31);
        let mut rule = rule_of(Some(&r), due);
        rule.interval = 2;
        let back = repeat_of(&rule, Some(&r), due).unwrap();
        assert_eq!((back.every, back.month_day), (2, Some(31)));
        // A weekly repeat made monthly: on the due day.
        let weekly = Repeat::weekly();
        let mut rule = rule_of(Some(&weekly), due);
        rule.frequency = DateRepeatFrequency::Monthly;
        let back = repeat_of(&rule, Some(&weekly), due).unwrap();
        assert_eq!((back.unit, back.month_day), (Unit::Month, Some(30)));
        assert!(back.weekdays.is_empty());
        // "Never" takes the repeat away.
        rule.frequency = DateRepeatFrequency::Never;
        assert_eq!(repeat_of(&rule, Some(&weekly), due), None);
    }

    #[test]
    fn only_a_changed_number_keeps_the_detail_as_it_is() {
        let a = Repeat::new(2, Unit::Day);
        let b = Repeat::new(5, Unit::Day);
        assert!(only_the_number_changed(Some(&a), Some(&b)));
        assert!(!only_the_number_changed(Some(&a), Some(&a)), "nothing changed");
        assert!(!only_the_number_changed(Some(&a), Some(&Repeat::new(2, Unit::Week))));
        assert!(!only_the_number_changed(None, Some(&a)));
        assert!(!only_the_number_changed(Some(&a), None));
    }

    #[test]
    fn a_saturday_week_starts_the_pickers_on_sunday() {
        assert!(matches!(picker_week_start(Weekday::Mon), DatePickerWeekStart::Monday));
        assert!(matches!(picker_week_start(Weekday::Sun), DatePickerWeekStart::Sunday));
        assert!(matches!(picker_week_start(Weekday::Sat), DatePickerWeekStart::Sunday));
    }
}
