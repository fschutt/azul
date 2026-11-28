//! The DOM of AzClock: the narrow column every screen is (the big time over
//! the page's list, the four places in the bottom bar, "+" floating over
//! the list), the dialogs, the ringing overlay and the clock's own settings
//! sections. Every id and class is a `__azclock_` name from `ids.rs`; every
//! button carries an [`Act`](super::actions::Act).
//!
//! The window is azul's S11 `MobileShell` (the app bar holds only Settings,
//! the page host the list and the floating button, the bottom tabs the four
//! places) in a column that never grows past [`COLUMN_MAX`] px: a desktop
//! shows the same column a phone does, centred. The alarm editor and the
//! add-city search are pages pushed over the list (a Back button in the app
//! bar), a ring is a page of its own: nothing opens a window of its own, which
//! a phone has none of.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DateRepeatPickerOnChangeCallbackType,
        MobileShellOnTabCallbackType, NumberInputOnValueChangeCallbackType,
        SwitchOnToggleCallbackType, TextInputOnTextInputCallbackType,
        TimePickerOnChangeCallbackType,
    },
    dom::{AccessibilityInfo, AccessibilityRole},
    menu::{Menu, MenuItem, StringMenuItem},
    option::OptionString,
    prelude::*,
    shells::{MobileShell, ShellBottomTab},
    str::String as AzString,
    widgets::{
        ButtonType, DatePickerWeekStart, DateRepeatPicker, Gauge, GaugeKind, NumberInput,
        Switch, TextInput, TimePicker,
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

/// The widest the column grows, in px: on a desktop it is centred in the window.
pub const COLUMN_MAX: f32 = 520.0;
/// "Ring for" (minutes).
pub const RING_MINUTES: [u32; 5] = [1, 5, 10, 15, 30];

// ==== Small parts ====

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

/// A button that shows only `icon`, named `name` (a key, or plain words) for a screen reader and
/// as its tooltip.
fn icon_button(icon: &str, name: &str, app: &RefAny, action: Action) -> Dom {
    Button::create("")
        .with_icon(icon)
        .with_on_click(act(app, action), actions::on_act as ButtonOnClickCallbackType)
        .dom()
        .with_accessibility_assign(AccessibilityInfo::named(label(name), AccessibilityRole::PushButton))
}

fn row_css(gap: u32) -> String {
    format!("display: flex; flex-direction: row; align-items: center; column-gap: {gap}px;")
}

// Under flora (`@theme(flora)` after the flat values; flat is unchanged): the figures are
// flora's Garamond (its figures are tabular and lining - they keep their width as they tick),
// secondary text is the label ink rather than a faded ink, the rules are the grid's.
const COLUMN: &str = "display: flex; flex-direction: column;";
const SPACER: &str = "flex-grow: 1;";
/// A column that takes what its parent leaves.
const FILL: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px;";
const SECONDARY: &str = "font-size: 13px; opacity: 0.7; @theme(flora) { opacity: 1; color: \
                         system:secondary-text; }";
const EMPTY: &str = "padding: 32px 24px; opacity: 0.7; font-size: 15px; text-align: center; \
                     @theme(flora) { opacity: 1; color: system:secondary-text; }";
/// A scrolling list that fills the rest of the page; the bottom padding keeps the last row clear
/// of the floating button.
const LIST: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; padding: 0px 8px 88px 8px;";
const ROW: &str = "display: flex; flex-direction: row; align-items: center; column-gap: 12px; min-height: 64px; padding: 8px 8px; border-bottom: 1px solid rgba(128,128,128,0.25); @theme(flora) { border-bottom: 1px solid system:grid; }";
/// A time, a lap, a countdown: flat's monospace, flora's Garamond (appended after the
/// part's own `font-family: monospace`).
const FIGURES_FLORA: &str = "@theme(flora) { font-family: EB Garamond, Georgia, serif; \
                             font-variant-numeric: tabular-nums lining-nums; }";
/// The time of day at the top of the Alarm and Clock pages: big, light, and smaller in a short
/// window (a phone on its side).
const BIG_TIME: &str = "font-size: 72px; font-weight: 300; line-height: 1.05; \
                        @media (max-height: 520px) { font-size: 48px; } \
                        @media (max-width: 340px) { font-size: 60px; } \
                        @theme(flora) { font-family: EB Garamond, Georgia, serif; \
                        font-variant-numeric: tabular-nums lining-nums; }";

// ==== The window ====

/// What the window knows about the screen it is on, from the layout pass.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Env {
    /// The room the system keeps at the window's four edges (a cutout, the home indicator), px.
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
    /// A phone: no window controls to leave room for, no window to drag.
    pub phone: bool,
}

