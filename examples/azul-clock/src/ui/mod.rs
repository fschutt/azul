//! AzClock's window: azul's S9 `UtilityShell` with the app-drawn title row
//! (`NoTitle` + `Titlebar`), the mode switch (World, Alarms, Timer,
//! Stopwatch), the screens ([`views`]), the alarm editor, the add-city
//! search and the ringing overlay (each a `Modal`), and azul-appkit's
//! settings page.
//!
//! Time: one azul `Timer` ticks every second (the clocks, the countdowns,
//! the alarms that are due); a second one ticks 30 times a second only while
//! the stopwatch runs on screen, and retexts the stopwatch's text node in
//! place (a marker + `change_node_text`, no relayout). Everything is derived
//! from the wall clock (`model`), so a slow frame never loses time.
//!
//! Ringing: while AzClock runs an alarm or a timer rings in the window (the
//! overlay, a tone through `AudioSink`, the window raised). Its next
//! occurrences are ALSO handed to the OS as scheduled notifications
//! (`schedule::plan`, `Notification::with_deliver_at`) after every change,
//! so on macOS / iOS and Windows they ring while AzClock is closed. Where the
//! OS cannot schedule (Linux, an unbundled macOS binary), closing the window
//! while something is armed minimizes it instead (settings: "Keep running").
//! A click on a notification - also one that launched AzClock - reaches
//! [`on_notification`], the app-level handler, with the payload that names
//! what rang.
//!
//! Files: `store.rs` (one JSON per alarm and timer), read when the window
//! opens and written after every change through azul-pim's write-behind
//! queue on azul-appkit's file thread.
//!
//! On stdout, for scripts/azclock_e2e.py: `AZCLOCK_SCREEN <name>`,
//! `AZCLOCK_LOADED <alarms> <timers> <cities>`, `AZCLOCK_SAVED <n>`,
//! `AZCLOCK_SCHEDULED <posted> <withdrawn>`, `AZCLOCK_RING <alarm|timer> <id>`,
//! `AZCLOCK_DISMISSED <id>`, `AZCLOCK_SNOOZED <id> <minutes>`,
//! `AZCLOCK_NOTIFICATION <kind> <payload>`.

pub mod actions;
pub mod views;

use std::path::PathBuf;

use azul::{
    audio::{AudioConfig, AudioFrame, AudioSink},
    callbacks::CallbackType,
    notification::{Notification, NotificationEventType, NotificationSound},
    prelude::*,
    shells::{ShellThemeAccent, ShellThemeScope, UtilityShell},
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    widgets::{DatePickerState, DateRepeatRule},
    window::{PlatformCapability, WindowFrame},
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::FileJob,
    shortcuts::Shortcut,
    ui as kit,
};
use azul_pim::write_queue::{Write, WriteQueue};
use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};

use crate::{
    alarm::{Alarm, Ring, RingKind},
    schedule::{self, Payload, Plan},
    stopwatch::Stopwatch,
    store,
    timer::CountdownTimer,
    tone::{self, Sound},
    world::{self, City, WorldFile},
};

// ==== The app's facts ====

/// The screens `--screen` opens.
pub const SCREENS: [&str; 5] = ["world", "alarms", "timer", "stopwatch", "settings"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzClock",
    binary: "AzClock",
    summary: "alarms, timers, a stopwatch and a world clock",
    screens: &SCREENS,
    files_help: "",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzClock",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Alarms that repeat as you like and ring also while AzClock is closed (where the \
              system can schedule them), timers, a stopwatch with laps and a world clock that \
              follows every city's daylight-saving rules. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: store::APP_FOLDER,
};

/// The keyboard shortcuts the settings page lists.
pub const SHORTCUTS: [Shortcut; 11] = [
    Shortcut::new("Clock", "Alt+1 .. Alt+4", "World, Alarms, Timer, Stopwatch"),
    Shortcut::new("Clock", "Mod+N", "New alarm, timer or city"),
    Shortcut::new("Clock", "Mod+Q", "Quit (also when AzClock keeps running)"),
    Shortcut::new("Ringing", "Space", "Snooze"),
    Shortcut::new("Ringing", "Escape", "Dismiss"),
    Shortcut::new("Timer", "Space", "Start or pause"),
    Shortcut::new("Timer", "+", "One more minute"),
    Shortcut::new("Stopwatch", "Space", "Start or stop"),
    Shortcut::new("Stopwatch", "L", "Lap"),
    Shortcut::new("Stopwatch", "R", "Reset"),
    Shortcut::new("Stopwatch", "Mod+C", "Copy the laps"),
];

