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
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    widgets::{DatePickerState, DateRepeatRule},
    window::{PlatformCapability, WindowFrame},
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::{FileJob, FileOutcome},
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
