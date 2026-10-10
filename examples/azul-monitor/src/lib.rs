//! AzMonitor: the system monitor (Task Manager, Activity Monitor, htop) on
//! the public azul API.
//!
//! The window is the old Windows Task Manager (XP / 7) on azul's S6
//! `RecordsShell`: the app-drawn title row (`NoTitle` + `Titlebar`), the tab
//! row - Processes, Performance, Networking, Users (Applications and
//! Services are not tabs: AzMonitor lists no windows and no system
//! services) -, the tab's page, the status bar "Processes: N | CPU Usage:
//! x% | Physical Memory: y%". Processes: a dense, sortable table (azul's
//! `DataTable`: Image Name, PID, User Name, CPU, Memory, Description) over
//! the filter and "End Process" at the bottom right. Performance: the CPU
//! and memory usage meters beside their history graphs (one per core),
//! the machine's figures under them. Networking: the network's history and
//! figures. Users: who runs the processes ([`graph`]).
//!
//! THE READINGS: a sampler on an azul `Thread` ([`sampler`]) reads the
//! system once a second (the update speed is a setting) through the
//! `sysinfo` crate ([`live`]), or the deterministic sample machine with
//! `--sample` ([`sample`]); every reading arrives as a write-back
//! ([`on_reading`]) and goes into the [`model::Model`]: the histories, the
//! process rows in the sort order, the filter, the selected process.
//!
//! THE 1 HZ PATH NEVER LAYS THE PAGE OUT AGAIN ([`ticks`]): the numbers
//! that change every second live in LIVE VIEWS - `VirtualView`s whose
//! callbacks read the model (the table, the Performance / Networking /
//! Users pages). A reading re-renders the live view of the tab shown
//! (`trigger_virtual_view_rerender`) and rewrites the status bar's marked
//! labels (`StatusBar::update_segment_label`); `layout()` does not run.
//! Only the first reading (the empty state gives way to the page) and what
//! the user does (a tab, a sort, a selection, the question) rebuild it.
//! While the user scrolls or drags in the table a reading leaves it alone
//! (the rows do not re-sort under the pointer). The table's scroll position
//! is a place in the processes, not a row number: a build after a reading
//! keeps the processes in view where they were ([`table::sync`]).
//!
//! SMOOTH GRAPHS: the graphs stand where a steady SCROLL stands (the reading
//! at their right edge, a reading number with a fraction, `graph::Scroll`);
//! a frame timer ([`on_frame`], 25 a second, only while a tab with graphs
//! shows) shifts each graph's strip as the scroll moves, and a reading
//! neither resets nor jolts it - its new drawing is shifted to where the
//! scroll stands, the newest reading comes in from the right edge, and the
//! pace adapts to when the readings really arrive ([`graph`]).
//!
//! END PROCESS: select a row, then "End Process" (or Delete): a question
//! (azul's `MessageBox` in a `Modal`) - End (SIGTERM), Kill (SIGKILL) or
//! Cancel; a process of another user says administrator rights are needed.
//! The sampler ends it on its thread and reads at once: the process leaves
//! the table and the status bar says what happened.
//!
//! DATA: the settings (theme, mode, update speed) in the kit's
//! `monitor/settings.json`; "Export the last minute" writes
//! `monitor/history/<date>.csv` into the data tree through the drive, on a
//! Thread (azul-appkit's file jobs).
//!
//! Switches (azul-appkit): `--screen
//! processes|performance|networking|users|settings`, `--theme flat|flora`,
//! `--mode light|dark|system`, `--size WxH`, `--shot <png>`, `--sample`
//! (the sample machine), `--data-dir <dir>`.
//!
//! On stdout, for scripts (`scripts/azmonitor_e2e.py`): `AZMON_LAYOUT <n>`
//! every time `layout()` runs, `AZMON_READY <processes>` at the first
//! reading, `AZMON_TICK <readings> <processes> <shown>` at every reading,
//! `AZMON_TOP <pid> <name>` (the first row), `AZMON_VIEW <top> <pid>
//! <selected> <name>` at every build of the table (its first row shown and
//! that row's process; the selected process' row of the screen, `-` none,
//! `out` not in view), `AZMON_SCROLL <top>` (the table scrolled),
//! `AZMON_SORT <text>`,
//! `AZMON_SHOWN <shown>` after a filter, `AZMON_SELECT <pid> <name>`,
//! `AZMON_ASK <pid> <name>` (the question opens), `AZMON_END <pid> <force>`,
//! `AZMON_NOTICE <text>`, `AZMON_SCREEN <name>`, `AZMON_SPEED <ms>`,
//! `AZMON_EXPORTED <key>`.

