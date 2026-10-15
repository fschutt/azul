//! The DOM of AzClock's screens: the mode row, the world clock (an analog
//! face of rotated divs and the cities), the alarms, the timer, the
//! stopwatch, the dialogs and the ringing overlay, and the clock's own
//! settings sections. Every id and class is a `__azclock_` name from
//! `ids.rs`; every button carries an [`Act`](super::actions::Act).

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DateRepeatPickerOnChangeCallbackType,
        DropDownOnChoiceChangeCallbackType, ModalOnCloseCallbackType,
        NumberInputOnValueChangeCallbackType, SegmentedOnChangeCallbackType,
        SwitchOnToggleCallbackType, TextInputOnTextInputCallbackType,
        TimePickerOnChangeCallbackType,
    },
    menu::{Menu, MenuItem, StringMenuItem},
    option::OptionString,
    prelude::*,
    str::String as AzString,
    vec::StringVec,
    widgets::{
        ButtonType, DatePickerWeekStart, DateRepeatPicker, DropDown, Gauge, GaugeKind, Modal,
        NumberInput, Segmented, Switch, TextInput, TimePicker,
    },
};
use azul_appkit::{
    l10n::{self, label, t, t_args, Arg, DateStyle},
    pieces::Clickable,
    ui::{self as kit, AppSection},
};
use chrono::{DateTime, Local, Timelike, Utc};

use super::{
    actions::{self, act, Action},
    now_ms, ClockApp, Ringing, Screen,
};
use crate::{fmt, ids, stopwatch::LapMark, tone::Sound, world};

/// From this width the world clock's face and list sit side by side.
pub const WIDE: f32 = 640.0;
/// "Ring for" (minutes).
pub const RING_MINUTES: [u32; 5] = [1, 5, 10, 15, 30];

// ==== Small parts ====

/// Choices: keys said in the window's language, plain words (a length) as they are.
fn strs(items: &[&str]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| label(s)).collect())
}

/// A key said, plain words (a name, a time) as they are.
fn text(content: &str) -> Dom {
    Dom::create_span_with_text(label(content))
}

/// A block of text with its own style.
fn line(content: &str, css: &str) -> Dom {
    Dom::create_div().with_css(css).with_child(text(content))
}

/// A button that does `action`.
fn button(name: &str, app: &RefAny, action: Action) -> Dom {
    Button::create(label(name))
        .with_on_click(act(app, action), actions::on_act as ButtonOnClickCallbackType)
        .dom()
}

/// The primary button of a group.
fn primary(name: &str, app: &RefAny, action: Action) -> Dom {
    Button::with_type(label(name), ButtonType::Primary)
        .with_on_click(act(app, action), actions::on_act as ButtonOnClickCallbackType)
        .dom()
}

fn row_css(gap: u32) -> String {
    format!("display: flex; flex-direction: row; align-items: center; column-gap: {gap}px;")
}

// Under flora (`@theme(flora)` after the flat values; flat is unchanged): the figures are
// flora's Garamond (its figures are tabular and lining - they keep their width as they tick),
// secondary text is the label ink rather than a faded ink, the rules are the grid's.
const COLUMN: &str = "display: flex; flex-direction: column;";
const SPACER: &str = "flex-grow: 1;";
const SECONDARY: &str = "font-size: 12px; opacity: 0.7; @theme(flora) { opacity: 1; color: \
                         system:secondary-text; }";
const EMPTY: &str = "padding: 24px; opacity: 0.7; font-size: 14px; text-align: center; \
                     @theme(flora) { opacity: 1; color: system:secondary-text; }";
/// A scrolling list that fills the rest of the screen.
const LIST: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; padding: 4px 12px;";
const ROW: &str = "display: flex; flex-direction: row; align-items: center; column-gap: 12px; padding: 8px 4px; border-bottom: 1px solid rgba(128,128,128,0.25); @theme(flora) { border-bottom: 1px solid system:grid; }";
/// A time, a lap, a countdown: flat's monospace, flora's Garamond (appended after the
/// part's own `font-family: monospace`).
const FIGURES_FLORA: &str = "@theme(flora) { font-family: EB Garamond, Georgia, serif; \
                             font-variant-numeric: tabular-nums lining-nums; }";

// ==== The mode row ====

