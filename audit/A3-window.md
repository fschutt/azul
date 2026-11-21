# A3-window: quadratics and per-frame O(nodes) work in `window.rs`, reconciliation / diff, restyle and list patching

Area agent A3-window of the 2026-11-21 audit. Blind: nothing here was compiled or run.
Branch `audit/A3-window`. Base: `audit/base` (`f48bcc7b4`). Line numbers are those of the base commit.

## Claims

First version (from the outline, before the full read); refined as the read goes on.

### Functions A3-window may rewrite or restructure (do not patch inside them; send the finding instead)

`layout/src/window.rs`
- `patch_display_list`, `apply_node_css_change`, `apply_node_style_change`, `changed_declaration_keys`,
  `write_bound_gpu_properties`, `publish_gpu_only_restyle`, `node_states`, `relayout_root_dom_in_place`,
  `apply_image_change`, `apply_background_image_change`
- `regenerate_display_list_for_dom` (the `LayoutWindow` method at 26848)
- `apply_content_change`, `apply_content_change_for_host`
- `unchanged_virtual_views`, `invoke_virtual_view_callback_with_dom`, `invoke_virtual_view_callback_impl`,
  `reclaim_unchanged_child`, `follow_host_in_kept_view`, `repoint_merged_datasets`,
  `reconcile_rematerialized_child`, `process_pending_virtual_view_updates`
- `begin_reconciliation`, `finish_reconciliation`
- `scan_for_virtual_views`, `scan_used_fonts`, `scan_used_images`, `images_in_display_lists`,
  `manager_state_fingerprint`, `compute_scroll_ids`
- `prepare_frame_cpu`, `prepare_frame_gl`, `prepare_frame_content` (only their per-frame scans; not what they call
  in A1's / A4's files)
- the free functions at the end of the file: `sync_css_gpu_values`, `unchanged_cascades`, `changed_styles`,
  `css_diff_between`, `overrides_of`, `types_to_compare`

`dll/src/desktop/shell2/common/event.rs`
- `apply_hover_restyle`, `apply_active_restyle`, `apply_focus_restyle*`, `apply_seat_focus_restyle`, `stage_css_dirty`,
  `gpu_only_restyle_result`, `repaint_restyled_dom`, `regenerate_display_list_for_dom`

`core/src/diff.rs`: all of it. `layout/src/managers/virtual_view.rs`: all of it.

### Not claimed (free for minimal patches by their neighbours)

- `layout_dom_recursive_impl` and `layout_and_generate_display_list*`: read and audited here, but changed by small
  patches only, so A3-solver / A3-core / A6 can patch next to them.
- animation ticks (`tick_animations*`, `park_culled_tracks`, `seed_state_change_transitions`): small patches only.
- everything of A6 (input, hit-test, focus, caret, selection, timers), A2-cpu (4326-4520), A3-core (19632-19900),
  A10 (600-760).

Tests: `layout/src/audit_tests/a3_window.rs`, `dll/src/desktop/audit_tests/a3_window.rs`, and `audit_a3_window_tests`
modules next to private items.

## Findings

(in progress: the blocks below are the state of the read; a block without `status: fixed <hash>` is not fixed yet)

### A3-window-01 reconcile_dom, passes B2 / B3: unmatched new nodes x old nodes of the same structural hash
- file:line: `core/src/diff.rs:1183-1249` (queues built at 921-936)
- class: quadratic
- evidence: for every new node no earlier pass matched, `old_hashed[hash].iter().position(|old| !consumed[old] && parent_agrees(old))`
  and the same on `old_structural[structural_hash]`. Entries consumed by A1 / A2 / A3 / B1 are never removed from the two
  queues, and `calculate_structural_hash` (`core/src/dom.rs:4090`) ignores a text node's text and every node's inline style:
  all text nodes of a DOM share one queue, all spans of one class with the same callback events another. A label of the
  editor is `span.tN` + text, keyless, each with 3 callbacks whose `RefAny` differs (so B2's content queue is a miss and B3
  scans). A settle whose new DOM has U nodes at places the old one had none (zoom out: 1.5k -> 12.5k nodes, U = 11k) scans
  the old queue of its kind per node: U x q, q = old nodes of that kind. 6k -> 12.5k nodes: 3.2k texts x 3k + 3.2k spans x
  (spans of the class) = about 10-20 M closure calls; worst case n^2 / 8 = 20 M at n = 12.5k. Flat or grouped: the same.
- fix: the two tiers index the old nodes still unconsumed when B2 / B3 start by (hash, parent): one queue per (hash, old
  parent id) for a new node whose parent is matched, one per (hash, old parent key) for one whose parent is not; a lookup
  pops the first unconsumed entry. Built lazily, only when a node reaches the tiers. The two maps of STEP 1 go.
- status: open (being fixed)

### A3-window-02 anim_keys_for_moves: the Z9 quadratic again, on every root rebuild
- file:line: `core/src/animation.rs:652-676`, called from `finish_reconciliation` (`layout/src/window.rs:17650`)
- class: quadratic
- evidence: one `calculate_reconciliation_key` per matched node; that function walks every earlier sibling at each level of
  the node's chain (`core/src/diff.rs:645-673`), the walk Z9 (`ef97cef83`) removed from `precompute_reconciliation_keys`
  only. s children under one parent: s^2 / 2 steps (12.5k: 78 M). Root DOM only (the callers pass `DomId::ROOT_ID`), so the
  editor pays it with its root's ~300 nodes (4k with the node graph panel open), not with the view's labels.
- fix: `precompute_reconciliation_keys` once, index by node.
- status: open (being fixed)

