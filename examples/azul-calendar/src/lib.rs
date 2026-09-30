//! AzCalendar: a small calendar on the public azul API that mints AzMeet links.
//!
//! A week view (Monday to Sunday, 08:00 to 20:00) lists the events of the shown week; events that
//! overlap sit side by side. "New event" opens a form (title, day, start, end, "Add AzMeet link").
//! Save writes the event as one JSON file, `<data dir>/events/<uuid>.json` (see `event.rs`); with
//! "Add AzMeet link" it first asks the meeting server (the `meet` Worker) for a room with
//! `POST /rooms`, on an azul `Thread` so no callback waits on the network, and keeps the returned
//! `azlin://meet/<room id>` link in the file. An event with a link shows "Join meeting", which
//! starts AzMeet with `AZMEET_JOIN=<link>` (or, without AzMeet, copies the link).
//!
//! Durable data is only the event files; the meeting server holds the rooms (minting and joining),
//! nothing of the calendar.
//!
//! Environment:
//! - `AZCAL_DATA`: the data folder (default: `AzCalendar` in the user's data folder).
//! - `AZMEET_WORKER`: the meeting server, as for AzMeet (default: AzMeet's built-in one, set at
//!   build time with `AZMEET_DEFAULT_WORKER`; none means no links can be made).
//! - `AZMEET_BIN`: the AzMeet program "Join meeting" starts (default: `AzMeet` next to AzCalendar).
//!
//! On stdout, for scripts: `AZCAL_SAVED <file>` and `AZCAL_LINK <link>` when an event is saved,
//! `AZCAL_JOIN_PID <pid>` when "Join meeting" started AzMeet.

pub mod event;
pub mod meeting;
pub mod week;

/// AzMeet's meeting links and room keys: AzMeet's own `rooms.rs`, compiled into AzCalendar too,
/// so the two apps read links the same way.
#[path = "../../azul-meet/src/rooms.rs"]
pub mod meet_rooms;

use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use azul::{
    dom::ClipboardContent,
    error::HttpError,
    file::FilePath,
    http::{HttpGetResult, HttpMethod, HttpRequestConfig},
    prelude::*,
    str::String as AzString,
    task::{Thread, ThreadId, ThreadReceiver, ThreadSender},
    uuid::Uuid,
    vec::{StyledTextRunVec, U8Vec},
    widgets::{
        ButtonType, CheckBoxState, DatePicker, DatePickerState, OnTextInputReturn, TextInputState,
        TextInputValid, TimePicker, TimePickerState,
    },
};
use chrono::{Datelike, NaiveDate, NaiveTime};
use event::{Event, EventError, Meeting};

/// The data folder's variable.
const DATA_VAR: &str = "AZCAL_DATA";
/// Pixels per hour in the week view.
const HOUR_PX: u32 = 64;
/// Width of the hour labels left of the days.
const GUTTER_PX: u32 = 56;
const HTTP_TIMEOUT_SECS: u64 = 10;
/// What the form's "Add AzMeet link" line says once it is ticked.
const WILL_MINT: &str = "A new AzMeet link is made when you save.";

const BODY: &str = "display: flex; flex-direction: column; height: 100%; margin: 0; font-family: \
                    sans-serif; font-size: 14px; color: #1d2330; background: #f4f5f8;";
const TOOLBAR: &str = "display: flex; flex-direction: row; align-items: center; padding: 10px \
                       16px; background: #ffffff; border-bottom: 1px solid #d9dce3;";
const NOTICE: &str = "padding: 6px 16px; font-size: 13px; color: #2c4a7a; background: #e6eefc;";
const FOOTER: &str = "padding: 4px 16px; font-size: 12px; color: #6b7385; background: #ffffff; \
                      border-top: 1px solid #d9dce3;";
const LINE: &str = "#d9dce3";
const LABEL: &str = "font-size: 12px; color: #4a5468; margin-top: 12px; margin-bottom: 4px;";

