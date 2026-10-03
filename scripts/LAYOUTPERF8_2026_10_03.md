# LAYOUTPERF8 - a 16 px slide re-laid out the whole page (wave 8, 2026-10-03)

Branch `wt/layoutperf8` (base e290321da, wave 7 integrated). Found by ANIM8 (report section 4, lead (a)):
each frame of the AzWidgets switch knob's `margin-left` slide re-laid out almost the whole window. Seen
again on the prebuilt wave-7 AzWidgets (headless, capped runner), root-caused, RED first, fixed. Nothing
compiled (house rule) - the numbers after the fix are expectations, measured how below.

## Seen (before)

`scripts/layoutperf8_tick_scenario_gen.py` (AZ_E2E: click the first Switch, then single 16.7 ms ticks of the
scripted clock; each tick is an `incremental_relayout` through the css-dirty channel), prebuilt AzWidgets
900x1300, `AZ_PROFILE=cpu`, per tick relayout:

| | per tick |
|---|---|
| `incremental_relayout` wall time | 309 / 183 / 159 / 157 ms |
| `root_layout_pass` | 107 - 229 ms |
| `text_layout_flow` (text runs broken into lines again) | 2853 |
| `fc_inline` / `fc_block` / `fc_flex_grid` | 7326 / 510 / 268 |
| `taffy_cache_get_miss` / `_hit` / `taffy_final_layout_stale` | 12181 / 17812 / 1738 |
| `size_cache_miss` (block per-node cache) | 559 |
| `reconcile_and_invalidate` | 5.5 - 8.7 ms |
| `generate_display_list` | ~6 ms |
| `fp_clean` (layout nodes found unchanged) | 2231 of 2231 |

Every layout node was a clean clone - nothing in the DOM changed - and still every flex item was laid out.

## Root cause

Every relayout that is not the resize fast path RECONCILES: `reconcile_recursive` clones every clean node of
the previous tree into the new one. `LayoutTreeBuilder::clone_node_from_old` (layout_tree.rs) CLEARED the
clone's `taffy_cache` and `measured_content_sizes` (deac0bebb, 2026-08-06). So after every reconcile every
flex / grid item had an empty measurement cache. The dirty knob is correctly lifted to the body (its
containers are flex all the way up - `promote_layout_roots_to_containers`), the body's flex algorithm runs,
and every item it asks about misses: its whole subtree is measured (min / max / definite) and laid out
again, every IFC re-flowed (the single-slot inline cache thrashes between the measure and the final width
type). The block path's per-node cache (`cache_map`) survives the reconcile (remapped by DOM id) - but flex
items never consult it (`compute_non_flex_layout` calls `layout_formatting_context` directly).

Why the clear was there: deac0bebb's ribbon bug ("tab 2's label 1.5 px low after a tab switch"), diagnosed
then as "measurements taken beside old siblings". A measurement is a function of the node's subtree and
its keyed inputs only; the real cause was a memo with side effects - a FINAL layout served from taffy's
cache after a MEASURE of the same tab had re-placed its children (1.5 px = half the difference between the
measured content height and the final height, `align-items: center`). That class is fixed at its point of
use since c60844cab (`NodeCache::final_layout_current`: a final is served only while nothing computed the
node since) plus the "a measure hands the node's size back" fix in `compute_non_flex_layout`. The resize
fast path has kept every taffy cache across passes since 4d0aa30c5 under exactly those guards.

## Fix (39f090082, d6ad6d687)

- `clone_node_from_old` keeps the flex measurements (the doc comment explains what invalidates them).
- `reconcile_recursive`: a clone drops them only where the dirty marks cannot see a change -
  under a restyled node (own or ancestor inline CSS / classes / state: inherited values may have moved),
  and when it was paired by POSITION with an old node of another id (it would carry that node's memo but
  get its own id's `final_layout_current` from the `cache_map` remap). That is exactly the old behaviour,
  for those nodes only.
- `try_reuse_anon_wrapper`: a matched anonymous block (the text beside a block) now carries the layout it
  last produced - inline layout, used size, offset, baseline, overflow, scrollbars, escaped margins - as a
  clone does. It carried none, so a memo above it skipped a wrapper that was never laid out in the new tree
  (latent bug A below - it already broke block pages; kept flex measurements would have broken flex ones).
- `ReconciliationResult::carried_indices` (old -> new index of every carried node); a cloned text node's
  `ifc_membership` is re-pointed at its IFC root's NEW index (it kept the old tree's index; a memo hit kept
  it stale - selection / caret lookups through `get_ifc_root_layout_index`).
- `layout_document` Step 1.2: a viewport change under a document that uses viewport units clears every
  flex measurement (no key carries the viewport). The resize fast path is unchanged.
- `paged_layout`'s reconcile path clears every flex measurement (its `cache_map` is resized by POSITION,
  so `final_layout_current` cannot vouch for a kept final there) - its behaviour before.
- Comments brought in line: `NodeCache::final_layout_current`, `layout_ifc`'s GlyphSwap exit, the dll
  ribbon test's doc (`switching_tabs_does_not_shift_the_other_tabs_text` stays the negative control).

(sections below: expected numbers, latent bugs, commits, api.json, compile risks, tests, left)
