//! The event editor's form - what the editor window (`editor_ui.rs`) shows and edits: what it
//! starts from (a new appointment, a new meeting, the week's draft, an event), the repeat
//! choices it offers and the rule each one makes, the reminder choices, the attendees line, and
//! the event it saves, or why it cannot.
//!
//! The repeat choices are Google Calendar's and Outlook's everyday ones, each made from the
//! event's first day: daily, weekly on its weekday, every weekday, monthly on its day of the
//! month, monthly on its nth (or last) weekday, yearly on its date - every N of them, ending
//! never, after a number of times or on a date. A rule that is none of these (an imported
//! one) is kept as it is and shown as "Custom: ...".

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Weekday};

use crate::{
    event::{is_email, Event, EventError, Meeting},
    rrule::{self, ByDay, Freq, RepeatEnd, Rule},
};

/// The repeat choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Repeat {
    Never,
    Daily,
    Weekly,
    Weekdays,
    MonthlyDay,
    MonthlyWeekday,
    Yearly,
    /// The event's own rule, which is none of the others: kept as it is.
    Custom,
}

impl Repeat {
    /// The choices the repeat list offers, in its order (Custom is added when the event has one).
    pub const CHOICES: [Repeat; 7] = [
        Repeat::Never,
        Repeat::Daily,
        Repeat::Weekly,
        Repeat::Weekdays,
        Repeat::MonthlyDay,
        Repeat::MonthlyWeekday,
        Repeat::Yearly,
    ];
}

/// When a repeat ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ends {
    Never,
    After,
    On,
}

impl Ends {
    /// The choices of the "Ends" list, in its order.
    pub const CHOICES: [Ends; 3] = [Ends::Never, Ends::After, Ends::On];

    #[must_use]
    pub const fn label(self) -> &'static str {
        todo!()
    }
}

/// The reminder choices: minutes before the start, and what the list says.
pub const REMINDERS: [(Option<u32>, &str); 8] = [
    (None, "None"),
    (Some(0), "At the start"),
    (Some(5), "5 minutes before"),
    (Some(10), "10 minutes before"),
    (Some(15), "15 minutes before"),
    (Some(30), "30 minutes before"),
    (Some(60), "1 hour before"),
    (Some(1440), "1 day before"),
];

/// How many times a repeat ends after when "After a number of times" is first picked.
pub const DEFAULT_COUNT: u32 = 10;

/// The editor's form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorForm {
    /// Which opening of the editor this is, so an answer for an older one is ignored.
    pub serial: u32,
    /// The event's id: a new one, or the edited event's.
    pub id: String,
    /// The event exists (its file is there): Delete is offered.
    pub existing: bool,
    /// Opened as "New meeting": the attendees line comes first.
    pub meeting_request: bool,
    pub title: String,
    pub location: String,
    /// The first day and the start time.
    pub date: NaiveDate,
    pub start: NaiveTime,
    /// The last day (an all-day event's; any other ends on its first day) and the end time.
    pub last_day: NaiveDate,
    pub end: NaiveTime,
    pub all_day: bool,
    pub repeat: Repeat,
    /// Every `interval` days, weeks, months or years.
    pub interval: u32,
    pub ends: Ends,
    pub count: u32,
    pub until: NaiveDate,
    /// The event's own rule when it is none of the choices (`Repeat::Custom`).
    pub custom: Option<Rule>,
    /// Minutes before the start.
    pub reminder: Option<u32>,
    /// The calendar's id (empty: the default calendar).
    pub calendar: String,
    pub notes: String,
    /// The attendees as typed: addresses separated by commas, semicolons or lines.
    pub attendees: String,
    /// "Add AzMeet link" is ticked.
    pub add_meet: bool,
    /// The event's meeting, kept as it is while "Add AzMeet link" stays ticked.
    pub meeting: Option<Meeting>,
    /// What the form does not show and keeps: the days a repeating event skips, an imported
    /// event's UID.
    pub except: Vec<NaiveDate>,
    pub uid: String,
    /// Why the last Save did not save.
    pub error: String,
}

impl EditorForm {
    /// A new appointment on `date` from `start` to `end`, in the calendar `calendar`.
    #[must_use]
    pub fn new_event(
        serial: u32,
        id: &str,
        date: NaiveDate,
        start: NaiveTime,
        end: NaiveTime,
        calendar: &str,
    ) -> EditorForm {
        todo!()
    }

