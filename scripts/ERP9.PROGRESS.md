# ERP9 progress - AzERP asset management (wave 9)

Branch `wt/erp9` from `e537ddbe2`. Brief: scripts/waves/wave9/PLAN.md "ERP9"; planning
../azul-apps/planning/other/erp/{README,asset-management}.md; the view JSON ../erp/json/ui/ui.assets.json.
Crate: examples/azul-erp (package + bin AzERP, lib azerp). NEVER compile; parse-check with
`rustfmt --edition 2021 <file>` (look for "error" in its output BEFORE committing).

## Decisions (made unattended)
- D1 The view-JSON interpreter IS the intended architecture (ERP README s1 "the UI is interpreted at run time,
  not generated", s3 "erp-views ... view -> azul Dom interpreter", asset-management "a good first module for the
  native interpreter"). This task builds its first slice inside the app crate (`src/views/`): serde models of the
  view JSON (table / form / detail / wizard), the asset views as a corrected copy of `ui.assets.json` embedded with
  `include_str!`, and the interpreter that turns a view into DataTable columns / form fields / detail tabs. Named
  panels (DepreciationSchedulePanel, ...) are hand-built, as the README says.
- D2 One crate `examples/azul-erp` (package + bin `AzERP`, lib `azerp`, prefix `__azerp_`, data folder `erp/`):
  the README's one AzERP binary with sections; Assets is the first section.
- D3 Money = integer minor units (`i64` cents), never floats (MoneyInput rule; README: "money is f64" is a bug).
  No `rust_decimal` (not in Cargo.lock); depreciation arithmetic in i128 with half-up rounding, the last period
  takes the remainder so a schedule always ends exactly at the residual value.
- D4 Records: one JSON file per record, `erp/assets/<uuid>.json`, `erp/categories/<uuid>.json`,
  `erp/locations/<uuid>.json`, `erp/maintenance/<uuid>.json`, `erp/checkouts/<uuid>.json`; exports
  `erp/exports/<name>.csv` (user ruling: exports go INTO the data tree). Each file has `format` + `version` like
  the aztasks files; ERP schema field names; money as decimal strings ("1596.64"). Writes go through azul-pim's
  `WriteQueue` (one batch in flight) run as azul-appkit `FileJob`s on appkit's file thread; the load is one
  `FileJob::GetAll` of `erp/`.
- D5 CSV through the `csv` 1.4 crate (already in Cargo.lock via AzSheets) - no third hand-written CSV parser
  (AzContacts has its own RFC 4180 reader: named as a twin in the report).
- D6 The depreciation run's "post" writes the run's journal as CSV into erp/exports/ (no ledger exists yet).
- D7 erp.ui.assets.json = the ERP repo's file unchanged (the user's own private repo) as the dialect test fixture.

## DONE (commits)
- 37698ac99 progress; 610a848fd crate registered (root Cargo.toml, workspace_test_members.txt, rust.yml) + money RED
- money GREEN; a0dd7f091 / b0e49325d model RED / GREEN; 08005b988 / 14da0b040 depreciation RED / GREEN
- 2f31bef79 / d14f5596f store RED / GREEN; 36a1685e8 / 986b28ede + ac800b731 csv_io RED / GREEN
- ceaefae32 / 4b41cd384 reports RED / GREEN
- 304be1f17 views JSON (src/views/ui.assets.json corrected + menu, erp.ui.assets.json original, en.json labels)
- c466b906c views RED: src/views/{mod,spec,rows}.rs - signatures + tests, bodies `todo!("GREEN")`
- 55fbfd5cf views/mod.rs GREEN; 3f48260d8 views/spec.rs GREEN; f7e2c33f2 views/rows.rs GREEN (+ MaintenanceKind
  code/parse). Steps 1-3 below are DONE.

- b9d176cee / 654519384 sample RED / GREEN (4a done)
- 2af7f47f6 app.rs RED (4b tests + stubs `todo!("GREEN")`): NEXT = GREEN of every stub in src/app.rs
  (State::new, open, go_back, save_form (+ helpers per view: asset form with number uniqueness + problems(),
  category / location forms, maintenance form (parent = params id), checkout form (refuse when open; asset
  CheckedOut + custodian), dispose form (status Disposed)), check_in, delete_asset, export_register,
  export_schedule, run_preview, post_run, load (notice "N file(s) could not be read"), seed_sample (only when
  no assets), start_import (page "/assets/import"), set_mapping, commit_import).
- D8 (coordinator rule 2026-10-03): nothing personal leaves the machine; AzERP talks to no outside service.
- 228358a0a app.rs GREEN (+ csv_io::export_journal). 4a + 4b DONE.

- c8e0e31e2 ids.rs; 91ae5d32d / a5fbe0c18 State::row_path RED / GREEN; ef1ca125b ui/mod.rs; 11de8317d ui/table.rs;
  3a9f4b4b7 ui/form.rs; b2804e235 ui/detail.rs.
- NEXT: src/ui/panels.rs with `overview(s: &Erp, asset: &Asset) -> Dom`, `schedule(s: &Erp, app, asset)`
  (schedule DataTable via table::grid_table + detail::on_inner_event, Chart line of closing values),
  `reports(s: &Erp, app, view)`, `run(s: &Erp, app, view)` (year TextInput, Next, preview, Run and post),
  `import(s: &Erp, app, view)` (FileDialog -> kit::spawn_outside_read(.., TAG_IMPORT, on_files), mapping
  DropDowns -> set_mapping, commit). Then lib.rs: `pub mod ui;`, SPEC (pub const, used as crate::SPEC), ABOUT
  (app_folder "erp"), SHORTCUTS, start() (AZERP_TODAY env for E2E, --screen -> path, args.files csv ->
  pending_import), on_window_created (kit::on_window_created + ui::spawn_load). Then E2E + report.