/// World / Alarms / Timer / Stopwatch, then "+" for the screen and Settings.
pub fn modes_row(s: &ClockApp, app: &RefAny) -> Dom {
    let labels: Vec<&str> = Screen::ALL.iter().map(|m| m.label()).collect();
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; column-gap: 8px; padding: 4px 8px;")
        .with_child(
            Segmented::create(strs(&labels))
                .with_selected_index(s.screen.index())
                .with_on_change(app.clone(), actions::on_screen as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::MODES),
        )
        .with_child(Dom::create_div().with_css(SPACER));
    let new = match s.screen {
        Screen::World => Some("azclock-add-city"),
        Screen::Alarms => Some("azclock-new-alarm"),
        Screen::Timer => Some("azclock-new-timer"),
        Screen::Stopwatch => None,
    };
    if let Some(name) = new {
        row.add_child(
            Button::create(label(name))
                .with_icon("add")
                .with_on_click(act(app, Action::New), actions::on_act as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::NEW),
        );
    }
    row.with_child(
        Button::create(label("azclock-settings"))
            .with_icon("settings")
            .with_on_click(act(app, Action::OpenSettings), actions::on_act as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::SETTINGS),
    )
}

// ==== The screen ====

/// The screen the mode row chose, over the status line.
pub fn screen(s: &ClockApp, app: &RefAny, now: DateTime<Utc>, wide: bool) -> Dom {
    let content = if !s.loaded {
        line("azclock-loading", EMPTY)
    } else {
        match s.screen {
            Screen::World => world_view(s, app, now, wide),
            Screen::Alarms => alarms_view(s, app, now),
            Screen::Timer => timer_view(s, app),
            Screen::Stopwatch => stopwatch_view(s, app),
        }
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
                .with_child(content),
        )
        .with_child(status_line(s, now))
}

/// The line under the screen: a notice, else when the next alarm rings,
/// else whether the system rings alarms while AzClock is closed.
fn status_line(s: &ClockApp, now: DateTime<Utc>) -> Dom {
    let text = if !s.notice.is_empty() {
        s.notice.clone()
    } else {
        match s.next_alarm(now) {
            Some((i, at)) => {
                let a = &s.alarms[i];
                format!(
                    "{}{}",
                    t_args("azclock-next-alarm-in", &[("span", Arg::from(fmt::until((at - now).num_milliseconds())))]),
                    if a.label.is_empty() {
                        String::new()
                    } else {
                        format!(" - {}", a.label)
                    }
                )
            }
            None => t("azclock-no-alarm-on"),
        }
    };
    let id = if s.notice.is_empty() { ids::STATUS } else { ids::NOTICE };
    Dom::create_div()
        .with_id(id)
        .with_css(
            "padding: 6px 12px; font-size: 12px; border-top: 1px solid rgba(128,128,128,0.25); \
             @theme(flora) { border-top: 1px solid system:grid; }",
        )
        .with_child(Dom::create_span_with_text(text))
}

// ==== World ====

/// An analog face: twelve ticks and three hands, rotated divs (CSS
/// `transform: rotate`), no canvas. Flat draws it in a mid grey with a red
/// second hand; flora in its inks - the ring and ticks in the quiet ink, the
/// hour and minute hands in the ink, the second hand and the hub in the
/// stone (its glow at night).
fn analog_face(hour: u32, minute: u32, second: u32, size: f32) -> Dom {
    let c = size / 2.0;
    let mut face = Dom::create_div().with_id(ids::FACE).with_css(format!(
        "position: relative; width: {size}px; height: {size}px; border-radius: {c}px; \
         border: 2px solid rgba(128,128,128,0.6); flex-shrink: 0; @theme(flora) {{ border: 2px \
         solid system:tertiary-text; }}"
    ));
    for i in 0..12 {
        let long = i % 3 == 0;
        let (w, h) = if long { (3.0, 12.0) } else { (2.0, 6.0) };
        face.add_child(Dom::create_div().with_css(format!(
            "position: absolute; left: {}px; top: 4px; width: {w}px; height: {h}px; \
             background: rgba(128,128,128,0.9); transform-origin: {}px {}px; transform: rotate({}deg); \
             @theme(flora) {{ background: system:tertiary-text; }}",
            c - w / 2.0,
            w / 2.0,
            c - 4.0,
            i * 30
        )));
    }
    let hand = |degrees: f32, width: f32, length: f32, color: &str, flora: &str, class: AzString| {
        Dom::create_div().with_class(class).with_css(format!(
            "position: absolute; left: {}px; top: {}px; width: {width}px; height: {length}px; \
             background: {color}; border-radius: {}px; transform-origin: {}px {length}px; \
             transform: rotate({degrees:.1}deg); @theme(flora) {{ background: {flora}; }}",
            c - width / 2.0,
            c - length,
            width / 2.0,
            width / 2.0
        ))
    };
    let seconds = second as f32;
    let minutes = minute as f32 + seconds / 60.0;
    let hours = (hour % 12) as f32 + minutes / 60.0;
    let ink = "system:text";
    let stone = "system:accent";
    face.add_child(hand(hours * 30.0, 5.0, size * 0.25, "rgba(128,128,128,1)", ink, ids::HAND_HOUR));
    face.add_child(hand(minutes * 6.0, 3.0, size * 0.36, "rgba(128,128,128,1)", ink, ids::HAND_MINUTE));
    face.add_child(hand(seconds * 6.0, 1.5, size * 0.42, "#d0453c", stone, ids::HAND_SECOND));
    face.with_child(Dom::create_div().with_css(format!(
        "position: absolute; left: {}px; top: {}px; width: 8px; height: 8px; border-radius: 4px; \
         background: #d0453c; @theme(flora) {{ background: {stone}; }}",
        c - 4.0,
        c - 4.0
    )))
}

