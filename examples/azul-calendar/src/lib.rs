//! AzCalendar: a small calendar on the public azul API that makes AzMeet links, online or not.
//!
//! A week view (Monday to Sunday) holds the whole day, 00:00 to 24:00: the day-header row stays
//! put while the hours scroll under it, with the hour labels. A pinch on the trackpad, or the
//! wheel with Ctrl / Cmd held, zooms the hours (20 to 240 px an hour) around the time under the
//! pointer; a plain wheel scrolls. Events that overlap sit side by side.
//!
//! Clicking empty time makes a draft event there (an hour from the quarter hour clicked);
//! dragging over empty time makes one over the dragged quarter hours. The draft shows in the
//! week, dashed, with a popover next to it (a `<transient-window>`): its title (focused), the
//! day and times, "Add AzMeet link", Cancel and Save. Enter or Save saves it; Escape, Cancel or a
//! press outside drops it. "New event" opens the same form as a side sheet, with a day picker
//! and times. A press on an existing event is the event's.
//!
//! Save writes the event as one JSON file, `<data dir>/events/<uuid>.json` (see `event.rs`), at
//! once. With "Add AzMeet link" the link is made HERE (a room id drawn like the meeting server's
//! own), so a new event with a link works offline; the file keeps it `pending` until the meeting
//! server has registered the room (`POST /rooms {room, starts_at, ends_at}`, the event's day and
//! times in UTC, on an azul `Thread` so no callback waits on the network). Pending links are sent
//! at start, after every save, every `AZCAL_SYNC_SECONDS` (30) and on "Sync meeting links now";
//! the server's answer (its code, and the times it keeps the room for) is written back. An event
//! with a registered link shows "Join meeting", which starts AzMeet with `AZMEET_JOIN=<link>`
//! (or, without AzMeet, copies the link).
//!
//! Durable data is only the event files; the meeting server holds the rooms (registering and
//! joining), nothing of the calendar. The meeting server is chosen in Settings > Meeting server
//! (`<data dir>/settings.txt`, AzMeet's settings format), else `AZMEET_WORKER`, else the built-in
//! one, else AzMeet's local development server: there always is one. The same file keeps the
//! week's zoom (`settings.rs`).
//!
//! Environment:
//! - `AZCAL_DATA`: the data folder (default: `AzCalendar` in the user's data folder).
//! - `AZMEET_WORKER`: the meeting server when none is saved in Settings, as for AzMeet (default:
//!   AzMeet's built-in one, set at build time with `AZMEET_DEFAULT_WORKER`).
//! - `AZCAL_SYNC_SECONDS`: how often pending links are sent again (default 30).
//! - `AZMEET_BIN`: the AzMeet program "Join meeting" starts (default: `AzMeet` next to AzCalendar).
//!
//! On stdout, for scripts: `AZCAL_SAVED <file>` and `AZCAL_LINK <link>` when an event is saved,
//! `AZCAL_SYNCED <link>` when a link's room is registered, `AZCAL_JOIN_PID <pid>` when "Join
//! meeting" started AzMeet.

pub mod args;
pub mod calendars;
pub mod editor;
pub mod event;
pub mod ics;
pub mod meeting;
pub mod rrule;
pub mod sample;
pub mod settings;
#[cfg(test)]
mod test_dir;
pub mod views;
pub mod week;

/// AzMeet's meeting links and room keys: AzMeet's own `rooms.rs`, compiled into AzCalendar too,
/// so the two apps read links the same way.
#[path = "../../azul-meet/src/rooms.rs"]
pub mod meet_rooms;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Instant,
};

use azul::{
    callbacks::CallbackType,
    css::WindowBackgroundMaterial,
    dom::{ClipboardContent, DomId, DomNodeId, NodeHierarchyItemId, NodeId, VirtualKeyCode},
    error::HttpError,
    file::FilePath,
    http::{HttpGetResult, HttpMethod, HttpRequestConfig},
    menu::{Menu, MenuItem, StringMenuItem},
    misc::{TransientAnchor, TransientDismiss},
    prelude::*,
    str::String as AzString,
    task::{Thread, ThreadId, ThreadReceiver, ThreadSender},
    time::SystemTimeDiff,
    vec::{StyledTextRunVec, U8Vec},
    widgets::{
        ButtonType, CheckBoxState, DatePicker, DatePickerState, OnTextInputReturn, TextInputState,
        TextInputValid, TimePicker, TimePickerState, Titlebar,
    },
    window::{TransientWindowConfig, WindowDecorations},
};
use chrono::{Datelike, NaiveDate, NaiveTime};
use event::{Event, EventError, Meeting};

/// The data folder's variable.
const DATA_VAR: &str = "AZCAL_DATA";
/// Width of the hour labels left of the days.
const GUTTER_PX: u32 = 56;
const HTTP_TIMEOUT_SECS: u64 = 10;
/// What the form's "Add AzMeet link" line says once it is ticked.
const WILL_MINT: &str = "A new AzMeet link is made when you save. It works offline too: the \
                         meeting server gets it as soon as it answers.";
/// How often pending links are sent again, in seconds: the variable, and the default.
const SYNC_VAR: &str = "AZCAL_SYNC_SECONDS";
const SYNC_SECONDS: u64 = 30;
/// What an event made in the week's popover without a title is called (Google Calendar's way:
/// such an event may have no title), and what its draft shows until it has one.
const UNTITLED: &str = "(No title)";
/// A press this soon after the popover closed by a click outside it is that click: it makes no
/// new draft (on some systems the popover's own window reports the click first).
const DISMISSING_PRESS: std::time::Duration = std::time::Duration::from_millis(250);
/// The `id` of the week's scroll area.
const WEEK_SCROLL_ID: &str = "week-scroll";

// Light and dark: the page follows the mode the window is in, like the widgets on it (a
// Button, a TextInput, a DatePicker paint the desktop's palette of that mode). Surfaces, text
// and rules are `system:` colours, resolved in whichever mode the window is in: the chrome (the
// page, the toolbar and the title row above it, the footer, the side sheet, the popover) is
// `system:window-background`, the week's days are `system:control-background`, text is
// `system:text` / `system:secondary-text`, rules are `system:separator`. The few colours of the
// app's own (the accent tints of events and the draft, the notice, the error red) carry a dark
// twin under `@media (prefers-color-scheme: dark)`.
const BODY: &str = "display: flex; flex-direction: column; height: 100%; margin: 0; font-family: \
                    sans-serif; font-size: 14px; color: system:text; background: \
                    system:window-background;";
const TOOLBAR: &str = "display: flex; flex-direction: row; align-items: center; padding: 10px \
                       16px; background: system:window-background; border-bottom: 1px solid \
                       system:separator;";
const NOTICE: &str = "padding: 6px 16px; font-size: 13px; color: #2c4a7a; background: #e6eefc; \
                      @media (prefers-color-scheme: dark) { color: #c4d7ff; background: \
                      #1f2d45; }";
const FOOTER: &str = "padding: 4px 16px; font-size: 12px; color: system:secondary-text; \
                      background: system:window-background; border-top: 1px solid \
                      system:separator;";
const LINE: &str = "system:separator";
const LABEL: &str = "font-size: 12px; color: system:secondary-text; margin-top: 12px; \
                     margin-bottom: 4px;";
/// A day's surface; today's is tinted with the accent.
const DAY_PAINT: &str = "background: system:control-background;";
const TODAY_PAINT: &str =
    "background: #f7faff; @media (prefers-color-scheme: dark) { background: #1b2433; }";
/// An event's box: a pale accent tint with an accent edge (a deep tint in dark mode).
const EVENT_PAINT: &str = "background: #dbe7ff; border-left: 3px solid #2f6db0; @media \
                           (prefers-color-scheme: dark) { background: #233a5e; border-left: 3px \
                           solid #6ea8ff; }";
/// The draft's box: paler than an event, with a dashed accent edge.
const DRAFT_PAINT: &str = "background: #eef4ff; border: 2px dashed #2f6db0; @media \
                           (prefers-color-scheme: dark) { background: #1a2c4d; border: 2px dashed \
                           #6ea8ff; }";
/// The draft's title, in the accent.
const DRAFT_TITLE: &str =
    "font-weight: bold; color: #2f6db0; @media (prefers-color-scheme: dark) { color: #8dbbff; }";
/// Secondary lines: an event's time, what a form line means.
const SECONDARY: &str = "color: system:secondary-text;";
/// A form's error line.
const ERROR: &str = "font-size: 13px; color: #b3261e; margin-top: 12px; @media \
                     (prefers-color-scheme: dark) { color: #f2b8b5; }";