/// The settings page's own category.
const APP_CATEGORIES: [&str; 1] = ["Clock"];

/// Write-back tags.
const TAG_LOAD: u64 = 1;
const TAG_SAVE: u64 = 2;

/// The tick of the clocks, the countdowns and the alarms.
const TICK_MS: u64 = 1000;
/// The stopwatch's tick while it runs on screen (about 30 Hz).
const FAST_TICK_MS: u64 = 33;
/// How much tone is queued ahead of the speaker while something rings.
const TONE_AHEAD_MS: i64 = 1500;
/// A ring stops by itself after this many minutes (settings: "Ring for").
const DEFAULT_RING_MINUTES: u32 = 10;

// ==== State ====

/// The four screens of the mode switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    World,
    Alarms,
    Timer,
    Stopwatch,
}

impl Screen {
    pub const ALL: [Screen; 4] = [Screen::World, Screen::Alarms, Screen::Timer, Screen::Stopwatch];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Screen::World => "World",
            Screen::Alarms => "Alarms",
            Screen::Timer => "Timer",
            Screen::Stopwatch => "Stopwatch",
        }
    }

    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Screen::World => "world",
            Screen::Alarms => "alarms",
            Screen::Timer => "timer",
            Screen::Stopwatch => "stopwatch",
        }
    }

    #[must_use]
    pub fn by_key(key: &str) -> Option<Screen> {
        Screen::ALL.into_iter().find(|s| s.key() == key)
    }

    #[must_use]
    pub fn index(self) -> usize {
        Screen::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// The alarm editor's fields (a new alarm when `id` is `None`).
#[derive(Clone, Debug)]
pub struct AlarmDraft {
    pub id: Option<String>,
    pub hour: u32,
    pub minute: u32,
    pub label: String,
    /// The repeat as azul's `DateRepeatPicker` edits it.
    pub rule: DateRepeatRule,
    pub sound: Sound,
    pub snooze_minutes: u32,
}

/// Something that rings right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ringing {
    /// An alarm; `ring` is what made it ring.
    Alarm { id: String, ring: Ring },
    /// A timer that finished.
    Timer { id: String },
}

impl Ringing {
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Ringing::Alarm { id, .. } | Ringing::Timer { id } => id,
        }
    }
}

/// The app's state.
pub struct ClockApp {
    pub kit: RefAny,
    pub data_root: PathBuf,
    pub sample: bool,
    pub screen: Screen,
    pub loaded: bool,
    pub alarms: Vec<Alarm>,
    pub timers: Vec<CountdownTimer>,
    /// The timer shown big (else the first).
    pub selected_timer: Option<String>,
    pub cities: Vec<City>,
    /// The city whose time the face shows (`None` = here).
    pub face_city: Option<usize>,
    pub stopwatch: Stopwatch,
    /// Writes waiting for the file thread, and the batch in flight.
    pub queue: WriteQueue,
    pub in_flight: Vec<Write>,
    /// What the OS was handed last (`None`: nothing known yet).
    pub last_plan: Option<Plan>,
    pub editor: Option<AlarmDraft>,
    /// The add-city dialog's query (`None` = closed).
    pub city_search: Option<String>,
    pub ringing: Vec<Ringing>,
    /// When the current ring started (ms), for "Ring for".
    pub ring_started: i64,
    pub sink: Option<AudioSink>,
    /// The tone is queued up to this instant (ms).
    pub tone_until: i64,
    /// A line for the user (a failed save, notifications that cannot show).
    pub notice: String,
    /// A short confirmation ("Alarm set for 15 h 58 min from now") and until when.
    pub toast: Option<(String, i64)>,
    /// Whether the OS rings while AzClock is closed, and why not.
    pub os_rings: (bool, String),
    /// Notifications can be posted at all (else nothing is scheduled).
    pub can_notify: bool,
    /// Mod+Q: the close that follows is not turned into a minimize.
    pub quitting: bool,
    /// The fast stopwatch tick, while it runs.
    pub fast_tick: Option<TimerId>,
    /// The zone the alarms were last planned in (a change re-plans).
    pub zone: String,
    /// The day the alarms were last planned on (a new day tops the schedule up).
    pub day: NaiveDate,
}

