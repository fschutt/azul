//! AzDashboard: a records dashboard on the public azul API - the answer to
//! "sortable / filterable tables + charts + an editable grid over 500k x 25
//! Excel rows, on Windows / Mac / Linux".
//!
//! The window is azul's S6 `RecordsShell` (records and dashboards): the
//! app-drawn title row (`NoTitle` + `Titlebar`), a tool row, the CHARTS
//! strip over the TABLE, a status bar with the row counts. The orders
//! ([`data`]: 500,000 x 25, deterministic) are generated on an azul
//! `Thread` when the window opens; until they arrive the table pane says so.
//!
//! The table half ([`table`]) is azul's `DataTable`: only the rows in view
//! are built, a header click sorts (Shift+click adds a second key), the
//! filter row filters (contains / =equals / ranges like `10..20`, `>5`,
//! `2024-01-01..2024-03-31`), F2 / Enter / a double-click edits a cell (the
//! app validates: [`data::DataSet::edit`]), the arrows, Page Up / Down,
//! Home / End and Ctrl+Home / Ctrl+End move, the column edges resize. The
//! sorting and filtering of 500,000 rows runs off the UI thread (the
//! widget's own job: the keys in slices on a timer, the sort on a Thread).
//!
//! The chart half (`chart`, CHART7) adds its module and fills the marked
//! block in [`layout`].
//!
//! Switches (azul-appkit): `--screen orders|settings`, `--theme flat|flora`,
//! `--mode light|dark|system`, `--size WxH`, `--shot <png>`. The number of
//! orders is [`data::ROWS`] unless `AZDASHBOARD_ROWS` says otherwise.
//!
//! On stdout, for scripts (`scripts/azdashboard_e2e.py`):
//! `AZDASH_READY <rows>` when the orders are in, `AZDASH_SHOWN <shown>
//! <rows>` after every sort / filter, `AZDASH_SORT <text>`, `AZDASH_TOP
//! <first shown row> <its order id>` after a scroll, `AZDASH_EDIT <order>
//! <column> <text>` after an accepted edit, `AZDASH_REFUSED <reason>` after
//! a refused one.

pub mod data;
/// The charts over the table (a line and a bar chart of the rows the table
/// shows): azul's Chart widget (the chart half).
pub mod chart;
/// The DOM ids and classes (`__azdash_` prefix), each defined once.
pub mod ids;
/// The orders table: azul's DataTable over the data set (the table half).
pub mod table;

use azul::{
    callbacks::WriteBackCallbackType,
    prelude::*,
    shells::{RecordsShell, ShellThemeAccent, ShellThemeScope},
    widgets::DataTableView,
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    shortcuts::Shortcut,
    ui as kit,
};

// ==== The app's facts ====

/// The screens `--screen` opens.
pub const SCREENS: [&str; 2] = ["orders", "settings"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzDashboard",
    binary: "AzDashboard",
    summary: "a records dashboard: 500,000 orders in a sortable, filterable, editable table, and charts",
    screens: &SCREENS,
    files_help: "",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzDashboard",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Half a million orders in azul's virtualized DataTable - sorted, filtered and \
              edited without blocking the window - and charts of the same data. Part of the \
              Azlin apps, built with azul.",
    license: "MIT",
    app_folder: "dashboard",
};

/// The keyboard shortcuts the settings page lists (the table's own keys).
pub const SHORTCUTS: [Shortcut; 12] = [
    Shortcut::new("Table", "Up  Down  Left  Right", "Move the cell cursor"),
    Shortcut::new("Table", "Shift+Up  Shift+Down", "Extend the row selection"),
    Shortcut::new("Table", "Page Up  Page Down", "Move by a screen of rows"),
    Shortcut::new("Table", "Home  End", "The first / last column"),
    Shortcut::new("Table", "Mod+Home  Mod+End", "The first / last row"),
    Shortcut::new("Table", "Mod+A", "Select every row shown"),
    Shortcut::new("Table", "Mod+C", "Copy the selected rows"),
    Shortcut::new("Edit", "F2  Enter", "Edit the cell"),
    Shortcut::new("Edit", "Enter  Tab", "Keep the edit (Tab moves right)"),
    Shortcut::new("Edit", "Escape", "Cancel the edit"),
    Shortcut::new("Sort", "Click  Shift+Click", "Sort by a column, add a second key"),
    Shortcut::new("Filter", "Mod+F", "Filter the cursor's column"),
];