/// The app.
struct CalState {
    data_dir: PathBuf,
    /// The meeting server, if any.
    worker: Option<String>,
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
}

/// The "New event" form.
struct Form {
    serial: u32,
    /// The new event's id, fixed when the form opens: its file's name.
    id: String,
    title: String,
    date: NaiveDate,
    start: NaiveTime,
    end: NaiveTime,
    add_meet: bool,
    /// `POST /rooms` is on its way.
    minting: bool,
    /// A link minted for this form whose event could not be saved yet: used, not minted again.
    minted: Option<Meeting>,
    error: String,
}

impl Form {
    fn event(&self, meeting: Option<Meeting>) -> Result<Event, EventError> {
        Event::create(
            &self.id,
            &self.title,
            self.date,
            self.start,
            self.end,
            meeting,
        )
    }
}

fn at(hour: u32, minute: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(hour, minute, 0).unwrap_or(NaiveTime::MIN)
}

/// What the form says about an event it cannot save.
fn form_error(e: &EventError) -> String {
    match e {
        EventError::EmptyTitle => String::from("Give the event a title."),
        EventError::EndNotAfterStart => String::from("The event must end after it starts."),
        other => format!("This event cannot be saved: {other}."),
    }
}

// ==== Layout ====

/// What the window shows, read from `CalState` before the DOM is built.
struct View {
    week_title: String,
    days: Vec<DayView>,
    form: Option<FormView>,
    can_mint: bool,
    notice: String,
    footer: String,
}

struct DayView {
    label: String,
    today: bool,
    blocks: Vec<BlockView>,
    earlier: usize,
    later: usize,
}

struct BlockView {
    id: String,
    title: String,
    time: String,
    /// Minutes from 08:00, and minutes long, inside the view.
    top: u32,
    height: u32,
    lane: u32,
    lanes: u32,
    joinable: bool,
}

struct FormView {
    title: String,
    date: NaiveDate,
    start: NaiveTime,
    end: NaiveTime,
    add_meet: bool,
    minting: bool,
    error: String,
}

fn view_of(s: &CalState) -> View {
    let lists = week::events_in_week(&s.events, s.week);
    let days = week::week_days(s.week)
        .iter()
        .zip(lists.iter())
        .map(|(date, list)| {
            let layout = week::lay_out_day(list);
            DayView {
                label: week::day_label(*date),
                today: *date == s.today,
                blocks: layout
                    .placements
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
                            joinable: e.meeting.is_some(),
                        }
                    })
                    .collect(),
                earlier: layout.earlier,
                later: layout.later,
            }
        })
        .collect();
    View {
        week_title: week::week_title(s.week),
        days,
        form: s.form.as_ref().map(|f| FormView {
            title: f.title.clone(),
            date: f.date,
            start: f.start,
            end: f.end,
            add_meet: f.add_meet,
            minting: f.minting,
            error: f.error.clone(),
        }),
        can_mint: s.worker.is_some(),
        notice: s.notice.clone(),
        footer: format!(
            "Events: {} · Meeting server: {}",
            s.data_dir.join(event::EVENTS_DIR).display(),
            s.worker.as_deref().unwrap_or("none (set AZMEET_WORKER)")
        ),
    }
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let Some(view) = data.downcast_ref::<CalState>().map(|s| view_of(&s)) else {
        return Dom::create_body();
    };
    let mut main = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 1;")
        .with_child(week_grid(&view, &data));
    if let Some(form) = &view.form {
        main.add_child(form_panel(form, view.can_mint, &data));
    }
    let mut body = Dom::create_body()
        .with_css(BODY)
        .with_child(toolbar(&view, &data));
    if !view.notice.is_empty() {
        body.add_child(Dom::create_span_with_text(view.notice.as_str()).with_css(NOTICE));
    }
    body.with_child(main)
        .with_child(Dom::create_span_with_text(view.footer.as_str()).with_css(FOOTER))
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