/// "Friday 9 October": the kit's words for a day.
fn day_text(day: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    l10n::date_text(DateStyle::WeekdayDayMonth, day.year(), day.month(), day.day(), day.weekday().num_days_from_monday())
}

fn world_view(s: &ClockApp, app: &RefAny, now: DateTime<Utc>, wide: bool) -> Dom {
    let twelve = s.twelve_hour();
    // The face shows here, or the city clicked last.
    let face_city = s.face_city.and_then(|i| s.cities.get(i));
    let (h, m, sec, date, place, abbreviation) = match face_city.and_then(|c| world::parse_zone(&c.zone).map(|z| (c, z))) {
        Some((city, zone)) => {
            let t = now.with_timezone(&zone);
            (
                t.hour(),
                t.minute(),
                t.second(),
                day_text(t.date_naive()),
                city.name.clone(),
                world::abbreviation(zone, now),
            )
        }
        None => {
            let t = now.with_timezone(&Local);
            let zone = world::local_zone();
            (
                t.hour(),
                t.minute(),
                t.second(),
                day_text(t.date_naive()),
                zone.map_or_else(
                    || l10n::t("azclock-here"),
                    |z| t_args("azclock-city-here", &[("city", Arg::from(world::city_name(z.name())))]),
                ),
                zone.map(|z| world::abbreviation(z, now)).unwrap_or_default(),
            )
        }
    };
    let clock_text = if twelve {
        format!("{}:{sec:02}", fmt::clock(h, m, true))
    } else {
        format!("{h:02}:{m:02}:{sec:02}")
    };
    let mut face_panel = Dom::create_div()
        .with_css("display: flex; flex-direction: column; align-items: center; row-gap: 6px; padding: 16px;")
        .with_child(analog_face(h, m, sec, 180.0))
        .with_child(
            Dom::create_div()
                .with_id(ids::FACE_TIME)
                .with_css(format!("font-size: 28px; font-weight: 600; font-family: monospace; {FIGURES_FLORA}"))
                .with_child(text(&format!("{clock_text} {abbreviation}"))),
        )
        .with_child(line(&date, "font-size: 14px;"))
        .with_child(line(&place, SECONDARY));
    if s.face_city.is_some() {
        face_panel.add_child(button("azclock-show-here", app, Action::FaceCity(None)));
    }

    let mut list = Dom::create_div().with_id(ids::CITIES).with_css(LIST);
    if s.cities.is_empty() {
        list.add_child(line("azclock-no-cities", EMPTY));
    }
    for (i, city) in s.cities.iter().enumerate() {
        let Some(r) = world::row(city, &Local, now) else {
            continue;
        };
        let when = format!(
            "{} - {} - {}",
            t(world::day_label(r.day_offset)),
            fmt::offset_difference(r.difference_min),
            t(if r.daytime { "azclock-day" } else { "azclock-night" })
        );
        list.add_child(
            Dom::create_div()
                .with_class(ids::CITY_ROW)
                .with_css(ROW)
                .with_child(
                    Dom::create_div()
                        .with_css(format!("{COLUMN} flex-grow: 1; cursor: pointer;"))
                        .with_child(line(&r.name, "font-size: 15px;"))
                        .with_child(line(&when, SECONDARY))
                        .clickable(&r.name, act(app, Action::FaceCity(Some(i))), actions::on_act),
                )
                .with_child(line(
                    &fmt::clock(r.hour, r.minute, twelve),
                    &format!("font-size: 22px; font-family: monospace; {FIGURES_FLORA}"),
                ))
                .with_child(button("azclock-up", app, Action::CityUp(i)))
                .with_child(button("azclock-remove", app, Action::CityRemove(i)))
                .with_context_menu(city_menu(app, i, s.cities.len())),
        );
    }
    let css = if wide {
        "display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;"
    } else {
        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;"
    };
    Dom::create_div()
        .with_id(ids::WORLD_VIEW)
        .with_css(css)
        .with_child(face_panel)
        .with_child(list)
}

