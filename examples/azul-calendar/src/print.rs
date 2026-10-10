//! Printing, as Outlook 2010's FILE > Print: the print styles - Daily (one day a page, its
//! hours as an agenda), Weekly (Outlook's "Weekly Agenda": the week's seven days with the
//! events' times) and Monthly (the month as a grid of days with the events' titles) - the range
//! each one prints, and what a printout holds: its pages, each with its days and their events,
//! as plain data. `print_ui.rs` lays that out on paper (a DOM the size of an A4 sheet), makes
//! the PDF with azul's PDF writer and the preview from it.
//!
//! The paper's geometry is here too (the sheet, its margin, header and footer, the lines of a
//! day), so how many events a day of a page shows is counted, not guessed: a day with more
//! than fit says "+N more", as the month view does.

use chrono::{Datelike, Duration, NaiveDate, NaiveTime, Timelike};

use crate::{
    calendars::{self, Calendar, Colour},
    event::Event,
    views::{self, ViewKind},
    week, UNTITLED,
};

/// An A4 sheet at 96 dpi, in CSS px: its short and its long edge.
pub const A4_SHORT_PX: f32 = 794.0;
pub const A4_LONG_PX: f32 = 1123.0;
/// The sheet's margin on every side.
pub const MARGIN_PX: f32 = 36.0;
/// The header band (the title and the two small months), and the gap under its rule.
pub const HEADER_PX: f32 = 96.0;
pub const HEADER_GAP_PX: f32 = 8.0;
/// The footer band (when it was printed, the calendars' colours).
pub const FOOTER_PX: f32 = 22.0;
/// The Monthly grid: the weekday names' row, a day's number line and one event line.
pub const MONTH_NAMES_PX: f32 = 18.0;
pub const MONTH_HEAD_PX: f32 = 16.0;
pub const MONTH_LINE_PX: f32 = 14.0;
/// The Weekly boxes: the gap between them, a day's heading and one event line.
pub const WEEK_GAP_PX: f32 = 8.0;
pub const WEEK_HEAD_PX: f32 = 20.0;
pub const WEEK_LINE_PX: f32 = 15.0;
/// The Daily agenda: one event line, and the all-day lines shown at most.
pub const DAY_LINE_PX: f32 = 16.0;
pub const DAY_ALL_DAY_LINES: usize = 3;
/// The hours a Daily page shows at least (Outlook's 07:00 to 19:00); an event outside them
/// widens the page's hours to it.
pub const DAY_FIRST_HOUR: u32 = 7;
pub const DAY_END_HOUR: u32 = 19;

/// A print style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Style {
    /// A page a day: the all-day events, then the hours with the events that start in each.
    Daily,
    /// A page a week: Monday to Sunday, each day with its events and their times.
    Weekly,
    /// A page a month: the month's weeks as a grid of days, the events' titles in them.
    Monthly,
}

impl Style {
    /// Every style, in the page's order.
    pub const ALL: [Style; 3] = [Style::Daily, Style::Weekly, Style::Monthly];

    /// The style's name in the DOM ids and on stdout.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Style::Daily => "daily",
            Style::Weekly => "weekly",
            Style::Monthly => "monthly",
        }
    }

    /// The style's name for people, as Outlook's print page lists it (a key of the resources).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Style::Daily => "azcalendar-print-daily",
            Style::Weekly => "azcalendar-print-weekly",
            Style::Monthly => "azcalendar-print-monthly",
        }
    }

    /// The style's icon (a Material icon name).
    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Style::Daily => "view_day",
            Style::Weekly => "view_week",
            Style::Monthly => "calendar_view_month",
        }
    }

    /// A day goes down the long edge; a week and a month go across it.
    #[must_use]
    pub const fn landscape(self) -> bool {
        !matches!(self, Style::Daily)
    }

    /// The sheet, width and height, in CSS px.
    #[must_use]
    pub const fn page_px(self) -> (f32, f32) {
        if self.landscape() {
            (A4_LONG_PX, A4_SHORT_PX)
        } else {
            (A4_SHORT_PX, A4_LONG_PX)
        }
    }

    /// The most pages one printout of the style makes: a month of days, half a year of weeks,
    /// a year of months.
    #[must_use]
    pub const fn max_pages(self) -> usize {
        match self {
            Style::Daily => 31,
            Style::Weekly => 26,
            Style::Monthly => 12,
        }
    }

    /// The style that prints what `view` shows.
    #[must_use]
    pub const fn for_view(view: ViewKind) -> Style {
        match view {
            ViewKind::Day | ViewKind::Schedule => Style::Daily,
            ViewKind::WorkWeek | ViewKind::Week | ViewKind::Agenda => Style::Weekly,
            ViewKind::Month => Style::Monthly,
        }
    }
}

