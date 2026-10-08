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
- f6ab76d52 test(code_view): RED - a wheel notch scrolls a code view without rebuilding the window
  (layout/tests/a_wheel_notch_..., code_view_tests::the_view_is_one_tab_stop_hosting_a_virtual_view,
  azcode_e2e.py wheel step: RED on the 04:25 build - "five wheel notches ... rebuilt the window 5 times").
- 18d977394 fix(code_view): a scroll renders the view's own lines, not the window (CodeView = view node +
  VirtualView `render_lines`; Scroll -> `rerender_lines`; AzCode answers Scroll with DoNothing).
- 716137961 code_view: lines_dom binds the pinned theme before the match.
- 530504dd1 scripts/azcode_scroll_probe.py (the per-notch cost + frame counters, AzCode vs AzWidgets).
- 414139c5a test(precascade): RED - an identical rebuild renders its virtual views from the fresh payload
  (dll headless test; the pre-cascade skip kept last build's VirtualView refany: CodeView's handlers and
  lines would split after e.g. AzCode's highlight-done RefreshDom).
- 0ce7e36fb fix(precascade): PreCascadeTransfers::virtual_views installed on the retained DOM before the
  dataset merge (core/src/diff.rs fingerprint_dom, dll/src/desktop/shell2/common/layout.rs).

## IN PROGRESS
- nothing (nothing compiled - the lead builds).

## NEXT (lead)
- cargo test -p azul-layout --lib code_view; --test all a_wheel_notch_scrolls_a_code_view_without_rebuilding_the_window;
  --lib widgets:: (manifest lints: code_view now `fixtures::sample_with_lines`); core --lib diff_test::;
  dll --lib --features build-dll an_identical_rebuild_renders; build dylib + AzCode;
  scripts/azcode_e2e.py; re-measure with /tmp/codescroll13/measure.py (or scripts/azcode_scroll_probe.py).

## Open questions / follow-ups (engine, not fixed here)
- A RefreshDom that changes one subtree re-cascades the whole window and recomputes intrinsic sizes of every
  node (730/730) - typing in AzCode pays it per key.
- Every layout pass calls hint_purge_allocator (malloc_zone_pressure_relief) twice (solver3/mod.rs, window.rs).
- CpuHitTester::rebuild_from_layout_with_gpu reads overflow-x/y per node through get_property_slow
  (winning_inline_in linear scans of theme-conditioned inline props): 6 ms per notch over 4 rebuilds.
- headless: every wait_frame repaint is a relayout_only of the whole DOM (inflates every headless timing).