/// Milliseconds since 1970 by the wall clock.
#[must_use]
pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// The device's zone's name ("Europe/Berlin"; "" when the OS does not say).
fn zone_name() -> String {
    world::local_zone().map(|z| z.name().to_string()).unwrap_or_default()
}

impl ClockApp {
    fn new(kit_ref: RefAny, args: &AppArgs) -> ClockApp {
        let mut k = kit_ref.clone();
        let (last_screen, data_root) = match k.downcast_ref::<kit::Kit>() {
            Some(kit) => (kit.settings.get("screen").map(str::to_string), kit.data_root.clone()),
            None => (None, PathBuf::from(".")),
        };
        let screen = args
            .screen
            .as_deref()
            .and_then(Screen::by_key)
            .or_else(|| last_screen.as_deref().and_then(Screen::by_key))
            .unwrap_or(Screen::Alarms);
        let cap = PlatformCapability::notifications();
        let os_rings = if cap.available && !cfg!(target_os = "linux") {
            (true, cap.backend.as_str().to_string())
        } else if cap.available {
            (
                false,
                "this system cannot schedule notifications: AzClock keeps running to ring".to_string(),
            )
        } else {
            (false, format!("{}: {}", cap.backend.as_str(), cap.reason.as_str()))
        };
        ClockApp {
            kit: kit_ref,
            data_root,
            sample: args.sample,
            screen,
            loaded: false,
            alarms: Vec::new(),
            timers: Vec::new(),
            selected_timer: None,
            cities: Vec::new(),
            face_city: None,
            stopwatch: Stopwatch::default(),
            queue: WriteQueue::new(),
            in_flight: Vec::new(),
            last_plan: None,
            editor: None,
            city_search: None,
            ringing: Vec::new(),
            ring_started: 0,
            sink: None,
            tone_until: 0,
            notice: String::new(),
            toast: None,
            os_rings,
            can_notify: cap.available,
            quitting: false,
            fast_tick: None,
            zone: zone_name(),
            day: Local::now().date_naive(),
        }
    }

    /// A setting of the clock's own (`settings.json`).
    fn setting_bool(&self, key: &str, default: bool) -> bool {
        let mut k = self.kit.clone();
        k.downcast_ref::<kit::Kit>()
            .map_or(default, |k| k.settings.get_bool(key, default))
    }

    fn setting_u32(&self, key: &str, default: u32) -> u32 {
        let mut k = self.kit.clone();
        k.downcast_ref::<kit::Kit>()
            .and_then(|k| k.settings.get(key).and_then(|v| v.trim().parse().ok()))
            .unwrap_or(default)
    }

    /// 12-hour times (settings).
    #[must_use]
    pub fn twelve_hour(&self) -> bool {
        self.setting_bool("twelve-hour", false)
    }

    /// Hand the next occurrences to the OS (settings: on by default).
    #[must_use]
    pub fn schedule_with_os(&self) -> bool {
        self.setting_bool("os-alarms", true)
    }

    /// Minimize instead of closing while something is armed (where the OS
    /// cannot ring by itself).
    #[must_use]
    pub fn keep_running(&self) -> bool {
        self.setting_bool("keep-running", true)
    }

    /// How long a ring lasts by itself, in minutes.
    #[must_use]
    pub fn ring_minutes(&self) -> u32 {
        self.setting_u32("ring-minutes", DEFAULT_RING_MINUTES).clamp(1, 60)
    }

    /// The tone's volume, 0 to 1 (settings, in percent).
    #[must_use]
    pub fn volume(&self) -> f32 {
        self.setting_u32("volume", 80).min(100) as f32 / 100.0
    }

    /// Something is armed: an alarm that is on, a running timer, a snooze.
    #[must_use]
    pub fn armed(&self) -> bool {
        self.alarms.iter().any(|a| a.enabled) || self.timers.iter().any(CountdownTimer::is_running)
    }

    /// The alarm with `id`.
    #[must_use]
    pub fn alarm_index(&self, id: &str) -> Option<usize> {
        self.alarms.iter().position(|a| a.id == id)
    }

    /// The timer with `id`.
    #[must_use]
    pub fn timer_index(&self, id: &str) -> Option<usize> {
        self.timers.iter().position(|t| t.id == id)
    }

    /// The timer shown big: the selected one, else a running one, else the first.
    #[must_use]
    pub fn shown_timer(&self) -> Option<usize> {
        self.selected_timer
            .as_deref()
            .and_then(|id| self.timer_index(id))
            .or_else(|| self.timers.iter().position(CountdownTimer::is_running))
            .or(if self.timers.is_empty() { None } else { Some(0) })
    }

