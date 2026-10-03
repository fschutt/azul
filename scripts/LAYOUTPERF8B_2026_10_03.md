# LAYOUTPERF8B - what still re-laid out ~10% of AzWidgets per knob frame (wave 8, 2026-10-03)

Branch `wt/layoutperf8b` (base 3428223f1 = wave 8 integrated, LAYOUTPERF8 included). Nothing compiled
(house rule); the "after" numbers are expectations.

## Seen (the coordinator's re-measure + mine, prebuilt wave-8 AzWidgets, headless 900x1300)

Per knob tick (`incremental_relayout`), with `AZ_PROFILE=cpu`: 55-58 ms wall; `solver3_layout_document`
33.6 ms, of it `root_layout_pass` 19.4 ms, `generate_display_list` 6.0, `reconcile_and_invalidate` 5.3,
`prepare_layout_context` 2.6; `fc_flex_grid` 203, `fc_block` 69, `fc_inline` 1234, `text_layout_flow`
288 (272 of them `ifc_reflow_width_dd_big`), `size_node` 654 with `size_cache_miss` 518
(`sizekey_w` 266, `sizekey_both` 252), `taffy_cache_get_miss` 618, `taffy_final_layout_stale` 303. Outside
the solver: `css_transition_tick` 2.9 ms, `font_chain_resolve` 2.2 ms, the e2e harness's
`debug_timer_callback` 2.6 ms.

Without `AZ_PROFILE` (my runs, `/tmp/lp8b/noprof.log`): a tick is 39-43 ms wall and a no-op incremental
relayout 12.4-12.7 ms - the CPU profiler itself adds ~15 ms per tick (it records every span). Quote
unprofiled wall times; use the profile for counts and shares.

The three "VirtualView passes 23-86 ms" are MICROseconds: the `[CPU]` table is in µs; each view's pass is
`solver3_layout_document` 138-485 µs (section "VirtualView passes" below).

## Root cause 1 - the run that ends a box after a block was rebuilt fresh on every pass (FIXED)

`AZ_TAFFY_DEBUG=1 AZ_RECON_DEBUG=1` on the same scenario (`/tmp/lp8b/taffy.log`, the second tick):

- 607 of the 618 taffy misses are layout nodes 2038-2227 - one region of the page; the knob's own chain
  is 11 misses (layout nodes 27, 32, 62, 65, 66).
- The reconcile prints, on EVERY tick: `COUNT MISMATCH parent dom 3459: old_relevant=0 new_relevant=1`,
  `intrinsic_dirty += 2228 (dom 3461, Layout)`, `2227 (dom 3460)`, `2226 (dom 3459)`; the profile has
  `recon_old_idx_none 3`, `fp_new_node 3`.
- DOM 3459-3461 (`get_node_hierarchy`): the form's "Send the raw form" `<button>`, its `<p>`, its text -
  the LAST inline-level child of the form column (div 3131; its last child 3462 is not laid out), i.e. a
  TRAILING inline run after block children, in an anonymous block.

`reconcile_recursive` (solver3/cache.rs) finds an inline child's old layout node in two branches. The run
before a block falls back from the parent's direct old children to the whole old tree
(`dom_to_layout`) - the children sit in the anonymous block, not under the parent. The TRAILING run did
not fall back (`old_children_by_dom.get(..)` only), so its children were never found: rebuilt fresh with
their subtree on every reconcile, intrinsic-dirty, and through `mark_dirty` / the Step 1.2 closure they
cleared the per-node and flex caches of every ancestor up to the body. The form column is a flex item
sized by its content, so every recomputation measured it at several widths, its 20 block children missed
the single-width size slot (`sizekey_w`), their flex rows were laid out again (`fc_flex_grid` 203) and
their text re-flowed at the probe widths (`dd_big` 272). LAYOUTPERF8's test page had text BEFORE a block
only - the shape that worked.

This is not AzWidgets-specific: any box that ends with inline content after a block (mail HTML, documents,
labels after a block) made every relayout of an unchanged page dirty from the root - even a relayout with
nothing changed (no early exit).

Fix (cea6b0840): one lookup, `old_layout_index_of(old_children_by_dom, old_tree, dom_id)`, for the three
sites that match a child built inside a box the parent's layout made around it - both inline-run branches
and `reconcile_child_under` (anonymous table boxes, which had the same fallback inline).

RED (d099bb32e), `layout/tests/a_one_box_slide_does_not_re_lay_out_the_page.rs`:
- `text_after_a_block_is_carried_over_by_the_next_layout`: a box with a block, then text and a span; two
  relayouts of the unchanged page must build 0 nodes fresh (`last_reconcile_fresh`) and find 0 dirty
  (`last_intrinsic_dirty`). RED: 3 fresh every pass.