/// A block's title: one line, cut with an ellipsis at the block's edge.
const CLIPPED_TITLE: &str = "font-weight: bold; white-space: nowrap; overflow: hidden; \
                             text-overflow: ellipsis;";
/// A block's other lines, the same way.
const CLIPPED_LINE: &str = "color: system:secondary-text; white-space: nowrap; overflow: hidden; \
                            text-overflow: ellipsis;";
/// The popover's card: the whole of its window.
const POPOVER: &str = "display: flex; flex-direction: column; width: 320px; padding: 16px; \
                       box-sizing: border-box; background: system:window-background; border: 1px \
                       solid system:separator; border-radius: 8px; font-family: sans-serif; \
                       font-size: 14px; color: system:text;";

/// The app.
struct CalState {
    data_dir: PathBuf,
    /// The meeting server new links are registered with (Settings, else `AZMEET_WORKER`, else
    /// the built-in one, else AzMeet's local one).
    server: String,
    events: Vec<Event>,
    today: NaiveDate,
    /// Monday of the shown week.
    week: NaiveDate,
    form: Option<Form>,
    /// Forms opened so far; each form's `serial`, so an answer for a closed form is ignored.
    forms_opened: u32,
    notice: String,
    /// AzMeet processes "Join meeting" started, until they end.
    launched: Vec<Child>,
    /// The height of an hour in the week, in logical px: the zoom.
    hour_px: f32,
    /// A press on empty time in the week, until it is let go.
    press: Option<Press>,
    /// The cumulative scale of the pinch in flight at its last update (`DetectedPinch::scale`).
    last_pinch_scale: Option<f32>,
    /// When the popover last closed by a click outside it or Escape.
    popover_closed_at: Option<Instant>,
    /// Events whose pending link is being registered right now.
    syncing: BTreeSet<String>,
    /// Events whose link the meeting server refused, with what it said: not sent again until
    /// "Sync meeting links now" or a new meeting server.
    sync_refused: BTreeMap<String, String>,
    /// Why the last registration did not happen (empty once one did).
    sync_error: String,
    /// How often pending links are sent again, in milliseconds.
    sync_every_ms: u64,
    /// The timer that sends them runs.
    sync_timer: bool,
    /// The Settings sheet, while it is open.
    settings: Option<SettingsForm>,
    /// A timer saves the zoom in a moment (`queue_zoom_save`).
    zoom_save_queued: bool,
}

/// The Settings sheet: the meeting server as typed.
struct SettingsForm {
    server: String,
    error: String,
}

/// Where a form is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormPlace {
    /// The "New event" side sheet.
    Sheet,
    /// The popover next to a draft in the week, made by a click or a drag on empty time.
    Popover,
}

/// A new event being made: the "New event" side sheet, or the week's draft and its popover.
struct Form {
    serial: u32,
    /// The new event's id, fixed when the form opens: its file's name.
    id: String,
    title: String,
    date: NaiveDate,
    start: NaiveTime,
    end: NaiveTime,
    add_meet: bool,
    /// The link made for this form, kept when its event could not be written, so the next Save
    /// uses the same one.
    link: Option<Meeting>,
    error: String,
    place: FormPlace,
}

impl Form {
    fn event(&self, meeting: Option<Meeting>) -> Result<Event, EventError> {
        let title = match self.place {
            FormPlace::Popover if self.title.trim().is_empty() => UNTITLED,
            _ => self.title.as_str(),
        };
        Event::create(&self.id, title, self.date, self.start, self.end, meeting)
    }
}

/// A press on empty time in a day's column: a click, or the start of a drag.
#[derive(Debug, Clone, Copy)]
struct Press {
    /// The column, 0 = Monday.
    day: usize,
    /// Where it went down, and where the pointer is now: y in the column (from midnight), px.
    from_y: f32,
    to_y: f32,
    /// It moved far enough to be a drag.
    dragging: bool,
}

impl Press {
    /// The event this press makes if it is let go now, `(start, end)` in minutes.
    fn range(&self, hour_px: f32) -> (u32, u32) {
        if self.dragging || week::is_drag(self.from_y, self.to_y) {
            week::drag_range(
                week::minute_at_y(self.from_y, hour_px),
                week::minute_at_y(self.to_y, hour_px),
            )
        } else {
            week::click_range(week::minute_at_y(self.from_y, hour_px))
        }
    }
}

/// What the form says about an event it cannot save.
fn form_error(e: &EventError) -> String {
    match e {
        EventError::EmptyTitle => String::from("Give the event a title."),
        EventError::EndNotAfterStart => String::from("The event must end after it starts."),
        other => format!("This event cannot be saved: {other}."),
    }
}

/// Opens a form for a new event on `date`, `start` to `end`, at `place`.
fn open_form(
    s: &mut CalState,
    place: FormPlace,
    date: NaiveDate,
    start: NaiveTime,
    end: NaiveTime,
) {
    s.forms_opened += 1;
    s.form = Some(Form {
        serial: s.forms_opened,
        id: event::new_event_id(),
        title: String::new(),
        date,
        start,
        end,
        add_meet: false,
        link: None,
        error: String::new(),
        place,
    });
    s.notice.clear();
}

/// Where the events of the shown week's `day` (0 = Monday) sit in its column.
fn placements_of(s: &CalState, day: usize) -> Vec<week::Placement> {
    let lists = week::events_in_week(&s.events, s.week);
    lists
        .get(day)
        .map(|list| week::lay_out_day(list))
        .unwrap_or_default()
}

// ==== Layout ====

/// What the window shows, read from `CalState` before the DOM is built.
struct View {
    week_title: String,
    days: Vec<DayView>,
    form: Option<FormView>,
    /// The draft in the week: the popover form's event, or the one a drag is making.
    draft: Option<DraftView>,
    hour_px: f32,
    settings: Option<SettingsView>,
    notice: String,
    footer: String,
}

struct SettingsView {
    server: String,
    error: String,
    /// How many links wait for the meeting server, and why the last try failed.
    pending: usize,
    sync_error: String,
}

struct DayView {
    label: String,
    today: bool,
    blocks: Vec<BlockView>,
    /// Minutes since midnight now, in today's column.
    now: Option<u32>,
}

struct BlockView {
    id: String,
    title: String,
    time: String,
    /// Minutes from midnight, and minutes long.
    top: u32,
    height: u32,
    lane: u32,
    lanes: u32,
    /// Its meeting's room is registered: "Join meeting".
    joinable: bool,
    /// Its meeting link waits for the meeting server.
    waiting: bool,
}

struct FormView {
    serial: u32,
    place: FormPlace,
    title: String,
    date: NaiveDate,
    start: NaiveTime,
    end: NaiveTime,
    add_meet: bool,
    error: String,
}

struct DraftView {
    /// The column, 0 = Monday.
    day: usize,
    /// Minutes from midnight.
    start: u32,
    end: u32,
    title: String,
    /// The popover is open on it (not while a drag is still making it).
    popover: bool,
}

fn pending_links(s: &CalState) -> usize {
    s.events
        .iter()
        .filter(|e| e.meeting.as_ref().is_some_and(|m| m.pending))
        .count()
}

fn view_of(s: &CalState) -> View {
    let pending = pending_links(s);
    let lists = week::events_in_week(&s.events, s.week);
    let now = week::minute_of_day(chrono::Local::now().time());
    let days = week::week_days(s.week)
        .iter()
        .zip(lists.iter())
        .map(|(date, list)| DayView {
            label: week::day_label(*date),
            today: *date == s.today,
            blocks: week::lay_out_day(list)
                .iter()
                .map(|p| {
                    let e = list[p.index];
                    BlockView {
                        id: e.id.clone(),
                        title: e.title.clone(),
                        time: week::time_range(e.start, e.end),
                        top: p.top,
                        height: p.height,
                        lane: p.lane,
                        lanes: p.lanes,
                        joinable: e.meeting.as_ref().is_some_and(|m| !m.pending),
                        waiting: e.meeting.as_ref().is_some_and(|m| m.pending),
                    }
                })
                .collect(),
            now: (*date == s.today).then_some(now),
        })
        .collect();
    let dragged = s.press.filter(|p| p.dragging).map(|p| {
        let (start, end) = p.range(s.hour_px);
        DraftView {
            day: p.day,
            start,
            end,
            title: String::new(),
            popover: false,
        }
    });
    let popover = s
        .form
        .as_ref()
        .filter(|f| f.place == FormPlace::Popover)
        .and_then(|f| {
            let day = usize::try_from((f.date - s.week).num_days()).ok()?;
            (day < 7).then(|| DraftView {
                day,
                start: week::minute_of_day(f.start),
                end: week::minute_of_day(f.end),
                title: f.title.clone(),
                popover: true,
            })
        });
    View {
        week_title: week::week_title(s.week),
        days,
        form: s.form.as_ref().map(|f| FormView {
            serial: f.serial,
            place: f.place,
            title: f.title.clone(),
            date: f.date,
            start: f.start,
            end: f.end,
            add_meet: f.add_meet,
            error: f.error.clone(),
        }),
        draft: dragged.or(popover),
        hour_px: s.hour_px,
        settings: s.settings.as_ref().map(|f| SettingsView {
            server: f.server.clone(),
            error: f.error.clone(),
            pending,
            sync_error: s.sync_error.clone(),
        }),
        notice: s.notice.clone(),
        footer: format!(
            "Events: {} · Meeting server: {}{}",
            s.data_dir.join(event::EVENTS_DIR).display(),
            s.server,
            match pending {
                0 => String::new(),
                1 => String::from(" · 1 meeting link waiting for it"),
                n => format!(" · {n} meeting links waiting for it"),
            }
        ),
    }
}