    /// The next alarm that rings, and when.
    #[must_use]
    pub fn next_alarm(&self, now: DateTime<Utc>) -> Option<(usize, DateTime<Utc>)> {
        self.alarms
            .iter()
            .enumerate()
            .filter_map(|(i, a)| a.next_ring(now, &Local).map(|at| (i, at)))
            .min_by_key(|(_, at)| *at)
    }
}

/// The app's start: switches, the kit (settings, data root), the window.
pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &APP_CATEGORIES, args.clone());
    if args.screen.as_deref() == Some("settings") {
        kit::open_settings(&kit_ref, None);
    }
    let app = ClockApp::new(kit_ref.clone(), &args);
    println!("AZCLOCK_SCREEN {}", app.screen.key());
    eprintln!("[azclock] alarms ring while AzClock is closed: {} ({})", app.os_rings.0, app.os_rings.1);
    let app_ref = RefAny::new(app);
    let mut config = kit::app_config(&kit_ref);
    // Every notification AzClock posts goes here - also the tap that
    // launched it, in a process that never posted it.
    config.set_notification_handler(app_ref.clone(), on_notification as CallbackType);
    let window = kit::window_options(&kit_ref, layout, (720.0, 560.0), (360.0, 480.0), on_window_created);
    App::create(app_ref, config).run(window);
}

/// Runs `f` on the app's state; afterwards the writes and the OS schedule
/// follow and the window is rebuilt.
pub(crate) fn with_app(
    app: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut ClockApp, &mut CallbackInfo, &RefAny),
) -> Update {
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<ClockApp>() else {
        return Update::DoNothing;
    };
    f(&mut guard, info, &handle);
    follow_up(&mut guard, info, &handle);
    Update::RefreshDom
}

// ==== Writes and the OS schedule ====

/// After every change: the writes go to the file thread, the OS is handed
/// what changed in the schedule, the stopwatch's fast tick follows its state.
pub(crate) fn follow_up(s: &mut ClockApp, info: &mut CallbackInfo, app: &RefAny) {
    flush(s, info, app);
    reschedule(s, info);
    fast_tick(s, info, app);
}

/// Queue an alarm's file.
pub(crate) fn save_alarm(s: &mut ClockApp, i: usize) {
    if let Some(a) = s.alarms.get(i) {
        s.queue.put(store::alarm_key(&a.id), store::to_bytes(a));
    }
}

/// Queue a timer's file.
pub(crate) fn save_timer(s: &mut ClockApp, i: usize) {
    if let Some(t) = s.timers.get(i) {
        s.queue.put(store::timer_key(&t.id), store::to_bytes(t));
    }
}

pub(crate) fn save_world(s: &mut ClockApp) {
    let file = WorldFile {
        cities: s.cities.clone(),
    };
    s.queue.put(store::WORLD_KEY.to_string(), store::to_bytes(&file));
}

pub(crate) fn save_stopwatch(s: &mut ClockApp) {
    s.queue.put(store::STOPWATCH_KEY.to_string(), store::to_bytes(&s.stopwatch));
}

/// Hand the waiting writes to the file thread (one batch at a time).
fn flush(s: &mut ClockApp, info: &mut CallbackInfo, app: &RefAny) {
    if !s.in_flight.is_empty() {
        return;
    }
    let Some(batch) = s.queue.take() else {
        return;
    };
    let jobs: Vec<FileJob> = batch
        .iter()
        .map(|w| match w {
            Write::Put { key, bytes } => FileJob::Put {
                key: key.clone(),
                bytes: bytes.clone(),
            },
            Write::Delete { key } => FileJob::Delete { key: key.clone() },
        })
        .collect();
    s.in_flight = batch;
    kit::spawn_file_jobs(info, &s.data_root, jobs, app.clone(), TAG_SAVE, on_files_done);
}

/// The notification for one planned instant.
fn notification_of(p: &schedule::Planned) -> Notification {
    let mut n = Notification::create(p.id.as_str(), p.title.as_str())
        .with_body(p.body.as_str())
        .with_payload(p.payload.to_text())
        .with_sound(if p.silent {
            NotificationSound::Silent
        } else {
            NotificationSound::Default
        })
        .with_deliver_at(u64::try_from(p.at_ms).unwrap_or(0));
    if p.snooze_minutes > 0 {
        n = n.with_action("snooze", format!("Snooze {} min", p.snooze_minutes));
    }
    n.with_action("dismiss", "Dismiss")
}

