//! What the sidebar shows and what its tags mean: the pure part of `ui_sidebar.rs`, so that the
//! rules (which rows there are, what a tag stands for, which quick-access buttons the rail has, what
//! the phone's sheets list) are tested without a window.
//!
//! The sidebar is the calendars as coloured check rows (the sheet picture of the wireframe: a box in
//! the calendar's colour, its name, "Add a calendar" at the end), the open tasks of the shared task
//! store as check rows under them, and in the collapsed rail the app's two quick-access buttons:
//! Today and, nearest the thumb, New appointment.

use chrono::NaiveDate;

use crate::calendars::Colour;

/// The most calendars the sidebar lists; a calendar folder holding more says so in the log and
/// keeps the first ones (every row of a list is a DOM node of the window).
pub(crate) const MAX_CALENDAR_ROWS: usize = 100;
/// The most open tasks the sidebar lists under the calendars.
pub(crate) const MAX_TASK_ROWS: usize = 6;

/// What a row of the sidebar stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NavTarget {
    /// The calendar at this index of the app's list: a click shows or hides it.
    Calendar(usize),
    /// "Add a calendar": opens the Calendars page.
    AddCalendar,
    /// The task at this index of the app's list: a click ticks it off.
    Task(usize),
}

const CALENDAR_BASE: usize = 1_000;
const TASK_BASE: usize = 1_000_000;
const ADD_CALENDAR: usize = 999;

/// The tag a row carries for `target`.
#[must_use]
pub(crate) const fn tag_of(target: NavTarget) -> usize {
    match target {
        NavTarget::Calendar(i) => CALENDAR_BASE + i,
        NavTarget::AddCalendar => ADD_CALENDAR,
        NavTarget::Task(i) => TASK_BASE + i,
    }
}

/// What `tag` stands for; `None` for a tag no row has.
#[must_use]
pub(crate) const fn target_of(tag: usize) -> Option<NavTarget> {
    if tag == ADD_CALENDAR {
        Some(NavTarget::AddCalendar)
    } else if tag >= TASK_BASE {
        Some(NavTarget::Task(tag - TASK_BASE))
    } else if tag >= CALENDAR_BASE {
        Some(NavTarget::Calendar(tag - CALENDAR_BASE))
    } else {
        None
    }
}

/// One row of the plan.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NavRow {
    pub label: String,
    pub tag: usize,
    /// `Some(shown)`: a check box before the label (a calendar's, a task's).
    pub checked: Option<bool>,
    /// The colour of the box (a calendar's); `None`: the sidebar's ink.
    pub colour: Option<Colour>,
    /// The icon (a Material name); empty: none.
    pub icon: &'static str,
}

/// One section of the plan.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NavSection {
    pub title: String,
    pub rows: Vec<NavRow>,
}

/// What the plan is made from.
#[derive(Debug, Clone)]
pub(crate) struct NavInput<'a> {
    /// The calendars, default first: their names as the window says them, their colours, whether
    /// "My calendars" ticks them.
    pub calendars: &'a [(String, Colour, bool)],
    /// The open tasks: their titles.
    pub tasks: &'a [String],
    /// The words: the section titles and the "Add a calendar" row (resolved by the caller).
    pub calendars_title: String,
    pub tasks_title: String,
    pub add_calendar: String,
}

/// The sections of the sidebar. A calendar row's box is ticked while its calendar shows; the
/// "Add a calendar" row comes last in its section.
#[must_use]
pub(crate) fn plan(input: &NavInput<'_>) -> Vec<NavSection> {
    let mut rows: Vec<NavRow> = input
        .calendars
        .iter()
        .take(MAX_CALENDAR_ROWS)
        .enumerate()
        .map(|(i, (name, colour, shown))| NavRow {
            label: name.clone(),
            tag: tag_of(NavTarget::Calendar(i)),
            checked: Some(*shown),
            colour: Some(*colour),
            icon: "",
        })
        .collect();
    rows.push(NavRow {
        label: input.add_calendar.clone(),
        tag: tag_of(NavTarget::AddCalendar),
        checked: None,
        colour: None,
        icon: "add",
    });
    let mut sections = vec![NavSection {
        title: input.calendars_title.clone(),
        rows,
    }];
    if !input.tasks.is_empty() {
        sections.push(NavSection {
            title: input.tasks_title.clone(),
            rows: input
                .tasks
                .iter()
                .take(MAX_TASK_ROWS)
                .enumerate()
                .map(|(i, title)| NavRow {
                    label: title.clone(),
                    tag: tag_of(NavTarget::Task(i)),
                    checked: Some(false),
                    colour: None,
                    icon: "",
                })
                .collect(),
        });
    }
    sections
}