/// The window's title row, drawn by azul (the window is `NoTitle`, so macOS
/// draws only the traffic lights): no fill of its own, so the page's
/// `system:window-background` shows through - the toolbar's surface, in light
/// and in dark - and no line of its own, so the two read as one bar. The
/// title's default ink has a dark twin.
fn title_row() -> Dom {
    Titlebar::create("AzCalendar").without_border_bottom().dom()
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let Some(view) = data.downcast_ref::<CalState>().map(|s| view_of(&s)) else {
        return Dom::create_body();
    };
    let mut main = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0;")
        .with_child(week_grid(&view, &data));
    if let Some(settings) = &view.settings {
        main.add_child(settings_panel(settings, &data));
    } else if let Some(form) = view.form.as_ref().filter(|f| f.place == FormPlace::Sheet) {
        main.add_child(form_panel(form, &data));
    }
    let mut body = Dom::create_body()
        .with_css(BODY)
        .with_menu_bar(menu_bar(&data))
        .with_child(title_row())
        .with_child(toolbar(&view, &data));
    if !view.notice.is_empty() {
        body.add_child(Dom::create_span_with_text(view.notice.as_str()).with_css(NOTICE));
    }
    body.with_child(main)
        .with_child(Dom::create_span_with_text(view.footer.as_str()).with_css(FOOTER))
}

/// The menu bar: Settings > Meeting server..., Sync meeting links now.
fn menu_bar(data: &RefAny) -> Menu {
    let item = |label: &str, callback: CallbackType| {
        MenuItem::string(StringMenuItem::create(label).with_callback(data.clone(), callback))
    };
    Menu::create(vec![MenuItem::string(
        StringMenuItem::create("Settings").with_children(vec![
            item("Meeting server\u{2026}", on_open_settings),
            item("Sync meeting links now", on_sync_now),
        ]),
    )])
}

fn toolbar(view: &View, data: &RefAny) -> Dom {
    let nav = |label: &str, on_click: extern "C" fn(RefAny, CallbackInfo) -> Update| {
        Button::create(label)
            .with_on_click(data.clone(), on_click)
            .dom()
            .with_css("margin-right: 6px;")
    };
    Dom::create_div()
        .with_css(TOOLBAR)
        .with_child(
            Dom::create_span_with_text("AzCalendar")
                .with_css("font-size: 20px; font-weight: bold; margin-right: 18px;"),
        )
        .with_child(nav("Previous week", on_previous_week))
        .with_child(nav("This week", on_this_week))
        .with_child(nav("Next week", on_next_week))
        .with_child(
            Dom::create_span_with_text(view.week_title.as_str())
                .with_css("font-size: 16px; margin-left: 12px;"),
        )
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
        .with_child(
            Button::with_type("New event", ButtonType::Primary)
                .with_on_click(data.clone(), on_new_event)
                .dom(),
        )
}

/// The week: the day-header row, fixed, over the scroll area (`#week-scroll`) that holds the
/// whole day (`#week-grid`: the hour labels and the seven day columns, `#day-0` .. `#day-6`).
/// The scroll area has `min-height: 0` (and so has every flex box above it), or it would grow
/// to the day's height instead of scrolling over it.
fn week_grid(view: &View, data: &RefAny) -> Dom {
    let gutter = format!("width: {GUTTER_PX}px; flex-shrink: 0;");
    let hour = view.hour_px;
    let mut header = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; flex-shrink: 0; {DAY_PAINT} \
             border-bottom: 1px solid {LINE};"
        ))
        .with_child(Dom::create_div().with_css(gutter.as_str()));
    let mut hours = Dom::create_div().with_css(gutter.as_str());
    for h in 0..24 {
        hours.add_child(
            Dom::create_div()
                .with_css(format!(
                    "height: {hour:.3}px; padding-right: 6px; font-size: 11px; {SECONDARY} \
                     text-align: right; box-sizing: border-box;"
                ))
                .with_child(Dom::create_span_with_text(week::hour_label(h))),
        );
    }
    let mut grid = Dom::create_div()
        .with_id("week-grid")
        .with_css(format!(
            "display: flex; flex-direction: row; flex-shrink: 0; height: {:.3}px;",
            week::day_height(hour)
        ))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Scroll),
            data.clone(),
            on_week_wheel,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::PinchIn),
            data.clone(),
            on_week_pinch,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::PinchOut),
            data.clone(),
            on_week_pinch,
        )
        .with_child(hours);
    for (index, day) in view.days.iter().enumerate() {
        header.add_child(day_header(day));
        let draft = view.draft.as_ref().filter(|d| d.day == index);
        grid.add_child(day_column(index, day, draft, view, data));
    }
    let scroll = Dom::create_div()
        .with_id(WEEK_SCROLL_ID)
        .with_css(
            "flex-grow: 1; flex-basis: 0px; min-height: 0; overflow-y: auto; overflow-x: hidden;",
        )
        .with_callback(
            EventFilter::Component(ComponentEventFilter::AfterMount),
            data.clone(),
            on_week_mounted,
        )
        .with_child(grid);
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-width: 0; min-height: 0; \
             padding: 0 8px 8px 0;",
        )
        .with_child(header)
        .with_child(scroll)
}

fn day_header(day: &DayView) -> Dom {
    let colour = if day.today {
        "color: system:accent; font-weight: bold;"
    } else {
        ""
    };
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; padding: 6px \
             8px; border-left: 1px solid {LINE}; {colour}"
        ))
        .with_child(Dom::create_span_with_text(day.label.as_str()))
}

/// A day's column (`#day-<index>`): the hour lines, the events, the "now" line on today, and
/// the draft with its popover. A press on empty time starts a click or a drag.
fn day_column(
    index: usize,
    day: &DayView,
    draft: Option<&DraftView>,
    view: &View,
    data: &RefAny,
) -> Dom {
    let hour = view.hour_px;
    let paint = if day.today { TODAY_PAINT } else { DAY_PAINT };
    let target = RefAny::new(DayRef {
        app: data.clone(),
        day: index,
    });
    let mut column = Dom::create_div()
        .with_id(format!("day-{index}"))
        .with_css(format!(
            "position: relative; flex-grow: 1; flex-basis: 0px; height: {:.3}px; border-left: \
             1px solid {LINE}; {paint}",
            week::day_height(hour)
        ))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            target.clone(),
            on_day_press,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            target.clone(),
            on_day_drag,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::LeftMouseUp),
            target,
            on_day_release,
        );
    for _ in 0..24 {
        column.add_child(Dom::create_div().with_css(format!(
            "height: {hour:.3}px; border-top: 1px solid {LINE}; box-sizing: border-box;"
        )));
    }
    for block in &day.blocks {
        column.add_child(event_block(block, hour, data));
    }
    if let Some(now) = day.now {
        column.add_child(Dom::create_div().with_css(format!(
            "position: absolute; left: 0px; width: 100%; top: {:.3}px; height: 2px; background: \
             #d93025;",
            week::y_of_minute(now as f32, hour)
        )));
    }
    if let Some(draft) = draft {
        column.add_child(draft_block(draft, view, data));
    }
    column
}

