# A3-window: quadratics and per-frame O(nodes) work in `window.rs`, reconciliation / diff, restyle and list patching

Area agent A3-window of the 2026-11-21 audit. Blind: nothing here was compiled or run.
Branch `audit/A3-window`. Base: `audit/base` (`f48bcc7b4`). Line numbers are those of the base commit unless a
block says otherwise. n = 12.5k nodes and 17.5k items in the view's child DOM, 2,118 reference frames, ~300 nodes and
60-70 items in the window's own (root) DOM.

## Claims

Functions this area rewrote or restructured (final state):

- `core/src/diff.rs`: `reconcile_dom` (STEP 1 and passes B2 / B3: `ShallowTiers`, `ParentGate`),
  `NodeDataFingerprint` (`compute`, `diff`, the three inline hashes), `hash_inline_style_by_reach`.
- `layout/src/window.rs`: `reconcile_rematerialized_child`, the stash / retire block of
  `layout_and_generate_display_list_timed` (3515-3531 and 3578-3579), the mount loop and the entered loop of
  `finish_reconciliation`, `patch_restyled_decorations` (new, directly above `regenerate_display_list_for_dom`),
  `manager_state_fingerprint`.
- `layout/src/solver3/display_list/node_patch.rs` (new file, a child module of the builder).
- `dll/src/desktop/shell2/common/event.rs`: `repaint_restyled_nodes` (new), the paint branch of
  `apply_hover_restyle` and `apply_active_restyle`, the test module of the state restyles.

Minimal patches in files of other areas (a few lines each, none inside a claimed function):

- A3-core: `core/src/animation.rs` `anim_keys_for_moves` (body), `layout/src/solver3/display_list.rs` (one line:
  `pub(crate) mod node_patch;`).
- A3-solver: `layout/src/solver3/cache.rs` `unshare` (a test-only counter `TREE_COPIES`).
- A11: `layout/src/e2e/full.rs` (`FrameReportResponse::dl_node_patches`), `layout/src/e2e/hover_text_cost_tests.rs`
  (one bound).
- A1 / A10: `dll/src/desktop/shell2/headless/mod.rs` (one bound of the hover sweep test).
- `FrameReport` (`window.rs:867`, outside the ranges of this area): one field, `dl_node_patches`.

A9's commit `f16fd2ab4` (the dropped-child loop of `remap_node_ids`, `window.rs:30405`) does not touch a line this
branch changed: no conflict expected.

## The four answers

### 1. One hover enter or leave over a label