fn week_grid(view: &View, data: &RefAny) -> Dom {
    let gutter = format!("width: {GUTTER_PX}px; flex-shrink: 0;");
    let mut header = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; background: #ffffff; border-bottom: 1px solid \
             {LINE};"
        ))
        .with_child(Dom::create_div().with_css(gutter.as_str()));
    let mut hours = Dom::create_div().with_css(gutter.as_str());
    for hour in week::FIRST_HOUR..week::END_HOUR {
        hours.add_child(
            Dom::create_div()
                .with_css(format!(
                    "height: {HOUR_PX}px; padding-right: 6px; font-size: 11px; color: #6b7385; \
                     text-align: right; box-sizing: border-box;"
                ))
                .with_child(Dom::create_span_with_text(format!("{hour:02}:00"))),
        );
    }
    let mut columns = Dom::create_div()
        .with_css("display: flex; flex-direction: row;")
        .with_child(hours);
    for day in &view.days {
        header.add_child(day_header(day));
        columns.add_child(day_column(day, data));
    }
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; padding: 0 8px 8px 0;")
        .with_child(header)
        .with_child(columns)
}

fn day_header(day: &DayView) -> Dom {
    let colour = if day.today {
        "color: #2f6db0; font-weight: bold;"
    } else {
        ""
    };
    let mut header = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; padding: 6px \
             8px; border-left: 1px solid {LINE}; {colour}"
        ))
        .with_child(Dom::create_span_with_text(day.label.as_str()));
    let mut outside = Vec::new();
    if day.earlier > 0 {
        outside.push(format!("+{} before 08:00", day.earlier));
    }
    if day.later > 0 {
        outside.push(format!("+{} after 20:00", day.later));
    }
    if !outside.is_empty() {
        header.add_child(
            Dom::create_span_with_text(outside.join(", "))
                .with_css("font-size: 11px; color: #6b7385; font-weight: normal;"),
        );
    }
    header
}

fn day_column(day: &DayView, data: &RefAny) -> Dom {
    let height = (week::END_HOUR - week::FIRST_HOUR) * HOUR_PX;
    let background = if day.today { "#f7faff" } else { "#ffffff" };
    let mut column = Dom::create_div().with_css(format!(
        "position: relative; flex-grow: 1; flex-basis: 0px; height: {height}px; border-left: 1px \
         solid {LINE}; background: {background};"
    ));
    for _ in week::FIRST_HOUR..week::END_HOUR {
        column.add_child(Dom::create_div().with_css(format!(
            "height: {HOUR_PX}px; border-top: 1px solid #e8eaf0; box-sizing: border-box;"
        )));
    }
    for block in &day.blocks {
        column.add_child(event_block(block, data));
    }
    column
}