/// One event in its day's column: title, time and, with a meeting link, "Join meeting".
fn event_block(block: &BlockView, hour_px: f32, data: &RefAny) -> Dom {
    let top = week::y_of_minute(block.top as f32, hour_px);
    let height = week::y_of_minute(block.height as f32, hour_px).max(week::MIN_BLOCK_PX);
    let width = 100.0 / block.lanes.max(1) as f32;
    let left = width * block.lane as f32;
    let mut dom = Dom::create_div()
        .with_css(format!(
            "position: absolute; top: {top:.3}px; left: {left:.3}%; width: {width:.3}%; height: \
             {height:.3}px; box-sizing: border-box; display: flex; flex-direction: column; \
             padding: 3px 6px; {EVENT_PAINT} border-radius: 4px; font-size: 12px; \
             overflow: hidden;"
        ))
        .with_child(Dom::create_span_with_text(block.title.as_str()).with_css(CLIPPED_TITLE))
        .with_child(Dom::create_span_with_text(block.time.as_str()).with_css(CLIPPED_LINE));
    if block.waiting {
        dom.add_child(
            Dom::create_span_with_text("AzMeet link waits for the server").with_css(format!(
                "{CLIPPED_LINE} font-size: 11px; font-style: italic;"
            )),
        );
    }
    if block.joinable {
        let target = RefAny::new(EventRef {
            app: data.clone(),
            id: block.id.clone(),
        });
        dom.add_child(
            Button::create("Join meeting")
                .with_on_click(target, on_join_meeting)
                .dom()
                .with_css("margin-top: 3px;"),
        );
    }
    dom
}

/// The draft (`#draft`): drawn like an event, dashed and pale, "(No title)" until it has one;
/// with its popover once the press that made it was let go.
fn draft_block(draft: &DraftView, view: &View, data: &RefAny) -> Dom {
    let hour = view.hour_px;
    let top = week::y_of_minute(draft.start as f32, hour);
    let minutes = draft.end.saturating_sub(draft.start) as f32;
    let height = week::y_of_minute(minutes, hour).max(week::MIN_BLOCK_PX);
    let title = if draft.title.trim().is_empty() {
        UNTITLED
    } else {
        draft.title.as_str()
    };
    let time = week::time_range(
        week::time_of_minute(draft.start),
        week::time_of_minute(draft.end),
    );
    let mut block = Dom::create_div()
        .with_id("draft")
        .with_css(format!(
            "position: absolute; top: {top:.3}px; left: 0px; width: 100%; height: \
             {height:.3}px; box-sizing: border-box; display: flex; flex-direction: column; \
             padding: 3px 6px; {DRAFT_PAINT} border-radius: 4px; font-size: 12px;"
        ))
        .with_child(
            // The text is clipped in a box of its own: the popover is the draft's child too.
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0; \
                     overflow: hidden;",
                )
                .with_child(
                    Dom::create_span_with_text(title)
                        .with_css(format!("{CLIPPED_TITLE} {DRAFT_TITLE}")),
                )
                .with_child(Dom::create_span_with_text(time).with_css(CLIPPED_LINE)),
        );
    if draft.popover {
        if let Some(form) = view.form.as_ref().filter(|f| f.place == FormPlace::Popover) {
            block.add_child(popover(form, data));
        }
    }
    block
}

/// The draft's popover: a `<transient-window>` that opens to the right of the draft (the
/// engine flips it left at the screen's edge). A press outside it, Escape, or its window
/// losing focus dismisses it (`Dismissed`: the draft goes). It takes the keyboard, and the
/// engine focuses its first control, the title.
fn popover(form: &FormView, data: &RefAny) -> Dom {
    let config = TransientWindowConfig::opened()
        .with_anchor(TransientAnchor::Right)
        .with_dismiss(TransientDismiss::Outside)
        .with_material(WindowBackgroundMaterial::Transparent);
    let target = RefAny::new(FormRef {
        app: data.clone(),
        serial: form.serial,
    });
    Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(config)))
        .with_callback(
            EventFilter::Component(ComponentEventFilter::Dismissed),
            target,
            on_popover_dismissed,
        )
        .with_child(popover_panel(form, data))
}

/// The popover's card: title (`#draft-title`), day and times (`#draft-when`), "Add AzMeet
/// link", and Cancel (`#draft-cancel`) / Save (`#draft-save`).
fn popover_panel(form: &FormView, data: &RefAny) -> Dom {
    let mut panel = Dom::create_div()
        .with_id("draft-panel")
        .with_css(POPOVER)
        .with_child(
            TextInput::create()
                .with_text(form.title.as_str())
                .with_placeholder("Add title")
                .with_on_text_input(data.clone(), on_title)
                .with_on_virtual_key_down(data.clone(), on_title_key)
                .dom()
                .with_id("draft-title"),
        )
        .with_child(
            Dom::create_span_with_text(week::draft_label(form.date, form.start, form.end))
                .with_id("draft-when")
                .with_css("font-size: 13px; color: system:secondary-text; margin-top: 10px;"),
        )
        .with_child(meet_toggle(form, data));
    if !form.error.is_empty() {
        panel.add_child(Dom::create_span_with_text(form.error.as_str()).with_css(ERROR));
    }
    panel.with_child(
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: row; justify-content: flex-end; margin-top: 16px;",
            )
            .with_child(
                Button::create("Cancel")
                    .with_on_click(data.clone(), on_cancel)
                    .dom()
                    .with_id("draft-cancel")
                    .with_css("margin-right: 8px;"),
            )
            .with_child(
                Button::with_type("Save", ButtonType::Primary)
                    .with_on_click(data.clone(), on_save)
                    .dom()
                    .with_id("draft-save"),
            ),
    )
}

/// "Add AzMeet link" (the box, and its label, which toggles it too) and what ticking it means.
/// The side sheet's and the popover's.
fn meet_toggle(form: &FormView, data: &RefAny) -> Dom {
    let mut part = Dom::create_div().with_css("display: flex; flex-direction: column;");
    part.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 16px;")
            .with_child(
                CheckBox::create(form.add_meet)
                    .with_on_toggle(data.clone(), on_meet_toggled)
                    .dom(),
            )
            .with_child(
                Dom::create_span_with_text("Add AzMeet link")
                    .with_css("margin-left: 8px; cursor: pointer;")
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::Click),
                        data.clone(),
                        on_meet_label,
                    ),
            ),
    );
    if form.add_meet {
        part.add_child(
            Dom::create_span_with_text(WILL_MINT)
                .with_css("font-size: 12px; color: system:secondary-text; margin-top: 4px;"),
        );
    }
    part
}

/// The "New event" form, a side sheet right of the week.
fn form_panel(form: &FormView, data: &RefAny) -> Dom {
    let label = |text: &str| Dom::create_span_with_text(text).with_css(LABEL);
    let mut panel = Dom::create_div()
        .with_css(format!(
            "width: 340px; flex-shrink: 0; display: flex; flex-direction: column; padding: 16px; \
             background: system:window-background; border-left: 1px solid {LINE}; overflow-y: \
             auto;"
        ))
        .with_child(
            Dom::create_span_with_text("Create an event")
                .with_css("font-size: 18px; font-weight: bold;"),
        )
        .with_child(label("Title"))
        .with_child(
            TextInput::create()
                .with_text(form.title.as_str())
                .with_placeholder("What is it about?")
                .with_on_text_input(data.clone(), on_title)
                .dom()
                .with_id("event-title"),
        )
        .with_child(label("Day"))
        .with_child(
            DatePicker::create(
                form.date.year().max(0) as u32,
                form.date.month(),
                form.date.day(),
            )
            .with_on_change(data.clone(), on_day)
            .dom(),
        )
        .with_child(label("Starts"))
        .with_child(time_picker(form.start, data, on_start))
        .with_child(label("Ends"))
        .with_child(time_picker(form.end, data, on_end))
        .with_child(meet_toggle(form, data));
    if !form.error.is_empty() {
        panel.add_child(Dom::create_span_with_text(form.error.as_str()).with_css(ERROR));
    }
    panel.with_child(
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: row; justify-content: flex-end; margin-top: 20px;",
            )
            .with_child(
                Button::create("Cancel")
                    .with_on_click(data.clone(), on_cancel)
                    .dom()
                    .with_css("margin-right: 8px;"),
            )
            .with_child(
                Button::with_type("Save event", ButtonType::Primary)
                    .with_on_click(data.clone(), on_save)
                    .dom(),
            ),
    )
}

fn time_picker(
    time: NaiveTime,
    data: &RefAny,
    on_change: extern "C" fn(RefAny, CallbackInfo, TimePickerState) -> Update,
) -> Dom {
    use chrono::Timelike;
    TimePicker::create(time.hour(), time.minute())
        .with_24h(true)
        .with_on_change(data.clone(), on_change)
        .dom()
}

// ==== Week navigation ====

fn show_week(data: &mut RefAny, weeks: Option<i64>) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let from = match weeks {
        Some(_) => s.week,
        None => s.today,
    };
    s.week = week::shift_weeks(from, weeks.unwrap_or(0));
    // A draft belongs to the week it was made in.
    if s.form
        .as_ref()
        .is_some_and(|f| f.place == FormPlace::Popover)
    {
        s.form = None;
    }
    s.press = None;
    Update::RefreshDom
}

