//! AzERP: the ERP on azul. Its first section is asset management (the ERP
//! README calls it "the best first one": 4 views, 11 endpoints).
//!
//! The screens are INTERPRETED from the ERP's view JSON, the way its web
//! frontend does it (`../erp/json/ui/ui.*.json`, README s1 "the UI is
//! interpreted at run time, not generated"): [`views`] holds the serde
//! models of `table` / `form` / `detail` / `wizard` views and turns a view
//! into DataTable columns, form fields, detail tabs and wizard steps. The
//! asset views are a corrected copy of `ui.assets.json` (the required
//! columns the ERP's form left out, both depreciation methods, location and
//! serial number, the maintenance and check-out views no ERP view defines
//! yet). The named panels (`DepreciationSchedulePanel`, ...) are built by
//! hand, as the README says.
//!
//! The model is plain Rust:
//! - [`money`]: amounts in integer minor units (never floats), parsed and
//!   formatted.
//! - [`model`]: the records (asset, category, location, maintenance entry,
//!   check-out) and their files.
//! - [`depreciation`]: straight-line and declining-balance schedules,
//!   monthly pro rata, the book value on a day.
//! - [`store`]: the data tree (`erp/<kind>/<uuid>.json`, one file per
//!   record) and loading it.
//! - [`csv_io`]: CSV export of the register and the schedule, CSV import
//!   with a column mapping.
//! - [`reports`]: totals by category and location, depreciation per year,
//!   maintenance due, overdue check-outs.
//!
//! The window is azul's S6 `RecordsShell`: the title row, the section tabs
//! (Register, Categories, Locations, Maintenance, Check-outs, Reports), the
//! record table (azul's DataTable), the record form in the side pane, the
//! status bar. Records are read on an azul `Thread` at start and written
//! through azul-pim's write queue as azul-appkit file jobs.

/// The app's state without a window: navigation, forms, what changes write.
pub mod app;
/// CSV export of the register and a schedule; CSV import with a mapping.
pub mod csv_io;
/// Straight-line and declining-balance schedules, the book value on a day.
pub mod depreciation;
/// The DOM ids and classes (`__azerp_` prefix), each defined once.
pub mod ids;
/// The records (asset, category, location, maintenance entry, check-out)
/// and their files.
pub mod model;
/// Amounts in integer minor units.
pub mod money;
/// Totals by category and location, the depreciation forecast,
/// maintenance due, overdue check-outs.
pub mod reports;
/// The `--sample` register.
pub mod sample;
/// The data tree (`erp/<kind>/<uuid>.json`) and the records in memory.
pub mod store;
/// The window: the RecordsShell around the interpreted views.
pub mod ui;
/// The view-JSON interpreter (first slice): the ERP's view dialect, routing,
/// labels, view -> columns / fields / tabs / steps, records by field name.
pub mod views;

use azul::prelude::*;
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    shortcuts::Shortcut,
    ui as kit,
};
use chrono::NaiveDate;

// ==== The app's facts ====

/// The screens `--screen` opens, and the paths they are.
pub const SCREENS: [&str; 9] = [
    "register",
    "categories",
    "locations",
    "maintenance",
    "checkouts",
    "reports",
    "import",
    "run",
    "settings",
];

/// The path a `--screen` opens.
#[must_use]
pub fn screen_path(screen: &str) -> &'static str {
    match screen {
        "categories" => "/assets/categories",
        "locations" => "/assets/locations",
        "maintenance" => "/assets/maintenance",
        "checkouts" => "/assets/checkouts",
        "reports" => "/assets/reports",
        "import" => app::IMPORT,
        "run" => "/assets/depreciation-runs/new",
        _ => app::HOME,
    }
}

pub const SPEC: AppSpec = AppSpec {
    name: "AzERP",
    binary: "AzERP",
    summary: "asset management: the register, depreciation, maintenance, check-out / check-in, CSV, reports",
    screens: &SCREENS,
    files_help: "a .csv file of assets to import",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzERP",
    version: env!("CARGO_PKG_VERSION"),
    summary: "The ERP's asset management on azul: fixed assets with straight-line and declining-balance \
              depreciation, categories, locations, a maintenance log, check-out and check-in, CSV import \
              and export, and reports. The screens come from the ERP's view definitions; every record is \
              a file in your data folder. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: store::APP_FOLDER,
};

/// The keyboard shortcuts the settings page lists.
pub const SHORTCUTS: [Shortcut; 6] = [
    Shortcut::new(
        "Register",
        "Up  Down  Page Up  Page Down",
        "Move through the assets",
    ),
    Shortcut::new("Register", "Enter  Double-click", "Open the asset"),
    Shortcut::new(
        "Register",
        "Click  Shift+Click",
        "Sort by a column, add a second key",
    ),
    Shortcut::new("Register", "Mod+F", "Filter the cursor's column"),
    Shortcut::new("Form", "Escape", "Close the form without saving"),
    Shortcut::new("Register", "Mod+C", "Copy the selected rows"),
];

/// The settings page's own categories (none: the kit's are enough).
const APP_CATEGORIES: [&str; 0] = [];

/// The day the app works on: `AZERP_TODAY` (`YYYY-MM-DD`, for scripts that
/// need the same book values every run), else today.
#[must_use]
pub fn today() -> NaiveDate {
    std::env::var("AZERP_TODAY")
        .ok()
        .and_then(|d| model::parse_date(&d))
        .unwrap_or_else(|| chrono::Local::now().date_naive())
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
    let mut state = app::State::new(today());
    match args.screen.as_deref() {
        Some("settings") => kit::open_settings(&kit_ref, None),
        Some(screen) => state.open(screen_path(screen)),
        None => {}
    }
    let window_size = args.size.unwrap_or((1280.0, 800.0));
    let mut erp = ui::Erp::new(kit_ref.clone(), state, args.sample, window_size);
    // A CSV file named on the command line: read now (it is outside the data
    // tree), imported once the records are in.
    if let Some(path) = args
        .files
        .iter()
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv")))
    {
        match azul_appkit::files::read_outside(path) {
            Ok(bytes) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                erp.pending_import = Some((name, String::from_utf8_lossy(&bytes).into_owned()));
            }
            Err(e) => eprintln!("[AzERP] {e}"),
        }
    }
    let config = kit::app_config(&kit_ref);
    let window = kit::window_options(
        &kit_ref,
        ui::layout,
        (1280.0, 800.0),
        (800.0, 520.0),
        on_window_created,
    );
    App::create(RefAny::new(erp), config).run(window);
}

/// The window is up: the `--shot` timer, then the records on the file thread.
extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some((kit_ref, root)) = data
        .downcast_ref::<ui::Erp>()
        .map(|s| (s.kit.clone(), s.data_root()))
    else {
        return Update::DoNothing;
    };
    kit::on_window_created(&kit_ref, &mut info);
    ui::spawn_load(&mut info, &app, &root);
    Update::DoNothing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_screen_opens_a_page_the_views_route() {
        let views = views::ViewFile::assets();
        for screen in SCREENS.iter().filter(|s| **s != "settings") {
            let path = screen_path(screen);
            assert!(
                views.route(path).is_some(),
                "--screen {screen} opens {path}"
            );
        }
        assert_eq!(ABOUT.app_folder, "erp");
    }
}