/// The history graphs and usage meters (the old Task Manager's look).
pub mod graph;
/// The history of one measure: a ring of the last readings.
pub mod history;
/// The DOM ids, classes and markers (`__azmonitor_` prefix), each once.
pub mod ids;
/// The live machine: this computer through the `sysinfo` crate.
pub mod live;
/// The sampling model: readings, rates, the process rows, sort and filter.
pub mod model;
/// The sample machine (`--sample`): a deterministic system.
pub mod sample;
/// The sampler: an azul Thread that reads the system once a second.
pub mod sampler;
/// The process table: azul's DataTable over the model.
pub mod table;
/// What a tick redraws (the live views, never the page).
pub mod ticks;
/// The window's parts: the tool row, the live views, the status bar, the
/// question, the settings.
pub mod ui;

use std::sync::{atomic::Ordering, Arc};

use azul::{
    callbacks::{TimerCallbackInfo, TimerCallbackReturn, WriteBackCallbackType},
    dom::{DomId, NodeId, VirtualKeyCode},
    prelude::*,
    shells::{RecordsShell, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::{Thread, ThreadId, Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    widgets::{DataTableView, StatusBar},
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    settings::AppSettings,
    shortcuts::Shortcut,
    ui as kit,
};

use crate::{
    model::{Model, SortKey},
    sampler::{Command, Reading, SamplerInit, Shared, DEFAULT_INTERVAL_MS},
    ticks::{LiveView, Screen},
};

// ==== The app's facts ====

/// The screens `--screen` opens.
pub const SCREENS: [&str; 5] = ["processes", "performance", "networking", "users", "settings"];

/// The frame timer's interval while a tab with graphs shows, ms (25 a
/// second: the history moves a fraction of a pixel a frame).
pub const FRAME_MS: u64 = 40;

/// A rate graph's least top, bytes per second (an idle network's noise is
/// not drawn as cliffs).
pub const RATE_FLOOR: f64 = 1024.0;

pub const SPEC: AppSpec = AppSpec {
    name: "AzMonitor",
    binary: "AzMonitor",
    summary: "the system monitor: processes, CPU, memory, disk and network",
    screens: &SCREENS,
    files_help: "",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzMonitor",
    version: env!("CARGO_PKG_VERSION"),
    summary: "What uses the CPU, the memory, the disk and the network - and a way to stop it. \
              Readings once a second on a background thread, a minute of history, and a \
              window that never lays itself out again to show them. Part of the Azlin apps, \
              built with azul.",
    license: "MIT",
    app_folder: "monitor",
};

/// The keyboard shortcuts the settings page lists.
pub const SHORTCUTS: [Shortcut; 8] = [
    Shortcut::new("Processes", "Up  Down", "Select a process"),
    Shortcut::new(
        "Processes",
        "Page Up  Page Down",
        "Move by a screen of processes",
    ),
    Shortcut::new("Processes", "Mod+Home  Mod+End", "The first / last process"),
    Shortcut::new(
        "Processes",
        "Click  Shift+Click",
        "Sort by a column, add a second key",
    ),
    Shortcut::new(
        "Processes",
        "Delete",
        "End the selected process (asks first)",
    ),
    Shortcut::new("Processes", "Mod+C", "Copy the selected rows"),
    Shortcut::new("Monitor", "F5", "Read the system now"),
    Shortcut::new("Monitor", "Escape", "Close the question"),
];

/// The settings page's own category (first on the page).
pub const APP_CATEGORIES: [&str; 1] = ["Monitor"];

// ==== The update speed ====

/// The update speeds the settings offer: a label and the interval in ms
/// (0 = paused).
pub const SPEEDS: [(&str, u64); 4] = [("0.5 s", 500), ("1 s", 1000), ("2 s", 2000), ("Paused", 0)];

/// The settings key of the update speed (`monitor/settings.json`).
pub const SPEED_KEY: &str = "update_ms";

/// The update speed a stored setting names: one of [`SPEEDS`]' intervals,
/// else the default (one reading a second).
#[must_use]
pub fn speed_from_setting(value: Option<&str>) -> u64 {
    value
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|ms| SPEEDS.iter().any(|(_, s)| s == ms))
        .unwrap_or(DEFAULT_INTERVAL_MS)
}

/// The place of `interval_ms` among [`SPEEDS`] (the default's when it is
/// none of them).
#[must_use]
pub fn speed_index(interval_ms: u64) -> usize {
    let of = |ms: u64| SPEEDS.iter().position(|(_, s)| *s == ms);
    of(interval_ms)
        .or_else(|| of(DEFAULT_INTERVAL_MS))
        .unwrap_or(0)
}

/// The status bar's word on the update speed: "Updated every 1 s", "Paused".
#[must_use]
pub fn speed_text(interval_ms: u64) -> String {
    if interval_ms == 0 {
        return "Paused".to_string();
    }
    if interval_ms % 1000 == 0 {
        format!("Updated every {} s", interval_ms / 1000)
    } else {
        format!(
            "Updated every {}.{} s",
            interval_ms / 1000,
            (interval_ms % 1000) / 100
        )
    }
}

/// The time between two readings as they really arrive (the sampler looks
/// at its clock every 50 ms, and a reading takes its time): the average so
/// far `gap_ms` with the `observed` gap, kept between half and three times
/// the update speed `interval_ms` (a reading asked for at once, a stall).
#[must_use]
pub fn next_gap(gap_ms: f64, observed: f64, interval_ms: f64) -> f64 {
    if !(interval_ms > 0.0) {
        return gap_ms;
    }
    let observed = if observed.is_finite() {
        observed.clamp(interval_ms * 0.5, interval_ms * 3.0)
    } else {
        interval_ms
    };
    if !(gap_ms > 0.0) {
        return observed;
    }
    0.7 * gap_ms + 0.3 * observed
}

/// What the sort is, for the scripts: "CPU desc, Name asc"; "PID" for none.
#[must_use]
pub fn sort_text(keys: &[SortKey]) -> String {
    if keys.is_empty() {
        return "PID".to_string();
    }
    keys.iter()
        .map(|k| {
            format!(
                "{} {}",
                k.column.title(),
                if k.descending { "desc" } else { "asc" }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ==== State ====

/// The end-process question, while it is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirm {
    pub pid: u32,
    pub name: String,
    pub user: String,
    /// The process belongs to another user: ending it needs administrator
    /// rights.
    pub other_user: bool,
}

/// The app's state.
pub struct Monitor {
    /// The appkit kit (settings, data root, the settings page's state).
    pub kit: RefAny,
    /// The readings so far.
    pub model: Model,
    /// The process table's view (scroll, selection, the header's arrows):
    /// its positions are places in `shown`.
    pub table: DataTableView,
    /// The rows the process table showed when it was last built (process
    /// ids, top to bottom). Its next build carries `table` over to the
    /// model's rows of then (`table::sync`).
    pub shown: Vec<u32>,
    /// How many rows the process table showed when it was last built.
    pub table_page: usize,
    /// What the table's next build keeps in view after a new sort.
    pub sort_anchor: Option<table::SortAnchor>,
    /// The screen shown.
    pub screen: Screen,
    /// The sample machine instead of this computer (`--sample`).
    pub sample: bool,
    /// What the window and the sampler share.
    pub shared: Arc<Shared>,
    /// The update speed in ms (0 = paused).
    pub interval_ms: u64,
    /// The window's size.
    pub window: (f32, f32),
    /// The end-process question, while it is open.
    pub confirm: Option<Confirm>,
    /// How many times `layout()` ran (the scripts check a tick runs none).
    pub layouts: u64,
    /// The app's own last word for the status bar (an export), "" = none.
    pub notice: String,
    /// The app's monotonic clock.
    pub clock: std::time::Instant,
    /// When the user last scrolled / dragged in the table (ms on `clock`).
    pub table_touched_ms: Option<u64>,
    /// When the latest reading arrived (the time between readings is learned
    /// from it).
    pub reading_at: Option<std::time::Instant>,
    /// The time between two readings as they really arrive, ms (an average).
    pub gap_ms: f64,
    /// Where the graphs stand: the reading at their right edge, moving on
    /// (`None` before the first reading).
    pub scroll: Option<graph::Scroll>,
    /// The steps of the graphs drawn last, strip by strip (`graph::Strips`).
    pub strips: Vec<f32>,
    /// The readings the graphs shown were drawn with (their newest).
    pub drawn: u64,
    /// The lag the strips were shifted by last (`graph::Scroll::lag`).
    pub shifted: f64,
    /// The frame timer runs.
    pub animating: bool,
    /// The network graph's top, bytes per second (moves in calm steps).
    pub net_top: f64,
}

impl Monitor {
    /// A monitor waiting for its first reading.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // a few thousand ms
    pub fn new(kit: RefAny, args: &AppArgs, interval_ms: u64) -> Self {
        let screen = args
            .screen
            .as_deref()
            .and_then(Screen::named)
            .unwrap_or_default();
        let model = Model::new();
        let table = table::view_for(model.sort());
        Self {
            kit,
            model,
            table,
            shown: Vec::new(),
            table_page: 0,
            sort_anchor: None,
            screen,
            sample: args.sample,
            shared: Arc::new(Shared::new(interval_ms)),
            interval_ms,
            window: args.size.unwrap_or((1200.0, 780.0)),
            confirm: None,
            layouts: 0,
            notice: String::new(),
            clock: std::time::Instant::now(),
            table_touched_ms: None,
            reading_at: None,
            gap_ms: interval_ms as f64,
            scroll: None,
            strips: Vec::new(),
            drawn: 0,
            shifted: 0.0,
            animating: false,
            net_top: 0.0,
        }
    }

    /// Milliseconds on the app's clock.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Milliseconds on the app's clock with their fraction (the graphs'
    /// scroll moves a fraction of a pixel a frame).
    #[must_use]
    pub fn clock_ms(&self) -> f64 {
        self.clock.elapsed().as_secs_f64() * 1000.0
    }

    /// How many readings right of the graphs' right edge the newest reading
    /// of a drawing of `newest` readings stands at `now_ms` (its strips'
    /// shift in steps): where the scroll stands - 0 while paused, the newest
    /// reading at the edge.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // reading counts far below 2^52
    pub fn graph_lag(&self, now_ms: f64, newest: u64) -> f64 {
        match self.scroll {
            Some(scroll) if self.interval_ms > 0 => scroll.lag(now_ms, newest as f64),
            _ => 0.0,
        }
    }

    /// Whether the user's hand is on the process table (a reading leaves it
    /// alone).
    #[must_use]
    pub fn hands_on_table(&self) -> bool {
        let since = self
            .table_touched_ms
            .map(|at| self.now_ms().saturating_sub(at));
        table::hands_on(&self.table, since)
    }

    /// A reading arrived `now` (the model has it): the time between readings
    /// is learned, and the graphs' scroll goes on at the pace that keeps it
    /// with the readings - from where it stands, so no point moves
    /// (`graph::Scroll::after_reading`).
    #[allow(clippy::cast_precision_loss)] // a few thousand ms, reading counts
    pub fn note_reading(&mut self, now: std::time::Instant) {
        if let Some(previous) = self.reading_at {
            let observed = now.duration_since(previous).as_secs_f64() * 1000.0;
            self.gap_ms = next_gap(self.gap_ms, observed, self.interval_ms as f64);
        }
        self.reading_at = Some(now);
        let now_ms = now.saturating_duration_since(self.clock).as_secs_f64() * 1000.0;
        self.scroll = Some(graph::Scroll::after_reading(
            self.scroll,
            now_ms,
            self.model.readings as f64,
            self.gap_ms,
        ));
        let peak = self
            .model
            .net_in
            .max()
            .unwrap_or(0.0)
            .max(self.model.net_out.max().unwrap_or(0.0));
        self.net_top = graph::next_top(self.net_top, peak, RATE_FLOOR);
    }

    /// The update speed in seconds (a paused monitor counts one second).
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // a few thousand ms
    pub fn seconds_per_reading(&self) -> f64 {
        if self.interval_ms == 0 {
            1.0
        } else {
            self.interval_ms as f64 / 1000.0
        }
    }

    /// What the status bar says last: the sampler's latest notice, else
    /// the app's own.
    #[must_use]
    pub fn last_word(&self) -> String {
        if !self.notice.is_empty() {
            return self.notice.clone();
        }
        self.model.notices.last().cloned().unwrap_or_default()
    }
}

/// The update speed the kit's settings name.
fn stored_speed(kit_ref: &RefAny) -> u64 {
    let mut kit_ref = kit_ref.clone();
    kit_ref
        .downcast_ref::<kit::Kit>()
        .map_or(DEFAULT_INTERVAL_MS, |k| {
            speed_from_setting(k.settings.get(SPEED_KEY))
        })
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
    let interval = stored_speed(&kit_ref);
    let app = Monitor::new(kit_ref.clone(), &args, interval);
    let config = kit::app_config(&kit_ref);
    let window = kit::window_options(
        &kit_ref,
        layout,
        (1200.0, 780.0),
        (720.0, 480.0),
        on_window_created,
    );
    App::create(RefAny::new(app), config).run(window);
}

// ==== The readings ====

/// The window is up: the `--shot` timer, then the sampler on its Thread.
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some((kit_ref, sample, shared)) = data
        .downcast_ref::<Monitor>()
        .map(|s| (s.kit.clone(), s.sample, s.shared.clone()))
    else {
        return Update::DoNothing;
    };
    kit::on_window_created(&kit_ref, &mut info);
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(SamplerInit {
                sample,
                shared,
                on_reading: on_reading as WriteBackCallbackType,
            }),
            app,
            sampler::sampler_thread,
        ),
    );
    Update::DoNothing
}

/// A reading arrived: into the model, then the tick - the live view of the
/// tab re-renders in place (not the table while the user's hand is on it),
/// the status labels are rewritten, the graphs' scroll takes the reading's
/// pace; the page is built only for the first reading.
pub extern "C" fn on_reading(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(snapshot) = msg
        .downcast_mut::<Reading>()
        .and_then(|mut r| r.snapshot.take())
    else {
        return Update::DoNothing;
    };
    let handle = app.clone();
    let (plan, labels) = {
        let Some(mut guard) = app.downcast_mut::<Monitor>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let first = s.model.readings == 0;
        for notice in &snapshot.notices {
            println!("AZMON_NOTICE {notice}");
        }
        if !snapshot.notices.is_empty() {
            s.notice.clear();
        }
        s.model.apply(snapshot);
        s.note_reading(std::time::Instant::now());
        // The table's view stays as the table shows it: the build this
        // reading asks for (none while the hand is on the table) carries it
        // over to the new rows (`table::sync`).
        let hands_on = s.hands_on_table();
        if first {
            println!("AZMON_READY {}", s.model.process_count());
        }
        println!(
            "AZMON_TICK {} {} {}",
            s.model.readings,
            s.model.process_count(),
            s.model.shown_count()
        );
        if let Some(top) = s.model.shown_row(0) {
            println!("AZMON_TOP {} {}", top.pid, top.name);
        }
        let settings = kit::settings_open(&s.kit);
        let plan = ticks::plan(first, s.screen, settings, hands_on);
        ensure_frames(&handle, s, &mut info);
        (plan, ui::status_labels(s))
    };
    if !plan.refresh_dom {
        // The view's new drawing is shifted to where the scroll stands
        // (`graph::Scroll::lag`): every old point where the last one had it.
        rerender(&mut info, &plan.views);
        if plan.status {
            for (marker, label) in labels {
                relabel(&mut info, marker, label);
            }
        }
    }
    if plan.refresh_dom {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// Shifts the strips of the graphs shown (`count` of them): strip `i` by
/// `px(i)` px to the right of where it is laid out.
///
/// The strips live in the view's own DOM, and only a rebuild of THAT DOM's
/// display list publishes a new matrix (the override channel alone marks
/// the window's display list dirty, which rebuilds DOM 0's only). So every
/// strip but the last takes the cheap override, and the last one
/// `set_css_property`, which rebuilds the view's display list once - with
/// every override of this frame in it. The strip is drawn with its
/// `translateX`: it has its reference frame from the start, a shift moves
/// no box.
fn shift_strips(info: &mut CallbackInfo, count: usize, px: impl Fn(usize) -> f32) {
    for i in 0..count {
        let Some(node) = info
            .get_node_id_by_marker(graph::strip_marker(i))
            .into_option()
        else {
            continue;
        };
        if i + 1 < count {
            info.override_css_property(node, graph::shift(px(i)));
        } else {
            info.set_css_property(node, graph::shift(px(i)));
        }
    }
}

/// Starts the frame timer (if it does not run) while a tab with graphs shows
/// and the monitor is not paused: the graphs scroll between readings.
pub fn ensure_frames(app: &RefAny, s: &mut Monitor, info: &mut CallbackInfo) {
    if s.animating || !s.screen.has_graphs() || s.interval_ms == 0 {
        return;
    }
    s.animating = true;
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_frame, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(FRAME_MS))),
    );
}