/// A city row's context menu: Move up (not the first), Move down (not the
/// last), Remove.
fn city_menu(app: &RefAny, i: usize, count: usize) -> Menu {
    let item = |name: &str, action: Action| {
        MenuItem::string(StringMenuItem::create(label(name)).with_callback(act(app, action), actions::on_act))
    };
    let mut items = Vec::new();
    if i > 0 {
        items.push(item("azclock-move-up", Action::CityUp(i)));
    }
    if i + 1 < count {
        items.push(item("azclock-move-down", Action::CityDown(i)));
    }
    items.push(item("azclock-remove", Action::CityRemove(i)));
    Menu::create(items)
}

// ==== Alarms ====

fn alarms_view(s: &ClockApp, app: &RefAny, now: DateTime<Utc>) -> Dom {
    let twelve = s.twelve_hour();
    let mut list = Dom::create_div().with_id(ids::ALARMS).with_css(LIST);
    if s.alarms.is_empty() {
        list.add_child(line("azclock-no-alarms", EMPTY));
    }
    for a in &s.alarms {
        let mut detail = a.repeat_label();
        if let Some(at) = a.next_ring(now, &Local) {
            detail.push_str(" - ");
            detail.push_str(&t_args("azclock-in-span", &[("span", Arg::from(fmt::until((at - now).num_milliseconds())))]));
        }
        if a.snoozed_until.is_some_and(|ms| ms > now.timestamp_millis()) {
            detail.push_str(" - ");
            detail.push_str(&t("azclock-snoozed"));
        }
        let dim = if a.enabled { "" } else { " opacity: 0.55;" };
        list.add_child(
            Dom::create_div()
                .with_class(ids::ALARM_ROW)
                .with_css(ROW)
                .with_child(
                    Dom::create_div()
                        .with_css(format!("{} flex-grow: 1; cursor: pointer;{dim}", row_css(16)))
                        .with_child(line(
                            &fmt::clock(a.hour, a.minute, twelve),
                            &format!(
                                "font-size: 30px; font-family: monospace; min-width: 110px; \
                                 {FIGURES_FLORA}"
                            ),
                        ))
                        .with_child(
                            Dom::create_div()
                                .with_css(COLUMN)
                                .with_child(line(
                                    if a.label.is_empty() { "azclock-alarm" } else { a.label.as_str() },
                                    "font-size: 15px;",
                                ))
                                .with_child(line(&detail, SECONDARY)),
                        )
                        .clickable(
                            if a.label.is_empty() { "azclock-alarm" } else { a.label.as_str() },
                            act(app, Action::EditAlarm(a.id.clone())),
                            actions::on_act,
                        ),
                )
                .with_child(
                    Switch::create(a.enabled)
                        .with_accessibility_name(t_args(
                            "azclock-alarm-at",
                            &[("time", Arg::from(fmt::clock(a.hour, a.minute, twelve)))],
                        ))
                        .with_on_toggle(
                            act(app, Action::EditAlarm(a.id.clone())),
                            actions::on_alarm_switch as SwitchOnToggleCallbackType,
                        )
                        .dom()
                        .with_class(ids::ALARM_SWITCH),
                ),
        );
    }
    let mut column = Dom::create_div()
        .with_id(ids::ALARMS_VIEW)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(list);
    if !s.os_rings.0 {
        column.add_child(line(
            &t_args("azclock-ring-while-running", &[("why", Arg::from(&s.os_rings.1))]),
            "padding: 4px 12px; font-size: 12px; opacity: 0.7; @theme(flora) { opacity: 1; \
             color: system:secondary-text; }",
        ));
    }
    column
}

// ==== Timer ====