/// The page's area between the header and the footer, inside the margin: width and height.
#[must_use]
pub fn content_px(style: Style) -> (f32, f32) {
    let (w, h) = style.page_px();
    (
        w - 2.0 * MARGIN_PX,
        h - 2.0 * MARGIN_PX - HEADER_PX - HEADER_GAP_PX - FOOTER_PX,
    )
}

/// How many lines of `line_px` a box `box_px` tall holds under its `head_px` heading (and 4 px
/// of padding): one at least.
#[must_use]
pub fn lines_in(box_px: f32, head_px: f32, line_px: f32) -> usize {
    if !(box_px.is_finite() && head_px.is_finite() && line_px.is_finite()) || line_px <= 0.0 {
        return 1;
    }
    (((box_px - head_px - 4.0) / line_px).floor().max(1.0)) as usize
}

/// The days one page of `style` holds around `day`: the day, its week (Monday to Sunday), its
/// month.
#[must_use]
pub fn unit(style: Style, day: NaiveDate) -> (NaiveDate, NaiveDate) {
    match style {
        Style::Daily => (day, day),
        Style::Weekly => {
            let monday = week::week_start(day);
            (monday, monday + Duration::days(6))
        }
        Style::Monthly => (views::month_start(day), views::month_end(day)),
    }
}

/// The first day of each page from the page of `from` to the page of `to`, `limit` at most.
#[must_use]
pub fn page_starts(style: Style, from: NaiveDate, to: NaiveDate, limit: usize) -> Vec<NaiveDate> {
    let mut starts = Vec::new();
    let mut page = unit(style, from).0;
    while page <= to && starts.len() < limit {
        starts.push(page);
        let next = match style {
            Style::Daily => page.succ_opt(),
            Style::Weekly => page.checked_add_signed(Duration::days(7)),
            Style::Monthly => views::month_end(page).succ_opt(),
        };
        match next {
            Some(next) => page = next,
            None => break,
        }
    }
    starts
}

