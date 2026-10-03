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

## NEXT (exact)
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