/// How many calendars past [`MAX_CALENDAR_ROWS`] the sidebar leaves out.
#[must_use]
pub(crate) const fn calendars_left_out(count: usize) -> usize {
    count.saturating_sub(MAX_CALENDAR_ROWS)
}

/// The red, green and blue of `colour`'s edge (its swatch): the sidebar tints a calendar's box with
/// it; `dark` picks the colour's twin for a dark window. A paint that is no `#rrggbb` is a grey.
#[must_use]
pub(crate) fn rgb_of(colour: Colour, dark: bool) -> (u8, u8, u8) {
    let paint = colour.paint();
    let hex = if dark { paint.dark_edge } else { paint.light_edge };
    let digits = hex.trim_start_matches('#');
    if digits.len() != 6 {
        return (128, 128, 128);
    }
    let byte = |i: usize| digits.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok());
    match (byte(0), byte(2), byte(4)) {
        (Some(r), Some(g), Some(b)) => (r, g, b),
        _ => (128, 128, 128),
    }
}

/// A calendar's box clicked: it hides when it showed and shows when it was hidden. Returns whether
/// the calendar is hidden now.
pub(crate) fn toggle_hidden(hidden: &mut std::collections::BTreeSet<String>, id: &str) -> bool {
    if hidden.remove(id) {
        false
    } else {
        hidden.insert(id.to_string());
        true
    }
}

// ---- the status slot: the meeting links ----

/// How the meeting links of the events stand, for the status slot above the rail's toggle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LinkStatus {
    /// How many of the links the meeting server has registered, 0 to 1.
    pub fraction: f32,
    /// The same as a whole percent, 0 to 100.
    pub percent: u32,
    pub tone: LinkTone,
}

/// What the pie of the links means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkTone {
    /// Every link is registered.
    Good,
    /// Links are on their way to the server.
    Busy,
    /// Links wait for the server, nothing is wrong yet.
    Warning,
    /// The server was tried and did not take them.
    Danger,
}

/// The status of `total` events with a meeting link, `pending` of them not registered yet,
/// `syncing` on their way now, `failed` when the last try was refused or went unanswered. `None`
/// when no event has a link: there is nothing to say.
#[must_use]
pub(crate) fn link_status(total: usize, pending: usize, syncing: usize, failed: bool) -> Option<LinkStatus> {
    if total == 0 {
        return None;
    }
    let pending = pending.min(total);
    let done = total - pending;
    let tone = if syncing > 0 {
        LinkTone::Busy
    } else if pending == 0 {
        LinkTone::Good
    } else if failed {
        LinkTone::Danger
    } else {
        LinkTone::Warning
    };
    Some(LinkStatus {
        fraction: done as f32 / total as f32,
        percent: u32::try_from(done * 100 / total).unwrap_or(100),
        tone,
    })
}

// ---- the rail's quick-access buttons ----

/// A quick-access button of the rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Quick {
    /// The view moves to today.
    Today,
    /// A new appointment, where the view is: the most used, so the one nearest the thumb.
    NewEvent,
}

/// What the rail holds, in the order it is drawn (the last is nearest the thumb): the key of the
/// button's name, its icon, and what it is.
pub(crate) static QUICK: [(&str, &str, Quick); 2] = [
    ("azcalendar-today", "today", Quick::Today),
    ("azcalendar-new-appointment", "add", Quick::NewEvent),
];

/// The tag of a quick-access button (its place in [`QUICK`]).
#[must_use]
pub(crate) fn quick_tag(quick: Quick) -> usize {
    QUICK.iter().position(|(_, _, q)| *q == quick).unwrap_or(0)
}

/// The button a tag stands for.
#[must_use]
pub(crate) fn quick_of(tag: usize) -> Option<Quick> {
    QUICK.get(tag).map(|(_, _, q)| *q)
}

// ---- the phone's action sheets ----

/// A row of an event's sheet (a long press on an event).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventAction {
    Edit,
    JoinMeeting,
    CopyLink,
    /// A repeating event skips this day.
    DeleteOccurrence,
    /// The event, with all of its repeats.
    Delete,
}