fn timer_view(s: &ClockApp, app: &RefAny) -> Dom {
    let now = now_ms();
    let mut view = Dom::create_div()
        .with_id(ids::TIMER_VIEW)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;");
    match s.shown_timer() {
        Some(i) => {
            let t = &s.timers[i];
            let percent = t.fraction_left(now) * 100.0;
            let running = t.is_running();
            let mut controls = Dom::create_div().with_css(format!("{} justify-content: center;", row_css(8)));
            controls.add_child(primary(
                if running { "azclock-pause" } else { "azclock-start" },
                app,
                Action::TimerToggle(t.id.clone()),
            ));
            controls.add_child(button("azclock-reset", app, Action::TimerReset(t.id.clone())));
            controls.add_child(button("azclock-plus-minute", app, Action::TimerPlusMinute(t.id.clone())));
            view.add_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: column; align-items: center; row-gap: 8px; padding: 20px 16px;")
                    // The ring drains with the time left; the countdown is in its centre,
                    // "of 5:00 - Tea" under it.
                    .with_child(
                        Gauge::create(f64::from(percent), 0.0, 100.0)
                            .with_kind(GaugeKind::Ring)
                            .with_size(240.0)
                            .with_thickness(14.0)
                            .with_value_text(fmt::countdown(t.remaining_ms(now)))
                            .with_label(format!(
                                "{}{}",
                                t_args("azclock-of-length", &[("length", Arg::from(fmt::countdown(t.duration_ms)))]),
                                if t.label.is_empty() {
                                    String::new()
                                } else {
                                    format!(" - {}", t.label)
                                }
                            ))
                            .with_accessibility_name(label("azclock-time-left"))
                            .dom()
                            .with_id(ids::TIMER_TIME)
                            .with_id(ids::TIMER_RING),
                    )
                    .with_child(controls),
            );
        }
        None => view.add_child(line("azclock-no-timer", EMPTY)),
    }
    // The presets.
    let mut presets = Dom::create_div()
        .with_id(ids::PRESETS)
        .with_css(format!("{} padding: 8px 12px; flex-wrap: wrap;", row_css(6)))
        .with_child(line("azclock-presets", SECONDARY));
    for minutes in crate::timer::PRESETS_MIN {
        presets.add_child(button(&fmt::minutes(minutes), app, Action::TimerPreset(minutes)));
    }
    view.add_child(presets);
    // Every timer, the shown one marked.
    let shown = s.shown_timer();
    let mut list = Dom::create_div().with_id(ids::TIMERS).with_css(LIST);
    for (i, t) in s.timers.iter().enumerate() {
        let left = || Arg::from(fmt::countdown(t.remaining_ms(now)));
        let state = if t.is_finished() {
            l10n::t("azclock-timer-done")
        } else if t.is_running() {
            t_args("azclock-timer-left", &[("left", left())])
        } else {
            t_args("azclock-timer-left-paused", &[("left", left())])
        };
        let name = if t.label.is_empty() {
            t_args("azclock-length-timer", &[("length", Arg::from(fmt::countdown(t.duration_ms)))])
        } else {
            t.label.clone()
        };
        let mark = if shown == Some(i) { " font-weight: 600;" } else { "" };
        list.add_child(
            Dom::create_div()
                .with_class(ids::TIMER_ROW)
                .with_css(ROW)
                .with_child(
                    Dom::create_div()
                        .with_css(format!("{} flex-grow: 1; cursor: pointer;{mark}", row_css(12)))
                        .with_child(line(&name, "font-size: 14px;"))
                        .with_child(line(&state, SECONDARY))
                        .clickable(&name, act(app, Action::TimerSelect(t.id.clone())), actions::on_act),
                )
                .with_child(button(
                    if t.is_running() { "azclock-pause" } else { "azclock-start" },
                    app,
                    Action::TimerToggle(t.id.clone()),
                ))
                .with_child(button("azclock-remove", app, Action::TimerRemove(t.id.clone()))),
        );
    }
    view.with_child(list)
}

// ==== Stopwatch ====

fn stopwatch_view(s: &ClockApp, app: &RefAny) -> Dom {
    let now = now_ms();
    let w = &s.stopwatch;
    let running = w.is_running();
    let mut controls = Dom::create_div().with_css(format!("{} justify-content: center;", row_css(8)));
    controls.add_child(button("azclock-lap", app, Action::StopwatchLap));
    controls.add_child(primary(if running { "azclock-stop" } else { "azclock-start" }, app, Action::StopwatchToggle));
    controls.add_child(button("azclock-reset", app, Action::StopwatchReset));
    if !w.laps.is_empty() {
        controls.add_child(button("azclock-copy-laps", app, Action::CopyLaps));
    }
    let mut list = Dom::create_div().with_id(ids::LAPS).with_css(LIST);
    if !w.laps.is_empty() {
        list.add_child(
            Dom::create_div()
                .with_css(format!("{} padding: 4px; {SECONDARY}", row_css(12)))
                .with_child(line("azclock-lap", "width: 48px;"))
                .with_child(line("azclock-lap-time", "width: 120px;"))
                .with_child(line("azclock-total", "width: 120px;")),
        );
    }
    for r in w.rows() {
        let badge = r.mark.label();
        // Under flora the fastest lap is the leaf stone, the slowest the clay (their glows at
        // night) - flora's alternates, not flat's green and red.
        let color = match r.mark {
            LapMark::Fastest => {
                " color: #2e8b57; @theme(flora) { color: #44684F; @media (prefers-color-scheme: \
                 dark) { color: #7FA98C; } }"
            }
            LapMark::Slowest => {
                " color: #d0453c; @theme(flora) { color: #7E4A42; @media (prefers-color-scheme: \
                 dark) { color: #B3837A; } }"
            }
            LapMark::None => "",
        };
        list.add_child(
            Dom::create_div()
                .with_class(ids::LAP_ROW)
                .with_css(format!(
                    "{} padding: 4px; font-family: monospace; {FIGURES_FLORA}{color}",
                    row_css(12)
                ))
                .with_child(line(&r.number.to_string(), "width: 48px;"))
                .with_child(line(&fmt::lap(r.lap_ms), "width: 120px;"))
                .with_child(line(&fmt::lap(r.total_ms), "width: 120px;"))
                .with_child(line(badge, "font-size: 12px;")),
        );
    }
    Dom::create_div()
        .with_id(ids::STOPWATCH_VIEW)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; align-items: center; row-gap: 12px; padding: 24px 16px;")
                .with_child(
                    // The fast tick retexts this text node in place (its marker).
                    Dom::create_div()
                        .with_id(ids::STOPWATCH_BOX)
                        .with_css(format!(
                            "font-size: 48px; font-weight: 600; font-family: monospace; {FIGURES_FLORA}"
                        ))
                        .with_child(
                            Dom::create_text_do_not_use_without_block_level_wrapper(fmt::stopwatch(w.elapsed(now)))
                                .with_marker(OptionString::Some(ids::STOPWATCH_TIME)),
                        ),
                )
                .with_child(controls),
        )
        .with_child(list)
}

