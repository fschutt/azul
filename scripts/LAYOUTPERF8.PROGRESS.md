# LAYOUTPERF8 progress (wave 8, branch wt/layoutperf8, base e290321da)

Task: a one-node change (the AzWidgets switch knob's margin-left tween) re-lays out almost the whole page
(118-290 ms per frame, 2853 text re-flows for a 16 px move; ANIM8 report section 4). Root-cause why the
cached layout is not reused for the unchanged parts; RED test counting re-laid-out nodes / text re-flows for a
one-node change in a large tree; fix. Never compile; never touch page_breaks.rs.

## DONE
- progress file (first commit)
- scripts/layoutperf8_tick_scenario_gen.py (AZ_E2E: click the switch, 3 single-frame ticks; run with AZ_PROFILE=cpu)

## MEASURED BEFORE (prebuilt AzWidgets, wave-7 build, headless 900x1300, AZ_PROFILE=cpu, /tmp/lp8/tick.log)
Each tick = `incremental_relayout` 309 / 183 / 159 / 157 ms. Per tick: root_layout_pass 107-229 ms,
text_layout_flow 2853, fc_inline 7326, fc_flex_grid 268, taffy_cache_get_miss 12181 / hit 17812,
taffy_final_layout_stale 1738, size_cache_miss 559, reconcile_and_invalidate 5.5-8.7 ms,
generate_display_list ~6 ms, fp_clean 2231 (every layout node a clean clone - nothing fresh).
The click's RefreshDom regenerate: root_layout_pass 203 ms, the same 3171 text flows.

## ROOT CAUSE (found)
`LayoutTreeBuilder::clone_node_from_old` (layout_tree.rs ~3825) CLEARS the clone's `taffy_cache` and
`measured_content_sizes` (deac0bebb, Aug 6). Every relayout that reconciles (every restyle / animation
tick / RefreshDom - only the resize fast path skips it) clones every clean node, so EVERY flex / grid item
in the window has an empty taffy cache. The css-dirty knob is lifted to the body (all-flex chain), the
root pass runs the body's flex algorithm, and every item below misses -> its whole subtree is measured
(min / max / definite) and laid out again, every IFC re-flowed (single-slot IFC cache thrash). The
per-node NodeCache (block path) survives the remap, but flex items never consult it
(`compute_non_flex_layout` calls `layout_formatting_context` directly).
deac0bebb's own diagnosis ("measured beside old siblings") was a memo-with-side-effects bug that
c60844cab later fixed at its root (`NodeCache::final_layout_current`) plus the "a measure hands the size
back" fix in compute_non_flex_layout - so the clear is no longer needed for correctness.

## IN PROGRESS
- RED test layout/tests/a_one_box_slide_re_lays_out_only_its_ancestors.rs (probe counts)

## NEXT
- RED test, register in all.rs; then GREEN: (1) clone keeps taffy measurements unless the node or an
  ancestor restyled (reconcile_recursive), (2) a matched anonymous wrapper carries its old layout state
  (try_reuse_anon_wrapper) so a memo above it never skips a never-laid-out wrapper, (3) viewport-unit
  guard in layout_document Step 1.2.

## Decisions / open questions
- Conservative: a clone under a restyled node (own or ancestor inline/class/state change) still clears
  its taffy cache (today's behaviour there) - inherited values may have moved.
