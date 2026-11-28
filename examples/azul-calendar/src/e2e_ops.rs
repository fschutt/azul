//! AzCalendar's ops for the scenario files and the debug server: the data a phone cannot be
//! started with (it has no command line for `--sample`), and the states the pictures need (a view, a
//! day, an event selected, a dialog or a sheet open). The same scenario file runs on a desktop and on
//! a phone. They exist only where the debug server does: a scenario or a request names them.
//!
//! - `azcal_load_sample`: the sample (a "Work" calendar and a week of events around today) into
//!   the calendar, as `--sample` does into an empty one.
//! - `azcal_view` `{"view": "month"}`: shows a view (`day`, `work-week`, `week`, `month`,
//!   `schedule`, `agenda`).
//! - `azcal_goto` `{"date": "2026-11-28"}`: moves the view to a day.
//! - `azcal_select_event` `{"title": "Sprint review"}`: selects the first event of that title.
//! - `azcal_page` `{"page": "print"}`: opens a File dialog (`info`, `open`, `print`, `calendars`,
//!   `options`, `about`); `{"page": "none"}` closes it.
//! - `azcal_sheet` `{"for": "event" | "file"}`: opens the phone's action sheet (for the selected
//!   event, or File's); `{"for": "none"}` closes it.

use azul::{
    error::CustomE2eOpResult,
    json::Json,
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
};
use chrono::{Datelike, NaiveDate};

use crate::{
    args::BackstagePage,
    calendars,
    event::{self, Event},
    sample,
    ui_sheet::SheetFor,
    views::{self, ViewKind},
    CalState,
};

pub const OP_LOAD_SAMPLE: &str = "azcal_load_sample";
pub const OP_VIEW: &str = "azcal_view";
pub const OP_GOTO: &str = "azcal_goto";
pub const OP_SELECT_EVENT: &str = "azcal_select_event";
pub const OP_PAGE: &str = "azcal_page";
pub const OP_SHEET: &str = "azcal_sheet";

/// The ops and what `list_app_ops` says of them.
static OPS: [(&str, &str); 6] = [
    (OP_LOAD_SAMPLE, "put the sample calendar (a Work calendar and a week of events around today) into the app"),
    (OP_VIEW, "show a view: {\"view\": \"day|work-week|week|month|schedule|agenda\"}"),
    (OP_GOTO, "move the view to a day: {\"date\": \"YYYY-MM-DD\"}"),
    (OP_SELECT_EVENT, "select the first event with a title: {\"title\": \"...\"}"),
    (OP_PAGE, "open a File dialog: {\"page\": \"info|open|print|calendars|options|about|none\"}"),
    (OP_SHEET, "open the phone's action sheet: {\"for\": \"event|file|none\"}"),
];

/// The description `list_app_ops` shows for an op.
#[must_use]
pub fn op_def(name: &str, summary: &str) -> String {
    serde_json::json!({
        "name": name,
        "summary": summary,
        "description": summary,
        "args": [],
        "examples": [],
    })
    .to_string()
}

/// Adds the ops to `config`.
pub fn register_ops(config: &mut AppConfig) {
    for (name, summary) in OPS {
        let json = Json::parse(AzString::from(op_def(name, summary).as_str()))
            .into_result()
            .unwrap_or_else(|_| Json::null());
        config.add_e2e_command(AzString::from(name), json, on_op);
    }
}

fn answer(json: &serde_json::Value) -> CustomE2eOpResult {
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(json.to_string().as_str()),
    }
}

fn failed(why: &str) -> CustomE2eOpResult {
    answer(&serde_json::json!({ "success": false, "error": why }))
}

/// The sample into the calendar: the Work calendar and its events, each queued for the file
/// thread like any save. Returns how many events were added.
fn load_sample(s: &mut CalState) -> usize {
    let work = s
        .calendars
        .iter()
        .find(|c| c.name == "Work")
        .map(|c| c.id.clone())
        .unwrap_or_else(|| {
            let id = calendars::new_calendar_id();
            let made = sample::work_calendar(&id);
            s.store_calendar(&made);
            s.calendars.push(made);
            id
        });
    let events = sample::sample_events(s.today, &work, event::new_event_id);
    let count = events.len();
    for e in events {
        let _ = s.store_event(e);
    }
    count
}

