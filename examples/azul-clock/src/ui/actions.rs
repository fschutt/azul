//! What the buttons, switches, pickers and keys of AzClock do. Every button
//! carries an [`Act`] (the app and an [`Action`]) and reaches [`on_act`]; the
//! controls with a value of their own (a switch, the time picker, the repeat
//! picker, a text field) have their own callbacks. Each change goes through
//! `with_app`, which writes the files and hands the OS the new schedule.

use azul::{
    dom::VirtualKeyCode,
    prelude::*,
    widgets::{
        DateRepeatRule, ModalState, NumberInputState, OnTextInputReturn, SegmentedState, SwitchState,
        TextInputState, TextInputValid, TimePickerState,
    },
};
use azul_appkit::ui as kit;
use chrono::{Local, Utc};

use super::{
    copy_laps, first_of, now_ms, rule_of, save_alarm, save_stopwatch, save_timer, save_world,
    with_app, AlarmDraft, ClockApp, Screen,
};
use crate::{
    alarm::{Alarm, DEFAULT_SNOOZE_MINUTES, MAX_SNOOZE_MINUTES},
    fmt, schedule, store,
    timer::{CountdownTimer, MINUTE_MS},
    tone::Sound,
    world::{self, City},
};

/// What a button does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// "+": a new alarm, timer or city, by the screen.
    New,
    OpenSettings,
    /// Open the editor on an alarm.
    EditAlarm(String),
    EditorSave,
    EditorCancel,
    EditorDelete,
    /// The weekday chips' shortcuts: no repeat, every day, weekdays, weekends.
    RepeatPreset(u8),
    /// Add the city of a zone (the search's result).
    CityAdd(String),
    CityRemove(usize),
    CityUp(usize),
    /// Show a city's time on the face (`None` = here).
    FaceCity(Option<usize>),
    CloseCitySearch,
    /// Start a new timer of this many minutes.
    TimerPreset(u32),
    TimerSelect(String),
    /// Start / pause.
    TimerToggle(String),
    TimerReset(String),
    TimerPlusMinute(String),
    TimerRemove(String),
    StopwatchToggle,
    StopwatchLap,
    StopwatchReset,
    CopyLaps,
    Snooze,
    Dismiss,
    DismissNotice,
    /// A switch of the clock's settings, by its key in `settings.json`.
    Setting(String),
}

/// A button's data: the app and what it does.
pub struct Act {
    pub app: RefAny,
    pub action: Action,
}

/// The data of a button that does `action`.
#[must_use]
pub fn act(app: &RefAny, action: Action) -> RefAny {
    RefAny::new(Act {
        app: app.clone(),
        action,
    })
}

/// Every button's callback.
pub extern "C" fn on_act(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<Act>()
        .map(|a| (a.app.clone(), a.action.clone()))
    else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, _| run(s, info, action))
}

/// The editor for a new alarm: 07:00, once, the default sound and snooze.
fn new_draft() -> AlarmDraft {
    let today = Local::now().date_naive();
    AlarmDraft {
        id: None,
        hour: 7,
        minute: 0,
        label: String::new(),
        rule: rule_of("", today),
        sound: Sound::default(),
        snooze_minutes: DEFAULT_SNOOZE_MINUTES,
    }
}

fn draft_of(a: &Alarm) -> AlarmDraft {
    AlarmDraft {
        id: Some(a.id.clone()),
        hour: a.hour,
        minute: a.minute,
        label: a.label.clone(),
        rule: rule_of(&a.repeat, a.first),
        sound: a.sound,
        snooze_minutes: a.snooze_minutes,
    }
}

/// The editor's alarm, saved: new or changed, switched on for its next time.
fn save_draft(s: &mut ClockApp) {
    let Some(d) = s.editor.take() else {
        return;
    };
    let now = Utc::now();
    let today = Local::now().date_naive();
    let first = first_of(&d.rule).unwrap_or(today);
    let rrule = d.rule.to_rrule().as_str().to_string();
    let i = match d.id.as_deref().and_then(|id| s.alarm_index(id)) {
        Some(i) => i,
        None => {
            s.alarms.push(Alarm::new(&store::new_id(), d.hour, d.minute, first));
            s.alarms.len() - 1
        }
    };
    let a = &mut s.alarms[i];
    a.hour = d.hour.min(23);
    a.minute = d.minute.min(59);
    a.label = d.label.trim().to_string();
    a.repeat = rrule;
    a.first = first;
    a.sound = d.sound;
    a.snooze_minutes = d.snooze_minutes.clamp(1, MAX_SNOOZE_MINUTES);
    a.arm(now, &Local);
    let id = a.id.clone();
    if let Some(at) = a.next_ring(now, &Local) {
        let text = format!(
            "Alarm set for {} from now",
            fmt::until((at - now).num_milliseconds())
        );
        s.toast = Some((text, now_ms() + 4000));
    }
    store::sort_alarms(&mut s.alarms);
    if let Some(i) = s.alarm_index(&id) {
        save_alarm(s, i);
    }
    println!("AZCLOCK_ALARM_SAVED {id}");
}