### A3-window-03 finish_reconciliation: a per-node key walk per mounted node, and Vec::contains per entered node
- file:line: `layout/src/window.rs:17707-17713` (mount key), `17793` (`pending.mounted.contains(node_id)`)
- class: quadratic
- evidence: (a) every mounted subtree root computes `calculate_reconciliation_key` (earlier siblings at each level) although
  the key is only compared with the zombies' keys: m mounted x s siblings. (b) `for node_id in &pending.entered { ... ||
  pending.mounted.contains(node_id)`: entered x mounted on a `Vec<NodeId>`. A root rebuild that adds 5k flat siblings:
  5k x 5k / 2 = 12 M sibling steps and 10k x 5k = 50 M comparisons. Root DOM only.
- fix: the keys of the new tree once (`precompute_reconciliation_keys`), only when a zombie exists; the mounted nodes as a
  set.
- status: open (being fixed)

### A3-window-04 reconcile_rematerialized_child: two deep copies of the view's arenas per settle
- file:line: `layout/src/window.rs:29738-29754`
- class: waste (linear, large constant)
- evidence: `r.styled_dom.node_data.as_ref().to_vec()` (old arena) and `new_styled_dom.node_data.as_ref().to_vec()` (new
  arena): 2 x 12.5k `NodeData` clones (inline `Css` rules, attributes, 3 callbacks with their `RefAny`, the text) and their
  drops, only to hand `reconcile_dom` two slices and to read the unmounted nodes' callbacks.
- fix: borrow both arenas; collect what the unmount events need before `self` is mutated.
- status: open (being fixed)

### A3-window-05 per-frame scan of every layout node of every DOM for callback images (Z4)
- file:line: `layout/src/window.rs:11401-11437` (`invoke_image_callbacks_into_overlay`, from `prepare_frame_content` /
  `prepare_frame_gl`: every CPU frame and every GL frame)
- class: waste (per-frame O(nodes))
- evidence: `for (dom_id, lr) in &self.layout_results { for node in &lr.layout_tree.nodes { ... NodeType::Image ...
  DecodedImage::Callback` : 12.5k layout nodes + their `NodeData` touched per frame (a pan move, a zoom step, a hover
  repaint), to find zero callback images. The doc comment says "No-op when there are no callback images".
- fix: the callback-image nodes of a DOM are found once per (styled DOM, layout tree) and kept.
- status: open (being fixed)

### A3-window-06 per-frame scan of every display-list item of every DOM for callback backgrounds (Z4)
- file:line: `layout/src/window.rs:11613-11721` (`invoke_background_image_callbacks_into_overlay`)
- class: waste (per-frame O(items))
- evidence: runs when `declared` (the callback images registered by id) is not empty. With the cargo feature `materials`
  (in azul-dll's `dist` and `build-dll` feature sets, `dll/Cargo.toml:807`) `image_cache_with_builtin_textures` registers
  four material callbacks in every window (`layout/src/texture.rs:153-156`), so the walk of all items of all lists runs on
  every frame of every window: 17.5k items, a hash and two map lookups per `Image` item. The editor builds azul with
  `default-features = false, features = ["link-static", "serde-json", "png"]`: no materials unless a dependency turns the
  feature on, then the early return at 11570 holds.
- fix: skip the walk of a DOM whose list holds no item that draws a declared callback or a layer's frame, known per list
  identity.
- status: open

### A3-window-07 hover enter / leave over a label rebuilds the whole list of the view (Z3)
- file:line: `dll/src/desktop/shell2/common/event.rs:1130` -> `repaint_restyled_dom` (700) ->
  `LayoutWindow::regenerate_display_list_for_dom` (`layout/src/window.rs:26848`)
- class: waste (per hover O(nodes + items))
- evidence: the editor's labels have `.tN:hover { background: rgba(66,134,244,0.35) }` (`mapedit3/src/ui/main/map.rs:1140-1147`): a
  paint-only, not GPU-only change of ONE node. The path: `node_states` (O(1)), `restyle_on_state_change` (A3-core),
  `seed_state_change_transitions` (returns at once for a child DOM), `stage_css_dirty` (O(pending)), then the full rebuild:
  `sync_css_gpu_values` over the DOM, `generate_display_list` over 12.5k nodes -> 17.5k items, the placeholder loop over
  the items, `apply_text_tweens`, the a11y update. Measured before (PERFORMANCE.md): 7.5-13 ms per move on samples of
  1.6k-4.4k view nodes.
- fix: see the design under "Z3" below.
- status: open

### Checked: fine
- `scan_used_fonts`, `scan_used_images` (`layout/src/window.rs:13211`, `13250`): no caller outside tests (git grep): not per
  frame, not per change.
- `images_in_display_lists` (13267): one production caller, `collect_image_resource_updates_and_halos_left`
  (`dll/src/desktop/wr_translate2.rs:756`), reached from `register_frame_resources`: when a GPU frame takes a rebuilt list
  (`generate_frame` with `display_list_was_rebuilt`) and in `build_webrender_transaction`. Per rebuilt list, not per frame;
  but it walks the items of ALL DOMs for one rebuilt DOM (see "For area A4").
- `precompute_reconciliation_keys`, passes A1, A3, B1 of `reconcile_dom`, `compute_subtree_hashes`, `create_migration_map`:
  linear (B1 pops consumed entries: amortised O(1)).
- `layout_dom_recursive_impl` tail (9498-9781): the layout tree and the list are `Arc`s shared with the solver's cache (no
  copy); `scan_for_virtual_views` and the scrollbar-key loop are one pass over the layout nodes / the items per LAYOUT pass.