/// The settings page's own categories (none: the kit's are enough).
const APP_CATEGORIES: [&str; 0] = [];

/// The orders generated unless `AZDASHBOARD_ROWS` says otherwise.
#[must_use]
pub fn rows_wanted() -> u32 {
    std::env::var("AZDASHBOARD_ROWS")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(data::ROWS)
}

// ==== State ====

/// The app's state.
pub struct Dashboard {
    /// The appkit kit (settings, data root, the settings page's state).
    pub kit: RefAny,
    /// The orders (a [`data::DataSet`]) once generated: the table's data
    /// source and the charts read it. `None` while the Thread runs.
    pub source: Option<RefAny>,
    /// How many orders are generated.
    pub rows: u32,
    /// The table's state (the DataTable's view, the last notice).
    pub table: table::TableState,
    /// The window's size (the table's viewport follows it).
    pub window: (f32, f32),
    /// The chart half's own state (CHART7 decides what it holds).
    pub chart: Option<RefAny>,
}

impl Dashboard {
    /// A dashboard waiting for its orders.
    #[must_use]
    pub fn new(kit: RefAny, args: &AppArgs) -> Self {
        Self {
            kit,
            source: None,
            rows: rows_wanted(),
            table: table::TableState::default(),
            window: args.size.unwrap_or((1280.0, 800.0)),
            // The bars group the shown orders by category; line and bars sum the sales.
            chart: Some(RefAny::new(chart::Charts::new(data::c::CATEGORY, data::c::SALES))),
        }
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
    let app = Dashboard::new(kit_ref.clone(), &args);
    let config = kit::app_config(&kit_ref);
    let window = kit::window_options(&kit_ref, layout, (1280.0, 800.0), (640.0, 420.0), on_window_created);
    App::create(RefAny::new(app), config).run(window);
}

// ==== The orders, generated on a Thread ====

/// What the generating Thread is handed.
struct GenerateInit {
    rows: u32,
    on_done: WriteBackCallbackType,
}

/// What it hands back.
struct Generated {
    data: Option<data::DataSet>,
}

/// Runs on the worker thread: the orders, then back to the UI thread.
extern "C" fn generate_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((rows, on_done)) = init.downcast_ref::<GenerateInit>().map(|i| (i.rows, i.on_done)) else {
        return;
    };
    let set = data::DataSet::generate(rows);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Generated { data: Some(set) }),
    )));
}

/// The orders are in: they become the table's data source.
extern "C" fn on_generated(mut app: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(set) = msg.downcast_mut::<Generated>().and_then(|mut g| g.data.take()) else {
        return Update::DoNothing;
    };
    let Some(mut s) = app.downcast_mut::<Dashboard>() else {
        return Update::DoNothing;
    };
    let rows = set.rows();
    s.rows = rows;
    s.source = Some(RefAny::new(set));
    s.table.reset(rows);
    println!("AZDASH_READY {rows}");
    println!("AZDASH_SHOWN {rows} {rows}");
    Update::RefreshDom
}

/// The window is up: the `--shot` timer, then the orders on a Thread.
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some((kit_ref, rows)) = data.downcast_ref::<Dashboard>().map(|s| (s.kit.clone(), s.rows)) else {
        return Update::DoNothing;
    };
    kit::on_window_created(&kit_ref, &mut info);
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(GenerateInit {
                rows,
                on_done: on_generated,
            }),
            app,
            generate_thread,
        ),
    );
    Update::DoNothing
}

// ==== The charts' view of the table ====

/// The rows the table shows, in its order (after its filter and sort), as the
/// charts read them.
struct Shown<'a> {
    set: &'a data::DataSet,
    view: &'a DataTableView,
    rows: u32,
}

impl Shown<'_> {
    /// The order at shown position `position`.
    fn row(&self, position: usize) -> Option<u32> {
        let position = u32::try_from(position).ok()?;
        self.view.row_at(position, self.rows).into_option()
    }
}