extern "C" fn on_previous_week(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_week(&mut data, Some(-1))
}

extern "C" fn on_next_week(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_week(&mut data, Some(1))
}

extern "C" fn on_this_week(mut data: RefAny, _info: CallbackInfo) -> Update {
    show_week(&mut data, None)
}

// ==== The week: scroll, zoom ====

fn root_dom() -> DomId {
    DomId { inner: 0 }
}

/// The week's scroll area as a callback sees it: its node, where it is in the window, and how
/// far it is scrolled.
struct WeekScroll {
    node: NodeHierarchyItemId,
    top: f32,
    height: f32,
    scroll_y: f32,
}

fn week_scroll(info: &CallbackInfo) -> Option<WeekScroll> {
    let node = info.get_node_id_by_id_attribute(root_dom(), WEEK_SCROLL_ID);
    // 0 is "no node"; a node's raw id is its index + 1.
    let index = node.into_raw().checked_sub(1)?;
    let rect = info
        .get_node_rect(DomNodeId {
            dom: root_dom(),
            node,
        })
        .into_option()?;
    let scroll_y = info
        .get_scroll_offset_for_node(root_dom(), NodeId::create(index))
        .into_option()
        .map_or(0.0, |offset| offset.y);
    Some(WeekScroll {
        node,
        top: rect.origin.y,
        height: rect.size.height,
        scroll_y,
    })
}

/// Zooms the week's hours by `factor`, keeping the time under the pointer (`pointer_y`, a
/// window y; the view's middle without one) where it is; the zoom is saved a moment later.
fn zoom(
    s: &mut CalState,
    info: &mut CallbackInfo,
    app: &RefAny,
    factor: f32,
    pointer_y: Option<f32>,
) -> Update {
    let old = s.hour_px;
    let new = week::clamp_hour_px(old * factor);
    if (new - old).abs() < 0.01 {
        return Update::DoNothing;
    }
    s.hour_px = new;
    queue_zoom_save(s, info, app);
    if let Some(view) = week_scroll(info) {
        let pointer = pointer_y.map_or(view.height / 2.0, |y| {
            (y - view.top).clamp(0.0, view.height.max(0.0))
        });
        let y = week::zoom_scroll(old, new, pointer, view.scroll_y, view.height);
        // Unclamped: the day is taller after zooming in than the layout the offset is checked
        // against now; the rebuild this returns lays the new height out before anything draws.
        info.scroll_to_unclamped(root_dom(), view.node, LogicalPosition { x: 0.0, y });
    }
    Update::RefreshDom
}

/// How long after a zoom step the zoom is saved: one write a second at most, not one per step.
const ZOOM_SAVE_DELAY_MS: u64 = 1000;

/// Saves the zoom in the settings file a moment from now, unless that is queued already.
fn queue_zoom_save(s: &mut CalState, info: &mut CallbackInfo, app: &RefAny) {
    if s.zoom_save_queued {
        return;
    }
    s.zoom_save_queued = true;
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_save_zoom, get_time).with_delay(Duration::System(
            SystemTimeDiff::from_millis(ZOOM_SAVE_DELAY_MS),
        )),
    );
}

/// Writes the zoom as it is now into the settings file, for the next start.
extern "C" fn on_save_zoom(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    if let Some(mut s) = data.downcast_mut::<CalState>() {
        s.zoom_save_queued = false;
        let line = settings::hour_px_line(s.hour_px);
        if let Err(e) = settings::write_line(&settings::path(&s.data_dir), &line) {
            eprintln!("[azcalendar] could not save the zoom: {e}");
        }
    }
    TimerCallbackReturn::terminate_unchanged()
}

/// The wheel over the week: with Ctrl or Cmd held it zooms (and the week does not scroll as
/// well); without, the week scrolls as any scroll area does.
extern "C" fn on_week_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let modifiers = info.get_key_modifiers();
    if !(modifiers.ctrl || modifiers.meta) {
        return Update::DoNothing;
    }
    let hit = info.get_hit_node();
    let node = NodeId::create(hit.node.into_raw().saturating_sub(1));
    let dy = info
        .get_scroll_delta(hit.dom, node)
        .into_option()
        .map_or(0.0, |delta| delta.y);
    if dy == 0.0 {
        return Update::DoNothing;
    }
    // The wheel has one consumer: this zoom. The scroll it would have made is taken back.
    info.prevent_default();
    let pointer_y = info
        .get_cursor_relative_to_viewport()
        .into_option()
        .map(|p| p.y);
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    zoom(
        &mut guard,
        &mut info,
        &app,
        week::wheel_zoom_factor(dy),
        pointer_y,
    )
}

/// A pinch over the week zooms it around the pinch's centre.
extern "C" fn on_week_pinch(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(pinch) = info.get_pinch().into_option() else {
        return Update::DoNothing;
    };
    // Cumulative since the gesture began: the zoom is the ratio to the previous update.
    let sample = week::PinchSample {
        scale: pinch.scale,
        began: pinch.began,
    };
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let factor = week::pinch_step(s.last_pinch_scale, sample);
    s.last_pinch_scale = Some(sample.scale);
    zoom(s, &mut info, &app, factor, Some(pinch.center.y))
}

/// The week opens at 08:00, or an hour before now on today's week; pending meeting links start
/// being sent.
extern "C" fn on_week_mounted(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((minute, hour_px)) = data.downcast_ref::<CalState>().map(|s| {
        let today_shown = week::week_start(s.today) == s.week;
        (
            week::first_minute_shown(today_shown, chrono::Local::now().time()),
            s.hour_px,
        )
    }) else {
        return Update::DoNothing;
    };
    let scroll = info.get_hit_node();
    info.scroll_to(
        scroll.dom,
        scroll.node,
        LogicalPosition {
            x: 0.0,
            y: week::y_of_minute(minute as f32, hour_px),
        },
    );
    // The app is up: send the links that wait for the meeting server, now and from time to time.
    start_syncing(&mut data, &mut info);
    Update::DoNothing
}

// ==== The week: click or drag to make an event ====

/// A day's column, for its callbacks.
struct DayRef {
    app: RefAny,
    /// 0 = Monday.
    day: usize,
}

/// A form, for a callback that must not act on a newer one.
struct FormRef {
    app: RefAny,
    serial: u32,
}

/// A press in a day's column. On empty time it starts a click or a drag; on an event it is the
/// event's. While the popover is open, it is the press that closes it: the draft goes and
/// nothing new starts (Google Calendar's way).
extern "C" fn on_day_press(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, day)) = data
        .downcast_ref::<DayRef>()
        .map(|r| (r.app.clone(), r.day))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.press = None;
    match s.form.as_ref().map(|f| f.place) {
        Some(FormPlace::Popover) => {
            s.form = None;
            s.popover_closed_at = Some(Instant::now());
            return Update::RefreshDom;
        }
        // The side sheet is making an event: the week waits.
        Some(FormPlace::Sheet) => return Update::DoNothing,
        None => {}
    }
    if s.popover_closed_at
        .is_some_and(|closed| closed.elapsed() < DISMISSING_PRESS)
    {
        return Update::DoNothing;
    }
    let Some(at) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let width = info
        .get_hit_node_rect()
        .into_option()
        .map_or(0.0, |rect| rect.size.width);
    let x_frac = if width > 0.0 { at.x / width } else { 0.5 };
    if week::event_at(&placements_of(s, day), at.y, x_frac, s.hour_px).is_some() {
        return Update::DoNothing;
    }
    s.press = Some(Press {
        day,
        from_y: at.y,
        to_y: at.y,
        dragging: false,
    });
    // The drag goes on when the pointer leaves the column (or the window).
    let column = info.get_hit_node();
    info.capture_pointer(column);
    Update::DoNothing
}

/// The pointer moving over a day's column: a press on empty time that moved a few pixels is a
/// drag, and the draft follows it (redrawn when its quarter hours change).
extern "C" fn on_day_drag(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((mut app, day)) = data
        .downcast_ref::<DayRef>()
        .map(|r| (r.app.clone(), r.day))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let hour_px = s.hour_px;
    let Some(press) = s.press.as_mut().filter(|p| p.day == day) else {
        return Update::DoNothing;
    };
    let Some(at) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let before = press.dragging.then(|| press.range(hour_px));
    press.to_y = at.y;
    if !press.dragging && !week::is_drag(press.from_y, press.to_y) {
        return Update::DoNothing;
    }
    press.dragging = true;
    if before == Some(press.range(hour_px)) {
        Update::DoNothing
    } else {
        Update::RefreshDom
    }
}

