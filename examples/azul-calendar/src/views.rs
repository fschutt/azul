//! The calendar's views, Outlook's "Arrange" group: Day, Work Week, Week, Month, Schedule View,
//! and the List (an agenda of the coming days). What each one shows (its days, around an anchor
//! day the navigation moves), its title, how Previous / Next move it, which events fall in it
//! (a repeating event once per date its rule makes), the month view's "+N more", and the
//! agenda's days. Weeks start on Monday, as the week view always did.

use chrono::{Datelike, Duration, NaiveDate};

use crate::{event::Event, week};

/// A view of the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewKind {
    Day,
    WorkWeek,
    Week,
    Month,
    /// The shown day's hours across, one row per calendar.
    Schedule,
    /// The coming days as a list.
    Agenda,
}

/// How many days the list shows ("Next 7 Days").
pub const AGENDA_DAYS: i64 = 7;
/// The month view's weeks: always six rows, as Outlook's and every month grid.
pub const MONTH_WEEKS: usize = 6;

impl ViewKind {
    /// Every view, in the ribbon's order.
    pub const ALL: [ViewKind; 6] = [
        ViewKind::Day,
        ViewKind::WorkWeek,
        ViewKind::Week,
        ViewKind::Month,
        ViewKind::Schedule,
        ViewKind::Agenda,
    ];

    /// The view's name in the settings file, on the command line and on stdout.
    #[must_use]
    pub const fn name(self) -> &'static str {
        todo!()
    }

    #[must_use]
    pub fn from_name(name: &str) -> Option<ViewKind> {
        todo!()
    }

    /// The view's name for people, as the ribbon labels its button.
    #[must_use]
    pub const fn label(self) -> &'static str {
        todo!()
    }

    /// The view's icon (a Material icon name).
    #[must_use]
    pub const fn icon(self) -> &'static str {
        todo!()
    }

    /// The view is hours down the page, one column a day (CAL2's week): a click or a drag makes
    /// an event there, a pinch zooms it.
    #[must_use]
    pub const fn is_time_grid(self) -> bool {
        todo!()
    }
}

/// The days the view shows around `anchor`, in order: the day; Monday to Friday or to Sunday of
/// its week; the six weeks of the month grid (Monday first); the list's days from the anchor.
#[must_use]
pub fn days_shown(kind: ViewKind, anchor: NaiveDate) -> Vec<NaiveDate> {
    todo!()
}

/// The first day of `day`'s month.
#[must_use]
pub fn month_start(day: NaiveDate) -> NaiveDate {
    todo!()
}

/// The last day of `day`'s month.
#[must_use]
pub fn month_end(day: NaiveDate) -> NaiveDate {
    todo!()
}

/// The Monday the month grid of `day`'s month starts on.
#[must_use]
pub fn month_grid_start(day: NaiveDate) -> NaiveDate {
    todo!()
}

/// The days the view is ABOUT, for the date navigator to light: the shown days, except that the
/// month view is its month (not the ends of the weeks around it).
#[must_use]
pub fn visible_range(kind: ViewKind, anchor: NaiveDate) -> (NaiveDate, NaiveDate) {
    todo!()
}

/// The anchor Previous (`by` = -1) or Next (`by` = 1) moves to: a day, a week, a month (the day
/// held to the month's length), or the list's length.
#[must_use]
pub fn step(kind: ViewKind, anchor: NaiveDate, by: i32) -> NaiveDate {
    todo!()
}

/// The view's title: "Wednesday, 30 September 2026", "28 September - 2 October 2026",
/// "October 2026", "30 September - 6 October 2026".
#[must_use]
pub fn title(kind: ViewKind, anchor: NaiveDate) -> String {
    todo!()
}

/// "28 September - 4 October 2026", "5 - 11 October 2026", "28 December 2026 - 3 January 2027".
#[must_use]
pub fn range_title(first: NaiveDate, last: NaiveDate) -> String {
    todo!()
}

/// One occurrence of an event in a view: the event (its index in the calendar's list) and the
/// days this occurrence takes (one day, or several for a several-day all-day event).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    pub index: usize,
    pub first: NaiveDate,
    pub last: NaiveDate,
}

impl Occurrence {
    /// The occurrence is on `day`.
    #[must_use]
    pub fn covers(&self, day: NaiveDate) -> bool {
        todo!()
    }
}

