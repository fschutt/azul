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

- 6d3d770b4 RED layout/tests/a_one_box_slide_does_not_re_lay_out_the_page.rs (3 tests, registered in all.rs)

## FOUND WHILE PROBING (prebuilt azul-doc e2e, probes in /tmp/lp8/e2e, to be copied to scripts/layoutperf8_e2e)
- LATENT BUG A (RED test 3): a block page, a box restyles via css dirt -> the text beside a block in its
  sibling (anonymous wrapper rebuilt by the reconcile, `try_reuse_anon_wrapper` carries no layout state)
  is never laid out: display list text_count 3 -> 2. Fixed by GREEN (2).
- LATENT BUG B: a block child of a block whose own margin-left changes through css dirt does not move
  (x stays 0, fresh layout gives 40): the dirty root is re-solved from its OLD slot
  (mod.rs adjusted_cb_pos) and `reposition_block_flow_siblings` keeps a dirty root's position. Not the
  perf issue; decide after GREEN (lift Full/SizingOnly css-dirty roots to the parent?).
- ifc_membership of cloned text nodes keeps the OLD tree's IFC-root index (stale after an index shift
  elsewhere) - a memo hit on a clean subtree keeps it stale (selection / caret lookups). GREEN (4) remaps.

- 39f090082 GREEN: clone keeps taffy measurements (clear only under a restyle), anon wrapper carries its
  layout, carried_indices + ifc_membership remap, vw guard (mod.rs Step 1.2), paged_layout clears all,
  comments + dll ribbon test doc.

- 04db13759 scripts/layoutperf8_e2e/ (text beside a block: FAIL on wave 7 -> expect PASS; margin: xfail)
- d6ad6d687 a clone paired by POSITION drops its measurements (flag and memo must describe one node)
- 22acb1c28 test a_page_laid_out_again_matches_a_fresh_window (cold oracle: knob frame, rebuild, knob frame)

## DECISIONS
- Bug B (css-dirty block margin not applied: clones carry stale box props; root re-solved from its old
  slot) and bug C (css-dirty block size change does not grow auto-height ancestors / move what follows -
  probe lp8_g: child 60 tall, parent stays 20, #after overlaps) are DOCUMENTED, NOT FIXED: they are the
  css-dirty channel's block-layout semantics (relayout boundaries + rebuilding css-dirty nodes fresh), not
  the per-frame cost; a fix needs a design pass + a build. Plan in the report.
- The wasm-lift diagnostic deep clone `cache.tree = Some((*new_tree).clone())` (mod.rs ~1206) is left
  (another session's diag scaffolding); listed as a follow-up lever.

## IN PROGRESS
- report scripts/LAYOUTPERF8_2026_10_03.md

## NEXT
- write + commit the report; final progress

## Decisions / open questions
- Conservative: a clone under a restyled node (own or ancestor inline/class/state change) still clears
  its taffy cache (today's behaviour there) - inherited values may have moved.