// ==== Dialogs and the ringing overlay ====

/// The dialogs that are open, the ringing overlay, the toast.
pub fn overlays(s: &ClockApp, app: &RefAny, now: DateTime<Utc>) -> Vec<Dom> {
    let mut out = Vec::new();
    if let Some(d) = s.editor.as_ref() {
        out.push(editor(s, d, app));
    }
    if let Some(query) = s.city_search.as_deref() {
        out.push(city_search(query, app));
    }
    if let Some(r) = s.ringing.first() {
        out.push(ringing(s, r, app, now));
    }
    if let Some((message, _)) = s.toast.as_ref() {
        out.push(
            Dom::create_div()
                .with_id(ids::TOAST)
                .with_css(
                    "position: absolute; left: 50%; bottom: 40px; transform: translateX(-50%); \
                     padding: 8px 16px; border-radius: 6px; background: rgba(40,40,40,0.92); \
                     color: #ffffff; font-size: 13px;",
                )
                .with_child(text(message))
                .clickable(message, act(app, Action::DismissNotice), actions::on_act),
        );
    }
    out
}

/// A labelled row of the editor.
fn field(label: &str, control: Dom) -> Dom {
    Dom::create_div()
        .with_css(format!("{} padding: 6px 0px;", row_css(12)))
        .with_child(line(label, "width: 80px; flex-shrink: 0; font-size: 13px;"))
        .with_child(control)
}

/// The alarm editor: time, repeat, label, sound, snooze; Delete, Cancel, Save.
fn editor(s: &ClockApp, d: &super::AlarmDraft, app: &RefAny) -> Dom {
    let twelve = s.twelve_hour();
    let picker = if twelve {
        let (h12, pm) = match d.hour {
            0 => (12, false),
            h if h < 12 => (h, false),
            12 => (12, true),
            h => (h - 12, true),
        };
        TimePicker::create(h12, d.minute).with_24h(false).with_pm(pm)
    } else {
        TimePicker::create(d.hour, d.minute).with_24h(true)
    };
    let sounds: Vec<&str> = Sound::ALL.iter().map(|s| s.label()).collect();
    let presets = Dom::create_div()
        .with_css(row_css(6))
        .with_child(button("azclock-repeat-once", app, Action::RepeatPreset(0)))
        .with_child(button("azclock-repeat-every-day", app, Action::RepeatPreset(1)))
        .with_child(button("azclock-repeat-weekdays", app, Action::RepeatPreset(2)))
        .with_child(button("azclock-repeat-weekends", app, Action::RepeatPreset(3)));
    let mut footer = Dom::create_div().with_css(format!("{} padding-top: 12px;", row_css(8)));
    if d.id.is_some() {
        footer.add_child(
            Button::with_type(label("azclock-delete"), ButtonType::Danger)
                .with_on_click(act(app, Action::EditorDelete), actions::on_act as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::EDITOR_DELETE),
        );
    }
    footer.add_child(Dom::create_div().with_css(SPACER));
    footer.add_child(button("kit-button-cancel", app, Action::EditorCancel).with_id(ids::EDITOR_CANCEL));
    footer.add_child(primary("azclock-save", app, Action::EditorSave).with_id(ids::EDITOR_SAVE));
    let content = Dom::create_div()
        .with_id(ids::EDITOR)
        .with_css("display: flex; flex-direction: column; padding: 8px 4px; min-width: 420px;")
        .with_child(field(
            "azclock-time",
            picker
                .with_accessibility_name(label("azclock-alarm-time"))
                .with_on_change(app.clone(), actions::on_editor_time as TimePickerOnChangeCallbackType)
                .dom()
                .with_id(ids::EDITOR_TIME),
        ))
        .with_child(field("azclock-repeat", presets))
        .with_child(field(
            "",
            DateRepeatPicker::create(d.rule)
                .with_week_start(DatePickerWeekStart::Monday)
                .with_end_option(true)
                .with_accessibility_name(label("azclock-repeat"))
                .with_on_change(app.clone(), actions::on_editor_repeat as DateRepeatPickerOnChangeCallbackType)
                .dom()
                .with_id(ids::EDITOR_REPEAT),
        ))
        .with_child(field(
            "azclock-label",
            TextInput::create()
                .with_text(d.label.as_str())
                .with_placeholder(label("azclock-alarm"))
                .with_accessibility_name(label("azclock-label"))
                .with_on_text_input(app.clone(), actions::on_editor_label as TextInputOnTextInputCallbackType)
                .dom()
                .with_id(ids::EDITOR_LABEL),
        ))
        .with_child(field(
            "azclock-sound",
            DropDown::create(strs(&sounds))
                .with_selected(d.sound.index())
                .with_accessibility_name(label("azclock-sound"))
                .with_on_choice_change(app.clone(), actions::on_editor_sound as DropDownOnChoiceChangeCallbackType)
                .dom()
                .with_id(ids::EDITOR_SOUND),
        ))
        .with_child(field(
            "azclock-snooze",
            Dom::create_div()
                .with_css(row_css(6))
                .with_child(
                    NumberInput::create(d.snooze_minutes as f32)
                        .with_accessibility_name(label("azclock-snooze-length"))
                        .with_on_value_change(app.clone(), actions::on_editor_snooze as NumberInputOnValueChangeCallbackType)
                        .dom()
                        .with_id(ids::EDITOR_SNOOZE),
                )
                .with_child(text("azclock-min")),
        ))
        .with_child(footer);
    Modal::create(content)
        .with_title(label(if d.id.is_some() { "azclock-edit-alarm" } else { "azclock-new-alarm" }))
        .with_open(true)
        .with_on_close(app.clone(), actions::on_modal_close as ModalOnCloseCallbackType)
        .dom()
        .with_id(ids::EDITOR_MODAL)
}

