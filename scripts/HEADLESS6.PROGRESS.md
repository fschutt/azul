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

## IN PROGRESS
- category B: find in layout/src/e2e/full.rs the `assert_damage_sound` 'pixel_identity' branch
  ("does not publish the damage-driven framebuffer"), see how layout/src/e2e/runner.rs publishes it,
  and make dll/src/desktop/shell2/headless/mod.rs `paint_cpu_frame` publish the same (RED first).
  Last commit: 8383b8251 (progress). No uncommitted work.

## NEXT
- C..G triage; 2. exit segfault; 3. headless menus; 4. window id on LayoutCallbackInfo; 5. child-window routing

## Decisions
- Category A fix lives in layout/src/window.rs `remap_node_ids` (manager lifecycle, unowned by another
  wave-6 task); minimal edit at the end of the function.
- POWER (coordinator, 02:45): on battery - no long headless runs until told otherwise; commit every unit.
- Corpus runs: AZ_E2E=<dir> dispatcher under run_capped (cap 1500 MB covers the 7 parallel children).