/// A frame of the graphs' scroll: every strip of the drawing shown shifted
/// to where the scroll stands now (its newest reading `lag` steps right of
/// the edge). The drawing shown is the one of `Monitor::drawn` readings - a
/// reading's new drawing may not be built yet, and the old one is shifted
/// as the old one. Ends itself when no tab with graphs shows (or the monitor
/// is paused, or the settings cover it).
extern "C" fn on_frame(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some((steps, lag, keep)) = data.downcast_mut::<Monitor>().map(|mut s| {
        let keep =
            s.screen.has_graphs() && s.interval_ms > 0 && !kit::settings_open(&s.kit);
        if !keep {
            s.animating = false;
        }
        let lag = s.graph_lag(s.clock_ms(), s.drawn);
        // A scroll that stands (it waits for a late reading) moves nothing.
        let moved = (lag - s.shifted).abs() > 1e-6;
        s.shifted = lag;
        let steps = if moved { s.strips.clone() } else { Vec::new() };
        (steps, lag, keep)
    }) else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    if !keep {
        return TimerCallbackReturn::terminate_unchanged();
    }
    #[allow(clippy::cast_possible_truncation)] // a few steps
    let lag = lag as f32;
    shift_strips(&mut info.callback_info, steps.len(), |i| {
        steps.get(i).map_or(0.0, |step| lag * step)
    });
    TimerCallbackReturn::continue_unchanged()
}

