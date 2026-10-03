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

use azul_pim::dates::{nth_weekday_of_month, ordinal_word, weekday_name, WORK_DAYS};

use crate::{
    event::{Event, EventError, Meeting},
    rrule::{ByDay, Freq, RepeatEnd, Rule},
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
    /// The ends, in the order a rule's end is tried in.
    pub const CHOICES: [Ends; 3] = [Ends::Never, Ends::After, Ends::On];
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
    /// The day of the repeating event's occurrence the form was opened on (`None`: a series
    /// opened as a whole, a plain event, a new one).
    pub occurrence: Option<NaiveDate>,
    /// Opened on an occurrence: Save edits the whole series (else that occurrence alone, which
    /// becomes an event of its own).
    pub whole_series: bool,
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
        EditorForm {
            serial,
            id: id.to_string(),
            existing: false,
            meeting_request: false,
            title: String::new(),
            location: String::new(),
            date,
            start,
            last_day: date,
            end,
            all_day: false,
            repeat: Repeat::Never,
            interval: 1,
            ends: Ends::Never,
            count: DEFAULT_COUNT,
            until: date + Duration::days(28),
            custom: None,
            reminder: Some(15),
            calendar: calendar.to_string(),
            notes: String::new(),
            attendees: String::new(),
            add_meet: false,
            meeting: None,
            except: Vec::new(),
            uid: String::new(),
            error: String::new(),
            occurrence: None,
            whole_series: false,
        }
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
        EditorForm {
            meeting_request: true,
            add_meet: true,
            ..EditorForm::new_event(serial, id, date, start, end, calendar)
        }
    }

    /// The form of an event that exists.
    #[must_use]
    pub fn from_event(serial: u32, event: &Event) -> EditorForm {
        let mut form = EditorForm::new_event(
            serial,
            &event.id,
            event.date,
            event.start,
            event.end,
            &event.calendar,
        );
        form.existing = true;
        form.title = event.title.clone();
        form.location = event.location.clone();
        form.last_day = event.last_day;
        form.all_day = event.all_day;
        if let Some(rule) = &event.repeat {
            let shown = repeat_of(rule, event.date);
            form.repeat = shown.repeat;
            form.interval = shown.interval;
            form.ends = shown.ends;
            form.count = shown.count;
            form.until = shown.until;
            form.custom = (shown.repeat == Repeat::Custom).then(|| rule.clone());
        }
        form.reminder = event.reminder;
        form.notes = event.notes.clone();
        form.attendees = event.attendees.join(", ");
        form.meeting_request = !event.attendees.is_empty();
        form.add_meet = event.meeting.is_some();
        form.meeting = event.meeting.clone();
        form.except = event.except.clone();
        form.uid = event.uid.clone();
        form
    }

    /// The form of the repeating `series`' occurrence on `day`: the event's fields on that
    /// day, editing that occurrence alone until "The whole series" is chosen.
    #[must_use]
    pub fn from_occurrence(serial: u32, series: &Event, day: NaiveDate) -> EditorForm {
        let mut form = EditorForm::from_event(serial, series);
        form.occurrence = Some(day);
        form.whole_series = false;
        form.move_to(day);
        form
    }

    /// Edits the whole series (`whole`, the form on the series' first day `series_first`) or
    /// the occurrence the form was opened on (on its day).
    pub fn set_whole_series(&mut self, whole: bool, series_first: NaiveDate) {
        let Some(day) = self.occurrence else {
            return;
        };
        self.whole_series = whole;
        self.move_to(if whole { series_first } else { day });
    }

    /// Shows the form on `day`, its days after the first moving along (the repeat's last date
    /// is the series', and stays).
    fn move_to(&mut self, day: NaiveDate) {
        let span = (self.last_day - self.date).num_days().max(0);
        self.date = day;
        self.last_day = day + Duration::days(span);
    }

    /// The form edits the occurrence it was opened on, not the series.
    #[must_use]
    pub fn edits_one_occurrence(&self) -> bool {
        self.occurrence.is_some() && !self.whole_series
    }

    /// What Save writes for one occurrence of `series`: the series skipping the occurrence's
    /// day, and the occurrence as an event of its own (`new_id`, no repeat, the form's edits,
    /// `meeting`); `Err` with what to tell the user.
    pub fn occurrence_events(
        &self,
        series: &Event,
        new_id: &str,
        meeting: Option<Meeting>,
    ) -> Result<(Event, Event), String> {
        let Some(day) = self.occurrence else {
            return Err(String::from("This appointment is not an occurrence of a series."));
        };
        let one = EditorForm {
            id: new_id.to_string(),
            repeat: Repeat::Never,
            custom: None,
            except: Vec::new(),
            uid: String::new(),
            ..self.clone()
        }
        .event(meeting)?;
        let mut kept = series.clone();
        if !kept.except.contains(&day) {
            kept.except.push(day);
            kept.except.sort();
        }
        let kept = kept.check().map_err(|e| error_text(&e))?;
        Ok((kept, one))
    }

    /// Moves the first day to `date`; an all-day event's last day moves along (the same number
    /// of days), and an end date before it moves to it.
    pub fn set_date(&mut self, date: NaiveDate) {
        let span = (self.last_day - self.date).num_days().max(0);
        self.date = date;
        self.last_day = date + Duration::days(span);
        if self.until < date {
            self.until = date;
        }
    }

    /// The last day (the end date).
    pub fn set_last_day(&mut self, day: NaiveDate) {
        self.last_day = day;
    }

    /// Ticks or clears "All day". An event with times ends on its first day.
    pub fn set_all_day(&mut self, all_day: bool) {
        self.all_day = all_day;
        if !all_day {
            self.last_day = self.date;
        }
    }

    /// The repeat rule the form's choices make; `Err` with what to tell the user.
    pub fn rule(&self) -> Result<Option<Rule>, String> {
        if self.repeat == Repeat::Custom {
            return Ok(self.custom.clone());
        }
        if self.repeat != Repeat::Never {
            match self.ends {
                Ends::After if self.count == 0 => {
                    return Err(String::from("Repeat it at least once."));
                }
                Ends::On if self.until < self.date => {
                    return Err(String::from(
                        "The repeat must end on or after the event's first day.",
                    ));
                }
                _ => {}
            }
        }
        Ok(rule_of(
            self.repeat,
            self.interval,
            self.ends,
            self.count,
            self.until,
            self.date,
        ))
    }

    /// The event the form saves, with `meeting`; `Err` with what to tell the user.
    pub fn event(&self, meeting: Option<Meeting>) -> Result<Event, String> {
        if !self.all_day && self.last_day != self.date {
            return Err(String::from(
                "An event with times ends on the day it starts: make it all day to span days.",
            ));
        }
        let repeat = self.rule()?;
        let attendees = parse_attendees(&self.attendees)?;
        Event {
            id: self.id.clone(),
            title: self.title.clone(),
            date: self.date,
            start: self.start,
            end: self.end,
            meeting,
            all_day: self.all_day,
            last_day: self.last_day,
            location: self.location.clone(),
            notes: self.notes.clone(),
            attendees,
            reminder: self.reminder,
            calendar: self.calendar.clone(),
            repeat,
            except: self.except.clone(),
            uid: self.uid.clone(),
        }
        .check()
        .map_err(|e| error_text(&e))
    }

    /// The editor window's title: "Team sync - Appointment", "Untitled - Meeting".
    #[must_use]
    pub fn window_title(&self) -> String {
        let title = self.title.trim();
        let title = if title.is_empty() { "Untitled" } else { title };
        let kind = if self.meeting_request || !self.attendees.trim().is_empty() {
            "Meeting"
        } else {
            "Appointment"
        };
        format!("{title} - {kind}")
    }

    /// Anything the form saves differs from `opened` (the form as the window opened with it):
    /// closing the window then asks "save changes?". The error line and the serial are no
    /// edits.
    #[must_use]
    pub fn changed_since(&self, opened: &EditorForm) -> bool {
        let edits = |f: &EditorForm| EditorForm {
            serial: 0,
            error: String::new(),
            ..f.clone()
        };
        edits(self) != edits(opened)
    }

    /// The rule the date repeat picker shows: the form's choice made into a rule, or its own
    /// rule; `None` when it does not repeat.
    #[must_use]
    pub fn shown_rule(&self) -> Option<Rule> {
        if self.repeat == Repeat::Custom {
            return self.custom.clone();
        }
        rule_of(
            self.repeat,
            self.interval,
            self.ends,
            self.count,
            self.until,
            self.date,
        )
    }

    /// Takes the rule the date repeat picker made (`None`: it does not repeat): a rule one of
    /// the choices makes shows as that choice, any other is kept as the form's own.
    pub fn set_rule(&mut self, rule: Option<Rule>) {
        let Some(rule) = rule else {
            self.repeat = Repeat::Never;
            self.custom = None;
            return;
        };
        let shown = repeat_of(&rule, self.date);
        self.repeat = shown.repeat;
        self.interval = shown.interval;
        self.ends = shown.ends;
        self.count = shown.count;
        self.until = shown.until;
        self.custom = (shown.repeat == Repeat::Custom).then_some(rule);
    }
}

