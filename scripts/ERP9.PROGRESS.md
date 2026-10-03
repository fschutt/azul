# ERP9 progress - AzERP asset management (wave 9)

Branch `wt/erp9` from `e537ddbe2`. Brief: scripts/waves/wave9/PLAN.md "ERP9"; planning
../azul-apps/planning/other/erp/{README,asset-management}.md; the view JSON ../erp/json/ui/ui.assets.json.

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
  the aztasks files. Writes go through azul-pim's `WriteQueue` (one batch in flight) run as azul-appkit
  `FileJob`s on appkit's file thread; the load is one `FileJob::GetAll`.
- D5 CSV through the `csv` 1.4 crate (already in Cargo.lock via AzSheets) - no third hand-written CSV parser
  (AzContacts has its own RFC 4180 reader: named as a twin in the report).

## DONE
- 37698ac99 progress file
- 610a848fd RED crate registered (root Cargo.toml, workspace_test_members.txt, rust.yml) + money tests
- (next) GREEN money; a0dd7f091 RED model; b0e49325d GREEN model; 08005b988 RED depreciation;
  14da0b040 GREEN depreciation

## IN PROGRESS
- store.rs (keys, load from FileOutcome::GotAll files, the Book of records)

## NEXT
- store RED/GREEN; csv_io RED/GREEN; reports; views (interpreter) RED/GREEN; sample; ids; UI; E2E; report

## Open questions
- none yet
