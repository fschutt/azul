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

/// Amounts in integer minor units.
pub mod money;
/// The records (asset, category, location, maintenance entry, check-out)
/// and their files.
pub mod model;
/// Straight-line and declining-balance schedules, the book value on a day.
pub mod depreciation;
/// The data tree (`erp/<kind>/<uuid>.json`) and the records in memory.
pub mod store;
/// CSV export of the register and a schedule; CSV import with a mapping.
pub mod csv_io;
/// Totals by category and location, the depreciation forecast,
/// maintenance due, overdue check-outs.
pub mod reports;
/// The view-JSON interpreter (first slice): the ERP's view dialect, routing,
/// labels, view -> columns / fields / tabs / steps, records by field name.
pub mod views;
/// The `--sample` register.
pub mod sample;
/// The app's state without a window: navigation, forms, what changes write.
pub mod app;