/// Hand the OS what changed since the last plan: the next occurrences of
/// every alarm that is on, every snooze, every running timer's end.
fn reschedule(s: &mut ClockApp, info: &mut CallbackInfo) {
    if !s.loaded {
        return;
    }
    let mut next = schedule::plan(&s.alarms, &s.timers, Utc::now(), &Local, s.twelve_hour());
    if !(s.schedule_with_os() && s.can_notify) {
        // Nothing for the OS: what it holds goes.
        let posted: Vec<String> = next.post.drain(..).map(|p| p.id).collect();
        next.withdraw.extend(posted);
    }
    let changes = schedule::diff(s.last_plan.as_ref(), &next);
    for p in &changes.post {
        info.post_notification(notification_of(p));
    }
    for id in &changes.withdraw {
        info.withdraw_notification(id.as_str());
    }
    if !changes.post.is_empty() || !changes.withdraw.is_empty() {
        println!("AZCLOCK_SCHEDULED {} {}", changes.post.len(), changes.withdraw.len());
    }
    s.last_plan = Some(next);
}

/// The stopwatch's 30 Hz tick runs exactly while it runs on screen.
fn fast_tick(s: &mut ClockApp, info: &mut CallbackInfo, app: &RefAny) {
    let wanted =
        s.stopwatch.is_running() && s.screen == Screen::Stopwatch && !kit::settings_open(&s.kit);
    match (wanted, s.fast_tick) {
        (true, None) => {
            let id = TimerId::unique();
            let timer = Timer::create(app.clone(), on_fast_tick, info.get_system_time_fn())
                .with_interval(Duration::System(SystemTimeDiff::from_millis(FAST_TICK_MS)));
            info.add_timer(id, timer);
            s.fast_tick = Some(id);
        }
        (false, Some(id)) => {
            info.remove_timer(id);
            s.fast_tick = None;
        }
        _ => {}
    }
}

// ==== Ringing ====

/// The sound of what rings first.
fn current_sound(s: &ClockApp) -> Option<Sound> {
    match s.ringing.first()? {
        Ringing::Alarm { id, .. } => s.alarm_index(id).map(|i| s.alarms[i].sound),
        Ringing::Timer { id } => s.timer_index(id).map(|i| s.timers[i].sound),
    }
}

/// One loop of a sound, in seconds (each pattern's own period).
fn pattern_seconds(sound: Sound) -> f32 {
    match sound {
        Sound::Chime => 1.6,
        _ => 1.0,
    }
}

/// Keep the speaker [`TONE_AHEAD_MS`] ahead while something rings.
fn feed_tone(s: &mut ClockApp) {
    let Some(sound) = current_sound(s) else {
        stop_tone(s);
        return;
    };
    if sound == Sound::Silent {
        return;
    }
    if s.sink.is_none() {
        let sink = AudioSink::open(AudioConfig {
            sample_rate: tone::SAMPLE_RATE,
            channels: 1,
        });
        if !sink.is_open() {
            if s.notice.is_empty() {
                s.notice = "No sound: the audio output could not be opened.".to_string();
            }
            return;
        }
        s.sink = Some(sink);
    }
    let volume = s.volume();
    let seconds = pattern_seconds(sound);
    let now = now_ms();
    let mut until = s.tone_until.max(now);
    if let Some(sink) = s.sink.as_ref() {
        while until < now + TONE_AHEAD_MS {
            let samples = tone::pattern(sound, seconds, volume);
            sink.play(AudioFrame {
                sample_rate: tone::SAMPLE_RATE,
                channels: 1,
                samples: samples.into(),
            });
            until += (seconds * 1000.0) as i64;
        }
    }
    s.tone_until = until;
}

fn stop_tone(s: &mut ClockApp) {
    if let Some(mut sink) = s.sink.take() {
        sink.close();
    }
    s.tone_until = 0;
}

