# CODESCROLL13 - AzCode lags when scrolling the code view (AzWidgets is smooth)

Worktree branch: worktree-agent-ac7f673a4a5f9d95c (fast-forwarded to cbd0ae0a8).
Measurement tools (not in the repo): /tmp/codescroll13/measure.py, run.sh (lock + run_capped), tree.py (sample pruning).

## MEASURED (release build 04:25, dylib 04:16, headless 1280x800, 40 wheel notches of -100 px)
- AzCode huge.rs: 71 ms per notch (wheel + wait_frame round trip; an idle frame is 22.6 ms in headless,
  whose every repaint is a relayout_only of the existing DOM). `dom_regenerations` 40 / 40 notches.
- AzCode src/code_view.rs (3300 lines, --folder): 67.7 ms per notch, dom_regenerations 40 / 40.
- AzWidgets (3473 nodes): 34-48 ms per notch in headless (all of it the headless repaint relayout + raster of
  the scroll glide frames), dom_regenerations 0 / 40 - the page scrolls by the engine's offset only.
- AzCode per notch, `sample` (no profiler), regenerate_layout ~48.6 ms: solver3 root layout 14.1 ms (taffy
  flex 7, intrinsic sizes 2.2 for all 730 nodes, solver3 reconcile 1.5, DL 1.5, malloc purge 1.3), the
  cascade of the whole window (create_from_dom) 11.1 ms, begin_reconciliation 4.7 ms, the app's layout
  callback 3.9 ms (CodeView::dom 2.5), the explorer's VirtualView re-rendered 1.9 ms, a11y 0.9 ms, the CPU
  hit testers 3.4 ms, raster 3.1 ms.
- AZ_PROFILE=cpu per notch: solver3_layout_document 16 ms (3 passes), cpu_hit_tester_rebuild 6 ms (4),
  cb:azcode::layout 3.25 ms, reconcile_and_invalidate 3 ms, raster 2.4 ms, DL 1.7 ms, a11y 1.2 ms;
  intrinsic_node_compute 730 / notch, fc_inline 938 / notch (the whole window, every notch).

## ROOT CAUSE
- CodeView scrolls by app rebuild: on_wheel (layout/src/widgets/code_view.rs, `on_wheel`) -> `deliver` ->
  the app's on_event -> AzCode `on_code_event` (examples/azul-code/src/ui.rs) -> `with_state` ->
  `Update::RefreshDom`: every notch re-runs the app's layout callback, re-cascades / reconciles / re-lays out
  the whole workbench and re-renders the explorer's VirtualView. TerminalView (AzTerm) already re-renders
  only its own VirtualView for a Scroll; CodeView never did.

## DONE
- (nothing committed yet besides this file)

## IN PROGRESS
- RED: layout/tests/a_wheel_notch_scrolls_a_code_view_without_rebuilding_the_window.rs + code_view_tests
  (the view hosts a VirtualView) + azcode_e2e.py (dom_regenerations 0 across wheel notches).

## NEXT
- GREEN: CodeView = host node (handlers, focus, a11y) + VirtualView child rendering the lines from the
  shared state; a Scroll re-renders that view (trigger_virtual_view_rerender); AzCode returns DoNothing for
  Scroll.

## Open questions / follow-ups (engine, not fixed here)
- A RefreshDom that changes one subtree re-cascades the whole window and recomputes intrinsic sizes of every
  node (730/730) - typing in AzCode pays it per key.
- Every layout pass calls hint_purge_allocator (malloc_zone_pressure_relief) twice (solver3/mod.rs, window.rs).
- CpuHitTester::rebuild_from_layout_with_gpu reads overflow-x/y per node through get_property_slow
  (winning_inline_in linear scans of theme-conditioned inline props): 6 ms per notch over 4 rebuilds.
- headless: every wait_frame repaint is a relayout_only of the whole DOM (inflates every headless timing).