/// What a close request does to the editor window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAnswer {
    /// The window closes: nothing unsaved (or the form was saved and is gone).
    Close,
    /// The close is held and the window asks "save changes?".
    Ask,
}

/// What a close request does to the editor window showing `form` (`None`: saved, cancelled or
/// deleted - gone), which opened with `opened`.
#[must_use]
pub fn close_answer(form: Option<&EditorForm>, opened: Option<&EditorForm>) -> CloseAnswer {
    match (form, opened) {
        (Some(form), Some(opened)) if form.changed_since(opened) => CloseAnswer::Ask,
        _ => CloseAnswer::Close,
    }
}

/// What the editor says about an event it cannot save.
#[must_use]
pub fn error_text(e: &EventError) -> String {
    match e {
        EventError::EmptyTitle => String::from("Give the event a title."),
        EventError::EndNotAfterStart => String::from("The event must end after it starts."),
        EventError::LastDayBeforeFirst => {
            String::from("The event must end on or after its first day.")
        }
        EventError::BadAttendee(who) => format!("{who:?} is not an e-mail address."),
        other => format!("This event cannot be saved: {other}."),
    }
}

/// The nth weekday a monthly rule repeats a `date` on: its count from the month's start, or
/// "last" (-1) for a fifth one (which most months do not have).
#[must_use]
pub fn monthly_weekday(date: NaiveDate) -> ByDay {
    let (nth, _) = nth_weekday_of_month(date);
    ByDay::nth(if nth >= 5 { -1 } else { nth }, date.weekday())
}

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
    let rule = match repeat {
        Repeat::Never | Repeat::Custom => return None,
        Repeat::Daily => Rule::new(Freq::Daily),
        Repeat::Weekly => Rule::new(Freq::Weekly).with_by_day(vec![ByDay::every(date.weekday())]),
        Repeat::Weekdays => {
            Rule::new(Freq::Weekly).with_by_day(WORK_DAYS.into_iter().map(ByDay::every).collect())
        }
        Repeat::MonthlyDay => {
            Rule::new(Freq::Monthly).with_by_month_day(vec![i8::try_from(date.day()).unwrap_or(1)])
        }
        Repeat::MonthlyWeekday => Rule::new(Freq::Monthly).with_by_day(vec![monthly_weekday(date)]),
        Repeat::Yearly => Rule::new(Freq::Yearly),
    };
    let end = match ends {
        Ends::Never => RepeatEnd::Never,
        Ends::After => RepeatEnd::Count(count.max(1)),
        Ends::On => RepeatEnd::Until(until),
    };
    Some(rule.with_interval(interval.max(1)).with_end(end))
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
    let (ends, count, until) = match rule.end {
        RepeatEnd::Never => (Ends::Never, DEFAULT_COUNT, date + Duration::days(28)),
        RepeatEnd::Count(n) => (Ends::After, n, date + Duration::days(28)),
        RepeatEnd::Until(d) => (Ends::On, DEFAULT_COUNT, d),
    };
    let found = [
        Repeat::Daily,
        Repeat::Weekly,
        Repeat::Weekdays,
        Repeat::MonthlyDay,
        Repeat::MonthlyWeekday,
        Repeat::Yearly,
    ]
    .into_iter()
    .find(|&choice| rule_of(choice, rule.interval, ends, count, until, date).as_ref() == Some(rule))
    .or_else(|| {
        // The same rules written the short way: FREQ=WEEKLY alone is "on the first day's
        // weekday", FREQ=MONTHLY alone "on the first day's day".
        let bare = rule.by_day.is_empty()
            && rule.by_month_day.is_empty()
            && rule.by_month.is_empty()
            && rule.week_start == Weekday::Mon;
        match rule.freq {
            Freq::Weekly if bare => Some(Repeat::Weekly),
            Freq::Monthly if bare => Some(Repeat::MonthlyDay),
            _ => None,
        }
    });
    Shown {
        repeat: found.unwrap_or(Repeat::Custom),
        interval: rule.interval.max(1),
        ends,
        count,
        until,
    }
}