/// Something starts ringing: the overlay, the tone, the window to the front.
pub(crate) fn start_ringing(s: &mut ClockApp, info: &mut CallbackInfo, item: Ringing) {
    if s.ringing.iter().any(|r| r.id() == item.id()) {
        return;
    }
    let kind = match item {
        Ringing::Alarm { .. } => "alarm",
        Ringing::Timer { .. } => "timer",
    };
    println!("AZCLOCK_RING {kind} {}", item.id());
    if s.ringing.is_empty() {
        s.ring_started = now_ms();
    }
    s.ringing.push(item);
    let mut state = info.get_current_window_state();
    if matches!(state.flags.frame, WindowFrame::Minimized) {
        state.flags.frame = WindowFrame::Normal;
        info.modify_window_state(state);
    }
    info.raise_window();
    feed_tone(s);
}

/// The first ring ends ("Dismiss"): an alarm keeps the occurrence as rung,
/// a timer goes back to its length; the notification the OS showed for a
/// snooze or a timer goes too.
pub(crate) fn dismiss(s: &mut ClockApp, info: &mut CallbackInfo) {
    if s.ringing.is_empty() {
        return;
    }
    let item = s.ringing.remove(0);
    match &item {
        Ringing::Alarm { id, .. } => {
            info.withdraw_notification(schedule::snooze_id(id).as_str());
        }
        Ringing::Timer { id } => {
            if let Some(i) = s.timer_index(id) {
                s.timers[i].reset();
                save_timer(s, i);
            }
            info.withdraw_notification(schedule::timer_id(id).as_str());
        }
    }
    println!("AZCLOCK_DISMISSED {}", item.id());
    if s.ringing.is_empty() {
        stop_tone(s);
    }
}

/// The first ring is snoozed: an alarm rings again after its snooze, a
/// timer gets one more minute.
pub(crate) fn snooze(s: &mut ClockApp) {
    if s.ringing.is_empty() {
        return;
    }
    let item = s.ringing.remove(0);
    let now = Utc::now();
    match &item {
        Ringing::Alarm { id, .. } => {
            if let Some(i) = s.alarm_index(id) {
                let minutes = s.alarms[i].snooze_minutes;
                s.alarms[i].snooze(now, minutes);
                save_alarm(s, i);
                println!("AZCLOCK_SNOOZED {id} {minutes}");
                s.toast = Some((
                    format!("Snoozed for {}", crate::fmt::minutes(minutes)),
                    now_ms() + 4000,
                ));
            }
        }
        Ringing::Timer { id } => {
            if let Some(i) = s.timer_index(id) {
                s.timers[i].add(now.timestamp_millis(), crate::timer::MINUTE_MS);
                save_timer(s, i);
            }
        }
    }
    if s.ringing.is_empty() {
        stop_tone(s);
    }
}

/// What came due at `now`: alarms ring, timers finish. `true` if anything did.
fn check_due(s: &mut ClockApp, info: &mut CallbackInfo, now: DateTime<Utc>) -> bool {
    let mut any = false;
    for i in 0..s.alarms.len() {
        if let Some(ring) = s.alarms[i].due(now, &Local) {
            s.alarms[i].rang(ring);
            save_alarm(s, i);
            let id = s.alarms[i].id.clone();
            start_ringing(s, info, Ringing::Alarm { id, ring });
            any = true;
        }
    }
    let now_ms = now.timestamp_millis();
    for i in 0..s.timers.len() {
        if s.timers[i].tick(now_ms) {
            save_timer(s, i);
            let id = s.timers[i].id.clone();
            start_ringing(s, info, Ringing::Timer { id });
            any = true;
        }
    }
    any
}

// ==== Ticks ====

/// Every second: the clocks move, alarms and timers come due, a ring goes
/// on or ends by itself, a new day or zone tops the OS schedule up.
extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<ClockApp>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let s = &mut *guard;
    if !s.loaded {
        return TimerCallbackReturn::continue_unchanged();
    }
    let cb = &mut info.callback_info;
    let now = Utc::now();
    let mut changed = check_due(s, cb, now);
    if !s.ringing.is_empty() {
        let limit = i64::from(s.ring_minutes()) * 60_000;
        if now.timestamp_millis() - s.ring_started > limit {
            while !s.ringing.is_empty() {
                dismiss(s, cb);
            }
            s.notice = "A ring ended by itself (settings: Ring for).".to_string();
        } else {
            feed_tone(s);
        }
        changed = true;
    }
    if s
        .toast
        .as_ref()
        .is_some_and(|(_, until)| *until <= now.timestamp_millis())
    {
        s.toast = None;
        changed = true;
    }
    let zone = zone_name();
    let day = Local::now().date_naive();
    if zone != s.zone || day != s.day {
        s.zone = zone;
        s.day = day;
        changed = true;
    }
    follow_up(s, cb, &app);
    // The stopwatch screen and the settings page show no clock: no rebuild
    // unless something happened.
    let still = s.screen == Screen::Stopwatch || kit::settings_open(&s.kit);
    if changed || !still {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// 30 times a second while the stopwatch runs on screen: its text, in place.
extern "C" fn on_fast_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut guard) = data.downcast_mut::<ClockApp>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let s = &mut *guard;
    if !s.stopwatch.is_running() || s.screen != Screen::Stopwatch {
        s.fast_tick = None;
        return TimerCallbackReturn::terminate_unchanged();
    }
    let text = crate::fmt::stopwatch(s.stopwatch.elapsed(now_ms()));
    if let Some(node) = info
        .callback_info
        .get_node_id_by_marker(crate::ids::STOPWATCH_TIME)
        .into_option()
    {
        info.callback_info.change_node_text(node, text);
    }
    TimerCallbackReturn::continue_unchanged()
}

