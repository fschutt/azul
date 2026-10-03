//! AzMonitor: the system monitor (Task Manager, Activity Monitor, htop) on
//! the public azul API.
//!
//! The window is azul's S6 `RecordsShell` (records and dashboards): the
//! app-drawn title row (`NoTitle` + `Titlebar`), the tab row (Processes,
//! Performance) with the filter and "End process", the CARDS strip (CPU,
//! memory, disk, network - a value and a minute of history each) over the
//! process TABLE (azul's `DataTable`), a status bar. The Performance tab is
//! a page of charts (azul's `Chart`): CPU, memory, disk, network over the
//! last minute, every core's usage, the machine's figures.
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
//! callbacks read the model (the cards, the table, the performance page).
//! A reading re-renders the live views the screen shows
//! (`trigger_virtual_view_rerender`) and rewrites the status bar's marked
//! labels (`StatusBar::update_segment_label`); `layout()` does not run.
//! Only the first reading (the empty state gives way to the page) and what
//! the user does (a tab, a sort, a selection, the question) rebuild it.
//!
//! END PROCESS: select a row, then "End process" (or Delete): a question
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
//! Switches (azul-appkit): `--screen processes|performance|settings`,
//! `--theme flat|flora`, `--mode light|dark|system`, `--size WxH`,
//! `--shot <png>`, `--sample` (the sample machine), `--data-dir <dir>`.
//!
//! On stdout, for scripts (`scripts/azmonitor_e2e.py`): `AZMON_LAYOUT <n>`
//! every time `layout()` runs, `AZMON_READY <processes>` at the first
//! reading, `AZMON_TICK <readings> <processes> <shown>` at every reading,
//! `AZMON_TOP <pid> <name>` (the first row), `AZMON_SORT <text>`,
//! `AZMON_SHOWN <shown>` after a filter, `AZMON_SELECT <pid> <name>`,
//! `AZMON_ASK <pid> <name>` (the question opens), `AZMON_END <pid> <force>`,
//! `AZMON_NOTICE <text>`, `AZMON_SCREEN <name>`, `AZMON_SPEED <ms>`,
//! `AZMON_EXPORTED <key>`.

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
    callbacks::WriteBackCallbackType,
    dom::{DomId, NodeId, VirtualKeyCode},
    prelude::*,
    shells::{RecordsShell, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    task::{Thread, ThreadId},
    widgets::{DataTableView, StatusBar},
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
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
pub const SCREENS: [&str; 3] = ["processes", "performance", "settings"];

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
    let _ = value;
    todo!("GREEN: speed_from_setting")
}

/// The place of `interval_ms` among [`SPEEDS`] (the default's when it is
/// none of them).
#[must_use]
pub fn speed_index(interval_ms: u64) -> usize {
    let _ = interval_ms;
    todo!("GREEN: speed_index")
}

/// The status bar's word on the update speed: "Updated every 1 s", "Paused".
#[must_use]
pub fn speed_text(interval_ms: u64) -> String {
    let _ = interval_ms;
    todo!("GREEN: speed_text")
}

/// What the sort is, for the scripts: "CPU desc, Name asc"; "PID" for none.
#[must_use]
pub fn sort_text(keys: &[SortKey]) -> String {
    let _ = keys;
    todo!("GREEN: sort_text")
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
    /// The process table's view (scroll, selection, the header's arrows).
    pub table: DataTableView,
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
}

impl Monitor {
    /// A monitor waiting for its first reading.
    #[must_use]
    pub fn new(kit: RefAny, args: &AppArgs, interval_ms: u64) -> Self {
        let screen = match args.screen.as_deref() {
            Some("performance") => Screen::Performance,
            _ => Screen::Processes,
        };
        let model = Model::new();
        let table = table::view_for(model.sort());
        Self {
            kit,
            model,
            table,
            screen,
            sample: args.sample,
            shared: Arc::new(Shared::new(interval_ms)),
            interval_ms,
            window: args.size.unwrap_or((1200.0, 780.0)),
            confirm: None,
            layouts: 0,
            notice: String::new(),
        }
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

/// A reading arrived: into the model, then the tick - the live views of the
/// screen re-render in place, the status labels are rewritten; the page is
/// built only for the first reading.
pub extern "C" fn on_reading(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(snapshot) = msg
        .downcast_mut::<Reading>()
        .and_then(|mut r| r.snapshot.take())
    else {
        return Update::DoNothing;
    };
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
        let selected = s.model.selected_position();
        let shown = s.model.shown_count();
        table::follow_selection(&mut s.table, selected, shown);
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
        (ticks::plan(first, s.screen, settings), ui::status_labels(s))
    };
    if !plan.refresh_dom {
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

// ==== The window ====

/// The window: the RecordsShell (title row, tab row, cards over the table -
/// or the performance page -, status bar) in the theme scope; the question
/// over it while it is open; the kit's and the app's keys on the window.
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
            kit::settings_page(&s.kit, ui::settings_sections(s, &app)),
        )
    } else if s.model.readings == 0 {
        RecordsShell::create(ui::tools(s, &app), ui::waiting(s))
    } else {
        match s.screen {
            Screen::Processes => {
                RecordsShell::create(ui::tools(s, &app), ui::live_view(&app, LiveView::Table))
                    .with_cards(ui::live_view(&app, LiveView::Cards))
            }
            Screen::Performance => RecordsShell::create(
                ui::tools(s, &app),
                ui::live_view(&app, LiveView::Performance),
            ),
        }
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
            "CPU desc, Name asc"
        );
    }
}