/// The weekday shortcuts of the editor: 0 = once, 1 = every day, 2 =
/// weekdays, 3 = weekends.
fn repeat_preset(d: &mut AlarmDraft, preset: u8) {
    let today = Local::now().date_naive();
    let first = first_of(&d.rule).unwrap_or(today);
    let rrule = match preset {
        1 => "FREQ=DAILY",
        2 => "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR",
        3 => "FREQ=WEEKLY;BYDAY=SA,SU",
        _ => "",
    };
    d.rule = rule_of(rrule, first);
}

/// What a button does to the state.
pub(crate) fn run(s: &mut ClockApp, info: &mut CallbackInfo, action: Action) {
    let now = now_ms();
    s.notice.clear();
    match action {
        Action::New => match s.screen {
            Screen::Alarms => s.editor = Some(new_draft()),
            Screen::World => s.city_search = Some(String::new()),
            Screen::Timer => run(s, info, Action::TimerPreset(10)),
            Screen::Stopwatch => {}
        },
        Action::OpenSettings => kit::open_settings(&s.kit, None),
        Action::EditAlarm(id) => {
            if let Some(i) = s.alarm_index(&id) {
                s.editor = Some(draft_of(&s.alarms[i]));
            }
        }
        Action::EditorSave => save_draft(s),
        Action::EditorCancel => s.editor = None,
        Action::EditorDelete => {
            if let Some(id) = s.editor.take().and_then(|d| d.id) {
                if let Some(i) = s.alarm_index(&id) {
                    s.alarms.remove(i);
                    s.queue.delete(store::alarm_key(&id));
                    s.ringing.retain(|r| r.id() != id);
                    for nid in schedule::ids_of_alarm(&id) {
                        info.withdraw_notification(nid.as_str());
                    }
                    println!("AZCLOCK_ALARM_DELETED {id}");
                }
            }
        }
        Action::RepeatPreset(p) => {
            if let Some(d) = s.editor.as_mut() {
                repeat_preset(d, p);
            }
        }
        Action::CityAdd(zone) => {
            if !s.cities.iter().any(|c| c.zone == zone) {
                s.cities.push(City::of_zone(&zone));
                save_world(s);
                println!("AZCLOCK_CITY_ADDED {zone}");
            }
            s.city_search = None;
        }
        Action::CityRemove(i) => {
            if i < s.cities.len() {
                s.cities.remove(i);
                s.face_city = None;
                save_world(s);
            }
        }
        Action::CityUp(i) => {
            if i > 0 && i < s.cities.len() {
                world::move_city(&mut s.cities, i, i - 1);
                save_world(s);
            }
        }
        Action::FaceCity(c) => s.face_city = c.filter(|i| *i < s.cities.len()),
        Action::CloseCitySearch => s.city_search = None,
        Action::TimerPreset(minutes) => {
            let mut t = CountdownTimer::new(&store::new_id(), "", i64::from(minutes) * MINUTE_MS);
            t.start(now);
            s.selected_timer = Some(t.id.clone());
            s.timers.push(t);
            let last = s.timers.len() - 1;
            save_timer(s, last);
        }
        Action::TimerSelect(id) => s.selected_timer = Some(id),
        Action::TimerToggle(id) => {
            if let Some(i) = s.timer_index(&id) {
                if s.timers[i].is_running() {
                    s.timers[i].pause(now);
                } else {
                    s.timers[i].start(now);
                }
                save_timer(s, i);
            }
        }
        Action::TimerReset(id) => {
            if let Some(i) = s.timer_index(&id) {
                s.timers[i].reset();
                save_timer(s, i);
            }
            s.ringing.retain(|r| r.id() != id);
        }
        Action::TimerPlusMinute(id) => {
            if let Some(i) = s.timer_index(&id) {
                s.timers[i].add(now, MINUTE_MS);
                save_timer(s, i);
            }
            s.ringing.retain(|r| r.id() != id);
        }
        Action::TimerRemove(id) => {
            if let Some(i) = s.timer_index(&id) {
                s.timers.remove(i);
                s.queue.delete(store::timer_key(&id));
                info.withdraw_notification(schedule::timer_id(&id).as_str());
            }
            s.ringing.retain(|r| r.id() != id);
            if s.selected_timer.as_deref() == Some(id.as_str()) {
                s.selected_timer = None;
            }
        }
        Action::StopwatchToggle => {
            s.stopwatch.toggle(now);
            save_stopwatch(s);
        }
        Action::StopwatchLap => {
            s.stopwatch.lap(now);
            save_stopwatch(s);
        }
        Action::StopwatchReset => {
            s.stopwatch.reset();
            save_stopwatch(s);
        }
        Action::CopyLaps => copy_laps(s, info),
        Action::Snooze => super::snooze(s),
        Action::Dismiss => super::dismiss(s, info),
        Action::DismissNotice => s.toast = None,
        // A switch's own callback sets it.
        Action::Setting(_) => {}
    }
}

