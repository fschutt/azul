# LAYOUTPERF8 progress (wave 8, branch wt/layoutperf8, base e290321da)

Task: a one-node change (the AzWidgets switch knob's margin-left tween) re-lays out almost the whole page
(118-290 ms per frame, 2853 text re-flows for a 16 px move; ANIM8 report section 4). Root-cause, RED test,
fix. Never compile; never touch page_breaks.rs.

## STATUS: DONE - report scripts/LAYOUTPERF8_2026_10_03.md

## DONE (oldest first)
- 2e6f1f370 progress file
- 0b156f22c scripts/layoutperf8_tick_scenario_gen.py + measured cost (prebuilt AzWidgets: 157-309 ms per
  tick, 2853 text flows, 12181 taffy misses) + root cause
- 6d3d770b4 RED layout/tests/a_one_box_slide_does_not_re_lay_out_the_page.rs (registered in all.rs)
- 39f090082 GREEN: clone_node_from_old keeps taffy_cache + measured_content_sizes; reconcile_recursive
  clears them under a restyle; try_reuse_anon_wrapper carries the wrapper's layout; carried_indices +
  ifc_membership remap; viewport-unit guard (mod.rs Step 1.2); paged_layout clears all; comments
- 04db13759 scripts/layoutperf8_e2e/ text beside a block (FAIL on wave 7) + own margin (xfail, bug B)
- d6ad6d687 a clone paired by position drops its measurements
- 22acb1c28 guard test a_page_laid_out_again_matches_a_fresh_window (cold oracle)
- cd12ec4da e2e a_parent_grows_with_its_restyled_child (xfail, bug C)
- report commits (5e2c7b8d4, 18a870c3c, + the last one)

## ROOT CAUSE
`LayoutTreeBuilder::clone_node_from_old` cleared every clone's flex measurements (deac0bebb); every
relayout reconciles and every clean node is a clone -> every flex item laid out again per pass.
deac0bebb's real cause (a memoised final served after a measure) is guarded since c60844cab.

## DECISIONS
- Bugs B (css-dirty block margin not applied: stale box props on clones) and C (css-dirty block size
  change does not grow auto-height ancestors) documented with xfail e2e probes + a plan; not fixed (not
  the per-frame cost; needs a design pass + build).
- The wasm-lift diagnostic deep tree clone (mod.rs ~1206) left alone (another session's scaffolding).

## NEXT (for whoever resumes)
- Nothing on this branch. Parent: build, run the test commands in the report, re-measure AzWidgets.

# LAYOUTPERF8B (branch wt/layoutperf8b, base 3428223f1 = wave 8 integrated incl. LAYOUTPERF8)

Coordinator re-measured on the wave-8 build: knob tick incremental_relayout 55-58 ms (one 23 ms), was
157-309; root_layout_pass 19.2 ms (expected 2-5); text_layout_flow 288 (expected ~0); taffy misses 618;
fc_inline 1234; fc_flex_grid 203 (26 ms); plus three VirtualView passes (23-86 ms) per tick. Task: find what
still re-flows ~10% of the page every tick, why a knob tick lays out the VirtualView child DOMs at all; RED
with counts, fix, expected numbers. Report scripts/LAYOUTPERF8B_<date>.md.

## 8B DONE
- (this section)

## 8B NEXT
- read /Users/fschutt/Development/azul-work/lp8/tick.log (a tick block: which spans), re-run with
  AZ_TAFFY_DEBUG / AZ_RECON_DEBUG to name the missing nodes.