- The AzWidgets-like card gains inline content after its block ("More about note i" + a span), so
  `a_knob_frame_costs_the_same_on_a_page_twice_as_long` is RED again on the wave-8 build (every card's
  trailing run rebuilt per frame - the cost doubles with the page) and GREEN with the fix.

## Root cause 2 - the host re-resolved its fonts on every pass because of its VirtualViews (FIXED)

`font_chain_resolve` 2.2 ms in every tick, `font_load_missing` 1 in the root's pass and 1 in each view's
pass: the root resolves its whole page's font stacks (the DOM scan is the 2.1 ms) on every relayout. The
font manager has ONE chain cache and ONE signature of the stacks it was resolved for
(`last_resolved_font_stacks_sig`); the skip ("font requirements unchanged") needs this DOM's signature to
match. A VirtualView's child DOM is laid out INSIDE its host's pass, after the host, and replaces both with
its own - so the host never matched its own signature again. Between passes the window's chain cache also
described the last view laid out, not the window.

Fix (a669a62a5): `layout_dom_recursive_with_viewport` already swaps the host's layout cache out for a child
DOM's pass; the font chains and their signature get the same treatment. The child resolves against an
empty slot (as it effectively did - its signature never matched the host's); afterwards the host's chains
and signature are back, and the child's chains for the stacks only it uses are kept beside them (text
edited inside a view still finds its chains; for a stack both use, the host's chain wins).

RED (137b1b2a4), `layout/tests/a_virtual_view_leaves_its_hosts_font_chains_in_place.rs`: a serif host with
a monospace view; after a pass the cache must still hold the host's serif chain, and a relayout of the
unchanged host may resolve fonts at most once (the view's) - `font_load_missing` was 2.

## Expected after the build (per knob tick, AzWidgets 900x1300 headless)

| | wave 8 (measured) | expected |
|---|---|---|
| `text_layout_flow` | 288 | 0 - 3 |
| `taffy_cache_get_miss` + `taffy_final_layout_stale` | 618 + 303 | ~11 + a few (the knob's chain) |
| `fc_flex_grid` / `fc_block` / `fc_inline` | 203 / 69 / 1234 | ~1-3 / a few / < 20 |
| `size_cache_miss` | 518 | < 10 |
| `root_layout_pass` | 19.4 ms | ~1 - 2 ms |
| `font_chain_resolve` in the root's pass | 2.2 ms | absent (each view's pass: ~20 µs) |
| `solver3_layout_document` | 33.6 ms | ~13 - 15 ms (reconcile ~5, display list ~6, the rest) |
| wall, `AZ_PROFILE=cpu` | 55 - 58 ms | ~30 - 35 ms |
| wall, unprofiled | 39 - 43 ms | ~20 - 25 ms |

Measure exactly as before (one app at a time, capped):

```
python3 scripts/layoutperf8_tick_scenario_gen.py /tmp/lp8/tick.json 900 1300 3
scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 150 --log /tmp/lp8/tick.log -- \
  env AZ_BACKEND=headless AZ_PROFILE=cpu AZ_E2E=/tmp/lp8/tick.json \
  /Users/fschutt/Development/azul/target/release/AzWidgets
# counts: the 74-phase [CPU] block inside each tick's [incremental_relayout] START .. COMPLETE
# unprofiled wall: the same without AZ_PROFILE, grep "← incremental_relayout"
# which nodes still miss: add AZ_TAFFY_DEBUG=1 AZ_RECON_DEBUG=1 (one line per lookup / dirty node);
#   the reconcile must print NO "COUNT MISMATCH" / "intrinsic_dirty" lines for the root DOM in a tick.
```

## VirtualView passes - why a knob tick lays out the view DOMs (DOCUMENTED, not fixed)

`layout_and_generate_display_list_impl` (window.rs) runs on every relayout, the animation tick's included,
and starts with `self.layout_results.clear()` - every child DOM's result too - followed by
`virtual_view_manager.reset_all_invocation_flags()` ("the child DOM was just destroyed by clear()"). So
`check_reinvoke` answers `InitialRender` for every view, every view's callback (user code) runs again and
its child DOM is laid out cold, on every frame of every animation. In AzWidgets that is three small views:
~0.14-0.48 ms each (the "35 / 32 phase" blocks), plus the callbacks. In an app whose views are heavy
(page views of a document, map tiles) this is the dominant per-frame cost of any animation.

Plan (window.rs VirtualView lifecycle - not done here: it touches child-DOM lifetime, the natural-size
second pass, scroll states and `previous_child_arenas`, and needs a build to verify): on the RELAYOUT
entry (`new_generation == false`), keep the child results whose host view is unchanged and do not reset
their invocation flags. "Unchanged" must be proven, not assumed, because tests and some hosts pass a new
DOM through the relayout entry: record in `VirtualViewState` an identity of the `VirtualViewNode` it was
last invoked for (callback pointer + the dataset `RefAny` instance), and keep a view only when the host
node at the same `(dom, node)` carries the identical node - true for an incremental relayout of the
retained `StyledDom`, false for any rebuilt DOM. `check_reinvoke` then decides as designed
(`BoundsExpanded`, edge scrolls). Expected: 0 view passes and 0 view callbacks per knob tick.

## Left (the rest of a ~20-25 ms frame after this branch, largest first)

1. Full display list, ~6 ms: a css-dirty pass never splices (`structure_ok` needs an empty css_dirty and an
   unchanged cascade epoch, and an override bumps the epoch). A pass whose css dirt is layout-only could
   splice with the css-dirty nodes (and their subtrees, for inherited paint) in the re-emit set.
2. The reconcile, ~5 ms: an animation tick changes no DOM node, yet fingerprints and clones all 2231 layout
   nodes. A latch like `resize_only_hint`, set by the tick's relayout when only overrides changed, could
   take the retained tree as is - but the DL patch arm then has to treat css-dirty nodes as re-emitted
   (see 1), or it would splice their stale paint.
3. `a11y_update_tree` ~3 ms per relayout (seen in the no-op relayout) - the accessibility tree is rebuilt
   for a tick that moved one box.
4. `css_transition_tick` ~3 ms (ANIM8's area).
5. VirtualView re-invocation per tick (above).
6. From LAYOUTPERF8, still open: bugs B and C (css-dirty block layout), the wasm-lift diagnostic deep tree
   clone in `layout_document`.
7. The per-node size cache keeps ONE entry per constraint class (`classify_size_key`): a block measured at
   several definite widths in one pass (the content-sized form column under its flex parent) misses every
   time it is measured again (`sizekey_w`). Not on the knob's path any more, but every relayout that does
   touch such a column pays it.

## Commits (wt/layoutperf8b, oldest first)

| Commit | What |
|---|---|
| 1a0ded9af | progress section |
| d099bb32e | RED: text after a block carried over; the AzWidgets-like card gains a trailing inline run |
| cea6b0840 | GREEN: `old_layout_index_of` - one old-node lookup for inline-run and anonymous-table children |
| 137b1b2a4 | RED: a VirtualView leaves its host's font chains in place (registered in `tests/all.rs`) |
| a669a62a5 | GREEN: the child-DOM pass stashes and restores the host's font chains + signature |
| (+ progress / report commits) | |

## api.json

No change (private functions and existing pub fields only).

## Least sure to compile

- cache.rs `old_layout_index_of(&old_children_by_dom, old_tree, inline_dom_id)` in the two inline-run
  loops (`old_children_by_dom` is the local `BTreeMap<NodeId, usize>`; `old_tree: Option<&LayoutTree>` is
  Copy) and `old_layout_index_of(old_children_by_dom, old_tree, child_dom_id)` in `reconcile_child_under`
  (there it is already a `&BTreeMap`).
- window.rs child branch: `core::mem::take(&mut self.font_manager.font_chain_cache)` (a `HashMap`, Default)
  and `.entry(key).or_insert(chain)`; `last_resolved_font_stacks_sig.take()` (an `Option<u64>`).
- The new test: `azul_core::callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn}`,
  `VirtualViewCallback::create(render_view)`, `VirtualViewReturn::with_dom(dom, rect, rect)` (as in
  `virtual_view_natural_size.rs`), `lw.font_manager.font_chain_cache` keys' `font_families`.

## Test commands (for the parent)

```
cargo test -p azul-layout --test all a_one_box_slide_does_not_re_lay_out_the_page \
  a_virtual_view_leaves_its_hosts_font_chains_in_place -- --nocapture
# around the reconcile and the view lifecycle:
cargo test -p azul-layout --test all cache_and_dirty_propagation resize_relayout_bug subtree_relayout \
  text_beside_a_block virtual_view_natural_size the_resize_fast_path_paints_what_a_relayout_paints \
  contenteditable_e2e ifc_caching an_anonymous_table_cell a_rows_stray_child
cargo test -p azul-layout --lib solver3::
cd dll && cargo test switching_tabs_does_not_shift_the_other_tabs_text
./target/release/azul-doc e2e e2e
./target/release/azul-doc e2e scripts/layoutperf8_e2e     # 1 passed, 2 xfailed (unchanged)
```