    /// A new meeting: an appointment with attendees and, ticked, "Add AzMeet link".
    #[must_use]
    pub fn new_meeting(
        serial: u32,
        id: &str,
        date: NaiveDate,
        start: NaiveTime,
        end: NaiveTime,
        calendar: &str,
    ) -> EditorForm {
        todo!()
    }

    /// The form of an event that exists.
    #[must_use]
    pub fn from_event(serial: u32, event: &Event) -> EditorForm {
        todo!()
    }

    /// Moves the first day to `date`; an all-day event's last day moves along (the same number
    /// of days), and an end date before it moves to it.
    pub fn set_date(&mut self, date: NaiveDate) {
        todo!()
    }

    /// The last day (the end date).
    pub fn set_last_day(&mut self, day: NaiveDate) {
        todo!()
    }

    /// Ticks or clears "All day". An event with times ends on its first day.
    pub fn set_all_day(&mut self, all_day: bool) {
        todo!()
    }

    /// The repeat rule the form's choices make; `Err` with what to tell the user.
    pub fn rule(&self) -> Result<Option<Rule>, String> {
        todo!()
    }

    /// The event the form saves, with `meeting`; `Err` with what to tell the user.
    pub fn event(&self, meeting: Option<Meeting>) -> Result<Event, String> {
        todo!()
    }

    /// The editor window's title: "Team sync - Appointment", "Untitled - Meeting".
    #[must_use]
    pub fn window_title(&self) -> String {
        todo!()
    }
}

/// What the editor says about an event it cannot save.
#[must_use]
pub fn error_text(e: &EventError) -> String {
    todo!()
}

/// The nth weekday a monthly rule repeats a `date` on: its count from the month's start, or
/// "last" (-1) for a fifth one (which most months do not have).
#[must_use]
pub fn monthly_weekday(date: NaiveDate) -> ByDay {
    todo!()
}

const WEEKDAYS: [Weekday; 5] = [
    Weekday::Mon,
    Weekday::Tue,
    Weekday::Wed,
    Weekday::Thu,
    Weekday::Fri,
];

/// The rule a repeat choice makes for an event that starts on `date`; `None` for `Never` and
/// `Custom` (whose rule is the event's own).
#[must_use]
pub fn rule_of(
    repeat: Repeat,
    interval: u32,
    ends: Ends,
    count: u32,
    until: NaiveDate,
    date: NaiveDate,
) -> Option<Rule> {
    todo!()
}

/// How the form shows a rule: its choice, interval and end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub repeat: Repeat,
    pub interval: u32,
    pub ends: Ends,
    pub count: u32,
    pub until: NaiveDate,
}

/// How the form shows `rule` for an event that starts on `date`: the choice whose rule it is
/// (any interval and end), else `Custom`.
#[must_use]
pub fn repeat_of(rule: &Rule, date: NaiveDate) -> Shown {
    todo!()
}

/// What the repeat list says for `repeat` on an event that starts on `date`.
#[must_use]
pub fn repeat_label(repeat: Repeat, date: NaiveDate, custom: Option<&Rule>) -> String {
    todo!()
}

/// The repeat list: the choices (and the custom rule, when there is one), with their labels.
#[must_use]
pub fn repeat_choices(date: NaiveDate, custom: Option<&Rule>) -> Vec<(Repeat, String)> {
    todo!()
}

/// The unit of "Every N ...": "days", "weeks", "months", "years".
#[must_use]
pub fn interval_unit(repeat: Repeat) -> &'static str {
    todo!()
}

/// The reminder list's index of `minutes` (an imported reminder that is no choice: the nearest
/// earlier one).
#[must_use]
pub fn reminder_index(minutes: Option<u32>) -> usize {
    todo!()
}

/// The attendees typed into the "To" line: addresses separated by commas, semicolons or line
/// breaks, each as `name <address>` or the address alone. `Err` names the first one that is no
/// address.
pub fn parse_attendees(text: &str) -> Result<Vec<String>, String> {
    todo!()
}

