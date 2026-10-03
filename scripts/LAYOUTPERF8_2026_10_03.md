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

## Expected after the build, and how to measure it

Re-measure exactly as before (one app at a time, capped):

```
python3 scripts/layoutperf8_tick_scenario_gen.py /tmp/lp8/tick.json        # 3 ticks; arg 4 = more
scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 150 --log /tmp/lp8/tick.log -- \
  env AZ_BACKEND=headless AZ_PROFILE=cpu AZ_E2E=/tmp/lp8/tick.json \
  /Users/fschutt/Development/azul/target/release/AzWidgets
grep -n "incremental_relayout\] START\|← incremental_relayout\|root_layout_pass \|text_layout_flow \|taffy_cache_get_miss \|taffy_final_layout_stale\|fc_flex_grid \|fc_inline " /tmp/lp8/tick.log
```

The tick relayouts are the `[incremental_relayout] START` blocks after the click's `regenerate_layout`
(the first `[CPU] === layout pass` block inside each is the root DOM; the small ones after it are the
VirtualView child DOMs). Expected per tick:

| | before | expected after |
|---|---|---|
| `text_layout_flow` | 2853 | 0 (a handful at most: only items on the knob's ancestor chain can re-flow) |
| `taffy_cache_get_miss` + `taffy_final_layout_stale` | 12181 + 1738 | tens (the chain: track, its row, the cards / sections above it, each 1-4 queries) |
| `fc_inline` / `fc_flex_grid` | 7326 / 268 | < 20 / a few |
| `root_layout_pass` | 107 - 229 ms | ~2 - 5 ms |
| `incremental_relayout` wall | 157 - 309 ms | ~20 - 35 ms (left: reconcile 5-9 ms, full display list ~6 ms, css_transition_tick 3-6 ms, the diag tree clone, positioning passes) |

The click's RefreshDom regenerate (`root_layout_pass` 203 ms, 3171 re-flows) should drop the same way: the
reconcile keeps every measurement except under the toggled switch (its inline style changed). Its
`create_from_dom` (~240 ms, the app's layout callback + cascade) is not layout and does not change.

The layout test `a_knob_frame_costs_the_same_on_a_page_twice_as_long` prints the counts of one knob frame
on a 40- and an 80-card page (`--nocapture`): after the fix both are equal and `text_flows` is 0.

## Latent bugs found while probing (prebuilt azul-doc e2e runner, `scripts/layoutperf8_e2e/`)

A. FIXED (RED: `text_beside_a_block_keeps_painting_when_a_sibling_restyles`, e2e
`text_beside_a_block_survives_a_sibling_restyle.json`). A block page; a box resizes through the css-dirty
channel (a stylesheet change on remount, or an animation override). The text beside a block in its
sibling vanished from the display list (`text_count` 3 -> 2): the anonymous block holding that text is
rebuilt by every reconcile and carried NO layout (`try_reuse_anon_wrapper`), and nothing lays the clean
sibling out again (it is only re-stacked). Every block page with a restyle-driven relayout and mixed
content (text and blocks in one box - mail HTML is full of it) lost that text until the next full layout.

B. NOT FIXED - documented (e2e `a_block_moves_with_its_own_margin.json`, `"expect": "fail"`). A block
child's own `margin-left` changed through the css-dirty channel does not move it (x stays 0; a fresh layout
puts it at 40). Root cause: the node is a clean CLONE, and a clone carries the box props (margins, padding,
borders) and `computed_style` / `formatting_context` resolved from the OLD cascade; the css-dirty fold
(`layout_document` Step 1.15) only marks it dirty, and a dirty block root is re-solved from its OLD slot
(`adjusted_cb_pos`) with the stale margins. Flex items are unaffected (taffy reads their styles fresh) -
which is why the AzWidgets knob moves.

C. NOT FIXED - documented (probe below). A block child's `height` changed through the css-dirty channel
(20 -> 60 px, no text inside) is applied to the child, but its auto-height parent stays 20 px and the next
sibling of the parent is not moved (it overlaps). The child is its own layout root (block in a block: no
lift), `reposition_clean_subtrees` only re-stacks the root's own siblings, and nothing re-sizes the
ancestors. (A child that holds text is lifted - its formatting context is `Inline` - which hides this.)

Probe for C (azul-doc e2e, `get_all_nodes_layout` after the remount): `#child` 60 tall, `#parent` 20,
`#after` at y = 20; scenario in the git history of this report's branch? No - kept here:
`mount <div id=root><div id=parent><div id=child></div></div><div id=after>after</div></div>` with
`#child { width: 50px; height: 20px }`, then the same with `height: 60px`.

Plan for B + C (a design pass, needs a build): (1) a css-dirty node with a layout scope is rebuilt FRESH in
the reconcile (pass the css-dirty set into `reconcile_and_invalidate`; treat it like `DirtyFlag::Layout`):
fresh box props, computed style and formatting context, its clean subtree still cloned; (2) its layout root
is lifted to its parent when the scope is `Full` (margins / float / position: the parent places it) and,
for size-changing scopes, on up through every ancestor whose block size depends on its content, stopping
at a definite-size box or a scroll container (a relayout boundary) or the tree root. With the caches kept
(this fix) that root pass costs the chain, not the page.

(sections below: commits, api.json, compile risks, tests, left)