/// The press let go: a draft of what it made (an hour from a click, the quarter hours of a
/// drag), with its popover.
extern "C" fn on_day_release(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((mut app, day)) = data
        .downcast_ref::<DayRef>()
        .map(|r| (r.app.clone(), r.day))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(mut press) = s.press.filter(|p| p.day == day) else {
        return Update::DoNothing;
    };
    s.press = None;
    if let Some(at) = info.get_cursor_relative_to_node().into_option() {
        press.to_y = at.y;
    }
    let (start, end) = press.range(s.hour_px);
    let date = week::week_days(s.week)[day.min(6)];
    open_form(
        s,
        FormPlace::Popover,
        date,
        week::time_of_minute(start),
        week::time_of_minute(end),
    );
    Update::RefreshDom
}

/// The popover was dismissed (a press outside it, Escape, its window losing focus): the draft
/// goes.
extern "C" fn on_popover_dismissed(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, serial)) = data
        .downcast_ref::<FormRef>()
        .map(|r| (r.app.clone(), r.serial))
    else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let ours = s
        .form
        .as_ref()
        .is_some_and(|f| f.serial == serial && f.place == FormPlace::Popover);
    if !ours {
        return Update::DoNothing;
    }
    s.form = None;
    s.popover_closed_at = Some(Instant::now());
    Update::RefreshDom
}

// ==== The form ====

extern "C" fn on_new_event(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if s.form.is_some() {
        return Update::DoNothing;
    }
    let (date, start, end) = week::new_event_slot(s.today, s.week, chrono::Local::now().time());
    open_form(s, FormPlace::Sheet, date, start, end);
    Update::RefreshDom
}

extern "C" fn on_cancel(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    s.form = None;
    Update::RefreshDom
}

extern "C" fn on_title(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<CalState>() {
        if let Some(form) = s.form.as_mut() {
            form.title = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Enter in the popover's title saves the event, as Save does.
extern "C" fn on_title_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = if matches!(
        key,
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter)
    ) {
        save(&mut data, &mut info)
    } else {
        Update::DoNothing
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_day(mut data: RefAny, _info: CallbackInfo, state: DatePickerState) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let Some(form) = s.form.as_mut() else {
        return Update::DoNothing;
    };
    let Some(date) = week::picked_date(state.year as i32, state.month, state.day) else {
        return Update::DoNothing;
    };
    // The picker restyles a picked day itself; a new month needs a new picker (its grid cannot
    // rebuild itself).
    let turned = (date.year(), date.month()) != (form.date.year(), form.date.month());
    form.date = date;
    if turned {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

fn set_time(data: &mut RefAny, state: TimePickerState, is_start: bool) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let Some(form) = s.form.as_mut() else {
        return Update::DoNothing;
    };
    let hour = if state.is_24h {
        state.hour
    } else {
        state.hour % 12 + if state.is_pm { 12 } else { 0 }
    };
    let Some(time) = NaiveTime::from_hms_opt(hour, state.minute, 0) else {
        return Update::DoNothing;
    };
    if is_start {
        form.start = time;
    } else {
        form.end = time;
    }
    Update::DoNothing
}

extern "C" fn on_start(mut data: RefAny, _info: CallbackInfo, state: TimePickerState) -> Update {
    set_time(&mut data, state, true)
}

extern "C" fn on_end(mut data: RefAny, _info: CallbackInfo, state: TimePickerState) -> Update {
    set_time(&mut data, state, false)
}

/// Ticks or clears "Add AzMeet link": `checked` from the box itself, or a toggle (its label).
fn set_add_meet(data: &mut RefAny, checked: Option<bool>) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let Some(form) = s.form.as_mut() else {
        return Update::DoNothing;
    };
    form.add_meet = checked.unwrap_or(!form.add_meet);
    form.error.clear();
    Update::RefreshDom
}

extern "C" fn on_meet_toggled(
    mut data: RefAny,
    _info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    set_add_meet(&mut data, Some(state.checked))
}

extern "C" fn on_meet_label(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_add_meet(&mut data, None)
}

extern "C" fn on_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    save(&mut data, &mut info)
}

/// Save (the button, or Enter in the popover's title): writes the event at once. With "Add AzMeet
/// link" the link is made here, pending, and its room is registered with the meeting server
/// right after (or as soon as the server answers), so saving never waits on the network.
fn save(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let server = s.server.clone();
    let Some(form) = s.form.as_mut() else {
        return Update::DoNothing;
    };
    form.error.clear();
    if let Err(e) = form.event(None) {
        form.error = form_error(&e);
        eprintln!("[azcalendar] cannot save: {}", form.error);
        return Update::RefreshDom;
    }
    let meeting = form.add_meet.then(|| {
        form.link
            .get_or_insert_with(|| {
                meeting::pending_meeting(&server, &meeting::new_room_id(room_entropy()))
            })
            .clone()
    });
    save_and_reveal(s, info, meeting);
    sync_links(s, info, &app);
    Update::RefreshDom
}

/// Three 64-bit draws of `event::random_seed` for a room id (130 of the bits are used).
fn room_entropy() -> [u64; 3] {
    [
        event::random_seed(),
        event::random_seed(),
        event::random_seed(),
    ]
}

/// `save_form`, then scrolls the week to the saved event if it is out of view (an event from
/// the side sheet can be at any time of the day).
fn save_and_reveal(s: &mut CalState, info: &mut CallbackInfo, meeting: Option<Meeting>) {
    let Some(start) = save_form(s, meeting) else {
        return;
    };
    let Some(view) = week_scroll(info) else {
        return;
    };
    let minute = week::minute_of_day(start);
    if let Some(y) = week::reveal_scroll(minute, s.hour_px, view.scroll_y, view.height) {
        info.scroll_to(root_dom(), view.node, LogicalPosition { x: 0.0, y });
    }
}

/// Writes the form's event, with `meeting`, to its file; adds it to the calendar, shows its week
/// and closes the form, and answers the event's start. On failure the form stays open with the
/// reason (and keeps the link).
fn save_form(s: &mut CalState, meeting: Option<Meeting>) -> Option<NaiveTime> {
    let form = s.form.as_mut()?;
    let event = match form.event(meeting.clone()) {
        Ok(event) => event,
        Err(e) => {
            form.error = form_error(&e);
            form.link = meeting;
            return None;
        }
    };
    match event::save(&s.data_dir, &event) {
        Ok(path) => {
            println!("AZCAL_SAVED {}", path.display());
            if let Some(m) = &event.meeting {
                println!("AZCAL_LINK {}", m.link);
            }
            eprintln!(
                "[azcalendar] saved \"{}\" to {}",
                event.title,
                path.display()
            );
            s.notice = match &event.meeting {
                Some(m) => format!("Saved \"{}\" with the AzMeet link {}", event.title, m.link),
                None => format!("Saved \"{}\".", event.title),
            };
            s.week = week::week_start(event.date);
            let start = event.start;
            s.events.push(event);
            s.form = None;
            Some(start)
        }
        Err(e) => {
            form.error = format!(
                "Could not write {}: {e}",
                event::event_path(&s.data_dir, &event.id).display()
            );
            eprintln!("[azcalendar] {}", form.error);
            form.link = event.meeting;
            None
        }
    }
}

// ==== Registering links: POST /rooms on an azul Thread, the answer on the UI thread ====

/// One pending link to register.
struct SyncJob {
    server: String,
    event_id: String,
    room_id: String,
    /// The `POST /rooms` body (`meeting::register_body`).
    body: String,
}

struct SyncInit {
    job: SyncJob,
    app: RefAny,
}

/// What the answer resumes with.
struct SyncReply {
    app: RefAny,
    event_id: String,
    room_id: String,
    server: String,
}

/// Sends every pending link that is not on its way already and was not refused: one thread
/// each. The event's times go along in UTC, so the server keeps the room for the meeting.
fn sync_links(s: &mut CalState, info: &mut CallbackInfo, app: &RefAny) {
    let jobs: Vec<SyncJob> = s
        .events
        .iter()
        .filter(|e| !s.syncing.contains(&e.id) && !s.sync_refused.contains_key(&e.id))
        .filter_map(|e| {
            let m = e.meeting.as_ref().filter(|m| m.pending)?;
            let room_id = meeting::room_id_of(m)?;
            let (starts, ends) = meeting::utc_window(&chrono::Local, e.date, e.start, e.end);
            Some(SyncJob {
                server: m.server.clone(),
                event_id: e.id.clone(),
                body: meeting::register_body(&room_id, starts, ends),
                room_id,
            })
        })
        .collect();
    for job in jobs {
        eprintln!(
            "[azcalendar] registering azlin://meet/{} with {}: {}",
            job.room_id, job.server, job.body
        );
        s.syncing.insert(job.event_id.clone());
        let init = RefAny::new(SyncInit {
            job,
            app: app.clone(),
        });
        info.add_thread(
            ThreadId::unique(),
            Thread::create(init, RefAny::new(()), sync_thread),
        );
    }
}

/// Runs `POST /rooms` on a worker thread: `http_request` blocks here, then queues its answer,
/// which the UI thread hands to `on_registered`.
extern "C" fn sync_thread(mut init: RefAny, _sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((url, body, reply)) = init.downcast_ref::<SyncInit>().map(|i| {
        (
            format!("{}/rooms", i.job.server),
            i.job.body.clone(),
            SyncReply {
                app: i.app.clone(),
                event_id: i.job.event_id.clone(),
                room_id: i.job.room_id.clone(),
                server: i.job.server.clone(),
            },
        )
    }) else {
        return;
    };
    let _request = HttpRequestConfig::create()
        .with_timeout(HTTP_TIMEOUT_SECS)
        .with_user_agent("AzCalendar/0.1")
        .with_header("accept", "application/json")
        .http_request(
            HttpMethod::Post,
            url.as_str(),
            U8Vec::from(body.into_bytes()),
            "application/json",
            RefAny::new(reply),
            on_registered,
        );
}

fn http_error_text(e: &HttpError) -> String {
    match e {
        HttpError::Timeout => String::from("timed out"),
        HttpError::InvalidUrl(s)
        | HttpError::ConnectionFailed(s)
        | HttpError::TlsError(s)
        | HttpError::IoError(s)
        | HttpError::Other(s) => s.as_str().to_string(),
        other => format!("{other:?}"),
    }
}

/// The registered meeting in the server's answer, or the answer's status (`None`: the server was
/// not reached) and what to tell the user.
fn register_outcome(
    server: &str,
    room_id: &str,
    result: RefAny,
) -> Result<Meeting, (Option<u16>, String)> {
    let Some(answer) = HttpGetResult::downcast(result).into_option() else {
        return Err((None, meeting::mint_failure(server, None, "no answer")));
    };
    match answer.result.into_result() {
        Ok(response) => {
            let body = response
                .body_as_string()
                .into_option()
                .map(|body| body.as_str().to_string())
                .unwrap_or_default();
            match response.status_code {
                status @ (200 | 201) => meeting::registered_meeting(server, room_id, &body)
                    .map_err(|message| (Some(status), message)),
                status => Err((
                    Some(status),
                    meeting::mint_failure(server, Some(status), &body),
                )),
            }
        }
        Err(HttpError::HttpStatus(e)) => Err((
            Some(e.status_code),
            meeting::mint_failure(server, Some(e.status_code), ""),
        )),
        Err(e) => Err((
            None,
            meeting::mint_failure(server, None, &http_error_text(&e)),
        )),
    }
}

/// The answer to a registration: the event's meeting becomes the registered one (its file is
/// rewritten); a failure leaves it pending, to be sent again unless the server refused it.
extern "C" fn on_registered(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut app, event_id, room_id, server)) = data.downcast_ref::<SyncReply>().map(|r| {
        (
            r.app.clone(),
            r.event_id.clone(),
            r.room_id.clone(),
            r.server.clone(),
        )
    }) else {
        return Update::DoNothing;
    };
    let outcome = register_outcome(&server, &room_id, result);
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.syncing.remove(&event_id);
    match outcome {
        Ok(registered) => {
            let link = registered.link.clone();
            let Some(event) = s.events.iter_mut().find(|e| e.id == event_id) else {
                return Update::DoNothing;
            };
            // Only the pending link this answer is for (a new meeting server may have taken it
            // over meanwhile).
            let ours = event
                .meeting
                .as_ref()
                .is_some_and(|m| m.pending && m.link == link && m.server == server);
            if !ours {
                return Update::DoNothing;
            }
            event.meeting = Some(registered);
            match event::save(&s.data_dir, event) {
                Ok(path) => {
                    println!("AZCAL_SYNCED {link}");
                    eprintln!(
                        "[azcalendar] {server} registered {link}; {} rewritten",
                        path.display()
                    );
                    s.sync_error.clear();
                }
                Err(e) => {
                    // Registered, but the file still says pending: the next start sends it
                    // again, and registering is idempotent.
                    s.sync_error =
                        format!("{link} is registered, but its file was not rewritten: {e}");
                    eprintln!("[azcalendar] {}", s.sync_error);
                }
            }
        }
        Err((status, message)) => {
            eprintln!("[azcalendar] azlin://meet/{room_id} not registered: {message}");
            // A link moved to another meeting server meanwhile is not this server's to refuse.
            let still_here = s.events.iter().any(|e| {
                e.id == event_id
                    && e.meeting
                        .as_ref()
                        .is_some_and(|m| m.pending && m.server == server)
            });
            if !still_here {
                return Update::DoNothing;
            }
            if !meeting::sync_again(status) {
                s.sync_refused.insert(event_id, message.clone());
            }
            // The week looks the same (the link still waits): redraw only for the Settings
            // sheet's status line, not every retry while the user types elsewhere.
            let shown = s.settings.is_some() && s.sync_error != message;
            s.sync_error = message;
            if !shown {
                return Update::DoNothing;
            }
        }
    }
    Update::RefreshDom
}