/// The add-city dialog: a search over every zone, a button per result.
fn city_search(query: &str, app: &RefAny) -> Dom {
    let mut results = Dom::create_div()
        .with_id(ids::CITY_RESULTS)
        .with_css("display: flex; flex-direction: column; max-height: 280px; overflow-y: auto; row-gap: 2px; padding-top: 8px;");
    let found = world::search(query, 30);
    if query.trim().is_empty() {
        results.add_child(line("azclock-city-search-hint", SECONDARY));
    } else if found.is_empty() {
        results.add_child(line("azclock-no-city", SECONDARY));
    }
    for city in found {
        let label = format!("{} - {}", city.name, city.zone);
        results.add_child(
            Button::create(label.as_str())
                .with_on_click(act(app, Action::CityAdd(city.zone.clone())), actions::on_act as ButtonOnClickCallbackType)
                .dom()
                .with_class(ids::CITY_RESULT),
        );
    }
    let content = Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-width: 380px; padding: 8px 4px;")
        .with_child(
            TextInput::create_search()
                .with_text(query)
                .with_placeholder(label("azclock-search-cities"))
                .with_accessibility_name(label("azclock-search-cities"))
                .with_on_text_input(app.clone(), actions::on_city_query as TextInputOnTextInputCallbackType)
                .dom()
                .with_id(ids::CITY_QUERY),
        )
        .with_child(results)
        .with_child(
            Dom::create_div()
                .with_css(format!("{} justify-content: flex-end; padding-top: 8px;", row_css(8)))
                .with_child(button("azclock-close", app, Action::CloseCitySearch)),
        );
    Modal::create(content)
        .with_title(label("azclock-add-a-city"))
        .with_open(true)
        .with_on_close(app.clone(), actions::on_modal_close as ModalOnCloseCallbackType)
        .dom()
        .with_id(ids::CITY_MODAL)
}