/// Re-renders the live views `views` in place (their VirtualViews, found by
/// marker): no `layout()`, no relayout of the page.
pub fn rerender(info: &mut CallbackInfo, views: &[LiveView]) {
    for view in views {
        let marker = ui::marker_of(*view);
        let Some(node) = info.get_node_id_by_marker(marker).into_option() else {
            continue;
        };
        // `into_raw` is the 1-based encoding (0 = none); `NodeId` is 0-based.
        let raw = node.node.into_raw();
        if raw != 0 {
            info.trigger_virtual_view_rerender(node.dom, NodeId { inner: raw - 1 });
        }
    }
}

/// Rewrites the status bar's label marked `marker` in place.
fn relabel(info: &mut CallbackInfo, marker: AzString, label: String) {
    if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
        let _ = StatusBar::update_segment_label(*info, node, label);
    }
}

/// The selected process, for the scripts.
pub fn print_selected(s: &Monitor) {
    match s.model.selected_row() {
        Some(r) => println!("AZMON_SELECT {} {}", r.pid, r.name),
        None => println!("AZMON_SELECT none"),
    }
}

/// Asks the sampler to end process `pid` (`force`: kill it).
pub fn end_process(s: &mut Monitor, pid: u32, force: bool) {
    s.shared.ask(Command::End { pid, force });
    println!("AZMON_END {pid} {force}");
}