/// The timer that sends pending links again.
extern "C" fn on_sync_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let app = data.clone();
    if let Some(mut s) = data.downcast_mut::<CalState>() {
        sync_links(&mut s, &mut info.callback_info, &app);
    }
    TimerCallbackReturn::continue_unchanged()
}

/// Starts the timer that sends pending links again (once), and sends them now.
fn start_syncing(data: &mut RefAny, info: &mut CallbackInfo) {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return;
    };
    let s = &mut *guard;
    if !s.sync_timer {
        s.sync_timer = true;
        let get_time = info.get_system_time_fn();
        info.add_timer(
            TimerId::unique(),
            Timer::create(app.clone(), on_sync_tick, get_time).with_interval(Duration::System(
                SystemTimeDiff::from_millis(s.sync_every_ms),
            )),
        );
    }
    sync_links(s, info, &app);
}

// ==== Settings ====

extern "C" fn on_open_settings(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let server = s.server.clone();
    s.settings = Some(SettingsForm {
        server,
        error: String::new(),
    });
    Update::RefreshDom
}

extern "C" fn on_settings_server(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<CalState>() {
        if let Some(settings) = s.settings.as_mut() {
            settings.server = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_settings_cancel(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    s.settings = None;
    Update::RefreshDom
}

/// Saves the meeting server. Links still waiting are registered with the new one (nobody could
/// have joined them anywhere yet); registered ones stay with the server that has their room.
extern "C" fn on_settings_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(settings) = s.settings.as_mut() else {
        return Update::DoNothing;
    };
    let Some(server) = meet_rooms::normalize_server(&settings.server) else {
        settings.error = String::from(
            "Give the meeting server's address, such as https://meet.example.com or \
             http://127.0.0.1:8787.",
        );
        return Update::RefreshDom;
    };
    if let Err(e) = meeting::save_server(&settings::path(&s.data_dir), &server) {
        settings.error = format!("Could not save the setting: {e}");
        return Update::RefreshDom;
    }
    eprintln!("[azcalendar] meeting server: {server}");
    for event in &mut s.events {
        let Some(m) = event
            .meeting
            .as_mut()
            .filter(|m| m.pending && m.server != server)
        else {
            continue;
        };
        m.server = server.clone();
        if let Err(e) = event::save(&s.data_dir, event) {
            eprintln!("[azcalendar] could not move {} to {server}: {e}", event.id);
        }
    }
    s.server = server;
    s.settings = None;
    s.sync_refused.clear();
    sync_links(s, &mut info, &app);
    Update::RefreshDom
}

/// "Sync meeting links now": sends every pending link again, refused ones too.
extern "C" fn on_sync_now(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.sync_refused.clear();
    sync_links(s, &mut info, &app);
    Update::RefreshDom
}

/// The Settings sheet, right of the week: the meeting server, and how the links stand.
fn settings_panel(settings: &SettingsView, data: &RefAny) -> Dom {
    let label = |text: &str| Dom::create_span_with_text(text).with_css(LABEL);
    let note = |text: &str| {
        Dom::create_span_with_text(text)
            .with_css("font-size: 12px; color: system:secondary-text; margin-top: 6px;")
    };
    let status = match (settings.pending, settings.sync_error.is_empty()) {
        (0, _) => String::from("Every meeting link is on the meeting server."),
        (1, true) => String::from("1 meeting link is being sent to the meeting server."),
        (n, true) => format!("{n} meeting links are being sent to the meeting server."),
        (1, false) => format!("1 meeting link waits: {}", settings.sync_error),
        (n, false) => format!("{n} meeting links wait: {}", settings.sync_error),
    };
    let mut panel = Dom::create_div()
        .with_css(format!(
            "width: 340px; flex-shrink: 0; display: flex; flex-direction: column; padding: 16px; \
             background: system:window-background; border-left: 1px solid {LINE}; overflow-y: auto;"
        ))
        .with_child(
            Dom::create_span_with_text("Settings").with_css("font-size: 18px; font-weight: bold;"),
        )
        .with_child(label("Meeting server"))
        .with_child(
            TextInput::create()
                .with_text(settings.server.as_str())
                .with_placeholder("https://meet.example.com")
                .with_on_text_input(data.clone(), on_settings_server)
                .dom()
                .with_id("settings-server"),
        )
        .with_child(note(
            "AzMeet links are made on this computer, so they work offline; this server gets \
             them as soon as it answers.",
        ))
        .with_child(note(status.as_str()));
    if !settings.error.is_empty() {
        panel.add_child(
            Dom::create_span_with_text(settings.error.as_str())
                .with_css(ERROR),
        );
    }
    panel.with_child(
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: row; justify-content: flex-end; margin-top: 20px;",
            )
            .with_child(
                Button::create("Sync now")
                    .with_on_click(data.clone(), on_sync_now)
                    .dom()
                    .with_id("settings-sync")
                    .with_css("margin-right: auto;"),
            )
            .with_child(
                Button::create("Cancel")
                    .with_on_click(data.clone(), on_settings_cancel)
                    .dom()
                    .with_id("settings-cancel")
                    .with_css("margin-right: 8px;"),
            )
            .with_child(
                Button::with_type("Save", ButtonType::Primary)
                    .with_on_click(data.clone(), on_settings_save)
                    .dom()
                    .with_id("settings-save"),
            ),
    )
}

// ==== Join meeting ====

/// An event, for a callback on its block.
struct EventRef {
    app: RefAny,
    id: String,
}

/// Starts AzMeet to join `meeting`. The debug-server port is this app's own, so AzMeet does not
/// inherit it.
fn launch_azmeet(program: &Path, meet: &Meeting) -> std::io::Result<Child> {
    if !program.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} does not exist", program.display()),
        ));
    }
    Command::new(program)
        .envs(meeting::join_env(meet))
        .env_remove("AZ_DEBUG")
        .stdin(Stdio::null())
        .spawn()
}