/// The four places of the bottom bar, in their order (the Screen's order).
fn tabs() -> Vec<ShellBottomTab> {
    Screen::ALL
        .iter()
        .map(|screen| ShellBottomTab::create(label(screen.label()), AzString::from("")))
        .collect()
}

/// The settings button of the app bar: a gear, named for a screen reader.
fn settings_button(app: &RefAny) -> Dom {
    icon_button("settings", "azclock-settings", app, Action::OpenSettings).with_id(ids::SETTINGS)
}

/// The floating "+" of the page that has one: a new alarm, a city.
fn fab(s: &ClockApp, app: &RefAny) -> Option<Dom> {
    let name = match s.screen {
        Screen::Alarms => "azclock-new-alarm",
        Screen::World => "azclock-add-city",
        Screen::Timer | Screen::Stopwatch => return None,
    };
    Some(
        Button::with_type(AzString::from(""), ButtonType::Primary)
            .with_icon("add")
            .with_on_click(act(app, Action::New), actions::on_act as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::NEW)
            .with_accessibility_assign(AccessibilityInfo::named(label(name), AccessibilityRole::PushButton)),
    )
}

/// The whole window below the settings page: the column and its shell; a ring instead of both
/// while something rings.
pub fn window(s: &ClockApp, app: &RefAny, now: DateTime<Utc>, env: &Env) -> Dom {
    let body = match s.ringing.first() {
        Some(r) => ring_page(s, r, app, now),
        None => shell(s, app, now, env),
    };
    let mut column = Dom::create_div()
        .with_css(format!(
            "{FILL} position: relative; width: 100%; max-width: {COLUMN_MAX}px;"
        ))
        .with_child(body);
    if !env.phone {
        // A window without a title bar: the empty part of the top bar moves it (the controls sit
        // over its left end, the gear is clear of it).
        column.add_child(
            Dom::create_div()
                .with_css(
                    "position: absolute; top: 0px; left: 0px; right: 64px; height: 48px; \
                     -azul-app-region: drag;",
                ),
        );
    }
    for overlay in overlays(s, app) {
        column.add_child(overlay);
    }
    // The column, centred; the system's room at the edges around it.
    Dom::create_div()
        .with_css(format!(
            "{FILL} flex-direction: row; justify-content: center; padding: 0px {}px {}px {}px;",
            env.right, env.bottom, env.left
        ))
        .with_child(column)
}

/// The shell of the places: the list page with the four places in the bottom bar and the
/// floating button, or - while an alarm is edited or a city searched - that page alone, with a
/// Back button, over where it came from.
fn shell(s: &ClockApp, app: &RefAny, now: DateTime<Utc>, env: &Env) -> Dom {
    let title = if let Some(d) = s.editor.as_ref() {
        label(if d.id.is_some() { "azclock-edit-alarm" } else { "azclock-new-alarm" })
    } else if s.city_search.is_some() {
        label("azclock-add-a-city")
    } else {
        AzString::from("")
    };
    let shell = MobileShell::create(title)
        .with_top_inset(env.top)
        .with_on_back(act(app, Action::Back), actions::on_act as ButtonOnClickCallbackType);
    if let Some(d) = s.editor.as_ref() {
        return shell
            .with_page(editor_page(s, d, app))
            .with_back(true)
            .with_actions(primary("azclock-save", app, Action::EditorSave).with_id(ids::EDITOR_SAVE))
            .dom();
    }
    if let Some(query) = s.city_search.as_deref() {
        return shell.with_page(city_page(query, app)).with_back(true).dom();
    }
    let mut shell = shell
        .with_page(page(s, app, now))
        .with_actions(settings_button(app))
        .with_tabs(azul::vec::ShellBottomTabVec::from_vec(tabs()))
        .with_active_tab(s.screen.index())
        .with_on_tab(app.clone(), actions::on_tab as MobileShellOnTabCallbackType);
    if let Some(fab) = fab(s, app) {
        shell = shell.with_fab(fab);
    }
    shell.dom()
}