## (old) 4c plan:
- src/ids.rs: `names!` macro like examples/azul-dashboard/src/ids.rs, prefix `__azerp_` (TABLE, TOOLS, TAB_ROW,
  FORM, FORM_SAVE, FORM_CANCEL, FIELD_<name> made at run time? NO - ids for fields: one const per form field
  name is too many; use `field_id(name)` = AzString::from(format!("__azerp_field-{name}")) - ONE helper).
- src/ui/mod.rs: `Erp { kit: RefAny, state: State, table: DataTableView, window: (f32,f32), in_flight:
  Vec<Write> }`; layout(): ShellThemeScope::create(column).with_accent(..).body() + RecordsShell::create(tabs,
  page).with_form(form pane when state.form is a `form` view).office_shell().with_title_row(kit::title_row)
  .with_status_bar(..); form_modal views in a Modal; on_key -> kit::handle_key; pump(): queue.take() ->
  FileJob::Put/Delete -> kit::spawn_file_jobs(info, root, jobs, app, TAG_WRITE, on_written).
- src/ui/table.rs: DataTable over `rows::grid` (source RefAny = Grid), Activate -> open row path.
- src/ui/form.rs: one input per FieldSpec (TextInput / TextArea / DropDown for Select & Reference / DatePicker
  for Date), callbacks set_value.
- src/ui/detail.rs + panels.rs: header + TabHeader; Overview, DepreciationSchedulePanel (DataTable + Chart line
  of closing values), EmbeddedTable, Reports (Chart bars by category + forecast), Import (mapping DropDowns),
  Run wizard (year, preview, post).
- lib.rs: SPEC / ABOUT / SHORTCUTS / start() / on_window_created (GetAll of erp/ on a Thread -> load ->
  seed_sample when --sample).

## NEXT (exact) - step 4 is next
4a. src/sample.rs: `pub fn book(today, new_id: &mut dyn FnMut() -> String) -> Book` (4 categories, 3 locations,
    ~12 assets, maintenance, check-outs) + tests (every asset has no problems, every kind present).
4b. src/app.rs: the pure state `State` (views, labels, book, page path, back stack, detail tab, `FormDraft`,
    notice, azul_pim WriteQueue, today, import draft, run year) and its operations returning nothing but queueing
    writes: open(path) (form / form_modal open a draft over the page), save(), cancel(), check_in, delete_asset,
    export_register, export_schedule, depreciation_run(year) (journal CSV into erp/exports, D6), import, load,
    seed_sample. RED tests first, then GREEN.
4c. UI: src/ids.rs, src/ui/{mod,table,form,detail,panels}.rs, lib.rs start() (AppSpec / AboutInfo / shortcuts
    like examples/azul-dashboard/src/lib.rs; RecordsShell tabs = menu, table = page, form = side pane draft).

## Old plan of steps 1-3 (done)
1. GREEN src/views/mod.rs: `View::kind_of_records` (api.get else post else put -> api_kind), `ViewFile::parse`
   (serde_json::from_str, map_err to_string), `ViewFile::route` (for each view, each pattern of view.path.all():
   match_path; pick the match with the most literal segments), `match_path` (split '/' ignoring empty trailing
   segment; same count; ':x' binds a non-empty segment), `fill_path`, `api_kind` (strip "?query", strip a trailing
   "/:id"; "/api/assets/fixed-assets" Asset, ".../categories" Category, ".../locations" Location,
   ".../maintenance" Maintenance, ".../checkouts" Checkout), `api_filter` ("?k=:p" -> (k, params[p])),
   `humanize` (drop first segment if fields/view/actions/tab/menu/steps/depreciation/maintenance; drop trailing
   label/title; '_' -> ' '; capitalise first letter).
2. GREEN src/views/spec.rs: columns, fields, default_text, actions (type link/action/submit/cancel; primary =
   variant "primary"), tabs, steps, eval_condition (split "||" then "&&", each `a == 'v'` / `a != 'v'`, quotes
   ' or "; unreadable -> true), validate (order of fields; skip !visible; required empty -> "<L> is required.";
   Integer parse i64 else "<L> must be a whole number."; Decimal money::parse_amount else "<L> is not an
   amount..."; min/max -> "<L> must be at least N." / "at most N." (decimal compares cents with N*100); Date
   model::parse_date else "<L> must be a day (YYYY-MM-DD)."; Select value not in options -> "<L> must be one of
   the choices.").
3. GREEN src/views/rows.rs: Value::{display, form_text, sort_value}; ViewRecord impls (Asset fields: id,
   asset_number, name, category (name), category_id, location (name), location_id, serial_number,
   acquisition_date, acquisition_cost, residual_value, useful_life_years, depreciation_method (Coded),
   declining_rate_percent (Money(bp): "25.00"), status (Coded), custodian, maintenance_interval_months,
   disposal_date, disposal_amount, book_value (ctx.today), notes; Category name/useful_life_years/
   depreciation_method/notes/asset_count; Location name/address/notes/asset_count; Maintenance date/asset_id/
   asset_number/asset_name/kind (Coded; add MaintenanceKind::code in model.rs)/description/cost/performed_by;
   Checkout asset_id/asset_number/asset_name/custodian/checked_out/due_date/checked_in/note), apply (validate then
   set visible fields), grid, reference_choices (assets as "A-0042 ThinkPad").
4. Then the UI (src/ids.rs, src/sample.rs, src/app.rs state + writes, src/ui/*.rs), lib.rs start(); E2E
   scripts/azerp_e2e.py; report scripts/ERP9_2026_10_03.md (or the finishing date).

## Open questions
- none blocking
