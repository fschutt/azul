# DATATABLE7 progress (wave 7, branch wt/datatable7, base 2e55eef06)

## DONE
- c1a08a335 progress file
- (next commit) examples/azul-dashboard skeleton: Cargo.toml, main.rs, lib.rs (RecordsShell window,
  the generating Thread, the CHART7 marked block), ids.rs, data.rs (500k x 25 deterministic orders +
  tests), table.rs (STUB), registered in Cargo.toml / workspace_test_members.txt / rust.yml dll_tests

## IN PROGRESS
- layout/src/widgets/data_table.rs: the model first (types, sort / filter, order computation, RED tests)

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
