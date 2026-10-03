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
- 1a0ded9af progress section
- d099bb32e RED: text_after_a_block_is_carried_over_by_the_next_layout; the AzWidgets-like card gains
  inline content after a block (the knob-frame cost test is RED again)
- cea6b0840 GREEN: old_layout_index_of - the trailing inline run's children are matched with their old
  layout nodes (they were rebuilt fresh every reconcile)
- 137b1b2a4 RED a_virtual_view_leaves_its_hosts_font_chains_in_place (registered in all.rs)
- a669a62a5 GREEN: child-DOM pass stashes/restores the host's font chains + signature (merge, host wins)

## 8B FINDINGS
- The coordinator's "VirtualView passes 23-86 ms" are MICROseconds (the [CPU] table is in µs): each VV
  DOM pass is 0.14-0.48 ms (solver3_layout_document 138-485 µs).
- 607 of 618 taffy misses of a knob tick are in the form region (layout idx 2038-2227): DOM 3459 button
  "Send the raw form" + its p + text are FRESH every reconcile (trailing inline run lookup without the
  dom_to_layout fallback) -> ancestors to the body dirty -> the form column re-measured at several widths
  (size_cache_miss_sizekey_w 266 / _both 252, ifc_reflow_width_dd_big 272).
- the knob chain itself: 11 misses (n27, n32, n62, n65, n66).

- VirtualView passes per tick: layout_and_generate_display_list_impl clears EVERY layout result and
  calls virtual_view_manager.reset_all_invocation_flags() on every relayout -> every view callback is
  re-invoked and its child DOM laid out cold every tick (3 x 0.14-0.48 ms here). DOCUMENTED, not fixed
  (VV lifecycle design: needs an identity latch for the host's VirtualViewNode; plan in the report).
- font_chain_resolve 2.2 ms per relayout: the root re-resolved its fonts every pass because each VV
  child's pass overwrote the single chain cache + signature slot -> fixed (a669a62a5).
- Unprofiled (no AZ_PROFILE) tick on the wave-8 build: 39-43 ms; no-op relayout 12.4-12.7 ms; the CPU
  profiler itself adds ~15 ms per tick. DOM lints (AZ_SUPPRESS=all) change nothing measurable.

## 8B NEXT
- report scripts/LAYOUTPERF8B_2026_10_03.md