/// The first event titled `title`, selected on its first occurrence in the view (else on its own
/// day).
fn select_event(s: &mut CalState, title: &str) -> bool {
    let Some((index, event)) = s
        .events
        .iter()
        .enumerate()
        .find(|(_, e): &(usize, &Event)| e.title == title)
    else {
        return false;
    };
    let (from, to) = views::visible_range(s.view, s.anchor);
    let day = s
        .occurrences(from, to)
        .into_iter()
        .find(|o| o.index == index)
        .map_or(event.date, |o| o.first);
    s.selected = Some((event.id.clone(), day));
    true
}

fn page_of(name: &str) -> Option<BackstagePage> {
    BackstagePage::ALL.into_iter().find(|p| p.name() == name)
}

fn date_of(text: &str) -> Option<NaiveDate> {
    let mut parts = text.trim().split('-');
    let y = parts.next()?.parse::<i32>().ok()?;
    let m = parts.next()?.parse::<u32>().ok()?;
    let d = parts.next()?.parse::<u32>().ok()?;
    NaiveDate::from_ymd_opt(y, m, d)
}

extern "C" fn on_op(mut data: RefAny, mut info: CallbackInfo, name: AzString, args: AzString) -> CustomE2eOpResult {
    let args: serde_json::Value = serde_json::from_str(args.as_str()).unwrap_or(serde_json::Value::Null);
    let text = |key: &str| args.get(key).and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let Some(mut s) = data.downcast_mut::<CalState>() else {
        return failed("the app has no calendar state");
    };
    let result = match name.as_str() {
        OP_LOAD_SAMPLE => {
            let added = load_sample(&mut s);
            answer(&serde_json::json!({ "success": true, "events": added, "total": s.events.len() }))
        }
        OP_VIEW => match ViewKind::from_name(&text("view")) {
            Some(view) => {
                s.set_view(view);
                answer(&serde_json::json!({ "success": true, "view": view.name() }))
            }
            None => failed("a view is day, work-week, week, month, schedule or agenda"),
        },
        OP_GOTO => match date_of(&text("date")) {
            Some(day) => {
                s.set_anchor(day);
                answer(&serde_json::json!({ "success": true, "date": day.to_string(), "year": day.year() }))
            }
            None => failed("a date is YYYY-MM-DD"),
        },
        OP_SELECT_EVENT => {
            if select_event(&mut s, &text("title")) {
                answer(&serde_json::json!({ "success": true }))
            } else {
                failed("there is no event with that title")
            }
        }
        OP_PAGE => {
            let which = text("page");
            if which == "none" {
                s.backstage = None;
                answer(&serde_json::json!({ "success": true }))
            } else if let Some(page) = page_of(&which) {
                crate::chrome::open_backstage(&mut s, page);
                answer(&serde_json::json!({ "success": true, "page": page.name() }))
            } else {
                failed("a page is info, open, print, calendars, options, about or none")
            }
        }
        OP_SHEET => match text("for").as_str() {
            "none" => {
                s.sheet = None;
                answer(&serde_json::json!({ "success": true }))
            }
            "file" => {
                s.sheet = Some(SheetFor::File);
                answer(&serde_json::json!({ "success": true }))
            }
            "event" => match s.selected.clone() {
                Some((id, date)) => {
                    s.sheet = Some(SheetFor::Event { id, date });
                    answer(&serde_json::json!({ "success": true }))
                }
                None => failed("no event is selected"),
            },
            _ => failed("a sheet is for event, file or none"),
        },
        _ => {
            return CustomE2eOpResult {
                handled: false,
                json: AzString::from(""),
            }
        }
    };
    drop(s);
    // The window is built again, now that the state has changed.
    let timer = Timer::create(data.clone(), on_rebuild, info.get_system_time_fn())
        .with_delay(Duration::System(SystemTimeDiff::from_millis(1)));
    info.add_timer(TimerId::unique(), timer);
    result
}

extern "C" fn on_rebuild(_data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    TimerCallbackReturn::terminate_and_refresh_dom()
}

#[cfg(test)]
#[path = "e2e_ops_test.rs"]
mod e2e_ops_test;