/// The ringing overlay: the time and the label in large, Snooze and Dismiss.
fn ringing(s: &ClockApp, r: &Ringing, app: &RefAny, now: DateTime<Utc>) -> Dom {
    let twelve = s.twelve_hour();
    let (big, label, snooze) = match r {
        Ringing::Alarm { id, .. } => match s.alarm_index(id) {
            Some(i) => {
                let a = &s.alarms[i];
                (
                    fmt::clock(a.hour, a.minute, twelve),
                    if a.label.is_empty() { t("azclock-alarm") } else { a.label.clone() },
                    t_args("azclock-snooze-span", &[("span", Arg::from(fmt::minutes(a.snooze_minutes)))]),
                )
            }
            None => (String::new(), t("azclock-alarm"), t("azclock-snooze")),
        },
        Ringing::Timer { id } => {
            let label = s
                .timer_index(id)
                .map(|i| &s.timers[i])
                .map_or_else(|| l10n::t("azclock-timer"), |timer| {
                    if timer.label.is_empty() {
                        t_args("azclock-length-timer", &[("length", Arg::from(fmt::countdown(timer.duration_ms)))])
                    } else {
                        timer.label.clone()
                    }
                });
            (l10n::t("azclock-time-is-up"), label, l10n::t("azclock-plus-minute"))
        }
    };
    let local = now.with_timezone(&Local);
    let content = Dom::create_div()
        .with_id(ids::RING)
        .with_css("display: flex; flex-direction: column; align-items: center; row-gap: 12px; padding: 24px 32px; min-width: 320px;")
        .with_child(line(
            &big,
            &format!("font-size: 52px; font-weight: 600; font-family: monospace; {FIGURES_FLORA}"),
        ))
        .with_child(line(&label, "font-size: 20px;"))
        .with_child(line(
            &format!(
                "{} {}",
                t(azul_pim::dates::weekday_message_id(chrono::Datelike::weekday(&local.date_naive()))),
                fmt::clock(local.hour(), local.minute(), twelve)
            ),
            SECONDARY,
        ))
        .with_child(
            Dom::create_div()
                .with_css(format!("{} padding-top: 12px;", row_css(12)))
                .with_child(button(&snooze, app, Action::Snooze).with_id(ids::RING_SNOOZE))
                .with_child(primary("azclock-dismiss", app, Action::Dismiss).with_id(ids::RING_DISMISS)),
        );
    Modal::create(content)
        .with_title(l10n::label(match r {
            Ringing::Alarm { .. } => "azclock-alarm",
            Ringing::Timer { .. } => "azclock-timer",
        }))
        .with_open(true)
        .with_on_close(app.clone(), actions::on_ring_close as ModalOnCloseCallbackType)
        .dom()
        .with_id(ids::RING_MODAL)
}

// ==== Settings ====

/// A settings switch bound to a key of `settings.json`.
fn setting_switch(app: &RefAny, key: &str, on: bool, name: &str) -> Dom {
    Switch::create(on)
        .with_accessibility_name(label(name))
        .with_on_toggle(
            act(app, Action::Setting(key.to_string())),
            actions::on_setting_switch as SwitchOnToggleCallbackType,
        )
        .dom()
        .with_id(ids::named(&format!("set-{key}")))
}

/// The clock's own settings sections (before the kit's Appearance, Data,
/// Shortcuts and About).
pub fn settings_sections(s: &ClockApp, app: &RefAny) -> Vec<AppSection> {
    let minutes = s.ring_minutes();
    let ring_index = RING_MINUTES.iter().position(|m| *m == minutes).unwrap_or(2);
    let ring_labels: Vec<String> = RING_MINUTES.iter().map(|m| fmt::minutes(*m)).collect();
    let ring_refs: Vec<&str> = ring_labels.iter().map(String::as_str).collect();
    let os_line = if s.os_rings.0 {
        t_args("azclock-os-rings", &[("how", Arg::from(&s.os_rings.1))])
    } else {
        t_args("azclock-only-while-running", &[("why", Arg::from(&s.os_rings.1))])
    };
    vec![
        AppSection {
            category: 0,
            title: "azclock-category-clock".to_string(),
            content: Dom::create_div()
                .with_css(COLUMN)
                .with_child(kit::row(
                    "azclock-twelve-hour",
                    setting_switch(app, "twelve-hour", s.twelve_hour(), "azclock-twelve-hour"),
                )),
        },
        AppSection {
            category: 0,
            title: "azclock-screen-alarms".to_string(),
            content: Dom::create_div()
                .with_css(COLUMN)
                .with_child(kit::row(
                    "azclock-ring-while-closed",
                    setting_switch(app, "os-alarms", s.schedule_with_os(), "azclock-ring-while-closed-long"),
                ))
                .with_child(kit::row(
                    "azclock-keep-running",
                    setting_switch(app, "keep-running", s.keep_running(), "azclock-keep-running-long"),
                ))
                .with_child(kit::row(
                    "azclock-ring-for",
                    DropDown::create(strs(&ring_refs))
                        .with_selected(ring_index)
                        .with_accessibility_name(label("azclock-ring-for"))
                        .with_on_choice_change(app.clone(), actions::on_ring_minutes as DropDownOnChoiceChangeCallbackType)
                        .dom()
                        .with_id(ids::SET_RING_MINUTES),
                ))
                .with_child(kit::note(&os_line))
                .with_child(kit::note("azclock-linux-note")),
        },
    ]
}