/// One event in its day's column: title, time and, with a meeting link, "Join meeting".
fn event_block(block: &BlockView, data: &RefAny) -> Dom {
    let top = block.top * HOUR_PX / 60;
    let height = (block.height * HOUR_PX / 60).max(18);
    let width = 100.0 / block.lanes.max(1) as f32;
    let left = width * block.lane as f32;
    let mut dom = Dom::create_div()
        .with_css(format!(
            "position: absolute; top: {top}px; left: {left:.3}%; width: {width:.3}%; height: \
             {height}px; box-sizing: border-box; display: flex; flex-direction: column; padding: \
             3px 6px; background: #dbe7ff; border-left: 3px solid #2f6db0; border-radius: 4px; \
             font-size: 12px;"
        ))
        .with_child(Dom::create_span_with_text(block.title.as_str()).with_css("font-weight: bold;"))
        .with_child(Dom::create_span_with_text(block.time.as_str()).with_css("color: #4a5468;"));
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

/// The "New event" form, a side sheet right of the week.
fn form_panel(form: &FormView, can_mint: bool, data: &RefAny) -> Dom {
    let label = |text: &str| Dom::create_span_with_text(text).with_css(LABEL);
    let mut panel = Dom::create_div()
        .with_css(format!(
            "width: 340px; flex-shrink: 0; display: flex; flex-direction: column; padding: 16px; \
             background: #ffffff; border-left: 1px solid {LINE};"
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
        .with_child(time_picker(form.end, data, on_end));
    if can_mint {
        panel.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 16px;",
                )
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
            panel.add_child(
                Dom::create_span_with_text(WILL_MINT)
                    .with_css("font-size: 12px; color: #4a5468; margin-top: 4px;"),
            );
        }
    } else {
        panel.add_child(
            Dom::create_span_with_text(
                "No meeting server is set: start AzCalendar with AZMEET_WORKER=<url> to add \
                 AzMeet links.",
            )
            .with_css("font-size: 12px; color: #6b7385; margin-top: 16px;"),
        );
    }
    if !form.error.is_empty() {
        panel.add_child(
            Dom::create_span_with_text(form.error.as_str())
                .with_css("font-size: 13px; color: #b3261e; margin-top: 12px;"),
        );
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
                Button::with_type(
                    if form.minting {
                        "Making the link..."
                    } else {
                        "Save event"
                    },
                    ButtonType::Primary,
                )
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

// ==== The form ====

extern "C" fn on_new_event(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if s.form.is_some() {
        return Update::DoNothing;
    }
    s.forms_opened += 1;
    s.form = Some(Form {
        serial: s.forms_opened,
        id: Uuid::v4().as_str().to_string(),
        title: String::new(),
        date: week::default_day(s.today, s.week),
        start: at(9, 0),
        end: at(10, 0),
        add_meet: false,
        minting: false,
        minted: None,
        error: String::new(),
    });
    s.notice.clear();
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
    if form.minting {
        return Update::DoNothing;
    }
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

/// Save: without a link, writes the event at once; with one, first asks the meeting server.
extern "C" fn on_save(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<CalState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let worker = s.worker.clone();
        let Some(form) = s.form.as_mut() else {
            return Update::DoNothing;
        };
        if form.minting {
            return Update::DoNothing;
        }
        form.error.clear();
        if let Err(e) = form.event(None) {
            form.error = form_error(&e);
            eprintln!("[azcalendar] cannot save: {}", form.error);
            return Update::RefreshDom;
        }
        match (form.add_meet, form.minted.clone(), worker) {
            (true, None, Some(server)) => {
                form.minting = true;
                MintJob {
                    server,
                    serial: form.serial,
                }
            }
            (true, None, None) => {
                form.error = String::from(
                    "No meeting server is set (AZMEET_WORKER), so no AzMeet link can be made.",
                );
                return Update::RefreshDom;
            }
            (true, Some(meeting), _) => {
                save_form(s, Some(meeting));
                return Update::RefreshDom;
            }
            (false, _, _) => {
                save_form(s, None);
                return Update::RefreshDom;
            }
        }
    };
    eprintln!("[azcalendar] asking {} for a meeting room", job.server);
    spawn_mint(&mut info, data.clone(), job);
    Update::RefreshDom
}

/// Writes the form's event, with `meeting`, to its file; adds it to the calendar, shows its week
/// and closes the form. On failure the form stays open with the reason (and keeps the link).
fn save_form(s: &mut CalState, meeting: Option<Meeting>) {
    let Some(form) = s.form.as_mut() else {
        return;
    };
    let event = match form.event(meeting.clone()) {
        Ok(event) => event,
        Err(e) => {
            form.error = form_error(&e);
            form.minted = meeting;
            return;
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
            s.events.push(event);
            s.form = None;
        }
        Err(e) => {
            form.error = format!(
                "Could not write {}: {e}",
                event::event_path(&s.data_dir, &event.id).display()
            );
            eprintln!("[azcalendar] {}", form.error);
            form.minted = event.meeting;
        }
    }
}

// ==== Minting a link: POST /rooms on an azul Thread, the answer on the UI thread ====

struct MintJob {
    server: String,
    /// The form the link is for.
    serial: u32,
}

struct MintInit {
    job: MintJob,
    app: RefAny,
}

/// What the answer resumes with.
struct MintReply {
    app: RefAny,
    serial: u32,
    server: String,
}

fn spawn_mint(info: &mut CallbackInfo, app: RefAny, job: MintJob) {
    let init = RefAny::new(MintInit { job, app });
    info.add_thread(
        ThreadId::unique(),
        Thread::create(init, RefAny::new(()), mint_thread),
    );
}

/// Runs `POST /rooms` on a worker thread: `http_request` blocks here, then queues its answer,
/// which the UI thread hands to `on_minted`.
extern "C" fn mint_thread(mut init: RefAny, _sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((url, reply)) = init.downcast_ref::<MintInit>().map(|i| {
        (
            format!("{}/rooms", i.job.server),
            MintReply {
                app: i.app.clone(),
                serial: i.job.serial,
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
            U8Vec::from(b"{}".to_vec()),
            "application/json",
            RefAny::new(reply),
            on_minted,
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

/// The meeting in the server's answer, or what to tell the user.
fn mint_outcome(server: &str, result: RefAny) -> Result<Meeting, String> {
    let Some(answer) = HttpGetResult::downcast(result).into_option() else {
        return Err(meeting::mint_failure(server, None, "no answer"));
    };
    match answer.result.into_result() {
        Ok(response) => {
            let body = response
                .body_as_string()
                .into_option()
                .map(|body| body.as_str().to_string())
                .unwrap_or_default();
            match response.status_code {
                200 | 201 => meeting::minted_meeting(server, &body),
                status => Err(meeting::mint_failure(server, Some(status), &body)),
            }
        }
        Err(HttpError::HttpStatus(e)) => {
            Err(meeting::mint_failure(server, Some(e.status_code), ""))
        }
        Err(e) => Err(meeting::mint_failure(server, None, &http_error_text(&e))),
    }
}

/// The answer to `POST /rooms`: saves the form's event with the new link.
extern "C" fn on_minted(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut app, serial, server)) = data
        .downcast_ref::<MintReply>()
        .map(|r| (r.app.clone(), r.serial, r.server.clone()))
    else {
        return Update::DoNothing;
    };
    let outcome = mint_outcome(&server, result);
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    // The form was closed, or another one opened, while the server answered: nothing to do.
    let Some(form) = s.form.as_mut().filter(|f| f.serial == serial && f.minting) else {
        return Update::DoNothing;
    };
    form.minting = false;
    match outcome {
        Ok(meeting) => {
            eprintln!("[azcalendar] {server} made the room {}", meeting.link);
            save_form(s, Some(meeting));
        }
        Err(message) => {
            eprintln!("[azcalendar] no link: {message}");
            form.error = message;
        }
    }
    Update::RefreshDom
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
    let worker = meeting::worker(
        std::env::var(meeting::WORKER_VAR).ok().as_deref(),
        meeting::BUILT_IN_WORKER,
    );
    eprintln!(
        "[azcalendar] {} event(s) in {}; meeting server: {}",
        events.len(),
        data_dir.display(),
        worker.as_deref().unwrap_or("none")
    );
    let today = chrono::Local::now().date_naive();
    let state = CalState {
        data_dir,
        worker,
        events,
        today,
        week: week::week_start(today),
        form: None,
        forms_opened: 0,
        notice: String::new(),
        launched: Vec::new(),
    };
    let app = App::create(RefAny::new(state), AppConfig::create());
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(1200.0, 980.0);
    window.window_state.title = AzString::from("AzCalendar");
    app.run(window);
}