/// The mode switch.
pub extern "C" fn on_screen(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        set_screen(s, info, Screen::ALL[state.selected_index.min(Screen::ALL.len() - 1)]);
    })
}

/// Switch to `screen` (remembered in the settings for the next start).
pub(crate) fn set_screen(s: &mut ClockApp, info: &mut CallbackInfo, screen: Screen) {
    if s.screen == screen {
        return;
    }
    s.screen = screen;
    println!("AZCLOCK_SCREEN {}", screen.key());
    kit::set_value(&s.kit, info, "screen", screen.key());
}

/// An alarm's switch: on (for its next time) or off.
pub extern "C" fn on_alarm_switch(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<Act>()
        .map(|a| (a.app.clone(), a.action.clone()))
    else {
        return Update::DoNothing;
    };
    let Action::EditAlarm(id) = action else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        let Some(i) = s.alarm_index(&id) else {
            return;
        };
        let now = Utc::now();
        if state.checked {
            s.alarms[i].arm(now, &Local);
            if let Some(at) = s.alarms[i].next_ring(now, &Local) {
                let text = format!(
                    "Alarm set for {} from now",
                    fmt::until((at - now).num_milliseconds())
                );
                s.toast = Some((text, now_ms() + 4000));
            }
        } else {
            s.alarms[i].enabled = false;
            s.alarms[i].snoozed_until = None;
        }
        save_alarm(s, i);
        println!("AZCLOCK_ALARM_SWITCH {id} {}", if state.checked { "on" } else { "off" });
    })
}

/// The editor's time.
pub extern "C" fn on_editor_time(mut data: RefAny, mut info: CallbackInfo, state: TimePickerState) -> Update {
    let hour = if state.is_24h {
        state.hour % 24
    } else {
        state.hour % 12 + if state.is_pm { 12 } else { 0 }
    };
    let minute = state.minute.min(59);
    edit_draft(&mut data, &mut info, |d| {
        d.hour = hour;
        d.minute = minute;
    })
}

/// The editor's repeat (azul's DateRepeatPicker).
pub extern "C" fn on_editor_repeat(mut data: RefAny, mut info: CallbackInfo, rule: DateRepeatRule) -> Update {
    edit_draft(&mut data, &mut info, |d| d.rule = rule)
}

/// The editor's sound.
pub extern "C" fn on_editor_sound(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    edit_draft(&mut data, &mut info, |d| {
        d.sound = Sound::ALL.get(index).copied().unwrap_or_default();
    })
}

/// The editor's snooze length.
pub extern "C" fn on_editor_snooze(mut data: RefAny, mut info: CallbackInfo, state: NumberInputState) -> Update {
    let minutes = state.number.round().clamp(1.0, MAX_SNOOZE_MINUTES as f32) as u32;
    edit_draft(&mut data, &mut info, |d| d.snooze_minutes = minutes)
}

