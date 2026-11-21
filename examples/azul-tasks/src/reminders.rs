//! Reminders: when each task's reminder goes off, which are due now, and the presets the
//! detail pane offers.
//!
//! A reminder is either a moment (`Reminder::At`) or some minutes before the due time
//! (`Reminder::Before`); a task due on a day without a time reminds before the settings'
//! reminder time on that day. A reminder that was shown is remembered in the task
//! (`reminded`), so it shows once - also across restarts - and again only when it changes.
//! While the app runs a timer asks [`due_now`] every few seconds; a reminder missed while
//! the app was closed shows at the next start.

use chrono::{Duration, NaiveDateTime, NaiveTime};

use crate::model::{self, Reminder, Task};

/// The moment `t` reminds, if it has a reminder that can go off.
#[must_use]
pub fn reminder_at(t: &Task, reminder_time: NaiveTime) -> Option<NaiveDateTime> {
    match t.reminder? {
        Reminder::At(at) => Some(at),
        Reminder::Before(minutes) => {
            let due = t.due?;
            let base = due.and_time(t.due_time.unwrap_or(reminder_time));
            Some(model::minutes_before(base, minutes))
        }
    }
}

/// The open tasks whose reminder is due at `now` and was not shown yet (indices).
#[must_use]
pub fn due_now(tasks: &[Task], now: NaiveDateTime, reminder_time: NaiveTime) -> Vec<usize> {
    tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.is_done())
        .filter_map(|(i, t)| {
            let at = reminder_at(t, reminder_time)?;
            (at <= now && t.reminded != Some(at)).then_some(i)
        })
        .collect()
}

/// The earliest reminder still to come after `now`.
#[must_use]
pub fn next_at(tasks: &[Task], now: NaiveDateTime, reminder_time: NaiveTime) -> Option<NaiveDateTime> {
    tasks
        .iter()
        .filter(|t| !t.is_done())
        .filter_map(|t| reminder_at(t, reminder_time))
        .filter(|at| *at > now)
        .min()
}

/// What the reminder control offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    None,
    AtDue,
    Before5,
    Before15,
    Before60,
    DayBefore,
    /// A moment of its own (`Reminder::At`).
    OnDate,
}

impl Preset {
    /// In the control's order.
    pub const ALL: [Preset; 7] = [
        Preset::None,
        Preset::AtDue,
        Preset::Before5,
        Preset::Before15,
        Preset::Before60,
        Preset::DayBefore,
        Preset::OnDate,
    ];

    /// The choice's name: a key of the resources.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Preset::None => "aztasks-reminder-none",
            Preset::AtDue => "aztasks-reminder-at-due",
            Preset::Before5 => "aztasks-reminder-5-minutes",
            Preset::Before15 => "aztasks-reminder-15-minutes",
            Preset::Before60 => "aztasks-reminder-1-hour",
            Preset::DayBefore => "aztasks-reminder-1-day",
            Preset::OnDate => "aztasks-reminder-on-date",
        }
    }

    /// The position in [`Preset::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Preset::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    /// The minutes before the due time, for the relative presets.
    #[must_use]
    pub fn minutes(self) -> Option<i64> {
        match self {
            Preset::AtDue => Some(0),
            Preset::Before5 => Some(5),
            Preset::Before15 => Some(15),
            Preset::Before60 => Some(60),
            Preset::DayBefore => Some(24 * 60),
            Preset::None | Preset::OnDate => None,
        }
    }
}

/// The preset a reminder is (a relative reminder of other minutes reads as "On a date").
#[must_use]
pub fn preset_of(reminder: Option<Reminder>) -> Preset {
    match reminder {
        None => Preset::None,
        Some(Reminder::At(_)) => Preset::OnDate,
        Some(Reminder::Before(m)) => Preset::ALL
            .into_iter()
            .find(|p| p.minutes() == Some(m))
            .unwrap_or(Preset::OnDate),
    }
}

/// The reminder a chosen preset sets on `t`: relative ones need a due date (without one
/// they become a moment: `fallback` minus the minutes); "On a date" keeps a moment the task
/// has, else takes when the task reminds now, else `fallback`.
#[must_use]
pub fn reminder_for(preset: Preset, t: &Task, reminder_time: NaiveTime, fallback: NaiveDateTime) -> Option<Reminder> {
    match preset {
        Preset::None => None,
        Preset::OnDate => Some(Reminder::At(match t.reminder {
            Some(Reminder::At(at)) => at,
            _ => reminder_at(t, reminder_time).unwrap_or(fallback),
        })),
        relative => {
            let minutes = relative.minutes().unwrap_or(0);
            if t.due.is_some() {
                Some(Reminder::Before(minutes))
            } else {
                Some(Reminder::At(fallback - Duration::minutes(minutes)))
            }
        }
    }
}

/// The reminder as the detail pane and the banner read it: "Fri 2 Oct 08:45".
#[must_use]
pub fn describe(t: &Task, reminder_time: NaiveTime, today: chrono::NaiveDate) -> Option<String> {
    let at = reminder_at(t, reminder_time)?;
    Some(format!(
        "{} {}",
        model::day_label(at.date(), today),
        model::format_time(at.time())
    ))
}

