# DATATABLE7 - a virtualized DataTable widget + the dashboard example (wave 7)
Why (user 2026-10-03, a Reddit beginner): "sortable / filterable tables + charts + an editable grid over 500k x 25
Excel rows, Win / Mac / Linux" - azul is not recommendable for that yet. You build the table half.
Owns: NEW layout/src/widgets/data_table.rs (+ tests, theme APPENDS in flat.rs / flora.rs, list APPENDS in
widgets/mod.rs), NEW examples/azul-dashboard (you create the crate; CHART7 adds its chart module later - see
PLAN.md contracts). Read first: layout/src/widgets/cell_grid.rs, list_view.rs, ListSelection, VirtualView, and
scripts/SHEETS*_2026_*.md (CellGrid's history).

1. DataTable: columns (title, width, alignment, a sort key kind: text / number / date), rows supplied by the app
   through a data-source callback (row count + get_cell(row, col)) so 500k rows never live in the DOM: only the
   visible rows (+ overscan) are built (virtualized; VirtualView or the scroll-window pattern CellGrid uses).
2. Built in: click a header to sort (asc / desc / none, stable, multi-column with Shift), a filter row or a search
   field (contains / equals / range for numbers), in-place editing (double-click / F2 / Enter, Escape cancels, an
   on_edit callback the app validates), selection via ListSelection, keyboard navigation (arrows, Page Up / Down,
   Home / End, Ctrl+Home), column resize, sticky header, a11y (grid / row / gridcell roles, aria-sort).
3. Sorting / filtering 500k rows must not block the UI: the index permutation is computed off the UI thread (a
   Thread) or incrementally; say which and why; a unit test of the sort / filter model on 500k synthetic rows
   (timed loosely, no flakiness).
4. Both themes x modes, the widget manifest entry, api.json list (exact methods, create*, callback triples like
   CloseGuard's).
5. examples/azul-dashboard: a window with the DataTable over a generated 500k x 25 data set (deterministic,
   generated in a Thread), a status bar with row counts; leave a marked placeholder block for CHART7's charts.
   Register the crate (house rules "Apps"). A headless E2E script scripts/azdashboard_e2e.py: sort, filter, edit,
   scroll to the end - assert on node text / layout.

Rules: scripts/waves/house_rules.md (read it fully first). Plan + who owns what: scripts/waves/wave7/PLAN.md.
Every behaviour change: a RED test commit first (test names are sentences), then the fix. Root causes, no
workarounds. Never compile. Commit after every unit; keep scripts/DATATABLE7.PROGRESS.md exact. Finish with the report
scripts/DATATABLE7_<YYYY_MM_DD>.md (built, commits, api.json list in api.json terms, least-sure-to-compile spots, test
commands, what is left) and commit it.