/// The editor's label: kept as typed, the field is not rebuilt.
pub extern "C" fn on_editor_label(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    if let Some(mut s) = data.downcast_mut::<ClockApp>() {
        if let Some(d) = s.editor.as_mut() {
            d.label = text;
        }
    }
    let _ = &mut info;
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Changes the open editor's draft; the window is rebuilt (the rows follow
/// the repeat), nothing is written until Save.
fn edit_draft(data: &mut RefAny, _info: &mut CallbackInfo, f: impl FnOnce(&mut AlarmDraft)) -> Update {
    let Some(mut s) = data.downcast_mut::<ClockApp>() else {
        return Update::DoNothing;
    };
    match s.editor.as_mut() {
        Some(d) => {
            f(d);
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

/// The editor's or the city search's Modal was closed (its X, Escape).
pub extern "C" fn on_modal_close(mut data: RefAny, mut info: CallbackInfo, _state: ModalState) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.editor = None;
        s.city_search = None;
    })
}

/// The ringing overlay's close: dismiss.
pub extern "C" fn on_ring_close(mut data: RefAny, mut info: CallbackInfo, _state: ModalState) -> Update {
    with_app(&mut data, &mut info, |s, info, _| super::dismiss(s, info))
}

/// The city search: the results follow the query.
pub extern "C" fn on_city_query(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    let update = match data.downcast_mut::<ClockApp>() {
        Some(mut s) if s.city_search.is_some() => {
            s.city_search = Some(text);
            Update::RefreshDom
        }
        _ => Update::DoNothing,
    };
    let _ = &mut info;
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// A switch of the clock's settings (`Action::Setting(key)`).
pub extern "C" fn on_setting_switch(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<Act>()
        .map(|a| (a.app.clone(), a.action.clone()))
    else {
        return Update::DoNothing;
    };
    let Action::Setting(key) = action else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, _| {
        kit::set_value(&s.kit, info, &key, if state.checked { "true" } else { "false" });
    })
}

/// "Ring for": the minutes a ring lasts by itself.
pub extern "C" fn on_ring_minutes(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        let minutes = super::views::RING_MINUTES.get(index).copied().unwrap_or(10);
        kit::set_value(&s.kit, info, "ring-minutes", &minutes.to_string());
    })
}

/// The window's keys (after the kit's: settings, F1, Escape there).
pub extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let kit_ref = match data.downcast_ref::<ClockApp>() {
        Some(s) => s.kit.clone(),
        None => return Update::DoNothing,
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let Some(key) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let mods = info.get_key_modifiers();
    let command = mods.primary_down();
    // Space and single letters are the focused control's (a button, a field).
    let free = info.get_focused_node().into_option().is_none();
    let (ringing, dialog, screen, shown_timer) = match data.downcast_ref::<ClockApp>() {
        Some(s) => (
            !s.ringing.is_empty(),
            s.editor.is_some() || s.city_search.is_some(),
            s.screen,
            s.shown_timer().map(|i| s.timers[i].id.clone()),
        ),
        None => return Update::DoNothing,
    };
    let action = match key {
        VirtualKeyCode::Q if command => {
            if let Some(mut s) = data.downcast_mut::<ClockApp>() {
                s.quitting = true;
            }
            info.close_window();
            return Update::DoNothing;
        }
        VirtualKeyCode::Escape if ringing => Some(Action::Dismiss),
        VirtualKeyCode::Space if ringing && free => Some(Action::Snooze),
        VirtualKeyCode::Escape if dialog => Some(Action::EditorCancel),
        VirtualKeyCode::N if command => Some(Action::New),
        VirtualKeyCode::Key1 | VirtualKeyCode::Key2 | VirtualKeyCode::Key3 | VirtualKeyCode::Key4 if mods.alt => {
            let index = match key {
                VirtualKeyCode::Key1 => 0,
                VirtualKeyCode::Key2 => 1,
                VirtualKeyCode::Key3 => 2,
                _ => 3,
            };
            info.prevent_default();
            return with_app(&mut data, &mut info, |s, info, _| set_screen(s, info, Screen::ALL[index]));
        }
        VirtualKeyCode::C if command && screen == Screen::Stopwatch => Some(Action::CopyLaps),
        _ if command || dialog || !free => None,
        VirtualKeyCode::Space if screen == Screen::Stopwatch => Some(Action::StopwatchToggle),
        VirtualKeyCode::L if screen == Screen::Stopwatch => Some(Action::StopwatchLap),
        VirtualKeyCode::R if screen == Screen::Stopwatch => Some(Action::StopwatchReset),
        VirtualKeyCode::Space if screen == Screen::Timer => shown_timer.clone().map(Action::TimerToggle),
        VirtualKeyCode::Plus | VirtualKeyCode::NumpadAdd | VirtualKeyCode::Equals if screen == Screen::Timer => {
            shown_timer.map(Action::TimerPlusMinute)
        }
        _ => None,
    };
    let Some(action) = action else {
        return Update::DoNothing;
    };
    info.prevent_default();
    let action = match action {
        // Escape in the city search closes it too.
        Action::EditorCancel => Action::CloseCitySearch,
        other => other,
    };
    with_app(&mut data, &mut info, |s, info, _| {
        if action == Action::CloseCitySearch {
            s.editor = None;
        }
        run(s, info, action);
    })
}
