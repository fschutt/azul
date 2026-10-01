# SHEETS_ENGINE: the AzSheets engine layer (fork of SHEETS, 2026-10-01)

Branch `wt/sheets-engine` from `06e80205d` (wt/sheets). Nothing compiled (house rule); every file
formatted with stable `rustfmt --edition 2021` (a parse check). Spec:
`scratchpad/sheets_engine_spec.md` (the contract with the UI in `lib.rs`).

## Commits (wt/sheets-engine)

| hash | what |
|---|---|
| `6531a1f28` | Cargo.toml (AzSheets, ironcalc `=0.8.3`) + engine.rs (the `SheetEngine` trait and its types) |
| `dcea9725f` | fake_engine.rs (`FakeEngine`) |
| `f4f628ea4` | ops.rs (stats, sort, remove duplicates, filter, find, CSV export, AutoSum range) |
| `3c11e222a` | worker.rs (the 256 MB engine thread, messages, snapshots) + sample.rs (Budget 2027) |
| `61bf03069` | storage.rs (`sheets/<uuid>.xlsx` + `.json` sidecar through a `Drive`) |
| `6ee5252e1` | ironcalc_engine.rs (`IronCalcEngine` + the mapping functions) |
| (this one) | this report + the progress file |

## Files (all under examples/azul-sheets/, no `azul` imports)

`Cargo.toml`, `src/engine.rs`, `src/ops.rs`, `src/fake_engine.rs`, `src/ironcalc_engine.rs`,
`src/worker.rs`, `src/sample.rs`, `src/storage.rs`. NOT created (the parent's): `src/lib.rs` (must declare
`pub mod engine; pub mod ops; pub mod fake_engine; pub mod ironcalc_engine; pub mod worker; pub mod sample;
pub mod storage;`), `src/main.rs`, the workspace / CI registration.

## Deviations from the spec

- `DEFAULT_FONT_SIZE = 12` (IronCalc's `Font::default().sz`), not the spec's placeholder 13.
- engine.rs also exports `LAST_ROW` / `LAST_COLUMN` (IronCalc's are crate-private).
- storage.rs also exports `DATA_VAR` ("AZSHEETS_DATA"), `now_secs()`, `Sidecar::titled(title)`;
  `Sidecar::default()` has zoom 100, active / top_left (1, 1).
- storage tests use azul-storage's `LocalDrive` on a temporary folder instead of a new in-memory Drive
  fake (no new helper; exercises the real backend).
- `ValueKind::of(&CellValue)` added (worker.rs).
- The fake's `save_xlsx` writes JSON (only the fake reads it back); styles are not in that JSON.
- `sort_area` / `remove_duplicates` move INPUTS: a moved formula keeps its text (references are not
  rewritten) and cell styles do not move with their rows. IronCalc has no sort; `paste_from_clipboard`
  would move styles and rewrite references but needs its private `ClipboardCell` built through serde -
  a possible follow-up.
- `set_frozen` on IronCalc is up to two undo steps (rows, columns); `filter_rows` is one step per run of
  rows; the sample leaves its building steps in the undo history (IronCalc has no "clear history").

## Least sure to compile (read these first)

1. `ironcalc_engine.rs`, `impl SheetEngine for IronCalcEngine` - `SheetEngine: Send` needs
   `UserModel<'static>: Send` (ironcalc.md says it was verified on 2026-09-15).
2. `ironcalc_engine.rs::IronCalcEngine::from_xlsx_bytes` - `Model::from_workbook(workbook, LANGUAGE)` returns
   `Model<'_>` tied to the `&'static str` language, then `UserModel::from_model(model)` (elided lifetimes)
   must infer `UserModel<'static>`; same in `empty_model` with `Model::new_empty`.
3. `ironcalc_engine.rs::cell_style` - `style_from(&style, &|c: &Color| self.model.resolve_color(c))`
   (a closure coerced to `&dyn Fn(&Color) -> String`).
4. `ironcalc_engine.rs::border_area` - `serde_json::from_value::<BorderArea>` relies on the serde names
   `item` / `type` (the field is `r#type`) and `BorderType`'s variant names (`All`, `Outer`, ...); the unit
   test `a_border_area_is_built_through_serde_for_every_preset` checks it at run time.
5. `ironcalc_engine.rs::value_from` - `IcCellValue::String(s) if is_error =>` (a by-move binding in a guarded
   arm; fine since Rust 1.39).
6. `ironcalc_engine.rs::tsv_of` / `ops.rs::export_csv` - `csv::Writer::into_inner()` error type (mapped to a
   fixed sentence / `unwrap_or_default`).
7. `worker.rs::handle` - the `catch_unwind(AssertUnwindSafe(|| run(engine, ..)))` closure must reborrow
   `engine: &mut dyn SheetEngine` (unique borrow), so `snapshot(engine, ..)` after it compiles.
8. `fake_engine.rs::parse_a1` - `c.to_ascii_uppercase() as i32` (char -> i32 cast).

IronCalc calls used (all read in the 0.8.3 sources): `Model::new_empty`, `Model::from_workbook`,
`model.workbook.{name, worksheet(i)}`, `Worksheet::{sheet_data, dimension()}`, `UserModel::{from_model,
get_model, get_name, evaluate, undo, redo, can_undo, can_redo, get_worksheets_properties, new_sheet,
rename_sheet, delete_sheet, move_sheet, set_sheet_color, set_user_input, paste_csv_string,
get_cell_content, get_formatted_cell_value, get_cell_type, get_cell_style, resolve_color,
update_range_style, set_area_with_border, range_clear_contents, range_clear_formatting, auto_fill_rows,
auto_fill_columns, insert_rows, delete_rows, insert_columns, delete_columns, get_column_width,
set_columns_width, get_row_height, set_rows_height, set_rows_hidden, get_frozen_rows_count,
get_frozen_columns_count, set_frozen_rows_count, set_frozen_columns_count, get_show_grid_lines,
set_show_grid_lines, get_defined_name_list, new_defined_name, delete_defined_name}`,
`Model::get_cell_value_by_index`, `Color::from_param`, `ironcalc::import::load_from_xlsx_bytes`,
`ironcalc::export::save_xlsx_to_writer`.

## Tests (run by the parent)

```sh
AZ_LINK_PATH=$PWD/target/azul-lib cargo test --release -p AzSheets --lib
```
(needs the parent's `lib.rs` with the `pub mod` lines above). Per module: `engine::tests` (CellArea),
`fake_engine::tests`, `ops::tests` (against the fake), `worker::tests` (snapshot dedup / hidden rows /
stats, handle, in-order thread, loop exit), `sample::tests`, `storage::tests` (LocalDrive in a temp folder),
`ironcalc_engine::tests` (mapping functions + real engine: formula, one undo step, styles/borders, xlsx
round trip, the sample on IronCalc, a 5,000-cell chain on the 256 MB engine thread - run in `--release`:
IronCalc's frames are far larger in debug).

`Cargo.lock` gains `AzSheets`, `ironcalc 0.8.3`, `ironcalc_base 0.8.3` and their new dependencies
(`bitcode`, `chrono-tz`, `statrs`, `zip 0.6`, ...) on the first build.