// ==== The window ====

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(s) = data.downcast_ref::<ClockApp>() else {
        return Update::DoNothing;
    };
    kit::on_window_created(&s.kit, &mut info);
    kit::spawn_file_jobs(
        &mut info,
        &s.data_root,
        store::load_jobs(),
        app.clone(),
        TAG_LOAD,
        on_files_done,
    );
    let timer = Timer::create(app, on_tick, info.get_system_time_fn())
        .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS)));
    info.add_timer(TimerId::unique(), timer);
    Update::DoNothing
}

extern "C" fn on_files_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, _| {
        if reply.tag == TAG_LOAD {
            let mut loaded = store::read_loaded(reply.outcomes);
            for problem in &loaded.problems {
                eprintln!("[azclock] {problem}");
            }
            if let Some(problem) = loaded.problems.first() {
                s.notice = problem.clone();
            }
            let first_sample = loaded.is_first_run() && s.sample;
            if first_sample {
                loaded = store::sample(Utc::now(), &Local);
            }
            s.alarms = loaded.alarms;
            s.timers = loaded.timers;
            s.cities = loaded.world.map(|w| w.cities).unwrap_or_default();
            s.stopwatch = loaded.stopwatch.unwrap_or_default();
            if first_sample {
                for i in 0..s.alarms.len() {
                    save_alarm(s, i);
                }
                for i in 0..s.timers.len() {
                    save_timer(s, i);
                }
                save_world(s);
                save_stopwatch(s);
            }
            s.loaded = true;
            println!(
                "AZCLOCK_LOADED {} {} {}",
                s.alarms.len(),
                s.timers.len(),
                s.cities.len()
            );
            // A timer that ended while AzClock was closed rings now; an
            // alarm missed by more than a few minutes does not.
            check_due(s, info, Utc::now());
            if s.armed() && s.can_notify && s.schedule_with_os() {
                info.request_notification_permission();
            }
            return;
        }
        let batch = std::mem::take(&mut s.in_flight);
        let mut failed: Vec<(Write, String)> = Vec::new();
        for (write, outcome) in batch.into_iter().zip(reply.outcomes.iter()) {
            if let Some(e) = outcome.error() {
                failed.push((write, e));
            }
        }
        let written = reply.outcomes.len().saturating_sub(failed.len());
        if let Some((_, e)) = failed.first() {
            s.notice = format!("Not saved: {e}");
        }
        s.queue.finish(failed);
        println!("AZCLOCK_SAVED {written}");
    })
}