impl chart::ChartSource for Shown<'_> {
    fn shown_rows(&self) -> usize {
        self.view.shown_count(self.rows) as usize
    }
    fn text(&self, row: usize, column: usize) -> &str {
        self.row(row)
            .map_or("", |r| self.set.category_text(r, column))
    }
    fn number(&self, row: usize, column: usize) -> Option<f64> {
        let value = self.set.value(self.row(row)?, column);
        value.is_finite().then_some(value)
    }
    fn column_name(&self, column: usize) -> &str {
        data::COLUMNS.get(column).map_or("", |c| c.title)
    }
    fn generation(&self) -> u64 {
        // A new order (filter / sort landed) or an edit changes what is shown.
        (u64::from(self.view.order_serial) << 32) | u64::from(self.set.edits)
    }
}

// ==== The window ====

/// The window: the RecordsShell (title row, tool row, charts over the
/// table, status bar) in the theme scope, the kit's keys on the window.
extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let window = (info.get_window_width(), info.get_window_height());
    let app = data.clone();
    if let Some(mut s) = data.downcast_mut::<Dashboard>() {
        if window.0 > 0.0 && window.1 > 0.0 {
            s.window = window;
        }
    }
    let Some(guard) = data.downcast_ref::<Dashboard>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let settings = kit::settings_open(&s.kit);

    // ==== CHART7: the charts (the RecordsShell's cards strip over the table) ====
    // Built from the rows the table shows (its filter and sort) once the orders
    // are in; `table::view` gets the strip's height so its viewport is right.
    let mut cards: Option<Dom> = None;
    if let (Some(charts), Some(source)) = (s.chart.as_ref(), s.source.as_ref()) {
        let mut source = source.clone();
        let strip = match source.downcast_ref::<data::DataSet>() {
            Some(set) => {
                let shown = Shown {
                    set: &set,
                    view: &s.table.view,
                    rows: set.rows(),
                };
                Some(chart::charts_dom(charts, &shown, s.window.0))
            }
            None => None,
        };
        cards = strip;
    }
    let charts_height: f32 = if cards.is_some() {
        chart::strip_height(s.window.0)
    } else {
        0.0
    };
    // ==== /CHART7 ====

    let shell = if settings {
        RecordsShell::create(Dom::create_div(), kit::settings_page(&s.kit, Vec::new()))
    } else {
        let mut shell = RecordsShell::create(table::tools(s, &app), table::view(s, &app, charts_height));
        if let Some(c) = cards {
            shell = shell.with_cards(
                Dom::create_div()
                    .with_id(ids::CHARTS)
                    .with_child(c),
            );
        }
        shell
    };
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(
            shell
                .office_shell()
                .with_title_row(kit::title_row(SPEC.name))
                .with_status_bar(table::status_bar(s))
                .dom(),
        );
    // The scope as the window's body: no UA margin, the full window height.
    ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Leaf)
        .body()
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_key)
}

/// The kit's keys (Mod+, settings, F1 shortcuts, Escape); the table's own
/// keys are the table's (it is focused).
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<Dashboard>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    kit::handle_key(&kit_ref, &mut info).unwrap_or(Update::DoNothing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dashboard_shows_half_a_million_orders_unless_told_otherwise() {
        assert_eq!(data::ROWS, 500_000);
        assert_eq!(data::COLUMNS.len(), 25);
    }

    #[test]
    fn every_class_and_id_of_the_dashboard_carries_the_one_azdash_prefix() {
        // The second, longer prefix, spelled in halves so the test does not find itself.
        let second_prefix = ["__azdash", "board_"].concat();
        let sources = [
            ("chart.rs", include_str!("chart.rs")),
            ("data.rs", include_str!("data.rs")),
            ("ids.rs", include_str!("ids.rs")),
            ("lib.rs", include_str!("lib.rs")),
            ("table.rs", include_str!("table.rs")),
        ];
        for (file, source) in sources {
            assert!(
                !source.contains(second_prefix.as_str()),
                "{file} names a class or an id with {second_prefix}, not {}",
                ids::PREFIX
            );
        }
    }
}