/// What to print: the style, and the range from the first day of its first page to the last
/// day of its last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub style: Style,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl Settings {
    /// `style` from `from` to `to`, widened to whole pages, `Style::max_pages` of them at most
    /// (the end comes back to the last page that fits). An end before the start is the start's
    /// page.
    #[must_use]
    pub fn new(style: Style, from: NaiveDate, to: NaiveDate) -> Settings {
        let first = unit(style, from).0;
        let mut last = unit(style, to.max(from)).1;
        let starts = page_starts(style, first, last, style.max_pages());
        if let Some(&final_page) = starts.last() {
            last = last.min(unit(style, final_page).1);
        }
        Settings {
            style,
            from: first,
            to: last,
        }
    }

    /// What the window shows: the view's style, one page of it around `anchor`.
    #[must_use]
    pub fn for_view(view: ViewKind, anchor: NaiveDate) -> Settings {
        let style = Style::for_view(view);
        Settings::new(style, anchor, anchor)
    }

    /// Another style: one page of it, at the start of the range.
    #[must_use]
    pub fn with_style(self, style: Style) -> Settings {
        Settings::new(style, self.from, self.from)
    }

    /// A new start: the end moves along when it was before it.
    #[must_use]
    pub fn with_start(self, day: NaiveDate) -> Settings {
        Settings::new(self.style, day, self.to.max(day))
    }

    /// A new end: the start moves back when it was after it.
    #[must_use]
    pub fn with_end(self, day: NaiveDate) -> Settings {
        Settings::new(self.style, self.from.min(day), day)
    }

    /// How many pages the printout has.
    #[must_use]
    pub fn pages(self) -> usize {
        page_starts(self.style, self.from, self.to, self.style.max_pages()).len()
    }

    /// The file a printout is saved as: `AzCalendar 2026-10.pdf`, `AzCalendar 2026-10-05 -
    /// 2026-10-11.pdf`, `AzCalendar 2026-10-07.pdf`.
    #[must_use]
    pub fn file_name(self) -> String {
        let format = if self.style == Style::Monthly {
            "%Y-%m"
        } else {
            "%Y-%m-%d"
        };
        let first = self.from.format(format).to_string();
        let last = self.to.format(format).to_string();
        if first == last {
            format!("AzCalendar {first}.pdf")
        } else {
            format!("AzCalendar {first} - {last}.pdf")
        }
    }

    /// The page note: "1 page, A4 landscape", "7 pages, A4 portrait".
    #[must_use]
    pub fn describe(self) -> String {
        let pages = self.pages();
        use azul_appkit::l10n::{t_args, Arg};
        t_args(
            if self.style.landscape() {
                "azcalendar-print-pages-landscape"
            } else {
                "azcalendar-print-pages-portrait"
            },
            &[("pages", Arg::from(pages))],
        )
    }
}

/// An event on a printed day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub title: String,
    pub location: String,
    pub all_day: bool,
    pub start: NaiveTime,
    pub end: NaiveTime,
    /// Its calendar's colour.
    pub colour: Colour,
}

impl Item {
    /// "09:00 - 10:00", or "All day".
    #[must_use]
    pub fn times(&self) -> String {
        if self.all_day {
            azul_appkit::l10n::t("azcalendar-all-day")
        } else {
            week::time_range(self.start, self.end)
        }
    }

    /// "09:00 Standup", or the title alone for an all-day event (a month day's line).
    #[must_use]
    pub fn short_line(&self) -> String {
        if self.all_day {
            self.title.clone()
        } else {
            format!("{} {}", self.start.format("%H:%M"), self.title)
        }
    }
}

/// A printed day: its events in the calendar's order (all-day first, then by start).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Day {
    pub date: NaiveDate,
    /// The day is in the page's month (a Monthly page's other days are dimmer).
    pub in_month: bool,
    pub items: Vec<Item>,
}

/// A printed page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// "Wednesday, 7 October 2026", "5 - 11 October 2026", "October 2026".
    pub title: String,
    /// "Week 41" (Daily and Weekly), empty on a Monthly page.
    pub subtitle: String,
    /// The page's first day (a Monthly page's: the first of its month).
    pub first: NaiveDate,
    /// The page's last day (a Monthly page's: the last of its month).
    pub last: NaiveDate,
    /// One day (Daily), seven (Weekly), the month's weeks of seven (Monthly).
    pub days: Vec<Day>,
}

/// A printout: everything its pages show, in plain data (made on the UI thread, laid out on
/// paper anywhere).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub settings: Settings,
    /// The day it was printed (in the footer).
    pub printed: NaiveDate,
    pub pages: Vec<Page>,
    /// The calendars whose events are in it, with their colours (in the footer).
    pub legend: Vec<(String, Colour)>,
    /// The language it is printed in (laid out on paper on another thread: it speaks this).
    pub voice: azul_appkit::l10n::Voice,
}

/// The days of the month of `first` a Monthly page shows: the weeks (Monday first) that hold a
/// day of the month.
#[must_use]
pub fn month_days(first: NaiveDate) -> Vec<NaiveDate> {
    let grid = azul_pim::dates::month_grid(first, chrono::Weekday::Mon);
    grid.chunks(7)
        .filter(|week| week.iter().any(|d| d.month() == first.month()))
        .flat_map(|week| week.iter().copied())
        .collect()
}