/// What an event's sheet lists, in order: the key of the row's name, its icon, the action, and
/// whether it is dangerous (drawn in red, last).
pub(crate) static EVENT_SHEET: [(&str, &str, EventAction, bool); 5] = [
    ("azcalendar-sheet-edit", "edit", EventAction::Edit, false),
    ("azcalendar-sheet-join", "video_call", EventAction::JoinMeeting, false),
    ("azcalendar-sheet-copy-link", "link", EventAction::CopyLink, false),
    ("azcalendar-delete-occurrence", "event_busy", EventAction::DeleteOccurrence, true),
    ("azcalendar-delete", "delete", EventAction::Delete, true),
];

/// Which rows of the event sheet an event has: Join and Copy link need a meeting, "Delete this
/// occurrence" a repeating event.
#[must_use]
pub(crate) fn event_sheet_rows(has_meeting: bool, repeats: bool) -> Vec<usize> {
    EVENT_SHEET
        .iter()
        .enumerate()
        .filter(|(_, (_, _, action, _))| match action {
            EventAction::JoinMeeting | EventAction::CopyLink => has_meeting,
            EventAction::DeleteOccurrence => repeats,
            EventAction::Edit | EventAction::Delete => true,
        })
        .map(|(i, _)| i)
        .collect()
}

/// What File's old items are (the `...` of the location: a phone's sheet, a desktop's menu).
pub(crate) static FILE_ITEMS: [(&str, &str, crate::args::BackstagePage); 6] = [
    ("azcalendar-file-info", "info", crate::args::BackstagePage::Info),
    ("azcalendar-file-open", "folder_open", crate::args::BackstagePage::Open),
    ("azcalendar-file-print", "print", crate::args::BackstagePage::Print),
    ("azcalendar-file-calendars", "edit_calendar", crate::args::BackstagePage::Calendars),
    ("azcalendar-file-options", "settings", crate::args::BackstagePage::Options),
    ("azcalendar-file-about", "help", crate::args::BackstagePage::About),
];

// ---- the location: what tab 0 says ----

/// The crumb of the year and the crumb of the month in the location's trail.
pub(crate) const TAG_YEAR: usize = 0;
pub(crate) const TAG_MONTH: usize = 1;

/// The words a place is said in (the window's language; a test's fake says it in English).
pub(crate) trait Say {
    fn year(&self, year: i32) -> String;
    fn month(&self, month: u32) -> String;
    /// A day for a name: "Sat 28 Nov".
    fn day(&self, day: NaiveDate) -> String;
    /// A range for a name: "23 - 29 Nov", "28 Nov - 4 Dec".
    fn range(&self, first: NaiveDate, last: NaiveDate) -> String;
    /// The list's name: "Next 7 Days".
    fn list(&self) -> String;
}

/// What the location block of tab 0 shows: the trail of crumbs over the name, big and bold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Place {
    /// The parents, outermost first, each with its tag ([`TAG_YEAR`], [`TAG_MONTH`]).
    pub crumbs: Vec<(String, usize)>,
    pub name: String,
}

/// The place `kind` shows around `anchor`; `shown` is its first and last day ([`crate::views::visible_range`]).
/// The month is the year's child ("2026 / November"), a week or a day the month's ("2026 / November /
/// 23 - 29 Nov").
#[must_use]
pub(crate) fn place_of(
    kind: crate::views::ViewKind,
    anchor: NaiveDate,
    shown: (NaiveDate, NaiveDate),
    say: &dyn Say,
) -> Place {
    use chrono::Datelike;
    use crate::views::ViewKind;
    let year = (say.year(anchor.year()), TAG_YEAR);
    // a week that crosses a month stands in the month it starts in
    let first = shown.0;
    let month = (say.month(first.month()), TAG_MONTH);
    match kind {
        ViewKind::Month => Place { crumbs: vec![year], name: say.month(anchor.month()) },
        ViewKind::Week | ViewKind::WorkWeek => Place {
            crumbs: vec![year, month],
            name: say.range(shown.0, shown.1),
        },
        ViewKind::Day | ViewKind::Schedule => Place {
            crumbs: vec![year, (say.month(anchor.month()), TAG_MONTH)],
            name: say.day(anchor),
        },
        ViewKind::Agenda => Place { crumbs: vec![year, month], name: say.list() },
    }
}

// ---- the pen of the location: "go to a date" ----