/// The banner's line for the tasks reminding now: "Reminder: Pay rent", "2 reminders: Pay
/// rent, Call Kai", "4 reminders: Pay rent, Call Kai and 2 more".
#[must_use]
pub fn banner_text(titles: &[&str]) -> String {
    match titles {
        [] => String::new(),
        [one] => azul_appkit::l10n::t_args("aztasks-banner-one", &[("title", azul_appkit::l10n::Arg::from(*one))]),
        [a, b] => azul_appkit::l10n::t_args(
            "aztasks-banner-two",
            &[("a", azul_appkit::l10n::Arg::from(*a)), ("b", azul_appkit::l10n::Arg::from(*b))],
        ),
        [a, b, rest @ ..] => azul_appkit::l10n::t_args(
            "aztasks-banner-many",
            &[
                ("count", azul_appkit::l10n::Arg::from(titles.len())),
                ("a", azul_appkit::l10n::Arg::from(*a)),
                ("b", azul_appkit::l10n::Arg::from(*b)),
                ("more", azul_appkit::l10n::Arg::from(rest.len())),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(d: u32, h: u32, m: u32) -> NaiveDateTime {
        day(2026, 10, d).and_hms_opt(h, m, 0).unwrap()
    }

    fn nine() -> NaiveTime {
        NaiveTime::from_hms_opt(9, 0, 0).unwrap()
    }

    fn task(id: &str) -> Task {
        Task::new(id.into(), "l".into(), id.into(), at(1, 8, 0))
    }

    #[test]
    fn a_reminder_before_a_timed_due_date_goes_off_before_that_time() {
        let mut t = task("rent");
        t.due = Some(day(2026, 10, 2));
        t.due_time = NaiveTime::from_hms_opt(14, 30, 0);
        t.reminder = Some(Reminder::Before(15));
        assert_eq!(reminder_at(&t, nine()), Some(at(2, 14, 15)));
        t.reminder = Some(Reminder::Before(24 * 60));
        assert_eq!(reminder_at(&t, nine()), Some(at(1, 14, 30)));
    }

    #[test]
    fn a_reminder_on_a_day_without_a_time_uses_the_reminder_time_setting() {
        let mut t = task("plants");
        t.due = Some(day(2026, 10, 3));
        t.reminder = Some(Reminder::Before(0));
        assert_eq!(reminder_at(&t, nine()), Some(at(3, 9, 0)));
        t.due = None;
        assert_eq!(reminder_at(&t, nine()), None, "relative to nothing");
        t.reminder = Some(Reminder::At(at(5, 7, 0)));
        assert_eq!(reminder_at(&t, nine()), Some(at(5, 7, 0)));
    }

    #[test]
    fn a_shown_reminder_does_not_show_again_until_it_changes() {
        let mut t = task("call");
        t.reminder = Some(Reminder::At(at(1, 9, 30)));
        let mut tasks = vec![t];
        assert!(due_now(&tasks, at(1, 9, 29), nine()).is_empty(), "not yet");
        assert_eq!(due_now(&tasks, at(1, 9, 30), nine()), vec![0]);
        tasks[0].reminded = Some(at(1, 9, 30));
        assert!(due_now(&tasks, at(1, 10, 0), nine()).is_empty(), "shown once");
        tasks[0].reminder = Some(Reminder::At(at(1, 9, 45)));
        assert_eq!(due_now(&tasks, at(1, 10, 0), nine()), vec![0], "changed: again");
    }

    #[test]
    fn completed_tasks_never_remind() {
        let mut t = task("done");
        t.reminder = Some(Reminder::At(at(1, 9, 0)));
        t.completed = Some(at(1, 8, 30));
        assert!(due_now(&[t], at(1, 12, 0), nine()).is_empty());
    }

    #[test]
    fn the_next_reminder_is_the_earliest_still_to_come() {
        let mut a = task("a");
        a.reminder = Some(Reminder::At(at(1, 18, 0)));
        let mut b = task("b");
        b.reminder = Some(Reminder::At(at(1, 11, 0)));
        let mut c = task("c");
        c.reminder = Some(Reminder::At(at(1, 9, 0)));
        assert_eq!(next_at(&[a, b, c], at(1, 10, 0), nine()), Some(at(1, 11, 0)));
    }

    #[test]
    fn presets_map_to_reminders_and_back() {
        let mut t = task("x");
        t.due = Some(day(2026, 10, 2));
        for p in Preset::ALL {
            let r = reminder_for(p, &t, nine(), at(1, 12, 0));
            assert_eq!(preset_of(r), p, "{p:?}");
            assert_eq!(Preset::ALL[p.index()], p);
        }
        t.due = None;
        assert_eq!(
            reminder_for(Preset::Before15, &t, nine(), at(1, 12, 0)),
            Some(Reminder::At(at(1, 11, 45))),
            "no due date: a moment"
        );
        assert_eq!(preset_of(Some(Reminder::Before(7))), Preset::OnDate);
    }

    #[test]
    fn the_banner_names_the_tasks() {
        crate::l10n::in_english();
        assert_eq!(banner_text(&["Pay rent"]), "Reminder: Pay rent");
        assert_eq!(banner_text(&["Pay rent", "Call Kai"]), "2 reminders: Pay rent, Call Kai");
        assert_eq!(
            banner_text(&["a", "b", "c", "d"]),
            "4 reminders: a, b and 2 more"
        );
        let mut t = task("x");
        t.reminder = Some(Reminder::At(at(2, 8, 45)));
        assert_eq!(describe(&t, nine(), day(2026, 10, 1)).as_deref(), Some("Tomorrow 08:45"));
    }
}