// ==== The page ====

/// The page the bottom bar chose: the header and the list of the place, over the notice.
pub fn page(s: &ClockApp, app: &RefAny, now: DateTime<Utc>) -> Dom {
    let body = if !s.loaded {
        line("azclock-loading", EMPTY)
    } else {
        match s.screen {
            Screen::Alarms => alarms_page(s, app, now),
            Screen::World => world_page(s, app, now),
            Screen::Timer => timer_page(s, app),
            Screen::Stopwatch => stopwatch_page(s, app),
        }
    };
    let mut page = Dom::create_div().with_css(FILL).with_child(body);
    if !s.notice.is_empty() {
        page.add_child(
            Dom::create_div()
                .with_id(ids::NOTICE)
                .with_css(
                    "padding: 8px 16px; font-size: 13px; border-top: 1px solid rgba(128,128,128,0.25); \
                     @theme(flora) { border-top: 1px solid system:grid; }",
                )
                .with_child(Dom::create_span_with_text(s.notice.as_str())),
        );
    }
    page
}

/// "Friday 9 October": the kit's words for a day.
fn day_text(day: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    l10n::date_text(DateStyle::WeekdayDayMonth, day.year(), day.month(), day.day(), day.weekday().num_days_from_monday())
}

/// The time here, big, over the day; `extra` (when the next alarm rings, the place) under them.
fn header(s: &ClockApp, now: DateTime<Utc>, extra: Option<(String, AzString)>) -> Dom {
    let local = now.with_timezone(&Local);
    let (time, half) = fmt::clock_parts(local.hour(), local.minute(), s.twelve_hour());
    let mut big = Dom::create_div()
        .with_id(ids::BIG_TIME)
        .with_css(format!("{BIG_TIME} display: flex; flex-direction: row; align-items: baseline;"))
        .with_child(text(&time));
    if let Some(half) = half {
        big.add_child(line(half, "font-size: 24px; font-weight: 400; margin-left: 8px;"));
    }
    let mut header = Dom::create_div()
        .with_id(ids::HEADER)
        .with_css("display: flex; flex-direction: column; flex-shrink: 0; padding: 4px 20px 12px 20px;")
        .with_child(big)
        .with_child(line(&day_text(local.date_naive()), "font-size: 17px; opacity: 0.8;").with_id(ids::DATE));
    if let Some((extra, id)) = extra {
        header.add_child(line(&extra, SECONDARY).with_id(id));
    }
    header
}

/// When the next alarm rings (the header of the Alarm page).
fn next_alarm_line(s: &ClockApp, now: DateTime<Utc>) -> String {
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
}

// ==== Alarms ====