/// A number typed into "Every N" or "After N times": at least 1, at most 999; `None` for no
/// number.
#[must_use]
pub fn parse_count(text: &str) -> Option<u32> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meeting;

    const ID: &str = "0b0f6f2e-5b8e-4c43-9a57-3f1f0d6f4b1a";

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    /// Wednesday 30 September 2026, 09:00 - 10:00.
    fn form() -> EditorForm {
        let mut f = EditorForm::new_event(1, ID, d(2026, 9, 30), at(9, 0), at(10, 0), "");
        f.title = String::from("Team sync");
        f
    }

    fn rule_text(f: &EditorForm) -> Option<String> {
        f.rule().unwrap().map(|r| r.to_rrule(false))
    }

    #[test]
    fn a_new_appointment_saves_as_a_plain_event() {
        let event = form().event(None).unwrap();
        assert_eq!(event.title, "Team sync");
        assert_eq!((event.start, event.end), (at(9, 0), at(10, 0)));
        assert_eq!(event.repeat, None);
        assert_eq!(event.reminder, Some(15));
        assert!(event.attendees.is_empty());
    }

    #[test]
    fn each_repeat_choice_makes_its_rule_from_the_first_day() {
        let mut f = form();
        let mut says = |repeat: Repeat| {
            f.repeat = repeat;
            rule_text(&f)
        };
        assert_eq!(says(Repeat::Never), None);
        assert_eq!(says(Repeat::Daily).as_deref(), Some("FREQ=DAILY"));
        assert_eq!(
            says(Repeat::Weekly).as_deref(),
            Some("FREQ=WEEKLY;BYDAY=WE")
        );
        assert_eq!(
            says(Repeat::Weekdays).as_deref(),
            Some("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR")
        );
        assert_eq!(
            says(Repeat::MonthlyDay).as_deref(),
            Some("FREQ=MONTHLY;BYMONTHDAY=30")
        );
        // 30 September 2026 is the fifth, and last, Wednesday of its month.
        assert_eq!(
            says(Repeat::MonthlyWeekday).as_deref(),
            Some("FREQ=MONTHLY;BYDAY=-1WE")
        );
        assert_eq!(says(Repeat::Yearly).as_deref(), Some("FREQ=YEARLY"));
    }

    #[test]
    fn an_interval_and_an_end_go_into_the_rule() {
        let mut f = form();
        f.repeat = Repeat::Weekly;
        f.interval = 2;
        f.ends = Ends::After;
        f.count = 5;
        assert_eq!(
            rule_text(&f).as_deref(),
            Some("FREQ=WEEKLY;INTERVAL=2;COUNT=5;BYDAY=WE")
        );
        f.ends = Ends::On;
        f.until = d(2026, 12, 31);
        assert_eq!(
            rule_text(&f).as_deref(),
            Some("FREQ=WEEKLY;INTERVAL=2;UNTIL=20261231T235959;BYDAY=WE")
        );
        f.until = d(2026, 9, 1);
        assert!(f.rule().is_err());
        f.ends = Ends::After;
        f.count = 0;
        assert!(f.rule().is_err());
    }

    #[test]
    fn a_saved_rule_opens_as_the_choice_that_made_it() {
        let date = d(2026, 9, 30);
        for repeat in Repeat::CHOICES.into_iter().skip(1) {
            for ends in Ends::CHOICES {
                let rule = rule_of(repeat, 3, ends, 4, d(2027, 1, 1), date).unwrap();
                let shown = repeat_of(&rule, date);
                assert_eq!(shown.repeat, repeat, "{}", rule.to_rrule(false));
                assert_eq!(shown.interval, 3);
                assert_eq!(shown.ends, ends);
            }
        }
        // the short ways of writing weekly and monthly
        assert_eq!(
            repeat_of(&Rule::new(Freq::Weekly), date).repeat,
            Repeat::Weekly
        );
        assert_eq!(
            repeat_of(&Rule::new(Freq::Monthly), date).repeat,
            Repeat::MonthlyDay
        );
    }

    #[test]
    fn a_rule_the_form_has_no_choice_for_is_kept_as_it_is() {
        let mut event = form().event(None).unwrap();
        let rule = Rule::parse("FREQ=YEARLY;BYMONTH=3;BYDAY=-1SU").unwrap();
        event.repeat = Some(rule.clone());
        let f = EditorForm::from_event(2, &event);
        assert_eq!(f.repeat, Repeat::Custom);
        assert_eq!(f.event(None).unwrap().repeat, Some(rule.clone()));
        let labels = repeat_choices(event.date, Some(&rule));
        assert_eq!(labels.len(), Repeat::CHOICES.len() + 1);
        assert_eq!(
            labels.last().unwrap().1,
            "Custom: Yearly on the last Sunday of March"
        );
    }

    #[test]
    fn the_repeat_list_names_each_choice_by_the_first_day() {
        let labels: Vec<String> = repeat_choices(d(2026, 10, 13), None)
            .into_iter()
            .map(|(_, l)| l)
            .collect();
        assert_eq!(
            labels,
            vec![
                "Does not repeat",
                "Daily",
                "Weekly on Tuesday",
                "Every weekday (Monday to Friday)",
                "Monthly on day 13",
                "Monthly on the second Tuesday",
                "Yearly on 13 October",
            ]
        );
    }

    #[test]
    fn an_event_opens_in_the_form_and_saves_back_unchanged() {
        let mut f = form();
        f.location = String::from("Room 4");
        f.notes = String::from("Agenda first.");
        f.attendees = String::from("Ana <ana@example.com>; bo@example.org");
        f.repeat = Repeat::Weekly;
        f.reminder = Some(30);
        f.calendar = String::from("9d4c1f3a-2b7e-4d10-8f6a-51c2e7b9a0d3");
        let link = meeting::pending_meeting("http://127.0.0.1:8787", "a2h859hyqkfaa11nhzxfh3gd7f");
        let event = f.event(Some(link.clone())).unwrap();
        assert_eq!(event.attendees, vec!["ana@example.com", "bo@example.org"]);
        let again = EditorForm::from_event(7, &event);
        assert!(again.existing && again.add_meet && again.meeting_request);
        assert_eq!(again.event(again.meeting.clone()), Ok(event));
    }

    #[test]
    fn an_all_day_event_spans_its_days_and_one_with_times_ends_on_its_day() {
        let mut f = form();
        f.set_all_day(true);
        f.set_last_day(d(2026, 10, 2));
        let event = f.event(None).unwrap();
        assert!(event.all_day);
        assert_eq!(event.last_day, d(2026, 10, 2));
        // moving the first day moves the last along
        f.set_date(d(2026, 10, 5));
        assert_eq!(f.last_day, d(2026, 10, 7));
        f.set_all_day(false);
        assert_eq!(f.last_day, f.date);
        f.last_day = d(2026, 10, 6);
        assert!(f.event(None).unwrap_err().contains("make it all day"));
    }

    #[test]
    fn the_attendees_line_takes_names_and_addresses_and_names_a_bad_one() {
        assert_eq!(
            parse_attendees("Ana <ana@example.com>, bo@example.org;\n ANA@example.com ; "),
            Ok(vec![
                String::from("ana@example.com"),
                String::from("bo@example.org")
            ])
        );
        assert_eq!(parse_attendees("  "), Ok(Vec::new()));
        assert_eq!(
            parse_attendees("ana@example.com, team"),
            Err(String::from("\"team\" is not an e-mail address."))
        );
        let mut f = form();
        f.attendees = String::from("nobody");
        assert_eq!(
            f.event(None),
            Err(String::from("\"nobody\" is not an e-mail address."))
        );
    }

    #[test]
    fn a_form_without_a_title_or_with_the_end_first_says_why() {
        let mut f = form();
        f.title = String::from("  ");
        assert_eq!(f.event(None), Err(String::from("Give the event a title.")));
        let mut f = form();
        f.end = at(8, 0);
        assert_eq!(
            f.event(None),
            Err(String::from("The event must end after it starts."))
        );
    }

    #[test]
    fn a_reminder_that_is_no_choice_takes_the_nearest_earlier_one() {
        assert_eq!(reminder_index(None), 0);
        assert_eq!(reminder_index(Some(0)), 1);
        assert_eq!(reminder_index(Some(15)), 4);
        assert_eq!(reminder_index(Some(20)), 4);
        assert_eq!(reminder_index(Some(100_000)), REMINDERS.len() - 1);
    }

    #[test]
    fn the_window_says_what_it_edits() {
        let mut f = form();
        assert_eq!(f.window_title(), "Team sync - Appointment");
        f.title.clear();
        f.attendees = String::from("ana@example.com");
        assert_eq!(f.window_title(), "Untitled - Meeting");
        let m = EditorForm::new_meeting(3, ID, d(2026, 9, 30), at(9, 0), at(10, 0), "");
        assert!(m.add_meet && m.meeting_request);
        assert_eq!(m.window_title(), "Untitled - Meeting");
    }

    #[test]
    fn counts_are_held_to_one_to_999() {
        assert_eq!(parse_count(" 3 "), Some(3));
        assert_eq!(parse_count("0"), Some(1));
        assert_eq!(parse_count("5000"), Some(999));
        assert_eq!(parse_count("two"), None);
    }
}
