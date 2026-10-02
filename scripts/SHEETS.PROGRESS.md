# SHEETS progress (branch wt/sheets, base 16d19442c)

Resume: read this, then `git status`, `git log -3`, continue at NEXT.

## DONE
- 218807a01 test(a11y): Grid / GridCell roles + AccessibilityInfo row_index / column_index (RED)
- 06e80205d fix(a11y): the GREEN (core/src/a11y.rs, layout/src/managers/a11y.rs)
- CellGrid (layout/src/widgets/cell_grid.rs): 8cab1ba79 types/callbacks/builders, 504da4746 pure logic,
  b957aac4a resolve + build + look struct, ac289ff8f handlers, 1260f86bd flat/flora looks + decl::border_right,
  357077f24 tests + widgets/mod.rs registration (module, manifest fixture, CHROME group, wheel guard).
- Engine layer (written by a fork on wt/sheets-engine, cherry-picked): ac41cc2c3 Cargo.toml + engine.rs,
  a66835b6e fake_engine.rs, 7c02c695f ops.rs, f3cd9cd52 worker.rs + sample.rs, 7c4218950 storage.rs,
  e26f77d1f ironcalc_engine.rs, c13246b19 scripts/SHEETS_ENGINE_REPORT.md (its least-sure list).
- 486903794 worker Command::SumRange (+ test); 56ebd973d args.rs + main.rs; 7b9f232ca functions.rs (catalogue +
  autocomplete); f886e0159 model.rs (fetch spans, fill targets, decimals, status numbers, tsv_of);
  lib.rs: 3af90670c state/cache/mapping, 13d2d8db2 engine plumbing + storage jobs, c41fc5c14 layout,
  0fe3d3bc6 callbacks/startup, 741658fe8 lib tests; 6253de85d workspace/test-members/CI registration.
- 8f04bc90e scripts/azsheets_e2e.py; 907422920 report; later fixes: 5c714f409 auto ink via ColorU::best_contrast_text,
  4f9804fe6 Options page (ShellSettingsLayout), 09b635019 lint fixture without app colours, 400dbd4ae / efb1630b2 /
  bbdafa887 (XVec::from_vec) / ae2e79d54 (no attributes on expressions) / b5abe1e84 compile-safety fixes,
  ed27f0c5d E2E re-focus.

## NEXT (precise)
- Nothing left in the task: the report (scripts/SHEETS_2026_10_01.md) is committed; the parent runs autofix,
  the build, the suites and the E2E.

## Design decisions (taken, unattended)
- IronCalc pinned to =0.8.3 (crates.io max_stable_version on 2026-10-01, same as the engine study).
- CellGrid scrolls by whole cells (Excel / IronCalc web UI): the grid owns top_row / left_column, renders
  only the visible window, frozen panes and headers by construction; NO native scroll box, so no 26 M px
  f32 extent (excel.md question 8). The user asked (mid-run) whether I know the infinity demo
  (examples/rust/src/infinity.rs, VirtualView): answered yes; offered a VirtualView variant for pixel
  scrolling vertically if wanted; continuing with whole-cell scrolling unless told otherwise.
- All mouse hit testing is geometric on the grid node (LeftMouseDown / MouseMove / MouseUp / DoubleClick +
  pointer capture, like SplitPane); cells carry no callbacks.
- Typing is handled by the focused grid (Focus TextInput + get_text_changeset, like ComboBox).
- App-owned view state (selection, top-left, drag, edit text) comes back in every CellGridEvent.
- Engine runs on a std thread with a 256 MB stack; the UI sends EngineMsg over std mpsc from callbacks
  (ordered), one short azul Thread per request waits for the Reply and writes it back.
- Sidecar json keeps title / zoom / selection / top-left (IronCalc keeps widths and frozen panes itself).

## Open questions
- (none)
