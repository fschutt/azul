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
- status: fixed `0e85f9a64`

### A3-window-02 anim_keys_for_moves: the Z9 quadratic again, on every root rebuild
- file:line: `core/src/animation.rs:652-676`, called from `finish_reconciliation` (`window.rs:17650`)
- class: quadratic (s^2 / 2 for s siblings; root DOM only)
- fix: `precompute_reconciliation_keys` once. test: `the_animation_keys_of_a_rebuild_take_a_step_per_node`.
- status: fixed `59a59a6ef`

### A3-window-03 finish_reconciliation: a key walk per mounted node, Vec::contains per entered node
- file:line: `window.rs:17707-17713`, `17793`
- class: quadratic (m mounted x s siblings; entered x mounted; root DOM only)
- fix: the keys once and only while a zombie exists; the mounted nodes as a set.
- test: none of its own (no step counter outside azul-core's cfg(test)); the linear pass is guarded in azul-core.
- status: fixed `54b7119bd`

### A3-window-04 a view's render deep-copied its arenas two to three times (also A9's H16)
- file:line: `window.rs:3519-3531`, `29738-29754`
- class: waste (linear, large constant: 2 x 12.5k `NodeData` clones and drops per render)
- fix: the arenas are borrowed where they lie (live result, retired result of the running pass, stash); the funnel
  copies an arena only for a child no view rendered again in the pass.
- test: `layout/src/audit_tests/a3_window.rs`: three behaviour tests (queued update, new generation, hidden view).
  The third assumes a `display: none` host lays out no box for the view (not verified).
- status: fixed `d5a1ff858`

### A3-window-05 per-frame walk of every layout node of every DOM for callback images (Z4)
- file:line: `window.rs:11401-11437`
- class: waste, per frame: 12.8k hot nodes + their `NodeData` (about 3 MB of memory traffic, est. 0.1-0.3 ms) per
  CPU and per GL frame (a pan move, a zoom step, a hover repaint).
- fix proposed: keep the callback-image nodes per DOM, valid while the DOM's layout result is the same one. No
  per-DOM generation exists to tie it to (see "For area A3-core"); a cache keyed by `Arc::as_ptr(layout_tree)` is
  not sound (a pass that keeps the tree re-wraps it, the allocator may hand the address out again) and a stale
  "no callback image" would leave a canvas undrawn. Needs the generation counter first.
- status: open (cost small, the sound fix needs a new field on `LayoutWindow`)

### A3-window-06 per-frame walk of every display-list item for callback backgrounds (Z4)
- file:line: `window.rs:11613-11721`
- class: waste, per frame when a callback image is registered by id: always with the `materials` feature (`dist`,
  `build-dll`: `dll/Cargo.toml:807`, `texture.rs:153`). 17.5k items, a hash and two map lookups per `Image` item.
- fix proposed: remember per list (`Arc::as_ptr` + the overlay's background count + `background_frames_generation`)
  that the walk found no layer and asked nothing, and skip it; same generation question as 05.
- status: open

### A3-window-07 hover / press over a label rebuilt the view's whole list (Z3, with the insert case)
- file:line: `event.rs:1130` -> `repaint_restyled_dom` -> `window.rs:26848`
- class: waste, per enter / leave: O(nodes + items) (7.5-13 ms per move measured on 1.6k-4.4k-node samples)
- fix: `patch_restyled_decorations` + `solver3/display_list/node_patch.rs` (see answer 1 and the module's doc).
  The insert place: the walk emits a box's background-and-border run, then for that box its scroll hit area, clip
  and scroll frames, the stacking contexts below it, its selection, its placeholder prompt, then its content run
  (`generate_for_stacking_context` 6856-6904, `paint_in_flow_child` 7370-7380, `paint_node_content` 9252); `plan`
  asks the build's own functions whether any of the five emits something; if none does the run goes right before
  the content run, whose first item is found by its `(layout index, Content)` tag.
- assumptions not verified by a run: (a) a label's span is ONE layout box (`dom_to_layout[node].len() == 1`),
  else the patch refuses and the rebuild runs (then the dll guard's "1 patch" fails); (b) a text run painted for a
  box below the node is tagged with a box of the node's subtree (`commit` updates proofs only there).
- gaps: text colour (`color`), `outline`, `cursor` + colour together, opacity groups around the node and nodes
  with more than 256 boxes below them still rebuild; `:focus` / `:seat-focus` still rebuild (caret, rings); the
  proof of a text run that had none stays `None` after a leave (slower path, same pixels).
- test: `a_hover_colour_patched_into_the_list_is_the_list_a_build_gives`,
  `a_hover_over_one_label_of_many_touches_that_labels_items_alone`, `a_restyle_that_is_no_decoration_is_not_patched`;
  the dll's hover / press tests (0 rebuilds, 1 patch).
- status: fixed `25cf3821f`

### A3-window-08 any inline-style change made a node layout-dirty (A3-solver item 40)
- file:line: `core/src/diff.rs` `NodeDataFingerprint::diff` (2803 at A3-solver's reading)
- class: waste (a node whose `transform` / `left` / `top` alone moved was built fresh: no intrinsic sizes, no inline
  layout kept)
- fix: three hashes; see the table below. Behaviour change: a transform / opacity VALUE change alone is
  `DirtyFlag::Paint` now. An inset-only change is flagged `INLINE_STYLE_INSETS` and still handled as layout.
- test: `an_inline_style_change_is_classified_by_the_declarations_that_differ`, `an_inline_opacity_change_alone_is_paint`.
- not done: the count test "a re-render where only positions and rotations changed lays out 0 label interiors"
  needs the solver to place an abspos box again without rebuilding it (`reconcile_node`, `cache.rs:3017-3041`:
  `needs_layout()` -> a fresh node). That is A3-solver's code; with it the flag is ready (see "For area A3-solver").
  Note for the editor: its labels are keyless, so a settle that changes the label set pairs different labels by
  place and their TEXT differs: they are layout-dirty by content whatever the style says. The classification pays
  once labels carry keys (`with_key`), see "For area A7-editor".
- status: fixed `f57838478` (classification); the inset fast path is open in the solver

#### The inline style of a node, by reach
| what differs between the old and the new inline style | fingerprint field | flag from `diff` | layout's answer today |
|---|---|---|---|
| any declaration that is not below; a `var()` / custom property; the conditions of a rule; which properties are declared, in which order | `inline_css_hash` | `INLINE_STYLE_LAYOUT` | fresh box (as before) |
| a `transform` or an inset that comes or goes; `transform: none` / an empty list / a CSS-wide keyword | `inline_css_hash` (the type marker or the full value) | `INLINE_STYLE_LAYOUT` | fresh box (a transformed box is a containing block: `scroll_chain.rs:384`) |
| only the VALUES of `top` / `right` / `bottom` / `left` | `inline_inset_hash` | `INLINE_STYLE_INSETS` (in `AFFECTS_LAYOUT`) | fresh box today; could be: place again, keep the interior, unless the size comes from the insets (`left` + `right` with `width: auto`, `top` + `bottom` with `height: auto`) or the box is not positioned (then nothing moves) |
| only the VALUE of an exact `transform` of >= 1 function and / or of `opacity` | `inline_gpu_hash` | `INLINE_STYLE_PAINT` | box kept, list takes the value (**new**) |
| insets and transform values together | both | `INLINE_STYLE_INSETS` | as the insets row |

### A3-window-09 a pass that keeps the layout tree copied it whole (A3-solver's second item)
- file:line: `window.rs:8136`, `8221`, `11167`, `12972`; `solver3/cache.rs:642` (`unshare`), `solver3/mod.rs:839`
- class: waste: one deep copy of a DOM's `LayoutTree` per kept-tree pass (resize, layout tween frame, unchanged
  DOM) while the window still holds the previous result. How often: once per such pass and DOM; not per frame
  without a layout; not on a view's re-render (it reconciles into a new tree). The view's child DOM paid it on
  every frame of a window resize (`follow_host_in_kept_view`).
- fix: the four callers drop the result's `layout_tree` before the pass.
- other copies of the tree, not changed: `DomLayoutResult::layout_tree_mut` / `place_inline_children`
  (`Arc::make_mut`) on the first write after a pass: a text edit's reshape (26191, 26390, 26450; once per pass,
  the edited DOM only) and `scroll_registration.rs:199 / 227` (A6's) when a scrollbar necessity is raised after
  layout: the cache's tree never gets the raised flag (the write goes to the result's copy), so a DOM whose view
  publishes a scroll extent pays one copy of its tree after EVERY layout pass (the editor's root: ~300 nodes).
  Holders across a pass outside this file: the E2E runner's `relayout_only` (`e2e/runner.rs:4020`) and the shells'
  resize path keep the old root result alive while its `styled_dom` is laid out again.
- test: `a_kept_view_laid_out_for_another_box_copies_no_layout_tree` (`TREE_COPIES`).
- status: fixed `bdc770f51` (the four callers); the rest is listed for A6 / A11

### A3-window-10 a String per frame in manager_state_fingerprint (A9's H12)
- file:line: `window.rs:10127`; class: waste (one allocation per frame while a text session is open)
- status: fixed `813dba21c` (no test: no allocation counter)

### Open, with what is missing
- **A3-window-11 Z8 rest, the drag highlight of a selection**: every selection change ends in
  `regenerate_display_list_for_dom` of the DOM that paints it (the callers at 16334-16825 and 27861-29035 are A6's
  ranges). Selection rects are painted by `paint_selections` between a box's clips and its content run, untagged
  (`layout_node_mapping` `None`), and a `::selection` recolour splits text runs. A patch in the style of
  `node_patch` needs (1) the selection run tagged (a fourth `EmitPhase`, A3-core's builder) so the old run can be
  found, (2) `paint_selections` + the content run of the affected IFC roots re-emitted together (the recolour
  changes Text items). Not started: the builder change is A3-core's and the callers are A6's.
- **A3-window-12 `transfer_states`**: `repoint_orphaned_refanys` walks ALL nodes after every merge
  (`diff.rs:1773`, `1890`): m merging nodes x n nodes. Root DOM only (a view's re-render does not merge datasets);
  m is the number of widgets with a dataset and a merge callback. A list of 1,000 stateful rows: 1 M node visits
  per rebuild. Fix: index the nodes by the allocation their `RefAny`s share before the loop. Open (the order of
  merges matters: a later node must see the repointed dataset).
- **A3-window-13 `reconcile_dom` pass A2**: `queue.iter().position(..)` over the queue of one subtree hash
  (`diff.rs:1058`): O(1) when old and new order agree; O(k) per node for k equal subtrees in k keyed containers in
  another order, or behind entries A1 consumed. Open, rare.
- **A3-window-14 `begin_reconciliation`** (root DOM): the old arena is deep-copied (17076), four `BTreeSet`s of n
  ids are built (17122-17147), `first_rects` asks `get_node_bounds` for every matched node although only nodes
  with `animation: move` read it (17105, read at 17679). Linear, large constant, per root rebuild. Open: the copy
  is needed while `transfer_states` writes the old arena; the rest is mechanical.
- **A3-window-15 `apply_node_css_change`, the non-GPU path** (10703-10752): `restyle_user_properties` (a rebuild of
  the cascade's tables, result unused) and `regenerate_display_list_for_dom` per written property set. A write
  of a decoration could take `patch_restyled_decorations` (the restyle result is exactly its input). Open; the
  editor writes only the bound transform there.
- **A3-window-16 tween patches** (`tick_animations`, 18673-18780, root DOM): per transition per frame a scan of
  all items (`items.iter().any(PushOpacity ..)`, `color_bindings.contains`, `list_shows_target`,
  `patch_paint_colors`, `patch_background_layers`, `bind_background_color`): O(transitions x items). 60-70 items in
  the editor's root. Open.
- **A3-window-17 a mouse release** scans every styled node of every DOM for `active` (`event.rs:991-1001`). Per
  click, linear. Open, cheap.
- **A3-window-18 `dll/.../common/layout.rs:1079`** (A3-core's item): `recompute_inheritance_and_compact_cache` on
  every window DOM regeneration although the cascade just built both. It is needed only after an `append_child`
  (the titlebar injection, or a callback that returns composed `StyledDom`s), which the shell cannot see. Fix:
  a flag on `StyledDom` set by `append_child` (A3-core's struct), tested here. Open.
- **`get_node_hit_test_bounds`** (13059, A3-core's item): one scan of the tag list and one of the DOM's items per
  call; callers are app callbacks, the E2E click selector and a headless test: not in a per-node loop. Fine unless
  an app calls it per node.

### Checked: fine
- `scan_used_fonts`, `scan_used_images` (13211, 13250): no caller outside tests.
- `images_in_display_lists` (13267): per list the GPU path takes (`register_frame_resources`), not per frame.
- `precompute_reconciliation_keys`, passes A1, A3, B1, `compute_subtree_hashes`, `create_migration_map`: linear.
- `write_bound_gpu_properties`, `publish_gpu_only_restyle`, `node_states`, `changed_declaration_keys` (per written
  node, per restyled node): no loop over the DOM.
- `patch_display_list` (11356): the copy-on-write is per patch and only while a backend holds the list; the scan of
  `patch_node_image` is one pass over the items per tile (tens of microseconds).
- `layout_dom_recursive_impl` tail (9498-9781): tree and list are shared `Arc`s; `scan_for_virtual_views` and the
  scrollbar-key loop are one pass per LAYOUT pass.
- `unchanged_virtual_views` (12179): a fixpoint over the views (v^2 worst), v = 1.
- `managers/virtual_view.rs`: state is a `BTreeMap` keyed by (DOM, node); `nested_doms_dropped_by` is quadratic in
  the number of views only.

## For area A3-core
- **A per-DOM generation does not exist.** Where a DOM's layout result is replaced or its exposed state written, in
  this area: `layout_dom_recursive_impl` inserts the result at 8937 (the `skip_gpu_sync` branch) and 9734;
  `reclaim_unchanged_child` puts a retired result back (12879) and adopts fresh payloads in place (12868);
  results leave at 3538-3550 (the funnel), 8136, 8221, 11167, 12972 (relayout helpers), 17512 (a zombie takes it),
  the dropped-child loop of `remap_node_ids`, `clear_caches`. Written in place: the list
  (`regenerate_display_list_for_dom`, `patch_display_list`, `apply_caret_visibility_in_place`, the tween patches,
  `patch_restyled_decorations`), the styled DOM (`restyle_on_state_change` in `event.rs`, `set_style`,
  `upsert_inline_css_property`, `set_clip_mask`, the user overrides), the tree (`layout_tree_mut`), the viewport
  size (`follow_host_in_kept_view`, 12961). `frame_report.layout_passes` counts funnel passes of the window, not
  DOMs; a view's own re-render does not pass the funnel. A generation bumped at the two inserts and at 12879 would
  cover "another result"; the in-place writes need their own bump or the a11y pass must not rely on it for them.
- `compute_uniform_text_bg` (5748) proves a text's background from its ANCESTORS only; a positioned sibling
  painted between the ancestor's fill and the text (a tile image under a label) is not looked at. If the picture
  root of a map had an opaque background, every label over a tile would be drawn on that colour by the pre-blended
  path. Not verified whether the editor's picture root has one; worth a pixel test.
- `enter_scroll_chain` (7097) runs per absolutely positioned box: `ScrollChain::of` walks the ancestor chain and
  five `Vec`s are allocated per box (12.5k labels per list build).
- `DisplayListBuilder::add_forced_page_break` (4007): `any` + push + sort per break (paged media only).
- `node_patch.rs` calls these private functions of the generator; a change of their signature or of what they emit
  between a box's background run and its content run must be mirrored in `plan_one`:
  `paint_node_background_and_border_inner`, `push_node_clips`, `paint_selections`, `maybe_paint_placeholder_prompt`,
  `collect_stacking_contexts`, `establishes_stacking_context`, `compute_uniform_text_bg`, `svg_stroke_for`,
  `get_paint_rect`, and the fields `canvas_painted`, `bound_colors`, `forced_page_breaks`, `open_opacity_groups`.
- `StyledDom::append_child` needs a "composed" flag for A3-window-18.

## For area A3-solver
- `NodeChangeSet::INLINE_STYLE_INSETS` (A3-window-08) is set when only inset values changed. `reconcile_node`
  (`cache.rs:3017`) can map it to "clone the old box with its caches, mark its position dirty" for an out-of-flow
  box whose size does not come from its insets; today it is part of `needs_layout()`.
- A new `LayoutContext` field must be added at TWO literals in `window.rs` now: `regenerate_display_list_for_dom`
  and `patch_restyled_decorations`.
- `dom_to_layout: BTreeMap<NodeId, Vec<LayoutNodeId>>` is 12.5k small `Vec`s per tree (and per tree copy).

## For area A4
- A paint restyle (or patch) of ONE DOM makes `register_frame_resources` scan the images and the text halos of the
  lists of ALL DOMs (`wr_translate2.rs:731-790`) and `submit_display_lists` translate the changed DOM's whole list
  (17.5k items for a label's hover colour). After A3-window-07 this is what a hover costs on the GPU path.
  `patch_restyled_decorations` could hand over the changed item range if a partial translation existed.
- An in-place patch of a list only the renderer's `Weak` holds moves the list to a new allocation
  (`Arc::make_mut`), which is how `display_list_is_held` notices it: keep that `Weak`.

## For area A6
- `scroll_registration.rs:199 / 227`: `layout_tree_mut()` copies the DOM's whole layout tree on the first raise
  after every layout pass (A3-window-09). Writing the raised flag into the solver's tree as well (or raising it in
  the pass) would end the copy.
- `window.rs:6578` `find_host_by_contenteditable_key`: a scan of every node of every DOM per caret resume; the key
  walk (`calculate_contenteditable_key`, earlier siblings per level, the Z9 pattern) runs for contenteditable
  nodes only. Fine at a few editable hosts; quadratic for a document of thousands of editable siblings.
- `stage_css_dirty` (`event.rs:904`) rebuilds a `BTreeMap` of everything staged per restyle; bounded by the DOM.
- The selection paths call `regenerate_display_list_for_dom` per change (A3-window-11).

## For area A11
- `FrameReport::dl_node_patches` and `FrameReportResponse::dl_node_patches` are new: a hover scenario that asserts
  a MINIMUM of `dl_rebuilds` (m4p `e2e/*.json`, `perf_matrix.py`) must count `dl_rebuilds + dl_node_patches`.
- `e2e/runner.rs:4020` `relayout_only` keeps the old root result alive across the layout (a tree copy per resize).

## For area A7-editor
- Labels without keys are matched by place: after a pan settle a different label sits at each place, its text
  differs, and the solver lays every label out again. `with_key(osm id, instance, section)` on the label span would
  let `reconcile_dom` follow the label, and with A3-window-08 a label that only moved keeps its text layout once
  the solver handles `INLINE_STYLE_INSETS`. (Keyed nodes are matched in pass A1 by a map lookup: linear.)
- The groups of 64 are not needed by this area's code any more (answer 4).

## api.json
No change.

## Questions for the coordinator
1. A3-window-07 changes three existing guards (dll hover / press tests, the headless hover sweep, the E2E hover
   cost test) from "at least one rebuild" to "rebuild or patch". If the patch refuses in those harnesses (assumption
   (a) above), the dll tests fail at "1 patch": then either the assumption is wrong for spans, or `plan_one`
   returned `None` for another reason; `AZ_NO_DL_PATCH=1` switches the patch off for a comparison.
2. Should `INLINE_STYLE_PAINT` for a transform-only change be kept if a layout turns out to read a transform value
   (scrollable overflow of a transformed box)? I found one reader of a transform in the solver, and it reads
   presence only (`scroll_chain.rs:384`).

## Not read
- `window.rs`: `layout_dom_recursive_impl` 8254-9490 only by a grep of its loops and clones plus 8640-8900 without
  comments (the font-chain block, the memory report and the probe blocks were not read); `layout_transient_content`;
  2552-3300 apart from `for_each_refany`; 6593-6990 (`splice_text_into`, `scratch_layout`); `update_drag_visual`
  read, `sync_sheen_keys` / `publish_sheen_values` / `step_material_sheen` not; `park_culled_tracks`,
  `tick_animations_now`, `published_flips`, `compute_scroll_ids` (19414), the frame-report bookkeeping, 19100-19630;
  everything from 22352 on except `regenerate_display_list_for_dom`, `serve_paint_dirt`,
  `verify_patched_display_list`, `process_pending_virtual_view_updates`, `reconcile_rematerialized_child`,
  `start_view_enter_tracks` and the head of `remap_node_ids` (the text-edit internals 24439-27460, the undo
  records, `collect_embedded_fonts`, the free functions `unchanged_cascades`, `changed_styles`, `css_diff_between`,
  `overrides_of`, `types_to_compare`, the transition helpers); `style_user_dom*`.
- `core/src/diff.rs`: `compute_node_changes` (266-478), `ChangeAccumulator`, `classify_change_scope`,
  `NodeDataFingerprint::compute` beyond the inline hash, `reconcile_cursor_position`.
- `layout/src/managers/virtual_view.rs`: `check_reinvoke`, `check_reinvoke_condition`, `scroll_demand`,
  `remap_node_ids` and the tests (read: the function list, `nested_doms_dropped_by`, `window_serves_resized_box`).
- `event.rs`: only 560-1141 (the restyle functions) and the state-restyle test module.
- `core/src/gpu.rs` (`synchronize_with_sizes`, behind `sync_css_gpu_values`), `StyledDom::restyle_*`: other areas.
- Three read-only helper agents for these sweeps could not be started (the session's agent limit was reached).