/// The printout of `settings`: the `shown` events of `events` (a repeating one on every date its
/// rule makes) in the colours of their calendars (`calendar_list`), printed on `printed`.
pub fn job(
    settings: Settings,
    printed: NaiveDate,
    events: &[Event],
    calendar_list: &[Calendar],
    shown: impl Fn(&Event) -> bool,
) -> Job {
    let calendar_of = |e: &Event| calendars::calendar_of(calendar_list, &e.calendar);
    let mut legend: Vec<(String, Colour)> = Vec::new();
    let mut legend_ids: Vec<String> = Vec::new();
    let mut pages = Vec::new();
    let style = settings.style;
    for first in page_starts(style, settings.from, settings.to, style.max_pages()) {
        let (page_first, page_last) = unit(style, first);
        let dates: Vec<NaiveDate> = match style {
            Style::Daily => vec![first],
            Style::Weekly => week::week_days(first).to_vec(),
            Style::Monthly => month_days(first),
        };
        let (Some(&from), Some(&to)) = (dates.first(), dates.last()) else {
            continue;
        };
        let occurrences = views::occurrences(events, from, to, &shown);
        let days = dates
            .iter()
            .map(|&date| Day {
                date,
                in_month: style != Style::Monthly || date.month() == first.month(),
                items: views::on_day(&occurrences, date)
                    .iter()
                    .map(|o| {
                        let e = &events[o.index];
                        let calendar = calendar_of(e);
                        let (id, name, colour) = calendar.map_or_else(
                            || (String::new(), String::from(calendars::DEFAULT_NAME), Colour::Blue),
                            |c| (c.id.clone(), c.name.clone(), c.colour),
                        );
                        if !legend_ids.contains(&id) {
                            legend_ids.push(id);
                            legend.push((name, colour));
                        }
                        Item {
                            title: if e.title.trim().is_empty() {
                                azul_appkit::l10n::t(UNTITLED)
                            } else {
                                e.title.clone()
                            },
                            location: e.location.clone(),
                            all_day: e.all_day,
                            start: e.start,
                            end: e.end,
                            colour,
                        }
                    })
                    .collect(),
            })
            .collect();
        let week_number = azul_appkit::l10n::t_args(
            "azcalendar-print-week",
            &[("week", azul_appkit::l10n::Arg::from(page_first.iso_week().week()))],
        );
        let (title, subtitle) = match style {
            Style::Daily => (views::title(ViewKind::Day, first), week_number),
            Style::Weekly => (views::range_title(page_first, page_last), week_number),
            Style::Monthly => (
                crate::day_text(azul_appkit::l10n::DateStyle::MonthYear, first),
                String::new(),
            ),
        };
        pages.push(Page {
            title,
            subtitle,
            first: page_first,
            last: page_last,
            days,
        });
    }
    Job {
        settings,
        printed,
        pages,
        legend,
        voice: azul_appkit::l10n::Voice::here(),
    }
}

/// The hours a Daily page shows for `items`, `(first, end)` (the end hour not included):
/// 07:00 to 19:00, wider when an event starts earlier or ends later.
#[must_use]
pub fn day_hours(items: &[Item]) -> (u32, u32) {
    let mut first = DAY_FIRST_HOUR;
    let mut end = DAY_END_HOUR;
    for item in items.iter().filter(|i| !i.all_day) {
        first = first.min(item.start.hour());
        let end_minute = week::minute_of_day(item.end);
        let ends_in = end_minute.div_ceil(60);
        end = end.max(ends_in).max(item.start.hour() + 1);
    }
    (first, end.min(24))
}