/// The day a typed text names, for the pen of the location ("2026-12-24", "24.12.2026",
/// "24 Dec", "today", "tomorrow", "+3" days, "-1w"): `today` is what "today" means. `None` for a
/// text that is no day. The year defaults to the shown day's.
#[must_use]
pub(crate) fn parse_goto(text: &str, today: NaiveDate, shown: NaiveDate) -> Option<NaiveDate> {
    use chrono::{Datelike, Duration};
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_lowercase();
    match lower.as_str() {
        "today" | "heute" => return Some(today),
        "tomorrow" | "morgen" => return Some(today + Duration::days(1)),
        "yesterday" | "gestern" => return Some(today - Duration::days(1)),
        _ => {}
    }
    // +3, -2, +1w, -1m: days, weeks, months from the shown day
    if let Some(rest) = lower.strip_prefix('+').or_else(|| lower.strip_prefix('-')) {
        let sign: i64 = if lower.starts_with('-') { -1 } else { 1 };
        let (digits, unit) = rest.split_at(rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len()));
        let n: i64 = digits.parse().ok()?;
        let n = n.clamp(0, 5_000);
        return match unit {
            "" | "d" => Some(shown + Duration::days(sign * n)),
            "w" => Some(shown + Duration::days(7 * sign * n)),
            "m" => {
                let index = i64::from(shown.year()) * 12 + i64::from(shown.month0()) + sign * n;
                let year = i32::try_from(index.div_euclid(12)).ok()?;
                let month0 = u32::try_from(index.rem_euclid(12)).ok()?;
                crate::week::picked_date(year, month0 + 1, shown.day())
            }
            _ => None,
        };
    }
    // 2026-12-24 and 2026/12/24
    let numbers: Vec<&str> = lower
        .split(|c: char| matches!(c, '-' | '/' | '.' | ' ' | ','))
        .filter(|p| !p.is_empty())
        .collect();
    let parsed: Option<Vec<u32>> = numbers.iter().map(|s| s.parse::<u32>().ok()).collect();
    let Some(n) = parsed else {
        // 24 Dec, Dec 24, 24 December 2026
        return parse_named_month(&numbers, shown);
    };
    let year_of = |y: u32| i32::try_from(y).ok();
    match (numbers.as_slice(), n.as_slice()) {
        // 2026-12-24 and 2026/12/24
        ([y, ..], [yy, m, d]) if y.len() == 4 => crate::week::picked_date(year_of(*yy)?, *m, *d),
        // 24.12.2026
        ([_, _, y], [d, m, yy]) if y.len() == 4 => crate::week::picked_date(year_of(*yy)?, *m, *d),
        // 2026-12: the month's first day
        ([y, _], [yy, m]) if y.len() == 4 => crate::week::picked_date(year_of(*yy)?, *m, 1),
        // 24.12. and 24/12: this year's (the shown day's)
        (_, [d, m]) => crate::week::picked_date(shown.year(), *m, *d),
        _ => None,
    }
}

/// "24 dec", "dec 24", "24 december 2026" (English and German month names, three letters or the
/// whole word).
fn parse_named_month(parts: &[&str], shown: NaiveDate) -> Option<NaiveDate> {
    use chrono::Datelike;
    static MONTHS: [(&str, &str); 12] = [
        ("jan", "jan"),
        ("feb", "feb"),
        ("mar", "mär"),
        ("apr", "apr"),
        ("may", "mai"),
        ("jun", "jun"),
        ("jul", "jul"),
        ("aug", "aug"),
        ("sep", "sep"),
        ("oct", "okt"),
        ("nov", "nov"),
        ("dec", "dez"),
    ];
    let month_of = |word: &str| -> Option<u32> {
        if word.chars().count() < 3 {
            return None;
        }
        let head: String = word.chars().take(3).collect();
        MONTHS
            .iter()
            .position(|(en, de)| head == *en || head == *de)
            .and_then(|i| u32::try_from(i + 1).ok())
    };
    let mut day: Option<u32> = None;
    let mut month: Option<u32> = None;
    let mut year: Option<i32> = None;
    for p in parts {
        if let Ok(n) = p.trim_end_matches('.').parse::<u32>() {
            if p.len() == 4 {
                year = i32::try_from(n).ok();
            } else if day.is_none() {
                day = Some(n);
            } else {
                return None;
            }
        } else if let Some(m) = month_of(p) {
            if month.is_some() {
                return None;
            }
            month = Some(m);
        } else {
            return None;
        }
    }
    crate::week::picked_date(year.unwrap_or(shown.year()), month?, day.unwrap_or(1))
}

#[cfg(test)]
#[path = "nav_test.rs"]
mod nav_test;
