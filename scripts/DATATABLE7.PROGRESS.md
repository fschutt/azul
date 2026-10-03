# DATATABLE7 progress (wave 7, branch wt/datatable7, base 2e55eef06)

## DONE
- c1a08a335 progress file
- a89b27c27 examples/azul-dashboard skeleton: Cargo.toml, main.rs, lib.rs (RecordsShell window,
  the generating Thread, the CHART7 marked block), ids.rs, data.rs (500k x 25 deterministic orders +
  tests), table.rs (STUB), registered in Cargo.toml / workspace_test_members.txt / rust.yml dll_tests
- 9bbf7bc1c, db50c5e84 data_table.rs: value types, view methods (click_sort, set_filter, with_order ...),
  filter grammar (number / date bounds, days_from_civil)
- 3c6b3d50e RED model tests (layout/src/widgets/data_table_tests.rs) + events, callbacks, DataTable
  struct + builders, fixtures; `pub mod data_table;` in widgets/mod.rs
- a7e892f8c GREEN plan_of / read_keys / compute_order

- 43a754724 cell_grid: axis_bands / band_at / take_wheel / cursor_in pub(crate)
- 55d00b13c RED + 8e2a57e3b GREEN aria-sort: AccessibilityState::SortedAscending / SortedDescending
  (core/src/a11y.rs append, a11y_test canary, layout/src/managers/a11y.rs mapping + test)
- b5da12b58 geometry, c81c5b492 build, 146485034 theme looks (flat.rs / flora.rs appends)
- 6f838ebdb keys / selection / editors (pure), 28548d408 the order job, b94f1a648 the handlers

- d000d64c4 widget tests (data_table_tests.rs) + Ctrl+Home/End select fix
- bdcf3637e manifest registration in widgets/mod.rs (every_widget_dom, CHROME group, wheel takers)
- (this commit) examples/azul-dashboard/src/table.rs for real: DataTable over the DataSet, on_event /
  on_edit / data callback, Clear filters / Clear sort (DataTable::start_query), status bar

- 80517503c, 25ab9880d dashboard fixes (start_query takes CallbackInfo by value, as the bindings do)
- a4520f9a5 scripts/azdashboard_e2e.py; 709e679b8 cleanup
- (this commit) the report scripts/DATATABLE7_2026_10_03.md

## IN PROGRESS / NEXT
- DONE. Only if resumed with time left: a last read-through of data_table.rs for compile slips
  (see the report's "least sure to compile").

## NEXT
1. data_table.rs model: DataTableColumn / Cell / CellRef / SortKey / Filter (+ parse) / View / Event,
   `compute_order` (stable multi-key sort + filters) + unit tests incl. 500k synthetic rows
2. widget: geometry (reuse cell_grid::axis_bands etc.), build (header, filter row, rows, editor, scrollbars),
   theme looks APPENDED in flat.rs / flora.rs (`// ==== data_table ====`)
3. handlers: keys, text, mouse down/move/up, double click, wheel, copy; the order job (Timer slices + Thread)
4. register in widgets/mod.rs (module, manifest every_widget_dom, CHROME group, wheel-takers list)
5. dashboard table.rs for real + scripts/azdashboard_e2e.py
6. report scripts/DATATABLE7_2026_10_03.md

## Decisions
- D1 virtualization: the CellGrid scroll-window pattern (whole rows, self-drawn scroll bars, one focus
  stop), NOT a VirtualView: EVENTS7 has the open bug "an event into a VirtualView child never reaches the
  parent DOM", and the table must be one focus stop whose key handler sees every key.
- D2 the filter row and the cell editor are drawn by the table itself (like CellGrid's editor), not
  TextInputs: key events bubble from a focused descendant to the table node (core/src/events.rs, Focus
  filters fire in the bubble phase), so a TextInput inside would also hit the table's keys.
- D3 sort / filter off the UI thread: the KEYS are read through the app's data callback on the UI thread
  in slices (a Timer, bounded rows per tick: the callback is the app's and may be a managed-language
  host's, which must run on the UI thread), then the SORT + FILTER of those plain keys runs on an azul
  Thread; the write-back fires `OrderReady`. Tables of <= 20,000 rows sort synchronously in the handler.
- D4 `OrderReady` builds its next view from the LATEST view: the table node carries a marker (its id) and
  its handler payload as dataset; the write-back finds it (get_node_id_by_marker + get_dataset), so a
  scroll / selection made while sorting is not lost; a superseded job (older query serial) is dropped.
- D5 reuse: cell_grid's axis_bands / band_at / take_wheel / cursor_in made pub(crate) (one-word edits in
  WIDGETS7's file), CellGridHorizontalAlign for the column alignment, CellGridSize(Vec) for resized widths,
  cells_to_tsv / cells_to_html for copy, date_picker::days_in_month for date filters, ListSelection for rows.
- D6 dashboard data are generated, not stored (no Drive writes): edits live in memory (the brief asks for a
  generated data set; noted as "left").

## Open questions
- (none)