/// What the repeat list says for `repeat` on an event that starts on `date`.
#[must_use]
pub fn repeat_label(repeat: Repeat, date: NaiveDate, custom: Option<&Rule>) -> String {
    match repeat {
        Repeat::Never => String::from("Does not repeat"),
        Repeat::Daily => String::from("Daily"),
        Repeat::Weekly => format!("Weekly on {}", weekday_name(date.weekday())),
        Repeat::Weekdays => String::from("Every weekday (Monday to Friday)"),
        Repeat::MonthlyDay => format!("Monthly on day {}", date.day()),
        Repeat::MonthlyWeekday => {
            let by = monthly_weekday(date);
            format!(
                "Monthly on the {} {}",
                ordinal_word(i32::from(by.nth)),
                weekday_name(by.weekday)
            )
        }
        Repeat::Yearly => format!("Yearly on {}", date.format("%-d %B")),
        Repeat::Custom => match custom {
            Some(rule) => format!("Custom: {}", rule.describe(date)),
            None => String::from("Custom"),
        },
    }
}

/// The repeat list: the choices (and the custom rule, when there is one), with their labels.
#[must_use]
pub fn repeat_choices(date: NaiveDate, custom: Option<&Rule>) -> Vec<(Repeat, String)> {
    let mut choices: Vec<(Repeat, String)> = Repeat::CHOICES
        .into_iter()
        .map(|r| (r, repeat_label(r, date, None)))
        .collect();
    if custom.is_some() {
        choices.push((Repeat::Custom, repeat_label(Repeat::Custom, date, custom)));
    }
    choices
}

