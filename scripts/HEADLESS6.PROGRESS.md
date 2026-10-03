# HEADLESS6 progress (wave 6, 2026-10-03)

Branch `wt/headless6` from base 25d78e309. Brief: scripts/waves/wave6/HEADLESS6.md.

## Corpus baseline (aa59b2d84 binaries, AzPaint headless, 2026-10-03 02:29)
40 passed / 22 failed. Categories:
- A (13) X10 gpu_state "GPU value cache still held for DOM 1": AzPaint's ProgressBar is a VirtualView
  (child DOM 1); `mount` unmounts it; remap dropped child DOMs only for thread owners. FIXED (811d7c1a0).
  anim-dom-transition, anim-slow-move-frames, cross-x1, css-anim-perf-transition, css-anim-perf-zombie,
  css-animation-out, css-catch-mid-exit, css-no-default-exit, css-zombie-relayout, manager-keys-drop,
  noninterference-idle-tick, noninterference-scroll, noninterference-tab-focus.
- B (4) assert_damage_sound pixel_identity: "host does not publish the damage-driven framebuffer":
  bug-slider-thumb-trail, op-resize-grow-exposed-strip, op-resize-grow-reflow, op-resize-shrink-stays-full.
- C bug-transform-offsets-hit-test: focus_state has_focus false (expected "below").
- D css-animation-multi: width 117.336 vs 120.
- E css-animation-transition: transitions 2 vs 1.
- F dl-text-patch: last_dl_build_patched false.
- G op-image-cache-id-repaints: nothing repainted.

## DONE
- f8c17345e RED / 811d7c1a0 GREEN: VirtualView child-DOM state dropped with its host (category A)
- 072a51d9c RED / 913e33c3e GREEN: headless backend publishes the painted frame (category B);
  tests in dll/src/desktop/shell2/headless/tests/e2e_host.rs (mod e2e_host next to mod idle_cpu)

## IN PROGRESS
- item 2 (exit segfault): design below; implement in run.rs (printer thread) + headless/mod.rs (loop).

## NEXT
- items 3 (headless menus), 4 (window id on LayoutCallbackInfo), 5 (child-window routing).
- C..G need a probe run (power permitting): probe scenarios in scratchpad/probe.
  C bug-transform-offsets-hit-test: 2nd click at (50,25) clears focus instead of focusing absolute #below;
    same CpuHitTester + resolve_tf in both hosts -> suspect layout/mount difference; probe with hit_test op.
  D css-animation-multi: width 117.336 = EXACTLY 31 steps of 16.666 ms instead of 30: the dll host's CSS
    driver timer ran one wall-clock frame between tick_animations and the measurement. The in-process runner
    FREEZES the engine clock (runner.rs reset_test_clock+freeze_test_clock); the AZ_E2E host does not, and
    cannot naively (the debug timer that pumps the scenario is engine-clock driven -> would deadlock).
  E css-animation-transition: transitions 2 instead of 1 after the 2nd mount (`animation: all`): which
    second property transitions in the AzPaint host? probe needed.
  F dl-text-patch: last_dl_build_patched false after set_node_text in the AzPaint host.
  G op-image-cache-id-repaints: add_image_to_cache by css id -> no paint damage in the AzPaint host.

## Item 2 design (exit segfault)
- Root cause: AZ_E2E's `e2e-result-printer` thread calls exit_dumping_profile -> libc exit() from a
  NON-UI thread while the UI loop, timers, workers and the debug server still run; atexit / TLS teardown
  races them (exit 139 seen with the instrumented build; normal build exited 1 cleanly in one probe).
- Fix: printer records the exit code + wakes the loops (request_e2e_exit); the headless loop sees it,
  closes, shuts down threads (joins), and exits via exit_dumping_profile on the UI thread. Fallback: the
  printer exits itself after a grace period if no loop took the request (desktop backends).
- headless run() EndProcess also calls raw std::process::exit(0) -> route through exit_dumping_profile.

## Decisions
- Category A fix lives in layout/src/window.rs `remap_node_ids` (manager lifecycle, unowned by another
  wave-6 task); minimal edit at the end of the function.
- POWER (coordinator, 02:45): on battery - no long headless runs until told otherwise; commit every unit.
- Corpus runs: AZ_E2E=<dir> dispatcher under run_capped (cap 1500 MB covers the 7 parallel children).