Before (base): `apply_hover_restyle` (`event.rs:1083`) per DOM of the change:
1. `node_states` (`window.rs:10917`): the entered / left nodes and their parts: O(changed nodes).
2. `StyledDom::restyle_on_state_change` (A3-core's): the span's background changes
   (`hover_text_tests.rs`: `BackgroundContent` / `BackgroundColor`, scope `None`).
3. `seed_state_change_transitions` (10964): returns at once for a child DOM (`dom_id != ROOT_ID`).
4. `stage_css_dirty` (`event.rs:904`): O(entries already staged for this DOM), a `BTreeMap` rebuilt per call.
5. `repaint_restyled_dom` -> `regenerate_display_list_for_dom` (26848): `sync_css_gpu_values` over the DOM (core,
   not read), `get_scroll_states_for_dom`, `generate_display_list` over all 12.5k nodes -> 17.5k items, one more
   pass over the items for the view placeholders (27014), `apply_text_tweens`, the incremental a11y update.
   Then the backends: the CPU compositor diffs old against new list (A1); the GPU path scans the images and halos
   of ALL lists (`collect_image_resource_updates_and_halos_left`, `wr_translate2.rs:731`) and translates the whole
   child list again (`submit_display_lists`), see "For area A4".

After `25cf3821f` (A3-window-07): steps 1-4 as before, then `patch_restyled_decorations`:
- `plan`: one `paint_node_background_and_border_inner` for the span (1 item), four probes for the span, its
  ancestor chain (depth 3-4) and its subtree (2 boxes); a `LayoutContext` is set up as for a build
  (`get_scroll_states_for_dom`, `build_cursor_locations`, `build_text_selections_map`: O(scroll nodes + carets));
- `locate`: ONE pass over `layout_node_mapping` (17.5k tags of 24 bytes, no item is read) and a comparison of the
  old and the new run (0-1 items);
- `commit`: one pass over the tags again for the text proofs (items are read only for the label's own runs), then
  the splice: 1 item written; on an insert or a removal the 17.5k - at items behind it move by one slot in six
  parallel vectors (a `memmove`), `FrameIndex` dropped (rebuilt by the next CPU frame, A1's).
- `Arc::make_mut`: in place when nobody else holds the list; a copy of the whole list when the CPU compositor
  holds the frame it last drew (as for every in-place patch: tiles, caret, tweens); a move to a new allocation
  when only the GPU record's `Weak` holds it.
So: items painted 1, items written 1, items moved <= 17.5k (memmove), tags scanned 2 x 17.5k, nodes touched ~6.
What stays O(items) is outside this area: the CPU damage diff, the list copy while a backend holds the old one,
the GPU resource scan and re-translation of the child list.

### 2. One pan move inside the overscan; one zoom step

Pan (a scroll offset / `patch_virtual_view_content_offset`, A2-cpu's) and zoom (`set_css_property(transform)` on
the picture root -> `apply_node_css_change` -> `write_bound_gpu_properties`, 10789: `set_user_property_override_fast`
+ `refresh_transform_value_of`, O(1) in nodes, tier `Paint`) build no list and lay nothing out (confirmed by
reading; the counts in PERFORMANCE.md agree). What the UI thread does per FRAME outside the renderer, in this area:
- `prepare_frame_content` / `prepare_frame_gl` (10040 / 10102): `manager_state_fingerprint` (O(scroll nodes); a
  `format!` per frame while a text session is open: fixed, A3-window-10);
- `invoke_image_callbacks_into_overlay` (11392): a walk of EVERY layout node of EVERY DOM (12.5k + 300) to find
  callback images: A3-window-05, open;
- `invoke_background_image_callbacks_into_overlay` (11533): a map of the callback images registered by id; with
  the `materials` feature (4 built-in material callbacks in every window) a walk of every ITEM of every list:
  A3-window-06, open. Without the feature (the editor's build as far as its Cargo.toml says) it returns at 11570;
- `refresh_scrollbar_gpu_cache_for_cpu_frame` (A6's managers, not read).
Not per frame: `scan_used_fonts`, `scan_used_images` (no production caller), `images_in_display_lists` (per list
the GPU takes, see A4), `scan_for_virtual_views` and the scrollbar-key loop (per layout pass).

### 3. A settle render of the view (12.5k nodes replaced)

`process_pending_virtual_view_updates` -> `invoke_virtual_view_callback_impl` (12399), in order:
| step | where | cost class | note |
|---|---|---|---|
| the app's callback | editor | linear | 24 ms measured |
| `fingerprint_dom` | `diff.rs:2987` | linear, 2 hashes per node | decides `reclaim_unchanged_child` |
| style (`style_user_dom_inheriting`) | A3-core | A3-core's verdict | |
| old + new arena deep-copied | 29738-29754 (and 3524 on a new generation) | linear, large constant | **fixed** A3-window-04 |
| `reconcile_dom` keys, A1-A3, B1 | `diff.rs:865` | linear; 3 `BTreeMap`s of n entries, 2-3 node hashes per node | B1 pops are amortised O(1) |
| `reconcile_dom` B2 / B3 | `diff.rs:1183` | **superlinear**: unmatched new x old of the same structural hash | **fixed** A3-window-01 |
| `css_diff_between` when the view's sheet moved | 34742 | linear in the matched nodes (read by signature and loop head only) | |
| `remap_node_ids` | 29871 | linear in the managers' entries (not read in full) | |
| layout (`layout_dom_recursive_with_viewport`) | A3-solver | A3-solver's verdict; a node whose inline style changed is built fresh | A3-window-08 gives the layout the classification |
| list build, scrollbar keys, `scan_for_virtual_views` | 9498-9781 | linear, once per pass | tree and list are shared `Arc`s, not copies |
| `start_view_enter_tracks` | 29842 | linear in the entered nodes (one property lookup each) | |
Superlinear before this branch: B2 / B3 only (in this area). Linear with a large constant: the arena copies (gone),
the node hashes (STEP 1's two per old node and STEP 3's two per matched pair: gone), the `BTreeMap`s of
`reconcile_dom` (kept).

### 4. Is a flat list of 12k absolutely positioned siblings safe in this area?

**Yes after this branch, for the view's child DOM**: the child's path is `reconcile_rematerialized_child` ->
`reconcile_dom` (linear in n after A3-window-01, independent of the number of siblings: the keys are one top-down
pass since Z9, the queues are per (hash, parent)) and the patch paths (per node, independent of siblings).
**For a ROOT DOM with a flat list** the base had three sibling-quadratic walks in `finish_reconciliation`, all fixed
here: `anim_keys_for_moves` (A3-window-02), the mount keys and `mounted.contains` (A3-window-03). Remaining
per-node sibling walk, not on a per-node loop: `calculate_reconciliation_key` for a zombie's nodes (17529) and in
`park_culled_tracks` (18061, not read), `calculate_contenteditable_key` (A6's callers at 4535 / 5569 / 6115 / 6578:
one walk of the earlier siblings per call, for contenteditable nodes only, see "For area A6").
Evidence: the size sweeps `the_shallow_tiers_take_a_step_per_node_when_a_view_grows`,
`the_animation_keys_of_a_rebuild_take_a_step_per_node`, and Z9's own.

## Findings

### A3-window-01 reconcile_dom, passes B2 / B3: unmatched new nodes x old nodes of the same structural hash
- file:line: `core/src/diff.rs:1183-1249` (queues built at 921-936)
- class: quadratic
- evidence: `old_hashed[hash].iter().position(|old| !consumed[old] && parent_agrees(old))` and the same on
  `old_structural`; consumed entries never left the queues; `calculate_structural_hash` (`dom.rs:4090`) ignores a
  text's content and every inline style, so all texts share one queue and all spans of a class another. U new nodes
  without a positional twin x q old nodes of the kind: 6k -> 12.5k nodes about 10-20 M steps, worst n^2 / 8.
- fix: `ShallowTiers`: queues per (hash, `ParentGate`) over the still unconsumed old nodes, built on first use; STEP
  1 no longer hashes every old node twice; STEP 3 asks `has_update_callback` before hashing both nodes of a pair.
- test: `core/src/diff_test.rs` `audit_a3_window_tests`: three behaviour tests and the sweep (601 / 2,401 steps).
- severity: high
- status: fixed 0e85f9a64

### A3-window-02 anim_keys_for_moves: the Z9 quadratic again, on every root rebuild
- file:line: `core/src/animation.rs:652-676`, called from `finish_reconciliation` (`window.rs:17650`)
- class: quadratic (s^2 / 2 for s siblings; root DOM only)
- fix: `precompute_reconciliation_keys` once. test: `the_animation_keys_of_a_rebuild_take_a_step_per_node`.
- severity: medium
- status: fixed 59a59a6ef

### A3-window-03 finish_reconciliation: a key walk per mounted node, Vec::contains per entered node
- file:line: `window.rs:17707-17713`, `17793`
- class: quadratic (m mounted x s siblings; entered x mounted; root DOM only)
- fix: the keys once and only while a zombie exists; the mounted nodes as a set.
- test: none of its own (no step counter outside azul-core's cfg(test)); the linear pass is guarded in azul-core.
- severity: medium
- status: fixed 54b7119bd

### A3-window-04 a view's render deep-copied its arenas two to three times (also A9's H16)
- file:line: `window.rs:3519-3531`, `29738-29754`
- class: waste (linear, large constant: 2 x 12.5k `NodeData` clones and drops per render)
- fix: the arenas are borrowed where they lie (live result, retired result of the running pass, stash); the funnel
  copies an arena only for a child no view rendered again in the pass.
- test: `layout/src/audit_tests/a3_window.rs`: three behaviour tests (queued update, new generation, hidden view).
  The third assumes a `display: none` host lays out no box for the view (not verified).
- severity: medium
- status: fixed d5a1ff858

### A3-window-05 per-frame walk of every layout node of every DOM for callback images (Z4)
- file:line: `window.rs` `invoke_image_callbacks_into_overlay`, phase 1
- class: waste, per frame: 12.8k hot nodes + their `NodeData` per CPU and per GL frame.
- fix: the callback-image nodes are kept per DOM for the DOM's generation (A3-window-19) and collected again only
  when it moved; each kept box is read fresh every frame (its rect, its callback).
- test: `a_frame_of_an_unchanged_dom_walks_no_layout_node_for_callback_images` (counter `CALLBACK_IMAGE_SCAN`),
  `a_canvas_that_comes_with_a_re_render_is_drawn_by_the_next_frame`.
- severity: medium
- status: fixed d305eaae7

### A3-window-06 per-frame walk of every display-list item for callback backgrounds (Z4)
- file:line: `window.rs` `invoke_image_callbacks_into_overlay`, phase 2 (11613-11721 at the base)
- class: waste, per frame when a callback image is registered by id: always with the `materials` feature (`dist`,
  `build-dll`: `dll/Cargo.toml:807`, `texture.rs:153`). 17.5k items, a hash and two map lookups per `Image` item.
- fix: see the fix commit (a list revision per DOM beside the generation; the walk runs once per revision).
- severity: medium
- status: open: being fixed in round 2 (list revision bumped by every writer of a list)

### A3-window-07 hover / press over a label rebuilt the view's whole list (Z3, with the insert case)
- file:line: `event.rs` `repaint_restyled_nodes` -> `window.rs` `patch_restyled_decorations` ->
  `solver3/display_list/node_patch.rs`
- class: waste, per enter / leave: O(nodes + items) (7.5-13 ms per move measured on 1.6k-4.4k-node samples)
- fix: `patch_restyled_decorations` + `node_patch.rs` (see answer 1 and the module's doc). The insert place: the walk
  emits a box's background-and-border run, then for that box its scroll hit area, clip and scroll frames, the
  stacking contexts below it, its selection, its placeholder prompt, then its content run; `plan` asks the build's
  own functions whether any of the five emits something; if none does the run goes right before the content run,
  whose first item is found by its `(layout index, Content)` tag.
- assumptions not verified by a run: (a) a label's span is ONE layout box (`dom_to_layout[node].len() == 1`), else
  the patch refuses and the rebuild runs; (b) a text run painted for a box below the node is tagged with a box of
  the node's subtree (`commit` updates proofs only there).
- **How to read the dll guards** (`a_hover_over_a_label_paints_its_run_...`, `a_press_on_a_label_paints_its_run_...`,
  `event.rs` state-restyle tests; they stay strict: 0 rebuilds, 1 patch):
  - fails at `"...: the label's run was painted into the list"` with `left: 0, right: 1`: the patch REFUSED and the
    list was rebuilt (`repaint_restyled_nodes` fell back). That is assumption (a) or another guard of `plan_one` /
    `locate` (`node_patch.rs`: `return None` sites; in order: more or less than one layout box, a marker, a
    canvas-painted box, a table part, a flow / opacity group on the node or an ancestor, an SVG stroke, an emitting
    clip / selection / placeholder / negative-z context between the two runs without a content run, an unchanged
    run, a fixed-position range or a caret at the place). Not a reason to accept a rebuild: a finding.
  - passes that line and fails at `"...: 0 list(s) rebuilt"`: the patch ran AND something else rebuilt the list
    (a second caller), a different defect.
  - passes both and fails in the caller's colour assertion: the patch ran and painted the wrong run / place.
  - `AZ_NO_DL_PATCH=1` switches the patch off (`dl_patching_enabled()`): then exactly these two tests fail at the
    first line (`left: 0, right: 1`) and the layout-side tests `a_hover_colour_patched_...`,
    `a_hover_over_one_label_of_many_...` fail at their `dl_node_patches` / pointer assertions; every other test
    passes. If the two dll tests fail the same way WITHOUT the variable while the layout-side tests pass, the
    difference is the dll page's label (its box count), not the patch.
- gaps: see A3-window-38.
- test: `a_hover_colour_patched_into_the_list_is_the_list_a_build_gives`,
  `a_hover_over_one_label_of_many_touches_that_labels_items_alone`, `a_restyle_that_is_no_decoration_is_not_patched`;
  the dll's hover / press tests (0 rebuilds, 1 patch).
- severity: high
- status: fixed 25cf3821f

### A3-window-08 any inline-style change made a node layout-dirty (A3-solver item 40)
- file:line: `core/src/diff.rs` `NodeDataFingerprint::diff`
- class: waste (a node whose `transform` / `left` / `top` alone moved was built fresh)
- fix: three hashes; see the table below. A transform / opacity VALUE change alone is `DirtyFlag::Paint`; one that
  comes or goes is layout (a transformed box is a containing block). An inset-only change is flagged
  `INLINE_STYLE_INSETS` (in `AFFECTS_LAYOUT`); A3-solver consumes it and writes the count test "a re-render where
  only positions changed lays out 0 label interiors".
- test: `an_inline_style_change_is_classified_by_the_declarations_that_differ`, `an_inline_opacity_change_alone_is_paint`,
  `a_transform_or_opacity_value_that_changes_while_the_property_stays_is_paint`,
  `a_transform_or_opacity_that_comes_or_goes_is_not_paint`.
- severity: medium
- status: fixed f57838478

#### The inline style of a node, by reach
| what differs between the old and the new inline style | fingerprint field | flag from `diff` | layout's answer today |
|---|---|---|---|
| any declaration that is not below; a `var()` / custom property; the conditions of a rule; which properties are declared, in which order | `inline_css_hash` | `INLINE_STYLE_LAYOUT` | fresh box (as before) |
| a `transform` or an inset that comes or goes; `transform: none` / an empty list / a CSS-wide keyword | `inline_css_hash` (the type marker or the full value) | `INLINE_STYLE_LAYOUT` | fresh box (a transformed box is a containing block: `scroll_chain.rs:384`) |
| only the VALUES of `top` / `right` / `bottom` / `left` | `inline_inset_hash` | `INLINE_STYLE_INSETS` (in `AFFECTS_LAYOUT`) | fresh box today; could be: place again, keep the interior, unless the size comes from the insets (`left` + `right` with `width: auto`, `top` + `bottom` with `height: auto`) or the box is not positioned (then nothing moves) |
| only the VALUE of an exact `transform` of >= 1 function and / or of `opacity` | `inline_gpu_hash` | `INLINE_STYLE_PAINT` | box kept, list takes the value (**new**) |
| insets and transform values together | both | `INLINE_STYLE_INSETS` | as the insets row |

### A3-window-09 a pass that keeps the layout tree copied it whole (A3-solver's second item)
- file:line: `window.rs` `layout_dom_recursive_sized_by_views`, `relayout_dom_for_virtual_view_sizes`,
  `relayout_root_dom_in_place`, `follow_host_in_kept_view`; `solver3/cache.rs` `unshare`
- class: waste: one deep copy of a DOM's `LayoutTree` per kept-tree pass (resize, layout tween frame, unchanged
  DOM) while the window still holds the previous result; the view's child DOM paid it on every frame of a resize.
- fix: the four callers drop the result's `layout_tree` before the pass.
- other copies of the tree, not in this area: `scroll_registration.rs:199 / 227` (A6: `layout_tree_mut()` on the
  first raise after every layout pass) and the E2E runner's `relayout_only` (A11); see their sections.
- test: `a_kept_view_laid_out_for_another_box_copies_no_layout_tree` (`TREE_COPIES`).
- severity: medium
- status: fixed bdc770f51

### A3-window-10 a String per frame in manager_state_fingerprint (A9's H12)
- file:line: `window.rs` `manager_state_fingerprint`; class: waste (one allocation per frame while a text session is open)
- severity: low
- status: fixed 813dba21c

### A3-window-11 Z8 rest: every selection change rebuilds the list of the DOM that paints it
- file:line: callers at 16334-16825 and 27861-29035 (A6's ranges) -> `regenerate_display_list_for_dom`
- class: waste, per selection change (a drag: per mouse move): O(nodes + items) of the DOM.
- fix: a patch in the style of `node_patch` on A3-core-17 (`505c953ee`: the rects of a selection carry their block's
  layout index and a phase of their own): re-emit the selection run and the content run of the affected IFC roots.
- severity: medium
- status: open: A3-core-17 exists now; the patch is not built yet

### A3-window-12 transfer_states: repoint_orphaned_refanys walks ALL nodes after every merge
- file:line: `core/src/diff.rs` `transfer_states`, `repoint_orphaned_refanys`
- class: quadratic (m merging nodes x n nodes; root DOM; 1,000 stateful rows = 1 M node visits per rebuild)
- fix: `RefAnyHolders`: the first merge that drops an allocation walks the arena, the second builds an index from
  allocation to the nodes holding a `RefAny` of it, later merges visit the listed nodes and move them to the kept
  allocation's list. Same writes in the same order.
- test: `a_merge_sees_what_the_merges_before_it_repointed`,
  `the_clones_of_a_kept_dataset_are_made_in_the_order_of_the_arena`,
  `an_image_a_merge_carried_over_follows_that_merge_when_the_index_is_read`,
  `the_merges_of_a_rebuild_take_a_step_per_node` (`core/src/diff_test.rs`).
- still open in this function's neighbourhood: see A3-window-42.
- severity: medium
- status: fixed fd02b5dd2

### A3-window-13 reconcile_dom pass A2: queue.iter().position(..) over the queue of one subtree hash
- file:line: `core/src/diff.rs` pass A2
- class: quadratic (k equal subtrees in another order or behind consumed entries: k^2 / 2)
- fix: one queue per (subtree hash, container), a claim pops the front. Same node chosen.
- test: `the_subtree_pass_takes_a_moved_node_from_its_own_container_in_any_order`,
  `the_subtree_pass_takes_no_node_the_key_pass_consumed`,
  `the_subtree_pass_takes_a_step_per_node_when_the_rows_come_in_another_order`.
- severity: low
- status: fixed fd02b5dd2

### A3-window-14 begin_reconciliation: arena deep copy, four BTreeSets of n ids, first_rects for every matched node
- file:line: `window.rs` `begin_reconciliation`
- class: waste (linear, large constant, per root rebuild)
- severity: low
- status: open: not started

### A3-window-15 apply_node_css_change, non-GPU path: cascade tables rebuilt, list rebuilt per written property set
- file:line: `window.rs` `apply_node_css_change`
- class: waste, per `set_css_property` of a non-GPU property
- fix: a decoration write takes `patch_restyled_decorations`.
- severity: low
- status: open: not started

### A3-window-16 tween patches scan all items per transition per frame; a patched tween copies the root list per tick
- file:line: `window.rs` `tick_animations` (`items.iter().any(PushOpacity ..)`, `color_bindings.contains`,
  `list_shows_target`, `patch_paint_colors`, `patch_background_layers`, `bind_background_color`); the copy:
  `Arc::make_mut(&mut result.display_list)` while `cached.5 = patched` keeps a second `Arc` after every patched tick.
- class: waste, per tween frame: O(transitions x items) + one deep copy of the root's list per frame of a colour or
  layer fade (root DOM only: 60-70 items in the editor, thousands in a document window).
- severity: medium
- status: open: not started

### A3-window-17 a mouse release scans every styled node of every DOM for `active`
- file:line: `event.rs` `apply_active_restyle`
- class: waste, per click: O(all nodes of all DOMs)
- severity: low
- status: open: fix in work

### A3-window-18 the compact property cache is built a second time per window DOM regeneration (A3-core's item)
- file:line: `dll/src/desktop/shell2/common/layout.rs` `recompute_inheritance_and_compact_cache` after `append_child`
- class: waste, per root rebuild: O(root nodes)
- fix: needed only after an `append_child` (titlebar injection, composed `StyledDom`s): a flag on `StyledDom`.
- severity: low
- status: open: not started

### A3-window-19 a per-DOM generation (asked by A3-core for the a11y pass, by 05 / 06)
- file:line: `window.rs` `DomGenerations`, `LayoutWindow::dom_generation`, `LayoutWindow::note_dom_changed`
- fix: one clock for the window, a value per DOM, bumped through `DomGenerations::bump` at every site that replaces
  a DOM's layout result or writes its layout-side content; see "For area A3-core" for the exact meaning.
- test: `a_doms_generation_moves_with_the_dom_and_only_then`,
  `a_view_rendered_again_has_another_generation_only_with_another_dom`.
- severity: medium
- status: fixed d305eaae7

### A3-window-20 a rebuild of the window's DOM (`RefreshDom`) rendered every view again (asked by A7-editor)
- file:line (why, at the base): the funnel `layout_and_generate_display_list_timed` kept views only on the SAME
  generation (`if new_generation { empty } else { self.unchanged_virtual_views(..) }`, `window.rs:3495-3503` at the
  base): on a new generation every child result is retired, `carry_over_views(&[])` resets every view's flags and
  `VirtualViewManager::check_reinvoke` answers `InitialRender` for a view never invoked in this generation
  (`virtual_view.rs` `check_reinvoke`, the `!state.was_invoked` branch). Independently the shells' two "the DOM
  did not change" exits of a rebuild call `queue_all_virtual_view_reinvoke()` (`dll/.../common/layout.rs` about
  817 and 1394). A `RefAny` is equal only by allocation and the callback reads state the engine cannot see, so
  nothing in the node says "unchanged".
- class: waste, per `RefreshDom`: the view's callback over 12.5k labels + styling + reconciliation + layout + list.
- fix (contract): a view node may state a VERSION: `Dom::with_virtual_view_version(u64)` /
  `set_virtual_view_version` (`NodeData` too). A rebuild of the window's DOM keeps a root-hosted view whose node
  has the same callback and the same `Some(version)` as the node it was last rendered for: its child DOM, layout
  and list stay, the callback is not invoked. No version (`None`, the default) = render again, as before. Views
  inside a kept view's document stay with it. `update_virtual_view` / `queue_virtual_view_reinvoke` still render.
  The shells' unchanged exits call `queue_virtual_view_reinvoke_after_rebuild()`.
- test: `a_rebuild_with_an_unchanged_view_version_invokes_no_view_callback`,
  `the_unchanged_dom_exit_of_a_rebuild_keeps_a_view_that_states_its_version`.
- severity: high
- status: fixed 3baac6687

### A3-window-21 an image replaced in place un-submitted the list the renderer holds (A4-09, this area's half)
- file:line: `window.rs` `apply_image_change`
- fix: after the in-place patch of a list the GPU renderer holds (same size, no callback image), the list is
  marked submitted again (`note_one_display_list_submitted`): the image update travels as a resource update.
- test: `an_image_replaced_in_place_leaves_the_list_the_renderer_holds`.
- severity: medium
- status: fixed 95cfcc78e

### A3-window-22 font resolution runs cold on every pass of a child DOM
- file:line: `window.rs` `layout_dom_recursive_with_viewport` (the child branch empties `font_chain_cache` and
  `last_resolved_font_stacks_sig` before the pass), guard `font_requirements_unchanged` in `layout_dom_recursive_impl`
- class: waste, per view render (settle) and per frame of a resize: `collect_font_stacks` O(n) + used codepoints
  O(T log U) + the registry probes per chain + `load_missing_for_chains` + the chain map rebuilt.
- fix: `FontResolutions::of_children`: a child's pass starts from the chains and the signature its last pass ended
  with. What stays per pass: one walk over the DOM's characters (`CharSet::of_dom`, a bit per character).
- test: `a_view_rendered_again_with_the_stacks_and_characters_it_had_resolves_no_font` (`FONT_RESOLUTIONS`).
- severity: medium
- status: fixed f064384e8

### A3-window-23 the font collector runs while child DOMs are retired
- file:line: `window.rs` `layout_dom_recursive_impl`, `single_dom`
- class: wrong-output: on a new generation the funnel takes every result out of `layout_results` (children retired
  or kept); `layout_results.keys().all(|d| *d == dom_id)` is then true for the root although child DOMs exist, the
  collector runs on the root's keep-set and evicts the faces only a child uses. A child taken back unchanged
  (`reclaim_unchanged_child`) gets no pass that would load them again.
- fix: `font_keep_set_is_the_windows` also asks that no child is set aside. Consequence to know (A9): in a window
  that holds a view the collector does not run; its fonts are bounded by what the app shows.
- test: `the_font_collector_does_not_run_while_a_views_document_is_set_aside`.
- severity: medium
- status: fixed f064384e8

### A3-window-24 the skip of font resolution compares font stacks only, not the characters
- file:line: `window.rs` `font_requirements_unchanged`
- class: wrong-output: the chains are resolved for the codepoints the DOM uses (`collect_used_codepoints_all`,
  `extend_chains_to_cover`); a rebuilt DOM with the same stacks and text in a script it did not have before skips
  the resolver and shapes that text with chains that have no face for it.
- fix: `FontResolutions::covered` (per DOM the characters its chains cover); the skip asks that they cover the
  DOM's text; a resolution under unchanged stacks also covers what was covered before
  (`solver3::getters::collect_and_resolve_font_chains_covering`, a new entry in A3-solver's file; the old one
  forwards to it).
- test: `a_rebuilt_page_resolves_its_fonts_again_for_a_character_it_did_not_have`.
- severity: medium
- status: fixed f064384e8

### A3-window-25 a root pass that resolves fonts again replaces the chain cache, the children's chains go
- file:line: `window.rs` `set_font_chain_cache_with_sig` call in `layout_dom_recursive_impl`
- class: wrong-output (text edited inside a view finds no chain for a stack only the view uses)
- severity: low
- status: fixed f064384e8

### A3-window-26 `AZ_MAP_DEBUG` is read from the environment per pass and per placeholder
- file:line: `window.rs` 9735, 9779
- class: waste (a syscall-free but locking `std::env::var` per layout pass and per view placeholder)
- severity: low
- status: open: not started

### A3-window-27 per layout pass, linear: `update_scrollbar_transforms`, `calculated_positions.clone()`, `apply_text_tweens`
- file:line: `window.rs` `layout_dom_recursive_impl` tail
- class: waste, per pass of a DOM: O(n) walk for scrollbars in a DOM without scrollers; one copy of the positions
  (12.5k); `apply_text_tweens` may `Arc::make_mut` the whole list when a focus ring / caret is in the DOM.
- severity: low
- status: open: not started

### A3-window-28 per frame, O(all layout nodes of all DOMs): `synchronize_scrollbar_opacity`
- file:line: `window.rs` `synchronize_scrollbar_opacity` (19430-19665 at round 2); callers `wr_translate2.rs:2744`,
  `2975`, `3124` (every WebRender frame), `common/layout.rs:69`, `window.rs:17286` (every CPU frame)
- class: waste, per frame: the layout writes `scrollbar_info` for EVERY box, so the whole body (the node, its fade
  timing from its style, two opacities, two key removals) ran for each of the label DOM's 12.5k boxes.
- fix: a box without a painted bar is left at once while its DOM holds no fade key. What stays: one read of
  `scrollbar_info` per box per call (est. 25-50 us at 12.5k boxes); a list of the boxes that have a bar would need
  the solver's tree to keep it (every writer of `warm.scrollbar_info`), see "For area A3-solver".
- test: `a_frame_computes_a_scrollbar_fade_for_no_box_of_a_page_without_bars` (`SCROLLBAR_FADE_WORK`).
- severity: medium
- status: fixed 867d9c402

### A3-window-29 `compute_scroll_ids` on every clean pass of a DOM without scrollers; `reconcile_webviews` walks every node of every DOM at the tail of every pass
- file:line: `window.rs` `compute_scroll_ids` (called from `solver3/mod.rs:1302` under `cache.scroll_ids.is_empty()`),
  `reconcile_webviews`
- class: waste, per pass: O(layout nodes) / O(all nodes of all DOMs)
- severity: low
- status: open: not started

### A3-window-30 `get_node_layout_rect` scans the layout tree per call; `None` matches the first anonymous box
- file:line: `window.rs` `get_node_layout_rect` (`nodes.iter().position(|node| node.dom_node_id == target)`)
- class: quadratic when asked per node (app callbacks: `get_hit_node_layout_rect`); wrong-output for a `None` id
- severity: medium
- status: open: not started (`dom_to_layout`, as `get_node_bounds`)

### A3-window-31 the CSS diff of a view's re-render: `values_for` scans every bucket, `types_to_compare` per node
- file:line: `window.rs` `css_diff_between`, `changed_styles`, `types_to_compare`, `unchanged_cascades`;
  `core/src/prop_cache.rs` `InheritedValues::values_for`
- class: quadratic-like (N x B: changed nodes x buckets of distinct inherited values of the whole DOM; T x k per
  node; 204 types x 2 lookups per node when the selector context differs), per view re-render whose
  `style_root` fingerprint moved (one label with another inline `left`): every settle of a map.
- fix: `InheritedIds` (the two stores read once per diff into id lists both sides share); `types_to_compare`
  leaves out a layer that reads the same on both sides; `css_diff_between` returns `None` under different
  selector contexts (the view lays out cold). `InheritedValues::buckets()` is a new accessor in A3-core's file.
- test: `the_css_diff_of_a_view_whose_labels_moved_takes_a_step_per_label`,
  `a_label_that_moved_compares_its_own_declarations_only`,
  `a_view_rendered_again_under_another_window_context_is_laid_out_without_a_diff` (`CSS_DIFF_WORK`).
- not changed: the restyle diff of the window's DOM under another context still resolves all 204 types per node
  (it needs the changed properties to start transitions; a window DOM of 300 nodes: about 120k lookups once per
  context change).
- severity: high
- status: fixed 71fdd0aec

### A3-window-32 styling a view's DOM walks the `Dom` tree about ten times beside the cascade
- file:line: `window.rs` `style_user_dom_inheriting` (`resolve_form_controls_in_dom`, `dom_has_webview`,
  `resolve_icons_in_dom_with_context`), `fingerprint_dom`; `core/src/styled_dom.rs` `create_from_dom_inheriting` ->
  `create_with_context_inheriting` (the four preparation walks run twice; every attached sheet is cloned)
- class: waste, per view render: about 10 x O(n) pointer-chasing walks
- severity: medium
- status: open: not started (the styled_dom half is A3-core's file)

### A3-window-33 `pending_css_dirty` is one slot: a second DOM's restyle drops the first DOM's staged layout dirt
- file:line: `event.rs` `stage_css_dirty`
- class: wrong-output (a pass that restyles two DOMs, e.g. a hover leave in the root and an enter in the view,
  loses the first DOM's dirt while the tier still says relayout); also O(P log P) per restyle (a map rebuilt)
- severity: medium
- status: open: not started

### A3-window-34 a view's reinvoke check after an override-delivered render compares against stale state
- file:line: `managers/virtual_view.rs` `check_reinvoke`, `scroll_demand`
- class: wrong-output (confirmed by reading the caller: `process_virtual_view_updates` records neither)
- fix: the staged path records the box and keeps the offset aside; `update_virtual_view_info` makes it the resting
  offset when the callback answered with another window. Not covered: a reason the unstaged path hands to a
  caller that invokes at once records the box but not the offset.
- test: `a_jump_back_to_the_first_offset_after_a_delivered_render_asks_for_its_rows`,
  `a_delivered_render_records_the_box_it_was_shown`,
  `an_edge_answered_with_the_same_window_is_asked_again_at_the_same_offset` (`virtual_view.rs`, at the end).
- severity: medium
- status: fixed 2348534fa

### A3-window-35 the view manager: `nested_doms_dropped_by` (V^2), `host_of_nested_dom` (linear per hop), `nested_dom_hosts` (every item of every list per Tab)
- file:line: `managers/virtual_view.rs`
- class: quadratic in views / waste per focus resolution
- fix: a range seek per dropped DOM; `host_by_nested` index; `nested_dom_host_map()` for A6's Tab order
  (`nested_dom_hosts` itself still walks the lists: its caller is A6's, see "For area A6").
- test: `dropping_a_chain_of_views_looks_at_each_view_once`,
  `the_host_index_agrees_with_the_scan_after_every_writer_of_the_states`, three behaviour tests.
- severity: low
- status: fixed 2348534fa

### A3-window-36 `collect_embedded_fonts` scans every item of every DOM after each view re-render
- file:line: `window.rs` `collect_embedded_fonts` (when the font pool is over its keep limit)
- class: waste, per view re-render
- severity: low
- status: open: not started

### A3-window-37 the writers of `GpuValueCache` maps in this area do not say so (A3-core-08)
- file:line: `window.rs` (A3-core's list: 9996, 10006, 10029, 11903, 11907, 18517-19356 at the base)
- class: waste (the readers A1 / A4 cannot skip unchanged values until every writer calls `note_value_write()`)
- severity: medium
- status: open: needs A3-core-08 (`b37653684`) under this branch; not started

### A3-window-38 hover / press restyles the patch does not serve still rebuild the DOM's list
- file:line: `window.rs` `patch_restyled_decorations` (the whitelist), `node_patch.rs` (the guards)
- class: waste, per enter / leave of such a node: text colour (`color`), `outline`, `cursor` + colour together,
  opacity groups around the node, nodes with more than 256 boxes below, more than 8 nodes; `:focus` /
  `:seat-focus` (caret, rings).
- severity: medium
- status: open: not started

### A3-window-39 smaller items of the full read (per app call or per resize event)
- `resize_needs_full_regeneration`: every author rule of every DOM, 4 allocations + 4 sorts per DOM, per resize event.
- `styled_dom_with_edits`: a whole `StyledDom` clone per `get_styled_dom_clone` / `get_dom_subtree`.
- `scratch_layout`: a cold text cache per measured DOM; the content overlay is passed although the measured DOM has
  the ROOT's id (typed text of root node k would be laid out into node k of the measured DOM): wrong-output.
- `apply_animation_momentum`: a linear find over `anim_key_to_node` and a copy of the root hierarchy.
- `for_each_refany` under `Repointing::repoint`: every `RefAny` x replaced allocations (`dropped_at` is a find).
- `published_flips`: animations x depth x up to 3 stacking-context evaluations.
- `unchanged_cascades`: `(None, None) => true` does not compare what a hosted root inherits from its host.
- severity: low
- status: open: not started

### A3-window-40 the CSS diff named a node whose own insets or transform values moved (A3-solver-41, the shell's half)
- file:line: `window.rs` `css_diff_between`, `begin_reconciliation` -> `restyle_is_left_to_the_fingerprint`;
  `core/src/diff.rs` `inline_style_moved_in_place`
- class: waste, per rebuild / per view re-render: a label that only moved was a fresh box (`left` / `top` answer
  `RelayoutScope::Full`, and the reconcile builds a named node fresh whatever its fingerprint says), and any entry
  turned the patch of the display list off.
- fix: a pair whose resolved insets / transform / opacity alone moved, by its OWN style and classified INSETS or
  PAINT by the fingerprint's three hashes, with nothing else of its cascade moved, is not named. An inset moved
  by a stylesheet is named as before.
- test: `a_box_whose_own_insets_moved_is_left_to_its_fingerprint`,
  `a_box_whose_own_width_moved_with_its_insets_is_named_in_the_css_diff`,
  `an_inset_a_stylesheet_moved_is_named_in_the_css_diff`. A3-solver's end-to-end test
  `audit_tests::a3_solver::labels_that_only_moved_keep_their_boxes_when_the_shell_rebuilds_the_page` is
  `#[ignore]`d on its branch until this is merged: remove the attribute at integration.
- severity: high
- status: fixed 072419824

### A3-window-41 the scrollbar fade flag was the last DOM's answer
- file:line: `window.rs` `synchronize_scrollbar_opacity` (`scrollbar_fade_active`)
- class: wrong-output: each DOM's pass overwrote the window's flag; with a view in the window (a child DOM without
  bars, walked last) a bar of the window's own DOM in mid-fade read "nothing fades", `frames_owed()` was false and
  the fade stood still until something else drew a frame (the editor: the sidebar's bar beside the map).
- fix: the pass of `DomId::ROOT_ID` starts the answer, the other DOMs' passes add to it.
- test: `a_fading_bar_of_the_window_asks_for_frames_with_a_view_in_the_window`.
- severity: medium
- status: fixed 7cbefeb45

### Checked: fine
- `scan_used_fonts`, `scan_used_images`: no caller outside tests.
- `images_in_display_lists`: per list the GPU path takes (`register_frame_resources`), not per frame.
- `precompute_reconciliation_keys`, passes A1, A3, B1, `compute_subtree_hashes`, `create_migration_map`: linear.
- `write_bound_gpu_properties`, `publish_gpu_only_restyle`, `node_states`, `changed_declaration_keys`: no loop over the DOM.
- `patch_display_list`: the copy-on-write is per patch and only while a backend holds the list.
- `unchanged_virtual_views`: a fixpoint over the views (v^2 worst), v = 1.
- `compute_node_changes`, `reconcile_dom_with_changes`, `merge_extended_diff` (`diff.rs`): no production caller
  (tests and `core/tests/**` only); their declarations x declarations loop is not on any path.
- `ChangeAccumulator`: a `BTreeMap` per restyled DOM, `entry` per add, linear queries; per hover event, a handful
  of nodes.
- `check_reinvoke_condition`, `scroll_demand`: arithmetic on three rects.
- `tick_animations_now`, `park_culled_tracks`, `release_undriven_animation_values` with nothing running: O(1).
- `get_node_hit_test_bounds` (A3-core's item): one scan of the tag list per call; callers are app callbacks, the
  E2E click selector and a headless test, not a per-node loop.
- A9's `f16fd2ab4` (inside `window.rs`): no conflict with this branch's hunks.

## Ranges read in full in round 2 (one line each)
- `layout_dom_recursive_impl` (whole): nothing superlinear in n; found 22-27 and the items for A3-core / A8 below.
- `window.rs` 2552-3400 (`for_each_refany`, `resize_needs_full_regeneration`, `query_pagination`, constructors,
  frame report): linear; found the items of 39.
- `window.rs` 6593-6990 (`styled_dom_with_edits`, `splice_text_into`, `scratch_layout`, `scratch_extent`,
  measuring, `node_is_visible_in_window`, head of `reconcile_webviews`): linear per call; found 29, 39.
- animation (`needs_animation_frame`, `update_animation_culling`, `park_culled_tracks`,
  `apply_animation_momentum`, `tick_animations_now`, `published_flips`, `tick_animations`): O(1) when idle; found 16, 39.
- 19100-19630 at the base (`release_undriven_animation_values`, `synchronize_scrollbar_opacity`,
  `compute_scroll_ids`, `get_node_layout_rect`): found 28, 29, 30.
- `style_user_dom*`, `inherited_values_of_host`: found 32.
- `unchanged_cascades`, `types_to_compare`, `changed_styles`, `css_diff_between`, `declared_*`, the transition
  starts: found 31, 39.
- `core/src/diff.rs` `compute_node_changes`, `ChangeAccumulator`, `classify_change_scope`: fine (see above).
- `managers/virtual_view.rs` (all non-test code): found 34, 35.
- `event.rs` restyle call sites; `process_window_events` and `process_window_events_inner` in full; parts of
  `apply_user_change` / `apply_system_change`: found 33 and the items for A6.

## For area A3-core
- **The per-DOM generation** (A3-window-19): `LayoutWindow::dom_generation(dom_id) -> u64` (0 = the DOM has no
  layout result), `LayoutWindow::note_dom_changed(dom_id)` for a writer outside `window.rs`; storage
  `LayoutWindow::dom_generations: DomGenerations` (`get`, `bump`, `forget_all_but`). One clock per window: a value is
  never handed out twice, so a `DomId` that goes and comes back has another value. It moves when the DOM's layout
  result is another one or its layout-side content was written: both `layout_results.insert` of
  `layout_dom_recursive_impl`, the viewport-only branch of `follow_host_in_kept_view`, a clip mask, a node style
  written by the app (`apply_node_style_change`, the non-GPU path of `apply_node_css_change`), a text reshape
  (`reshape_text_node`). It does NOT move for: a child taken back unchanged (`reclaim_unchanged_child`: same
  nodes, same boxes), a `:hover` / `:active` / `:focus` restyle (the node STATES change, not the nodes), a tween
  frame, a bound GPU value write, any list patch, a scroll. For the a11y pass (A3-core-10 fix 2): "same
  `(DomId, generation)` as at the last pass" proves same nodes, same text, same boxes; focus, hover, scroll
  offsets and typed-but-unsynced text (the content overlay) are not covered and need their own comparison.
- `compute_uniform_text_bg` proves a text's background from its ANCESTORS only (a positioned sibling painted
  between is not looked at): A3-core-15 documents it as a hint.
- `node_patch.rs` calls these private functions of the generator; a change of their signature or of what they emit
  between a box's background run and its content run must be mirrored in `plan_one`:
  `paint_node_background_and_border_inner`, `push_node_clips`, `paint_selections`, `maybe_paint_placeholder_prompt`,
  `collect_stacking_contexts`, `establishes_stacking_context`, `compute_uniform_text_bg`, `svg_stroke_for`,
  `get_paint_rect`, and the fields `canvas_painted`, `bound_colors`, `forced_page_breaks`, `open_opacity_groups`.
  A3-core-17 adds a fourth `EmitPhase`: `node_patch::locate` matches on `EmitPhase` and needs the new arm at
  integration (it is a compile error until then; the arm does nothing, as `ScWalk`).
- `set_dynamic_selector_context` walks the author rules before its equality check (seen from
  `layout_dom_recursive_impl`).
- `create_with_context_inheriting` repeats `fixup_children_estimated`, `scope_inline_css`, `collect_css_from_dom`,
  `strip_css_from_dom` on the tree `create_from_dom_inheriting` already prepared (A3-window-32).
- `StyledDom::append_child` needs a "composed" flag for A3-window-18.

## For area A3-solver
- `NodeChangeSet::INLINE_STYLE_INSETS` (A3-window-08) is set when only inset values changed.
- **Merge note**: `window.rs` has TWO `LayoutContext` literals now, `regenerate_display_list_for_dom` and
  `patch_restyled_decorations` (for `node_patch`). A3-solver's new field `pass_memo` must be added to the second
  literal at integration (same value as in the first).
- `dom_to_layout: BTreeMap<NodeId, Vec<LayoutNodeId>>` is 12.5k small `Vec`s per tree (and per tree copy).

## For area A4
- A paint restyle (or patch) of ONE DOM makes `register_frame_resources` scan the lists of ALL DOMs and
  `submit_display_lists` translate the changed DOM's whole list (A4-23 has the numbers).
- An in-place patch of a list only the renderer's `Weak` holds moves the list to a new allocation
  (`Arc::make_mut`), which is how `display_list_is_held` notices it: keep that `Weak`.
- `generate_frame` runs a full animation tick while any track exists, even a culled one, and clones two
  `FullWindowState`s per frame (`common/layout.rs` about 2008).

## For area A6
- `scroll_registration.rs:199 / 227`: `layout_tree_mut()` copies the DOM's whole layout tree on the first raise
  after every layout pass (A3-window-09).
- `find_host_by_contenteditable_key`: a scan of every node of every DOM per caret resume.
- The selection paths call `regenerate_display_list_for_dom` per change (A3-window-11).
- `event.rs` `process_window_events_inner`, per mouse move: the press-edge analysis (`press_targets`,
  `press_on_editable`: per hit node an ancestor walk of 32, a child scan of 64) is computed every pass and read
  only under `if left_down && !lw.prev_left_down`; the whole hit test is cloned
  (`hover_manager.get_current(..).cloned()`); about nine separate `.iter().any()` scans of `synthetic_events`; a
  `FullWindowState` clone per pass.
- `event.rs` the `ScrollTo` arm returns `ShouldUpdateDisplayListCurrentWindow` when it queues a view re-render; the
  sibling arm `UpdateVirtualView` returns `ShouldReRenderCurrentWindow` because the other tier made macOS
  regenerate the root's list.
- `event.rs` `DeleteNode`: the first-child branch is empty and the node's `next_sibling` is zeroed: deleting a first
  child looks like it cuts off its following siblings; the arm neither resets the incremental state nor touches
  the list. `InsertChildNode` rebuilds the list from the OLD layout tree right after the node count changed, then
  returns `ShouldIncrementalRelayout` (a wasted rebuild). The `WheelInput` arm calls `process_window_events(0)`
  from inside `apply_user_change` (a nested full pass).
- `event.rs` a layout-affecting hover rule ends in `CommonWindowState::incremental_relayout`, which always finishes
  with `rebuild_cpu_hit_tester()`.

## For area A2-cpu
- `virtual_view_frame_scroll_offsets` (`window.rs` about 4490, per GPU frame): `all_view_keys()` allocates a `Vec`
  per frame; O(views).

## For area A11
- `FrameReport::dl_node_patches` and `FrameReportResponse::dl_node_patches` are new: a hover scenario that asserts
  a MINIMUM of `dl_rebuilds` (m4p `e2e/*.json`, `perf_matrix.py`) must count `dl_rebuilds + dl_node_patches`.
- `e2e/runner.rs` `relayout_only` keeps the old root result alive across the layout (a tree copy per resize).

## For area A7-editor
- **The map view survives a `RefreshDom`** (A3-window-20). Exact change, `mapedit3/src/ui/main/map.rs:337`:
  `Dom::create_virtual_view(app_data, render_map_view)` becomes
  `Dom::create_virtual_view(app_data, render_map_view).with_virtual_view_version(v)`, with `v: u64` a counter in
  the editor's state that is bumped whenever ANYTHING `render_map_view` reads changes: the project's labels and
  corrections, the style, the selection and hover state it draws, the tile set, the display settings, and the
  view's own scale / pan state if the callback reads it from app state instead of from the callback info. A
  `RefreshDom` that leaves `v` as it was (a panel that opens, a sidebar edit that does not touch the map) then
  invokes `render_map_view` 0 times and keeps the view's DOM, layout and list. Whenever `v` moves, or when the
  editor calls `update_virtual_view` (the settle path), the view renders as before. Without the call nothing
  changes (no version = render again). The functions are new in `api.json` (see below): the generated bindings
  need a codegen run; from Rust against `azul-core` they exist as `Dom::with_virtual_view_version` /
  `Dom::set_virtual_view_version`.
- Labels without keys are matched by place: after a pan settle a different label sits at each place, its text
  differs, and the solver lays every label out again. `with_key(osm id, instance, section)` on the label span would
  let `reconcile_dom` follow the label, and with A3-window-08 a label that only moved keeps its text layout once
  the solver handles `INLINE_STYLE_INSETS`.
- The groups of 64 are not needed by this area's code any more (answer 4).

## Integration
- **A4-23** (a hover as a bound colour). What `node_patch` does today when the hovered node's background is bound to
  a colour key (`GpuValueCache::color_keys`, read into `bound_colors` by `plan_one`): it emits the node's run as
  the build would, a bound rect with the key; when the list already holds that bound rect the run is unchanged
  and `locate` REFUSES an unchanged run (it cannot tell "nothing to paint" from "painted somewhere this patch does
  not look"), so the list is rebuilt. For A4-23 the order must be: the value path first, the patch after.
  In `patch_restyled_decorations`, before `plan`: a node whose ONLY change is its background colour (solid to
  solid) and whose background rect is bound gets `current_color_values[key] = new colour` +
  `note_value_write()`; if every changed node was served so, return `true` without touching the list. Nothing in
  `node_patch` itself changes ("skip", not "do differently"). The result for a colour-only change:
  `ShouldReRenderCurrentWindow` is right, and it is what `repaint_restyled_nodes` returns today for the patch as
  well as for the rebuild (`event.rs`, `repaint_restyled_nodes` / `repaint_restyled_dom`); no path of the hover
  restyle returns `ShouldUpdateDisplayListCurrentWindow`. Cost of the design to weigh: a transparent bound rect
  per node that has a `:hover` background rule is 12.5k more items and colour keys in the view's list (30k instead
  of 17.5k items for every frame, on both renderers), and WebRender cannot drop a bound primitive. The patch as
  it is changes the list on enter and on leave (one translation + scene build each on the GPU path) and costs
  nothing while the pointer rests.
- `node_patch::locate` needs an arm for A3-core-17's new `EmitPhase` (see "For area A3-core").
- The second `LayoutContext` literal needs A3-solver's `pass_memo` (see "For area A3-solver").

## api.json
- `Dom::with_virtual_view_version(self, version: u64) -> Dom` and `Dom::set_virtual_view_version(&mut self,
  version: u64)` (A3-window-20), added to the `Dom` class before `with_key`; hand-written Rust in
  `core/src/dom.rs` (`Dom`, `NodeData`). `VirtualViewNode` (internal, not in `api.json`) gained
  `version: Option<u64>`. The generated bindings need a codegen run.

## Questions for the coordinator
- none open.

## Not read
- `window.rs`: `layout_transient_content`; `sync_sheen_keys` / `publish_sheen_values` / `step_material_sheen`;
  the text-edit internals 24439-27460 and the undo records (reached from A6's ranges); the body of
  `reconcile_webviews` after its first loop; `AnimationManager::tick`, `run_track_frames`,
  `node_groups_on_screen`, `extract_gpu_values`.
- `event.rs`: `apply_user_change` and `apply_system_change` only in parts (6265-6722, 6898-7159, 7510-7999,
  8156-8352, 8498-8771, 8772-9371 not read); `dispatch_events_propagated`.
- `core/src/gpu.rs` (`synchronize_with_sizes`), `StyledDom::restyle_*`, the cascade: other areas.