/// The reminder list's index of `minutes` (an imported reminder that is no choice: the nearest
/// earlier one).
#[must_use]
pub fn reminder_index(minutes: Option<u32>) -> usize {
    let Some(minutes) = minutes else {
        return 0;
    };
    REMINDERS
        .iter()
        .enumerate()
        .filter(|(_, (m, _))| m.is_some_and(|m| m <= minutes))
        .map(|(i, _)| i)
        .last()
        .unwrap_or(1)
}

/// The attendees typed into the "To" line: addresses separated by commas, semicolons or line
/// breaks, each as `name <address>` or the address alone. `Err` names the first one that is no
/// address.
pub fn parse_attendees(text: &str) -> Result<Vec<String>, String> {
    // A separator inside a quoted name ("Lovelace, Ada" <ada@example.org>) is part of the name.
    azul_pim::mail_address::address_list(text)
        .map_err(|entry| format!("{entry:?} is not an e-mail address."))
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
    fn an_attendee_written_last_comma_first_in_quotes_is_one_attendee() {
        // What AzMail's To line and Outlook write: the comma is inside the quoted name.
        assert_eq!(
            parse_attendees("\"Lovelace, Ada\" <ada@example.org>, bo@example.org"),
            Ok(vec![
                String::from("ada@example.org"),
                String::from("bo@example.org")
            ])
        );
        assert_eq!(
            parse_attendees("\"Lovelace; Ada\" <ada@example.org>;\n\"Ada, L.\" <ADA@example.org>"),
            Ok(vec![String::from("ada@example.org")])
        );
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

    const OTHER_ID: &str = "7d3c0f1e-2a4b-4c5d-8e6f-0a1b2c3d4e5f";

    /// "Team sync", weekly on Wednesdays from 30 September 2026, 09:00 - 10:00.
    fn series() -> Event {
        let mut f = form();
        f.set_rule(Rule::parse("FREQ=WEEKLY;BYDAY=WE").ok());
        f.event(None).unwrap()
    }

    /// Outlook's "Open this occurrence": the form shows the occurrence's day; Save keeps the
    /// series (skipping that day) and makes the occurrence an event of its own with the edits.
    #[test]
    fn an_occurrence_is_edited_alone_and_saved_as_an_event_of_its_own() {
        let series = series();
        let day = d(2026, 10, 14);
        let mut f = EditorForm::from_occurrence(2, &series, day);
        assert_eq!((f.date, f.last_day), (day, day), "the form is on the occurrence's day");
        assert_eq!(f.occurrence, Some(day));
        assert!(f.edits_one_occurrence());
        assert!(f.existing);
        f.title = String::from("Team sync (moved)");
        f.start = at(11, 0);
        f.end = at(12, 0);
        let (kept, one) = f.occurrence_events(&series, OTHER_ID, None).unwrap();
        assert_eq!(kept.id, series.id);
        assert_eq!(kept.except, vec![day], "the series skips the day");
        assert_eq!(kept.repeat, series.repeat);
        assert_eq!((kept.title.as_str(), kept.start), ("Team sync", at(9, 0)), "the series is as it was");
        assert_eq!(one.id, OTHER_ID);
        assert_eq!((one.date, one.start, one.end), (day, at(11, 0), at(12, 0)));
        assert_eq!(one.title, "Team sync (moved)");
        assert_eq!(one.repeat, None, "the occurrence does not repeat");
        assert!(one.except.is_empty());
        assert_eq!(one.uid, "");
        assert_eq!(one.calendar, series.calendar);
        // An edit that fails says why and writes nothing.
        f.title.clear();
        assert!(f.occurrence_events(&series, OTHER_ID, None).is_err());
    }

    #[test]
    fn the_whole_series_is_edited_from_its_first_day() {
        let series = series();
        let day = d(2026, 10, 14);
        let mut f = EditorForm::from_occurrence(2, &series, day);
        f.set_whole_series(true, series.date);
        assert!(!f.edits_one_occurrence());
        assert_eq!(f.date, series.date, "the series is shown from its first day");
        assert_eq!(rule_text(&f).as_deref(), Some("FREQ=WEEKLY;BYDAY=WE"));
        f.set_whole_series(false, series.date);
        assert!(f.edits_one_occurrence());
        assert_eq!(f.date, day, "back on the occurrence's day");
        // A plain event has no occurrence to edit alone.
        assert!(!EditorForm::from_event(3, &form().event(None).unwrap()).edits_one_occurrence());
    }

    /// Every close goes through the window's CloseRequested - the app's own close_window after
    /// Save & Close too (INFRA6): a saved form is gone by then, so the window closes without
    /// asking; an edited open form asks; an unedited one closes.
    #[test]
    fn save_closes_the_editor_without_asking() {
        let opened = form();
        let mut edited = opened.clone();
        edited.title.push_str(" (moved)");
        // Save & Close: the form was written and taken out of the state before close_window.
        assert_eq!(close_answer(None, Some(&opened)), CloseAnswer::Close);
        assert_eq!(close_answer(None, None), CloseAnswer::Close);
        assert_eq!(close_answer(Some(&edited), Some(&opened)), CloseAnswer::Ask);
        assert_eq!(close_answer(Some(&opened), Some(&opened)), CloseAnswer::Close);
    }

    /// Closing the editor asks "save changes?" only after an edit: a form is changed since it
    /// was opened when anything it saves differs - not when only its error line or its serial
    /// do, and not after an edit that was undone by hand.
    #[test]
    fn a_form_is_changed_only_when_what_it_saves_differs_from_when_it_opened() {
        let opened = form();
        let mut f = opened.clone();
        assert!(!f.changed_since(&opened), "a form just opened is unchanged");
        f.error = String::from("Give the event a title.");
        f.serial += 1;
        assert!(!f.changed_since(&opened), "the error line and the serial are not edits");
        f.title.push('!');
        assert!(f.changed_since(&opened), "a title typed");
        f.title.pop();
        assert!(!f.changed_since(&opened), "the same title again");
        f.set_rule(Rule::parse("FREQ=DAILY").ok());
        assert!(f.changed_since(&opened), "a repeat chosen");
        let mut g = opened.clone();
        g.reminder = None;
        assert!(g.changed_since(&opened), "a reminder taken away");
    }

    /// The date repeat picker hands the form a rule: one of the form's choices shows as that
    /// choice (any interval and end), any other - Monday and Wednesday every week - is kept as
    /// the form's own rule; the rule the editor shows is the form's.
    #[test]
    fn a_rule_from_the_date_repeat_picker_becomes_the_forms_repeat() {
        let mut f = form();
        assert_eq!(f.shown_rule(), None, "a new appointment does not repeat");
        let every_two_weeks = Rule::parse("FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=WE").unwrap();
        f.set_rule(Some(every_two_weeks.clone()));
        assert_eq!(f.repeat, Repeat::Weekly);
        assert_eq!((f.interval, f.ends, f.count), (2, Ends::After, 4));
        assert_eq!(f.custom, None);
        assert_eq!(f.shown_rule(), Some(every_two_weeks));
        assert_eq!(rule_text(&f).as_deref(), Some("FREQ=WEEKLY;INTERVAL=2;COUNT=4;BYDAY=WE"));

        let two_days = Rule::parse("FREQ=WEEKLY;BYDAY=MO,WE").unwrap();
        f.set_rule(Some(two_days.clone()));
        assert_eq!(f.repeat, Repeat::Custom);
        assert_eq!(f.custom.as_ref(), Some(&two_days));
        assert_eq!(f.shown_rule(), Some(two_days));

        f.set_rule(None);
        assert_eq!((f.repeat, f.custom.as_ref()), (Repeat::Never, None));
        assert_eq!(rule_text(&f), None);
    }
}