/// The events of a Daily page that start in `hour`.
#[must_use]
pub fn in_hour(items: &[Item], hour: u32) -> Vec<&Item> {
    items
        .iter()
        .filter(|i| !i.all_day && i.start.hour() == hour)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn event(n: u32, title: &str, date: NaiveDate, start: NaiveTime, end: NaiveTime) -> Event {
        let id = format!("00000000-0000-4000-8000-{n:012}");
        Event::create(&id, title, date, start, end, None).unwrap()
    }

    fn item(start: NaiveTime, end: NaiveTime) -> Item {
        Item {
            title: String::from("x"),
            location: String::new(),
            all_day: false,
            start,
            end,
            colour: Colour::Blue,
        }
    }

    // Wednesday 7 October 2026.
    fn wed() -> NaiveDate {
        d(2026, 10, 7)
    }

    #[test]
    fn the_print_page_starts_with_the_style_of_the_view() {
        assert_eq!(Style::for_view(ViewKind::Day), Style::Daily);
        assert_eq!(Style::for_view(ViewKind::Schedule), Style::Daily);
        assert_eq!(Style::for_view(ViewKind::Week), Style::Weekly);
        assert_eq!(Style::for_view(ViewKind::WorkWeek), Style::Weekly);
        assert_eq!(Style::for_view(ViewKind::Month), Style::Monthly);
        let s = Settings::for_view(ViewKind::Month, wed());
        assert_eq!((s.style, s.from, s.to), (Style::Monthly, d(2026, 10, 1), d(2026, 10, 31)));
        let s = Settings::for_view(ViewKind::Week, wed());
        assert_eq!((s.from, s.to), (d(2026, 10, 5), d(2026, 10, 11)));
        assert_eq!(s.pages(), 1);
    }

    #[test]
    fn a_range_is_whole_pages_and_its_end_follows_its_start() {
        let s = Settings::new(Style::Weekly, d(2026, 10, 7), d(2026, 10, 20));
        assert_eq!((s.from, s.to), (d(2026, 10, 5), d(2026, 10, 25)));
        assert_eq!(s.pages(), 3);
        // A start after the end takes the end along; an end before the start the start.
        let later = s.with_start(d(2026, 11, 2));
        assert_eq!((later.from, later.to), (d(2026, 11, 2), d(2026, 11, 8)));
        let earlier = s.with_end(d(2026, 9, 30));
        assert_eq!((earlier.from, earlier.to), (d(2026, 9, 28), d(2026, 10, 4)));
        // Another style is one page of it at the start.
        let daily = s.with_style(Style::Daily);
        assert_eq!((daily.from, daily.to), (d(2026, 10, 5), d(2026, 10, 5)));
        let monthly = s.with_style(Style::Monthly);
        assert_eq!((monthly.from, monthly.to), (d(2026, 10, 1), d(2026, 10, 31)));
    }

    #[test]
    fn a_printout_has_at_most_the_styles_pages() {
        let s = Settings::new(Style::Daily, d(2026, 1, 1), d(2026, 12, 31));
        assert_eq!(s.pages(), 31);
        assert_eq!(s.to, d(2026, 1, 31));
        let s = Settings::new(Style::Monthly, d(2026, 1, 15), d(2030, 1, 1));
        assert_eq!((s.from, s.to, s.pages()), (d(2026, 1, 1), d(2026, 12, 31), 12));
        let s = Settings::new(Style::Monthly, d(2026, 11, 15), d(2027, 2, 3));
        assert_eq!(
            page_starts(s.style, s.from, s.to, 99),
            vec![d(2026, 11, 1), d(2026, 12, 1), d(2027, 1, 1), d(2027, 2, 1)]
        );
    }

    #[test]
    fn a_printout_is_named_after_its_range() {
        crate::l10n::in_english();
        let month = Settings::for_view(ViewKind::Month, wed());
        assert_eq!(month.file_name(), "AzCalendar 2026-10.pdf");
        let week = Settings::for_view(ViewKind::Week, wed());
        assert_eq!(week.file_name(), "AzCalendar 2026-10-05 - 2026-10-11.pdf");
        let day = Settings::for_view(ViewKind::Day, wed());
        assert_eq!(day.file_name(), "AzCalendar 2026-10-07.pdf");
        assert_eq!(month.describe(), "1 page, A4 landscape");
        assert_eq!(
            Settings::new(Style::Daily, wed(), wed() + Duration::days(2)).describe(),
            "3 pages, A4 portrait"
        );
    }

    #[test]
    fn a_monthly_page_has_the_weeks_of_its_month_and_dims_the_other_days() {
        // October 2026 starts on a Thursday and ends on a Saturday: five weeks.
        let days = month_days(d(2026, 10, 1));
        assert_eq!((days.len(), days[0], days[34]), (35, d(2026, 9, 28), d(2026, 11, 1)));
        // February 2027 starts on a Monday: four weeks.
        assert_eq!(month_days(d(2027, 2, 1)).len(), 28);
        // A title can be empty in a file made elsewhere (`Event::create` wants one).
        let mut untitled = event(2, "x", d(2026, 9, 29), at(12, 0), at(13, 0));
        untitled.title = String::new();
        let events = vec![event(1, "Standup", wed(), at(9, 30), at(10, 0)), untitled];
        let calendars = vec![Calendar::default_calendar()];
        let job = job(
            Settings::for_view(ViewKind::Month, wed()),
            wed(),
            &events,
            &calendars,
            |_| true,
        );
        assert_eq!(job.pages.len(), 1);
        let page = &job.pages[0];
        assert_eq!(page.title, "October 2026");
        assert!(!page.days[0].in_month && page.days[3].in_month);
        // The other month's day prints its event too, an untitled one as "(No title)".
        assert_eq!(page.days[1].items[0].title, "(No title)");
        let wednesday = page.days.iter().find(|day| day.date == wed()).unwrap();
        assert_eq!(wednesday.items[0].short_line(), "09:30 Standup");
        assert_eq!(wednesday.items[0].times(), "09:30 - 10:00");
        assert_eq!(job.legend, vec![(String::from("Calendar"), Colour::Blue)]);
    }

    #[test]
    fn a_weekly_page_is_monday_to_sunday_and_hidden_calendars_are_left_out() {
        crate::l10n::in_english();
        let events = vec![
            event(1, "Shown", wed(), at(9, 0), at(10, 0)),
            event(2, "Hidden", wed(), at(11, 0), at(12, 0)),
        ];
        let job = job(
            Settings::for_view(ViewKind::Week, wed()),
            wed(),
            &events,
            &[Calendar::default_calendar()],
            |e| e.title != "Hidden",
        );
        let page = &job.pages[0];
        assert_eq!(page.title, "5 - 11 October 2026");
        assert_eq!(page.subtitle, "Week 41");
        assert_eq!(page.days.len(), 7);
        assert_eq!(page.days[0].date, d(2026, 10, 5));
        let titles: Vec<&str> = page.days[2].items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, vec!["Shown"]);
    }

    #[test]
    fn a_daily_page_shows_seven_to_seven_and_any_event_outside_it() {
        assert_eq!(day_hours(&[]), (7, 19));
        assert_eq!(day_hours(&[item(at(6, 30), at(7, 0))]), (6, 19));
        assert_eq!(day_hours(&[item(at(20, 0), at(21, 15))]), (7, 22));
        assert_eq!(day_hours(&[item(at(23, 0), at(23, 59))]), (7, 24));
        let items = [
            item(at(9, 0), at(10, 0)),
            item(at(9, 45), at(11, 0)),
            item(at(10, 0), at(11, 0)),
        ];
        assert_eq!(in_hour(&items, 9).len(), 2);
        assert_eq!(in_hour(&items, 10).len(), 1);
    }

    #[test]
    fn every_style_leaves_room_for_lines_on_its_sheet() {
        for style in Style::ALL {
            let (w, h) = content_px(style);
            assert!(w > 600.0 && h > 500.0, "{style:?}: {w} x {h}");
        }
        assert_eq!(lines_in(100.0, 16.0, 14.0), 5);
        assert_eq!(lines_in(10.0, 16.0, 14.0), 1);
        assert_eq!(lines_in(f32::NAN, 16.0, 14.0), 1);
    }
}