/// Sets the update speed: the sampler follows at once, the setting is kept.
pub fn set_speed(s: &mut Monitor, info: &mut CallbackInfo, interval_ms: u64) {
    s.interval_ms = interval_ms;
    s.shared.interval_ms.store(interval_ms, Ordering::Relaxed);
    s.shared.ask(Command::ReadNow);
    kit::set_value(&s.kit, info, SPEED_KEY, &interval_ms.to_string());
    println!("AZMON_SPEED {interval_ms}");
}

/// Cancel on the settings page put the settings back: the sampler follows the speed they name.
fn reload_settings(app: &mut RefAny, _info: &mut CallbackInfo, settings: &AppSettings) {
    let interval_ms = speed_from_setting(settings.get(SPEED_KEY));
    if let Some(mut s) = app.downcast_mut::<Monitor>() {
        if s.interval_ms != interval_ms {
            s.interval_ms = interval_ms;
            s.shared.interval_ms.store(interval_ms, Ordering::Relaxed);
            s.shared.ask(Command::ReadNow);
            println!("AZMON_SPEED {interval_ms}");
        }
    };
}

// ==== The window ====

/// The window: the RecordsShell (title row, tab row, the tab's page, status
/// bar) in the theme scope; the question over it while it is open; the
/// kit's and the app's keys on the window.
extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let window = (info.get_window_width(), info.get_window_height());
    let app = data.clone();
    if let Some(mut s) = data.downcast_mut::<Monitor>() {
        s.layouts += 1;
        println!("AZMON_LAYOUT {}", s.layouts);
        if window.0 > 0.0 && window.1 > 0.0 {
            s.window = window;
        }
    }
    let Some(guard) = data.downcast_ref::<Monitor>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let shell = if kit::settings_open(&s.kit) {
        RecordsShell::create(
            Dom::create_div(),
            kit::settings_page_with_reload(
                &s.kit,
                ui::settings_sections(s, &app),
                &app,
                reload_settings,
            ),
        )
    } else if s.model.readings == 0 {
        RecordsShell::create(ui::tools(s, &app), ui::waiting(s))
    } else {
        let page = match s.screen {
            Screen::Processes => ui::process_page(s, &app),
            other => ui::live_view(&app, other.view()),
        };
        RecordsShell::create(ui::tools(s, &app), page)
    };
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(
            shell
                .office_shell()
                .with_title_row(kit::title_row(SPEC.name))
                .with_status_bar(ui::status_bar(s))
                .dom(),
        );
    if let Some(question) = s.confirm.as_ref() {
        column.add_child(ui::confirm_dom(question, &app));
    }
    // The scope as the window's body: no UA margin, the full window height.
    ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Slate)
        .body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app,
            on_key,
        )
}