fn alarms_page(s: &ClockApp, app: &RefAny, now: DateTime<Utc>) -> Dom {
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
                        .with_css(format!("{} flex-grow: 1; min-width: 0px; cursor: pointer;{dim}", row_css(14)))
                        .with_child(line(
                            &fmt::clock(a.hour, a.minute, twelve),
                            &format!("font-size: 32px; font-weight: 300; {FIGURES_FLORA}"),
                        ))
                        .with_child(
                            Dom::create_div()
                                .with_css(format!("{COLUMN} min-width: 0px;"))
                                .with_child(line(
                                    if a.label.is_empty() { "azclock-alarm" } else { a.label.as_str() },
                                    "font-size: 16px;",
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
    if !s.os_rings.0 {
        list.add_child(line(
            &t_args("azclock-ring-while-running", &[("why", Arg::from(&s.os_rings.1))]),
            "padding: 12px 8px; font-size: 12px; opacity: 0.7; @theme(flora) { opacity: 1; \
             color: system:secondary-text; }",
        ));
    }
    Dom::create_div()
        .with_id(ids::ALARMS_VIEW)
        .with_css(FILL)
        .with_child(header(s, now, Some((next_alarm_line(s, now), ids::NEXT_ALARM))))
        .with_child(list)
}

// ==== World ====

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

fn world_page(s: &ClockApp, app: &RefAny, now: DateTime<Utc>) -> Dom {
    let twelve = s.twelve_hour();
    let here = world::local_zone().map_or_else(
        || l10n::t("azclock-here"),
        |z| t_args("azclock-city-here", &[("city", Arg::from(world::city_name(z.name())))]),
    );
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
                        .with_css(format!("{COLUMN} flex-grow: 1; min-width: 0px;"))
                        .with_child(line(&r.name, "font-size: 17px;"))
                        .with_child(line(&when, SECONDARY)),
                )
                .with_child(line(
                    &fmt::clock(r.hour, r.minute, twelve),
                    &format!("font-size: 26px; font-weight: 300; {FIGURES_FLORA}"),
                ))
                .with_child(icon_button(
                    "close",
                    &t_args("azclock-remove-named", &[("name", Arg::from(&r.name))]),
                    app,
                    Action::CityRemove(i),
                ))
                .with_context_menu(city_menu(app, i, s.cities.len())),
        );
    }
    Dom::create_div()
        .with_id(ids::WORLD_VIEW)
        .with_css(FILL)
        .with_child(header(s, now, Some((here, ids::PLACE))))
        .with_child(list)
}

// ==== Timer ====

fn timer_page(s: &ClockApp, app: &RefAny) -> Dom {
    let now = now_ms();
    let mut view = Dom::create_div()
        .with_id(ids::TIMER_VIEW)
        .with_css(FILL)
        .with_css("overflow-y: auto;");
    match s.shown_timer() {
        Some(i) => {
            let t = &s.timers[i];
            let percent = t.fraction_left(now) * 100.0;
            let running = t.is_running();
            let mut controls = Dom::create_div().with_css(format!("{} justify-content: center; flex-wrap: wrap;", row_css(8)));
            controls.add_child(primary(
                if running { "azclock-pause" } else { "azclock-start" },
                app,
                Action::TimerToggle(t.id.clone()),
            ));
            controls.add_child(button("azclock-reset", app, Action::TimerReset(t.id.clone())));
            controls.add_child(button("azclock-plus-minute", app, Action::TimerPlusMinute(t.id.clone())));
            view.add_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: column; align-items: center; row-gap: 8px; padding: 20px 16px; flex-shrink: 0;")
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
    // The presets: a tap starts a timer of that length.
    let mut presets = Dom::create_div()
        .with_id(ids::PRESETS)
        .with_css(format!("{} padding: 8px 16px; flex-wrap: wrap; row-gap: 8px; flex-shrink: 0;", row_css(8)))
        .with_child(line("azclock-presets", SECONDARY));
    for minutes in crate::timer::PRESETS_MIN {
        presets.add_child(button(&fmt::minutes(minutes), app, Action::TimerPreset(minutes)));
    }
    view.add_child(presets);
    // Every timer, the shown one marked.
    let shown = s.shown_timer();
    let mut list = Dom::create_div().with_id(ids::TIMERS).with_css("display: flex; flex-direction: column; padding: 0px 8px 16px 8px; flex-shrink: 0;");
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
                        .with_css(format!("{COLUMN} flex-grow: 1; min-width: 0px; cursor: pointer;{mark}"))
                        .with_child(line(&name, "font-size: 16px;"))
                        .with_child(line(&state, SECONDARY))
                        .clickable(&name, act(app, Action::TimerSelect(t.id.clone())), actions::on_act),
                )
                .with_child(button(
                    if t.is_running() { "azclock-pause" } else { "azclock-start" },
                    app,
                    Action::TimerToggle(t.id.clone()),
                ))
                .with_child(icon_button(
                    "close",
                    &t_args("azclock-remove-named", &[("name", Arg::from(&name))]),
                    app,
                    Action::TimerRemove(t.id.clone()),
                )),
        );
    }
    view.with_child(list)
}