/// Every occurrence of the `shown` events on any day from `from` to `to` (both included), in
/// order of first day, then all-day before timed, start, end and title.
pub fn occurrences(
    events: &[Event],
    from: NaiveDate,
    to: NaiveDate,
    shown: impl Fn(&Event) -> bool,
) -> Vec<Occurrence> {
    todo!()
}

/// The occurrences on `day`, in the order `occurrences` gave them.
#[must_use]
pub fn on_day(occurrences: &[Occurrence], day: NaiveDate) -> Vec<Occurrence> {
    todo!()
}

/// How a month cell with room for `rows` lines shows `count` events: `(shown, more)`. When all
/// fit, all; otherwise one line fewer, and "+N more" in the last line for the rest.
#[must_use]
pub fn month_cell(count: usize, rows: usize) -> (usize, usize) {
    todo!()
}

/// How many event lines a month cell `cell_px` high holds under its day number (`head_px`), at
/// `line_px` a line.
#[must_use]
pub fn month_cell_rows(cell_px: f32, head_px: f32, line_px: f32) -> usize {
    todo!()
}

/// "+3 more"
#[must_use]
pub fn more_label(more: usize) -> String {
    todo!()
}

/// The list's days: each day from `from` to `to` that has occurrences, with them.
#[must_use]
pub fn agenda(
    occurrences: &[Occurrence],
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<(NaiveDate, Vec<Occurrence>)> {
    todo!()
}

/// "Today", "Tomorrow", or "Friday 2 October": a list day's heading.
#[must_use]
pub fn agenda_day_label(day: NaiveDate, today: NaiveDate) -> String {
    todo!()
}

/// The view a `--screen` / settings name and an anchor make when nothing says which: the week of
/// today.
#[must_use]
pub fn default_view() -> ViewKind {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use chrono::NaiveTime;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn at(h: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, 0, 0).unwrap()
    }

    fn event(n: u32, title: &str, date: NaiveDate, start: u32) -> Event {
        let id = format!("00000000-0000-4000-8000-{n:012}");
        Event::create(&id, title, date, at(start), at(start + 1), None).unwrap()
    }

    // Wednesday 30 September 2026.
    fn wed() -> NaiveDate {
        d(2026, 9, 30)
    }

    #[test]
    fn each_view_shows_its_days() {
        assert_eq!(days_shown(ViewKind::Day, wed()), vec![wed()]);
        assert_eq!(
            days_shown(ViewKind::WorkWeek, wed()),
            vec![
                d(2026, 9, 28),
                d(2026, 9, 29),
                wed(),
                d(2026, 10, 1),
                d(2026, 10, 2)
            ]
        );
        let week = days_shown(ViewKind::Week, wed());
        assert_eq!(
            (week[0], week[6], week.len()),
            (d(2026, 9, 28), d(2026, 10, 4), 7)
        );
        let month = days_shown(ViewKind::Month, d(2026, 10, 15));
        // October 2026 starts on a Thursday: the grid starts on Monday 28 September.
        assert_eq!((month[0], month.len()), (d(2026, 9, 28), 42));
        assert_eq!(month[41], d(2026, 11, 8));
        let list = days_shown(ViewKind::Agenda, wed());
        assert_eq!((list[0], list[6], list.len()), (wed(), d(2026, 10, 6), 7));
    }

    #[test]
    fn the_month_view_lights_its_month_and_the_others_their_days() {
        assert_eq!(
            visible_range(ViewKind::Month, d(2026, 10, 15)),
            (d(2026, 10, 1), d(2026, 10, 31))
        );
        assert_eq!(
            visible_range(ViewKind::WorkWeek, wed()),
            (d(2026, 9, 28), d(2026, 10, 2))
        );
        assert_eq!(visible_range(ViewKind::Day, wed()), (wed(), wed()));
    }

    #[test]
    fn previous_and_next_move_by_the_views_length() {
        assert_eq!(step(ViewKind::Day, wed(), 1), d(2026, 10, 1));
        assert_eq!(step(ViewKind::Week, wed(), -1), d(2026, 9, 23));
        assert_eq!(step(ViewKind::WorkWeek, wed(), 1), d(2026, 10, 7));
        assert_eq!(step(ViewKind::Agenda, wed(), 1), d(2026, 10, 7));
        assert_eq!(step(ViewKind::Month, d(2026, 1, 31), 1), d(2026, 2, 28));
        assert_eq!(step(ViewKind::Month, d(2026, 1, 15), -1), d(2025, 12, 15));
        assert_eq!(step(ViewKind::Month, d(2026, 12, 15), 1), d(2027, 1, 15));
    }

    #[test]
    fn each_view_has_its_title() {
        assert_eq!(title(ViewKind::Day, wed()), "Wednesday, 30 September 2026");
        assert_eq!(
            title(ViewKind::Week, wed()),
            "28 September - 4 October 2026"
        );
        assert_eq!(
            title(ViewKind::WorkWeek, d(2026, 10, 7)),
            "5 - 9 October 2026"
        );
        assert_eq!(title(ViewKind::Month, wed()), "September 2026");
        assert_eq!(
            range_title(d(2026, 12, 28), d(2027, 1, 3)),
            "28 December 2026 - 3 January 2027"
        );
    }

    #[test]
    fn a_weekly_event_is_in_the_next_week_too() {
        let mut e = event(1, "Standup", wed(), 9);
        e.repeat = Some(crate::rrule::Rule::new(crate::rrule::Freq::Weekly));
        let events = vec![e.check().unwrap(), event(2, "Lunch", wed(), 12)];
        let this = occurrences(&events, d(2026, 9, 28), d(2026, 10, 4), |_| true);
        assert_eq!(this.len(), 2);
        let next = occurrences(&events, d(2026, 10, 5), d(2026, 10, 11), |_| true);
        assert_eq!(
            next,
            vec![Occurrence {
                index: 0,
                first: d(2026, 10, 7),
                last: d(2026, 10, 7)
            }]
        );
        // a hidden calendar's events are not shown
        assert!(
            occurrences(&events, d(2026, 10, 5), d(2026, 10, 11), |e| e.title
                != "Standup")
            .is_empty()
        );
    }

    #[test]
    fn occurrences_come_by_day_then_all_day_first_then_by_start() {
        let id = "00000000-0000-4000-8000-000000000009";
        let events = vec![
            event(1, "Late", wed(), 15),
            event(2, "Early", wed(), 8),
            Event::create_all_day(id, "Holiday", d(2026, 9, 29), wed()).unwrap(),
        ];
        let all = occurrences(&events, d(2026, 9, 28), d(2026, 10, 4), |_| true);
        let titles: Vec<&str> = all.iter().map(|o| events[o.index].title.as_str()).collect();
        assert_eq!(titles, vec!["Holiday", "Early", "Late"]);
        let wednesday: Vec<&str> = on_day(&all, wed())
            .iter()
            .map(|o| events[o.index].title.as_str())
            .collect();
        assert_eq!(wednesday, vec!["Holiday", "Early", "Late"]);
        assert!(on_day(&all, d(2026, 10, 1)).is_empty());
    }

    #[test]
    fn a_month_cell_shows_what_fits_and_says_how_many_more() {
        assert_eq!(month_cell(2, 3), (2, 0));
        assert_eq!(month_cell(3, 3), (3, 0));
        assert_eq!(month_cell(5, 3), (2, 3));
        assert_eq!(month_cell(5, 1), (0, 5));
        assert_eq!(more_label(3), "+3 more");
        assert_eq!(month_cell_rows(100.0, 22.0, 18.0), 4);
        assert_eq!(month_cell_rows(30.0, 22.0, 18.0), 1);
        assert_eq!(month_cell_rows(f32::NAN, 22.0, 18.0), 1);
    }

    #[test]
    fn the_list_has_the_days_with_events_and_names_today_and_tomorrow() {
        let events = vec![event(1, "A", wed(), 9), event(2, "B", d(2026, 10, 2), 9)];
        let all = occurrences(&events, wed(), d(2026, 10, 6), |_| true);
        let days: Vec<NaiveDate> = agenda(&all, wed(), d(2026, 10, 6))
            .into_iter()
            .map(|(day, _)| day)
            .collect();
        assert_eq!(days, vec![wed(), d(2026, 10, 2)]);
        assert_eq!(
            agenda_day_label(wed(), wed()),
            "Today, Wednesday 30 September"
        );
        assert_eq!(
            agenda_day_label(d(2026, 10, 1), wed()),
            "Tomorrow, Thursday 1 October"
        );
        assert_eq!(agenda_day_label(d(2026, 10, 2), wed()), "Friday 2 October");
    }

    #[test]
    fn a_views_name_reads_back() {
        for v in ViewKind::ALL {
            assert_eq!(ViewKind::from_name(v.name()), Some(v));
        }
        assert_eq!(ViewKind::from_name("fortnight"), None);
        assert!(ViewKind::Week.is_time_grid() && !ViewKind::Month.is_time_grid());
    }
}