extern "C" fn on_join_meeting(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = data
        .downcast_ref::<EventRef>()
        .map(|r| (r.app.clone(), r.id.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some((title, meet)) = s
        .events
        .iter()
        .find(|e| e.id == id)
        .and_then(|e| Some((e.title.clone(), e.meeting.clone()?)))
    else {
        return Update::DoNothing;
    };
    // Forget the AzMeet windows that were closed.
    s.launched
        .retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    let program = meeting::azmeet_program(
        std::env::var(meeting::AZMEET_BIN_VAR).ok().as_deref(),
        std::env::current_exe().ok().as_deref(),
    );
    let launched = match program {
        Some(program) => launch_azmeet(&program, &meet),
        None => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "the AzCalendar program's folder is unknown",
        )),
    };
    match launched {
        Ok(child) => {
            println!("AZCAL_JOIN_PID {}", child.id());
            eprintln!(
                "[azcalendar] AzMeet joins {} (pid {})",
                meet.link,
                child.id()
            );
            s.notice = format!("Opening AzMeet for \"{title}\"...");
            s.launched.push(child);
        }
        Err(e) => {
            info.set_clipboard_content(ClipboardContent {
                plain_text: AzString::from(meet.link.as_str()),
                styled_runs: StyledTextRunVec::create(),
                html: azul::option::OptionString::None,
            });
            eprintln!("[azcalendar] AzMeet not started ({e}); link copied");
            s.notice = format!(
                "AzMeet could not be started ({e}), so the meeting link was copied: {}",
                meet.link
            );
        }
    }
    Update::RefreshDom
}

// ==== Start ====

fn user_data_dir() -> Option<PathBuf> {
    FilePath::get_data_dir()
        .into_option()
        .map(|dir| PathBuf::from(dir.inner.as_str()))
}

pub fn start() {
    let data_dir = event::data_dir(std::env::var(DATA_VAR).ok().as_deref(), user_data_dir());
    let (events, skipped) = event::load_all(&data_dir);
    for file in &skipped {
        eprintln!(
            "[azcalendar] left out {}: {}",
            file.path.display(),
            file.reason
        );
    }
    let saved = settings::read_text(&settings::path(&data_dir));
    let server = meeting::server_setting(
        saved.as_deref(),
        std::env::var(meeting::WORKER_VAR).ok().as_deref(),
        meeting::BUILT_IN_WORKER,
    );
    let hour_px = saved
        .as_deref()
        .and_then(settings::hour_px)
        .unwrap_or(week::DEFAULT_HOUR_PX);
    let sync_seconds = std::env::var(SYNC_VAR)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(SYNC_SECONDS);
    eprintln!(
        "[azcalendar] {} event(s) in {}; meeting server: {server}; {hour_px} px an hour",
        events.len(),
        data_dir.display(),
    );
    let today = chrono::Local::now().date_naive();
    let state = CalState {
        data_dir,
        server,
        events,
        today,
        week: week::week_start(today),
        form: None,
        forms_opened: 0,
        notice: String::new(),
        launched: Vec::new(),
        hour_px,
        press: None,
        last_pinch_scale: None,
        popover_closed_at: None,
        syncing: BTreeSet::new(),
        sync_refused: BTreeMap::new(),
        sync_error: String::new(),
        sync_every_ms: sync_seconds.saturating_mul(1000),
        sync_timer: false,
        settings: None,
        zoom_save_queued: false,
    };
    let app = App::create(RefAny::new(state), AppConfig::create());
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(1200.0, 980.0);
    window.window_state.title = AzString::from("AzCalendar");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    app.run(window);
}

#[cfg(test)]
mod mode_tests {
    use super::*;

    /// Every `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa` colour written into `css`.
    fn fixed_colours(css: &str) -> Vec<String> {
        let bytes = css.as_bytes();
        let mut found = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'#' {
                i += 1;
                continue;
            }
            let digits = bytes[i + 1..]
                .iter()
                .take_while(|c| c.is_ascii_hexdigit())
                .count();
            if matches!(digits, 3 | 4 | 6 | 8) {
                found.push(css[i..=i + digits].to_string());
            }
            i += 1 + digits;
        }
        found
    }

    /// The page's chrome and text are the desktop's palette of the mode the window is in, like
    /// the widgets on it: a fixed light surface put the dark-mode Buttons (a translucent white
    /// face, white ink) on white, where they vanished.
    #[test]
    fn the_page_chrome_and_its_text_are_system_colours_in_light_and_dark() {
        for (name, css) in [
            ("BODY", BODY),
            ("TOOLBAR", TOOLBAR),
            ("FOOTER", FOOTER),
            ("LINE", LINE),
            ("LABEL", LABEL),
            ("POPOVER", POPOVER),
        ] {
            let fixed = fixed_colours(css);
            assert!(
                fixed.is_empty(),
                "{name} paints {fixed:?} in both modes; it must follow the window's mode \
                 (a `system:` colour): {css}"
            );
        }
    }

    /// A colour of the app's own (an accent tint, the notice, the error red) is chosen for a
    /// light page; each carries the twin it takes on a dark one.
    #[test]
    fn every_colour_of_the_apps_own_has_a_dark_twin() {
        for (name, css) in [
            ("NOTICE", NOTICE),
            ("TODAY_PAINT", TODAY_PAINT),
            ("EVENT_PAINT", EVENT_PAINT),
            ("DRAFT_PAINT", DRAFT_PAINT),
            ("DRAFT_TITLE", DRAFT_TITLE),
            ("ERROR", ERROR),
        ] {
            assert!(
                !fixed_colours(css).is_empty(),
                "{name} names a colour of its own: {css}"
            );
            assert!(
                css.contains("@media (prefers-color-scheme: dark) {"),
                "{name} has no dark twin: {css}"
            );
        }
        assert!(
            fixed_colours(DAY_PAINT).is_empty(),
            "a day is the desktop's surface"
        );
        assert!(
            fixed_colours(SECONDARY).is_empty(),
            "secondary ink is the desktop's"
        );
    }
}