// ==== Stopwatch ====

fn stopwatch_page(s: &ClockApp, app: &RefAny) -> Dom {
    let now = now_ms();
    let w = &s.stopwatch;
    let running = w.is_running();
    let mut controls = Dom::create_div().with_css(format!("{} justify-content: center; flex-wrap: wrap; row-gap: 8px;", row_css(8)));
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
            LapMark::Fastest => " color: system:success;",
            LapMark::Slowest => " color: system:danger;",
            LapMark::None => "",
        };
        list.add_child(
            Dom::create_div()
                .with_class(ids::LAP_ROW)
                .with_css(format!(
                    "{} padding: 6px 4px; font-family: monospace; {FIGURES_FLORA}{color}",
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
        .with_css(FILL)
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; align-items: center; row-gap: 12px; padding: 24px 16px; flex-shrink: 0;")
                .with_child(
                    // The fast tick retexts this text node in place (its marker).
                    Dom::create_div()
                        .with_id(ids::STOPWATCH_BOX)
                        .with_css(format!(
                            "font-size: 48px; font-weight: 300; font-family: monospace; \
                             @media (max-width: 400px) {{ font-size: 40px; }} {FIGURES_FLORA}"
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

/// What floats over the column: the toast.
pub fn overlays(s: &ClockApp, app: &RefAny) -> Vec<Dom> {
    let mut out = Vec::new();
    if let Some((message, _)) = s.toast.as_ref() {
        out.push(
            Dom::create_div()
                .with_id(ids::TOAST)
                .with_css(
                    "position: absolute; left: 50%; bottom: 80px; transform: translateX(-50%); \
                     max-width: 90%; padding: 8px 16px; border-radius: 6px; background: rgba(40,40,40,0.92); \
                     color: #ffffff; font-size: 13px;",
                )
                .with_child(text(message))
                .clickable(message, act(app, Action::DismissNotice), actions::on_act),
        );
    }
    out
}

/// A labelled row of the editor: the label over its control.
fn field(label: &str, control: Dom) -> Dom {
    Dom::create_div()
        .with_css(format!("{COLUMN} row-gap: 6px; padding: 10px 0px;"))
        .with_child(line(label, "font-size: 13px; opacity: 0.7;"))
        .with_child(control)
}

/// The page of the alarm editor: time, repeat, label, sound, snooze (and Delete, for an alarm
/// that exists); Back cancels, Save in the top bar saves.
fn editor_page(s: &ClockApp, d: &super::AlarmDraft, app: &RefAny) -> Dom {
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
    let presets = Dom::create_div()
        .with_css(format!("{} flex-wrap: wrap; row-gap: 8px;", row_css(8)))
        .with_child(button("azclock-repeat-once", app, Action::RepeatPreset(0)))
        .with_child(button("azclock-repeat-every-day", app, Action::RepeatPreset(1)))
        .with_child(button("azclock-repeat-weekdays", app, Action::RepeatPreset(2)))
        .with_child(button("azclock-repeat-weekends", app, Action::RepeatPreset(3)));
    // The sounds: a tap picks one, the picked one is the primary button.
    let mut sounds = Dom::create_div().with_css(format!("{} flex-wrap: wrap; row-gap: 8px;", row_css(8)));
    for (i, sound) in Sound::ALL.iter().enumerate() {
        let on = *sound == d.sound;
        sounds.add_child(
            Button::with_type(label(sound.label()), if on { ButtonType::Primary } else { ButtonType::Default })
                .with_on_click(act(app, Action::EditorSound(i)), actions::on_act as ButtonOnClickCallbackType)
                .dom(),
        );
    }
    let mut content = Dom::create_div()
        .with_id(ids::EDITOR)
        .with_css(format!("{COLUMN} padding: 8px 20px 32px 20px; min-width: 0px; box-sizing: border-box; overflow-y: auto; flex-grow: 1; min-height: 0px;"))
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
        .with_child(field("azclock-sound", sounds.with_id(ids::EDITOR_SOUND)))
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
        ));
    if d.id.is_some() {
        content.add_child(
            Dom::create_div().with_css("padding-top: 16px;").with_child(
                Button::with_type(label("azclock-delete"), ButtonType::Danger)
                    .with_on_click(act(app, Action::EditorDelete), actions::on_act as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::EDITOR_DELETE),
            ),
        );
    }
    content
}

/// The add-city page: a search over every zone, a button per result.
fn city_page(query: &str, app: &RefAny) -> Dom {
    let mut results = Dom::create_div()
        .with_id(ids::CITY_RESULTS)
        .with_css("display: flex; flex-direction: column; row-gap: 4px; padding-top: 8px;");
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
    Dom::create_div()
        .with_css(format!("{COLUMN} padding: 8px 20px 24px 20px; min-width: 0px; box-sizing: border-box; overflow-y: auto; flex-grow: 1; min-height: 0px;"))
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
}

/// The ring: the time and the label in large, Snooze and Dismiss, on a page of its own (the
/// column, over everything else, as an alarm clock rings).
fn ring_page(s: &ClockApp, r: &Ringing, app: &RefAny, now: DateTime<Utc>) -> Dom {
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
    Dom::create_div()
        .with_id(ids::RING)
        .with_css(format!(
            "{FILL} align-items: center; justify-content: center; row-gap: 14px; padding: 24px 16px;"
        ))
        .with_child(line(
            &big,
            &format!(
                "font-size: 64px; font-weight: 300; @media (max-height: 520px) {{ font-size: 44px; }} {FIGURES_FLORA}"
            ),
        ))
        .with_child(line(&label, "font-size: 22px;"))
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
                .with_css(format!("{} padding-top: 16px; flex-wrap: wrap; justify-content: center; row-gap: 12px;", row_css(12)))
                .with_child(button(&snooze, app, Action::Snooze).with_id(ids::RING_SNOOZE))
                .with_child(primary("azclock-dismiss", app, Action::Dismiss).with_id(ids::RING_DISMISS)),
        )
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

/// `- 10 min +`: how long a ring lasts by itself, a step of [`RING_MINUTES`] either way. A
/// drop-down would open a popup window, which a phone has none of.
fn ring_stepper(app: &RefAny, index: usize) -> Dom {
    // At either end of the list the button stays (it does nothing) but is dimmed.
    let step = |icon: &str, name: &str, delta: i32, enabled: bool| {
        let button = icon_button(icon, name, app, Action::RingFor(delta));
        if enabled {
            button
        } else {
            button.with_css("opacity: 0.4;")
        }
    };
    let minutes = RING_MINUTES.get(index).copied().unwrap_or(10);
    Dom::create_div()
        .with_id(ids::SET_RING_MINUTES)
        .with_css("display: flex; flex-direction: row; align-items: center; column-gap: 4px;")
        .with_child(step("remove", "azclock-ring-shorter", -1, index > 0))
        .with_child(
            Dom::create_div()
                .with_css("min-width: 72px; text-align: center; font-size: 13px;")
                .with_child(Dom::create_span_with_text(fmt::minutes(minutes))),
        )
        .with_child(step("add", "azclock-ring-longer", 1, index + 1 < RING_MINUTES.len()))
}

/// The clock's own settings sections (before the kit's Appearance, Data,
/// Shortcuts and About).
pub fn settings_sections(s: &ClockApp, app: &RefAny) -> Vec<AppSection> {
    let minutes = s.ring_minutes();
    let ring_index = RING_MINUTES.iter().position(|m| *m == minutes).unwrap_or(2);
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
                    ring_stepper(app, ring_index),
                ))
                .with_child(kit::note(&os_line))
                .with_child(kit::note("azclock-linux-note")),
        },
    ]
}