/// The app-level notification handler: a click, a button, a dismissal or a
/// failure of any notification AzClock posted - also in a process that
/// never posted it (the tap that launched AzClock). The payload names what.
extern "C" fn on_notification(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(event) = info.get_notification_event().into_option() else {
        return Update::DoNothing;
    };
    let payload = event.payload.as_str().to_string();
    let action = event.action_id.as_str().to_string();
    let reason = event.reason.as_str().to_string();
    let kind = match event.kind {
        NotificationEventType::Activated => "activated",
        NotificationEventType::ActionInvoked => "action",
        NotificationEventType::Dismissed => "dismissed",
        NotificationEventType::Failed => "failed",
    };
    println!("AZCLOCK_NOTIFICATION {kind} {payload}");
    with_app(&mut data, &mut info, |s, info, _| {
        if kind == "failed" {
            s.os_rings = (false, reason.clone());
            s.notice = format!("The system cannot ring alarms while AzClock is closed: {reason}");
            return;
        }
        let Some(p) = Payload::parse(&payload) else {
            return;
        };
        let ringing_now = s.ringing.first().is_some_and(|r| r.id() == p.id());
        match (kind, action.as_str()) {
            ("activated", _) => {
                s.screen = match p {
                    Payload::Timer { .. } => Screen::Timer,
                    _ => Screen::Alarms,
                };
            }
            ("action", "snooze") if ringing_now => snooze(s),
            ("action", "dismiss") if ringing_now => dismiss(s, info),
            ("action", "snooze") => {
                // It rang while AzClock was closed: snooze it from here.
                if let Some(i) = s.alarm_index(p.id()) {
                    if let Payload::Alarm { at_ms, .. } = p {
                        s.alarms[i].rang(Ring {
                            at: crate::alarm::instant(at_ms),
                            kind: RingKind::Occurrence,
                        });
                    }
                    let minutes = s.alarms[i].snooze_minutes;
                    s.alarms[i].snooze(Utc::now(), minutes);
                    save_alarm(s, i);
                    println!("AZCLOCK_SNOOZED {} {minutes}", p.id());
                }
            }
            ("action", "dismiss") => {
                if let (Some(i), Payload::Alarm { at_ms, .. }) = (s.alarm_index(p.id()), &p) {
                    s.alarms[i].rang(Ring {
                        at: crate::alarm::instant(*at_ms),
                        kind: RingKind::Occurrence,
                    });
                    save_alarm(s, i);
                }
            }
            _ => {}
        }
    })
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let wide = info.window_width_greater_than(views::WIDE);
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<ClockApp>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let now = Utc::now();
    let settings = kit::settings_open(&s.kit);
    let content = if settings {
        kit::settings_page(&s.kit, views::settings_sections(s, &app))
    } else {
        views::screen(s, &app, now, wide)
    };
    let mut shell = UtilityShell::create(content)
        .with_title_row(kit::title_row(SPEC.name))
        .with_label(SPEC.name)
        .with_min_size(360.0, 480.0);
    if !settings {
        shell = shell.with_modes(views::modes_row(s, &app));
    }
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(shell.dom());
    if !settings {
        for overlay in views::overlays(s, &app, now) {
            column.add_child(overlay);
        }
    }
    // The scope as the window's body: no UA margin, the full window height.
    ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Plum)
        .body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            actions::on_key,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::CloseRequested),
            app,
            on_close_requested,
        )
}

/// Closing the window while an alarm or a timer is armed, where the OS
/// cannot ring by itself: AzClock stays running, minimized (settings: "Keep
/// running"), and says so. Mod+Q quits for real.
extern "C" fn on_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<ClockApp>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if s.quitting || s.os_rings.0 || !s.keep_running() || !s.armed() {
        return Update::DoNothing;
    }
    info.prevent_window_close();
    let mut state = info.get_current_window_state();
    state.flags.frame = WindowFrame::Minimized;
    info.modify_window_state(state);
    s.notice =
        "AzClock keeps running (minimized) so your alarms and timers ring. Mod+Q quits.".to_string();
    println!("AZCLOCK_KEPT_RUNNING");
    Update::RefreshDom
}

/// The repeat rule the editor shows for an alarm's RRULE, from its first day.
pub(crate) fn rule_of(rrule: &str, first: NaiveDate) -> DateRepeatRule {
    let start = DatePickerState {
        year: u32::try_from(first.year()).unwrap_or(1970),
        month: first.month(),
        day: first.day(),
    };
    DateRepeatRule::from_rrule(rrule, start)
        .into_option()
        .unwrap_or_else(|| DateRepeatRule::create(start))
}

/// The first day of an alarm the editor's rule makes.
pub(crate) fn first_of(rule: &DateRepeatRule) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(
        i32::try_from(rule.start.year).ok()?,
        rule.start.month,
        rule.start.day,
    )
}

/// The laps to the clipboard (Lap, Lap time, Total; newest first).
pub(crate) fn copy_laps(s: &ClockApp, info: &mut CallbackInfo) {
    use azul::{dom::ClipboardContent, option::OptionString, str::String as AzString, vec::StyledTextRunVec};
    let text = s.stopwatch.laps_text();
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(text.as_str()),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
    println!("AZCLOCK_COPIED {}", s.stopwatch.laps.len());
}
