# SHEETS progress (branch wt/sheets, base 16d19442c)

Resume: read this, then `git status`, `git log -3`, continue at NEXT.

## DONE
- 218807a01 test(a11y): Grid / GridCell roles + AccessibilityInfo row_index / column_index (RED)
- (next commit) fix(a11y): the GREEN for it (core/src/a11y.rs, layout/src/managers/a11y.rs)

## NEXT (precise)
1. CellGrid widget, layout/src/widgets/cell_grid.rs, in pieces:
   a. skeleton: types (CellGridCellRef, CellGridRange(+Vec), CellGridDrag(+Kind), CellGridEditMode,
      CellGridView, CellGridCellKind, CellGridHorizontalAlign, CellGridVerticalAlign, CellGridCellStyle,
      CellGridCell, CellGridSize(+Vec), CellGridEventKind, CellGridEvent), callbacks (CellGridOnEvent form 2,
      CellGridDataSource / CellGridStyleSource form 4 + HostOut), CellGrid struct + builders. commit.
   b. pure core: geometry (visible window, hit_test), navigation (keys -> new view), wheel steps,
      tsv/html encode/decode, column_label. commit each.
   c. RED tests (virtual window, selection, keyboard, a11y, wheel lint). commit.
   d. build() + CellGridLook + handlers; flat/flora looks appended to theme files; decl::border_right;
      mod.rs registration (pub mod, every_widget_dom, CHROME group, wheel_ownership list). commit each.
2. examples/azul-sheets (package AzSheets): engine.rs trait spec -> fake_engine.rs, ironcalc_engine.rs,
   worker.rs (256 MB std thread + azul Thread waiters), sample.rs, storage.rs, args.rs, lib.rs UI.
3. scripts/azsheets_e2e.py; report scripts/SHEETS_2026_10_01.md.

## Design decisions (taken, unattended)
- IronCalc pinned to =0.8.3 (crates.io max_stable_version on 2026-10-01, same as the engine study).
  Sources read from the crate tarballs (scratchpad/sheets_src).
- CellGrid scrolls by whole cells (Excel / IronCalc web UI): the grid owns top_row / left_column, renders
  only the visible window, frozen panes and headers by construction; NO native scroll box, so no 26 M px
  f32 extent (answers excel.md question 8). Wheel over the grid steps rows (the grid is the scroll surface
  -> added to the wheel_ownership exceptions).
- All mouse hit testing is geometric on the grid container (MouseDown / MouseMove / MouseUp / DoubleClick
  + pointer capture, like SplitPane); cells carry no callbacks.
- Typing is handled by the focused grid itself (Focus TextInput + get_text_changeset, like ComboBox), no
  TextInput widget inside the grid (no focus juggling across rebuilds).
- App-owned view state (selection, top-left, drag, edit text) comes back in every CellGridEvent.
- a11y: new engine roles Grid / GridCell + row_index / column_index (1-based) on AccessibilityInfo.

## Open questions
- (none yet)