/// Whether the keyboard focus is in the page itself (the filter field, a
/// button) rather than in a live view (the table): Delete belongs to a text
/// field there.
fn focus_in_page(info: &CallbackInfo) -> bool {
    info.get_focused_node()
        .into_option()
        .is_some_and(|n| n.dom == DomId { inner: 0 })
}

/// The kit's keys (Mod+, settings, F1 shortcuts, Escape), then the app's:
/// Delete asks to end the selected process, F5 reads now, Escape closes the
/// question.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<Monitor>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let asking = data
        .downcast_ref::<Monitor>()
        .is_some_and(|s| s.confirm.is_some());
    if asking && matches!(key, Some(VirtualKeyCode::Escape)) {
        if let Some(mut s) = data.downcast_mut::<Monitor>() {
            s.confirm = None;
        }
        info.prevent_default();
        return Update::RefreshDom;
    }
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    match key {
        Some(VirtualKeyCode::Delete) if !asking && !focus_in_page(&info) => {
            let Some(mut s) = data.downcast_mut::<Monitor>() else {
                return Update::DoNothing;
            };
            if ui::ask_to_end(&mut s) {
                info.prevent_default();
                Update::RefreshDom
            } else {
                Update::DoNothing
            }
        }
        Some(VirtualKeyCode::F5) => {
            if let Some(s) = data.downcast_ref::<Monitor>() {
                s.shared.ask(Command::ReadNow);
            }
            Update::DoNothing
        }
        _ => Update::DoNothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Column;

    #[test]
    fn the_update_speed_comes_from_the_settings_or_is_one_second() {
        assert_eq!(speed_from_setting(None), 1000);
        assert_eq!(speed_from_setting(Some("2000")), 2000);
        assert_eq!(speed_from_setting(Some("0")), 0);
        assert_eq!(speed_from_setting(Some(" 500 ")), 500);
        // Not one of the offered speeds, or not a number: the default.
        assert_eq!(speed_from_setting(Some("1234")), 1000);
        assert_eq!(speed_from_setting(Some("fast")), 1000);
    }

    #[test]
    fn each_speed_has_its_place_and_its_words() {
        assert_eq!(speed_index(500), 0);
        assert_eq!(speed_index(1000), 1);
        assert_eq!(speed_index(0), 3);
        assert_eq!(speed_index(7), 1);
        assert_eq!(speed_text(1000), "Updated every 1 s");
        assert_eq!(speed_text(500), "Updated every 0.5 s");
        assert_eq!(speed_text(0), "Paused");
    }

    #[test]
    fn the_sort_reads_as_its_columns_and_directions() {
        assert_eq!(sort_text(&[]), "PID");
        assert_eq!(
            sort_text(&[
                SortKey::new(Column::Cpu, true),
                SortKey::new(Column::Name, false)
            ]),
            "CPU desc, Image Name asc"
        );
    }

    #[test]
    fn the_graphs_learn_the_real_time_between_readings() {
        // The first gap is taken as it is; later ones are averaged in.
        assert!((next_gap(0.0, 1040.0, 1000.0) - 1040.0).abs() < 0.001);
        assert!((next_gap(1000.0, 1100.0, 1000.0) - 1030.0).abs() < 0.001);
        // A reading asked for at once, or a stall, counts as half / three
        // times the update speed at most.
        assert!((next_gap(1000.0, 10.0, 1000.0) - 850.0).abs() < 0.001);
        assert!((next_gap(1000.0, 60_000.0, 1000.0) - 1600.0).abs() < 0.001);
        // Paused: nothing to learn.
        assert!((next_gap(1000.0, 5000.0, 0.0) - 1000.0).abs() < 0.001);
    }

    #[test]
    fn every_tab_is_a_screen_switch() {
        for screen in Screen::ALL {
            assert!(SCREENS.contains(&screen.name()), "{screen:?}");
        }
        assert!(SCREENS.contains(&"settings"));
    }
}
