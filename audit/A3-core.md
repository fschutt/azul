# A3-core: quadratics and waste in `core/`, `css/`, the display list builder, a11y

Area agent A3-core of the blind audit (2026-11-21). Nothing here was compiled or run.
Branch `audit/A3-core`, base `audit/base` (`f48bcc7b4`). Append-only since the integration started.

## Claims

Functions this area rewrote or restructured (final):

- `core/src/style.rs`: `construct_html_cascade_tree`; `ScopedRuleIndex` + `MergedRules` are gone, replaced by
  `SubjectRuleIndex` (+ `subject_class`).
- `core/src/prop_cache.rs`: `InheritedValues` (`set_node_near`, `bucket_of`, `push_bucket`, `by_hash`, `append`),
  `CssPropertyCache::compute_inherited_values`, `store_if_changed`, `FlatVecVec::dedup_runs`, the rule loop of
  `CssPropertyCache::restyle` (candidates only); fields `text_color_override_seen`, `composed_since_recompute`.
- `core/src/styled_dom.rs`: `take_css_from_dom` (new), its two callers; `collect_css_from_dom` / `strip_css_from_dom`
  are test-only; `needs_recompute_after_append`; the override writers raise `text_color_override_seen`.
- `core/src/gpu.rs`: `GpuValueCache::value_generation` / `note_value_write` and the bodies of
  `apply_transform_events`, `apply_opacity_events`, `refresh_transform_value(s)_of`, `refresh_opacity_value_of`.
- `core/src/compact.rs`: the box slot writes (`encode_css_pixel_as_px_slot`, `encode_margin_px_slot`,
  `encode_gap_px_slot`), `note_px_overflow`, four arms of `update_dom_declared_flags`.
- `css/src/css.rs`: `Css::from_string` (memo), `ParsedStyles` (`kept_bytes`, `keep`).
- `css/src/compact_cache.rs`: `PxSlot` and its codec, the 18 box slots of `CompactNodeProps` and their accessors,
  `px_overflow`, `DOM_HAS_TEXT_TRANSFORM` / `_VERTICAL_ALIGN` / `_ZOOM_OR_ASPECT_RATIO`.
- `css/src/props/basic/length.rs`: `FloatValue::new` (rounds).
- `layout/src/solver3/display_list.rs`: `get_tag_id` -> `DisplayListGenerator::tag_id_of` / `tags_by_node`,
  `enter_scroll_chain` (`box_chain_key`, `box_chain_links`), `paint_selections`, `paint_cursor`,
  `EmitPhase::Selection`, `add_forced_page_break`, `DisplayListGenerator::loaded_fonts`.
- `layout/src/managers/a11y.rs`: `A11yManager::{on_demand, stale, handed_focus, hand_to_adapter, update_pending,
  replay_key}`, `rebuild_retained` (the per-DOM replay), `RetainedA11yDom`, `A11yTreeInputs::dom_generations`;
  `layout/src/window.rs`: `update_a11y_tree`, `a11y_update_pending`, `a11y_update_on_demand`, `update_a11y_tree_now`.
- In A3-solver's files, few lines each: `getters.rs` (`get_css_property_pixel!`, `declared_border_info`,
  `compact_border_width_needs_cascade`, `compute_all_zooms`, `get_aspect_ratio_property`,
  `get_vertical_align_property`, the `text-transform` read, `inherited_color_override`), `sizing.rs` (the
  `flex-wrap` read), `taffy_bridge.rs` (gap decode), `fc/table_borders.rs` (width decode).

Private generator functions of `display_list.rs` that A3-window's `display_list/node_patch.rs` calls and that were
NOT renamed and keep their signatures: `paint_node_background_and_border_inner`, `push_node_clips`,
`paint_selections`, `maybe_paint_placeholder_prompt`, `collect_stacking_contexts`, `establishes_stacking_context`,
`compute_uniform_text_bg`, `svg_stroke_for`, `get_paint_rect`; fields `canvas_painted`, `bound_colors`,
`forced_page_breaks`, `open_opacity_groups`; `DisplayListGenerator::new` (same arguments).

## Findings

Real sizes used below: Berlin view 12,470 DOM nodes (about 6,200 labels = span + text, 1 text style: 3 rules in the
view's sheet), Tuebingen view 2,192 nodes / 17,495 display list items / 2,118 reference frames / 3,703 hit-test areas
(26 text styles + 21 point styles: about 73 rules). Costs are estimates from reading (steps x a guessed ns), never
measured: the ranking is by steps.

### A3-core-01 the display list scans the DOM's tag list once per box
- file:line: `layout/src/solver3/display_list.rs` `get_tag_id` (12202 at base), asked by `paint_node_content_inner`
  for every box, `generate_for_stacking_context`, `paint_inline_shape_of`.
- class: quadratic (boxes x tags). severity: high
- evidence: `tag_ids_to_node_ids.iter().find(node)`: Berlin 6,200 tagged spans x ~3,100 + 6,200 text nodes x 6,200 =
  about 54 million entry compares per display list of the view (est. 40-80 ms); at every build of the view's list.
- fix: `DisplayListGenerator::tag_id_of` reads a per-build table by node index. Identical answers.
- test: `audit_tests::a3_core::{a_display_list_reads_the_tag_list_once_not_once_per_box,
  every_hit_test_area_carries_the_tag_of_its_node}`.
- status: fixed 4720e5091

### A3-core-02 the cascade tree walks back over the siblings of every node that has children
- file:line: `core/src/style.rs` `construct_html_cascade_tree`.
- class: quadratic (k^2 / 2 for k siblings that have children). severity: high
- evidence: per non-leaf node `preceding_siblings(..).count()`; flat, 6,200 labels = 19 million sibling steps per
  styling (the third flat-sibling quadratic after Z9, Z10).
- fix: the child walk records each child's count of earlier element siblings; a parent reads it.
- test: `style_test::audit_a3_core_tests::{a_parent_of_many_parents_is_walked_once,
  the_cascade_tree_is_the_one_the_walk_back_built}`.
- status: fixed a2ff3cc60

### A3-core-03 selection and caret painters copy every text block's lines before looking for a selection
- file:line: `layout/src/solver3/display_list.rs` `paint_selections`, `paint_cursor`.
- class: waste (one `Arc<UnifiedLayout>` per text block per display list). severity: medium
- evidence: Berlin: 6,200 allocations and ~50,000 cluster conversions per list build (est. 2-4 ms).
- fix: existence check first, the copy only in the branch that paints.
- test: `audit_tests::a3_core::a_block_without_a_selection_or_a_caret_copies_no_lines`.
- status: fixed e1a9d8435

### A3-core-04 selector matching was nodes x rules for one sheet on a subtree root
- file:line: `core/src/prop_cache.rs` `restyle` (rule loop), `core/src/style.rs` `ScopedRuleIndex`.
- class: quadratic-like (nodes x rules). severity: high
- evidence: Tuebingen 2,192 nodes x 75 rules = 164,000 matches; a Berlin-sized view with Tuebingen's styles 935,000
  (est. 45 ms).
- fix: `SubjectRuleIndex`: rules whose subject compound names a class are offered only to nodes carrying it.
- test: `prop_cache_test::audit_a3_core_tests::{a_node_is_tried_against_the_rules_of_its_own_classes,
  the_rules_of_a_nodes_classes_apply_in_sheet_order_and_scope}`.
- status: fixed d08a602d4

### A3-core-05 a scroll chain walk per positioned box
- file:line: `layout/src/solver3/display_list.rs` `enter_scroll_chain`.
- class: waste (O(depth) cascade lookups per label per list). severity: medium
- fix: memo by `(parent layout index, absolute | fixed | other)` (`box_chains`).
- test: `audit_tests::a3_core::{the_positioned_boxes_of_one_parent_share_one_scroll_chain_walk,
  a_shared_scroll_chain_is_the_chain_of_every_box_that_shares_it}`.
- status: fixed 1a172f584

### A3-core-06 the inheritance pass was nodes x distinct inherited values
- file:line: `core/src/prop_cache.rs` `compute_inherited_values`, `InheritedValues::{values_for, set_node}`.
- class: quadratic-like (nodes x buckets x log nodes). severity: medium
- fix: an ancestor stack hands a node its parent's stored vector; `set_node_near` places a value by hash.
- test: `prop_cache_test::audit_a3_core_tests::the_inheritance_pass_does_not_read_every_distinct_value_for_every_node`.
- status: fixed 736f93547

### A3-core-06b `InheritedValues::get` scans all buckets for one inherited value
- file:line: `core/src/prop_cache.rs` `InheritedValues::get`.
- class: waste (O(buckets) per read of an inherited, non-compact property the node does not declare). severity: low
- evidence: 15 buckets for Berlin, 150-200 for Tuebingen's styles; reached from `get_property_slow`.
- fix: a per-type list of buckets.
- status: open: not written yet (this round)

### A3-core-07 `FlatVecVec::dedup_runs` compared every node's run with the table from the start
- file:line: `core/src/prop_cache.rs` `dedup_runs`.
- class: quadratic-like (nodes x distinct rule sets, capped at 1,024). severity: medium
- fix: the matched run is swapped to the front (nodes of a style come in a row).
- test: `prop_cache_test::audit_a3_core_tests::nodes_of_one_rule_set_do_not_each_read_the_table_of_runs`.
- status: fixed ffd50bd86

### A3-core-07b `dedup_runs` with interleaved styles
- class: waste (a node whose style differs from its predecessor's scans the table). severity: low
- status: open: not written yet (this round)

### A3-core-08 write generation on `GpuValueCache` (A9 H4/H6/H7)
- file:line: `core/src/gpu.rs`.
- class: waste (enabler; no reader yet). severity: medium
- fix: `value_generation` + `note_value_write()`; core's own writers bump by value, events untouched.
- test: `gpu_test::audit_a3_core_tests::the_value_generation_moves_with_a_value_and_not_with_an_event`.
- status: fixed b37653684
- (The direct writers in other areas' files and the readers are listed under "For area" below: their rows.)

### A3-core-09 sheets copied and parsed per render
- file:line: `core/src/styled_dom.rs` `collect_css_from_dom` / `strip_css_from_dom`, `css/src/css.rs`
  `Css::from_string`, `ParsedStyles::keep`.
- class: waste. severity: medium
- fix: `take_css_from_dom` (move); `PARSED_SHEETS` memo in `from_string`; `kept_bytes`.
- test: `styled_dom_test::audit_a3_core_tests::take_css_from_dom_moves_what_collect_listed_and_strips_the_tree`,
  `css::scoped_parse_tests::{a_sheet_is_parsed_once_per_text, the_bound_of_the_parsed_styles_counts_the_parsed_rules}`.
- status: fixed 806490941

### A3-core-09b a memo hit of `Css::from_string` deep-copies the parsed sheet (A9's item)
- file:line: `css/src/css.rs` `ParsedStyles::get`; the copy is needed because `scope_inline_css` writes each node's
  range into its rules' paths. Sharing needs `Dom::css` to hold `(Arc<Css>, scope)`: an api.json change.
- class: waste, once per view render. severity: low
- evidence: the editor's sheet (`mapedit3/src/ui/main/map.rs` `ViewSheet`: per text layer `.tN { 10-14
  declarations }` + `.tN:hover { background }`, per point style `.yN { 5 declarations }`): Tuebingen 26 + 26 + 21 =
  73 rules, about 440 declarations; Berlin 3 rules, about 14 declarations. A copy is one `Vec` of selectors, one of
  declarations and the class string per rule, the family list of each text class, and the declarations themselves:
  about 300 heap blocks and (at an assumed 64-72 bytes per declaration; `size_of` cannot be evaluated here) about
  40 KB for Tuebingen, 1.3 KB for Berlin: est. 30 us per view render, against a render of 10 ms and more (0.3 %).
- decision: noise; no api.json change for it.
- status: checked: fine

### A3-core-10a a11y: a pass walks every exposed node of every DOM
- file:line: `layout/src/managers/a11y.rs` `rebuild_retained`; `layout/src/window.rs` `update_a11y_tree_now`.
- class: waste (O(exposed nodes) per layout pass: 12,470 view nodes re-read for a keystroke in a sidebar).
  severity: medium
- fix: told the DOMs' generations (`A11yTreeInputs::dom_generations`), the pass keeps per DOM what its walk found
  and replays a DOM whose generation, replay key (pixel factor, language, scroll offsets and travel), edit state,
  window origin and host placement are the ones of that walk: no node of it is read (`A11yPassStats::walked` /
  `replayed`). The rest of the pass (reading order, child invariants, child list compare, removal, focus) is
  untouched and still sees every id: one map lookup and a push per replayed node remain.
- test: `audit_tests::a3_core::an_unchanged_dom_is_put_back_into_the_accessibility_tree_unread` (a manager told the
  generations against one that is not, eight passes at k = 50 and 200 labels: same updates; 0 nodes read when
  nothing changed, the window's 4 at both k when the window changed).
- status: open: one line in `update_a11y_tree_now` (`dom_generations: Some(&generations)`, the map collected from
  `self.layout_results.keys()` and `self.dom_generation(..)`) once A3-window's d305eaae7 is on the branch; that
  commit does not cherry-pick onto this branch (it needs its earlier commits). Mechanism and test: 240793513
- re-architecture not done, design: for a zero-cost replay the third pass, the invariant guard and the removal
  would have to work on per-DOM segments (`order` as one `Vec` per DOM, `seen` per DOM, the guard's `valid` /
  `claimed` sets only for walked DOMs plus the host nodes of replayed ones, whose own-DOM children are their
  retained children with ids of other DOMs filtered out). Not needed for the editor once 10b / 10c are in: the
  pass then runs only while a reader is attached.

### A3-core-10b a11y: the pass runs whether or not an assistive technology reads the tree (layout side)
- file:line: `layout/src/window.rs` `update_a11y_tree`, `a11y_update_pending`, `a11y_update_on_demand`;
  `layout/src/managers/a11y.rs` `hand_to_adapter`.
- class: waste (est. 5-12 ms per layout pass at 12.5k nodes, per 100 ms of scrolling). severity: medium
- fix: `A11yManager::on_demand`: a pass asked for only marks the tree stale; `a11y_update_on_demand(full)` runs it
  inside the adapter's `update_if_active` closure and always returns an update the adapter can take (whole tree for
  an adapter that holds none, a patch otherwise). Nothing is built and the retained tree does not advance while no
  reader is active. Inert until a shell sets `on_demand`.
- test: `audit_tests::a3_core::{an_a11y_pass_on_demand_runs_when_the_update_is_asked_for,
  nothing_is_built_while_no_reader_is_active_and_the_first_update_is_the_whole_tree,
  an_adapter_is_always_handed_an_update_it_can_take}`.
- status: fixed 43ff05b24

### A3-core-10c a11y: the macOS shell builds the update before `update_if_active` and activates its adapter itself
- file:line: `dll/src/desktop/shell2/macos/mod.rs` 10723-10888, `macos/accessibility.rs` (A10's files).
- class: waste (the closure runs on every call because `init_accessibility` forces the adapter active).
  severity: medium
- fix: the exact change is under "For area A10" below (switch `AZ_A11Y_EAGER=1` keeps today's path).
- status: open: the shell files are A10's; the change is written out under "For area A10"

### A3-core-10d a11y: Win32, X11 and Wayland build the update before `update_if_active`
- file:line: `windows/accessibility.rs:96-129` + drains `windows/mod.rs` 1953, 7662; `linux/x11/accessibility.rs:87-115`
  + drains `x11/mod.rs` 6706, 8408 and `wayland/mod.rs` 1744, 6896.
- proposed change (the same shape as macOS; these adapters are not forced active, so no init change): set
  `on_demand`; the activation handler answers `None` after `feed.placeholder_requested()` and wakes the loop
  (`loop_waker::wake()`; X11 / Wayland handlers run on accesskit's thread); `update_tree_on_demand(build)` as on
  macOS, with `try_lock` failing meaning "nothing ran, try again at the next drain" (no `feed.missed()`); each drain
  becomes `if lw.a11y_update_pending() || adapter.awaiting_tree() { .. }`. X11 / Wayland: the deactivation handler
  has to set the feed's phase to inactive. iOS (`UIAccessibilityIsVoiceOverRunning`) and Android build their own
  snapshot (`build_a11y_snapshot`); not looked at further.
- status: noted (other platform)

### A3-core-11a `restyle` generates the tag ids and every caller generates them again
- file:line: `core/src/prop_cache.rs` (end of `restyle`), `core/src/styled_dom.rs` (callers).
- class: waste (2 x O(nodes), est. 1-2 ms at 12.5k). severity: low
- status: open: not written yet (this round)

### A3-core-11b repeated whole-tree walks of the `Dom` before styling
- file:line: `core/src/styled_dom.rs` (`fixup_children_estimated` x3, `scope_inline_css` x2), the pre-passes of
  `LayoutWindow::style_user_dom_for`.
- class: waste (about 10 pointer-chasing walks of 12.5k nodes, est. 1-2 ms). severity: low
- status: open: not written yet (this round)

### A3-core-11c `copy_special_moving_complex` clones callbacks, a11y info and fluent args it could move
- file:line: `core/src/dom.rs` `copy_special_moving_complex`.
- class: waste (about 19,000 `RefAny` clone + drop pairs per Berlin render). severity: low
- status: open: not written yet (this round)

### A3-core-11d per text block: an ancestor walk for the background hint, three `BTreeMap`s and a `Vec`
- file:line: `layout/src/solver3/display_list.rs` `compute_uniform_text_bg`, `paint_inline_content`.
- class: waste (linear). severity: low
- status: open: not written yet (this round)

### A3-core-11e the general sibling combinator `~` walks all earlier siblings per candidate
- file:line: `core/src/style.rs` `matches_html_element_in`.
- class: quadratic for a sheet that uses `~` (`.a ~ .b` over 12,000 `.b` siblings without an `.a`: 72 million
  steps). severity: low (the editor's sheet has none)
- status: open: not written yet (this round)

### A3-core-12 the compact box slots were tenths of a pixel in an `i16` (A3-solver 36a)
- file:line: `css/src/compact_cache.rs` `CompactNodeProps` (18 slots), `core/src/compact.rs` (47 writes),
  `layout/src/solver3/getters.rs` `get_css_property_pixel!`.
- class: wrong-output (rounding: `left: 10.04px` was 10.0 at rest and 10.04 hovered) + waste (a cascade lookup per
  read beyond +-3276 px). severity: medium
- fix: (1) d91394f21: one number through both paths, and an overflow table (`px_overflow`) filled when the cache is
  built for what a slot cannot hold; (2) 10fece2b9: the slots are `PxSlot` = `i32` thousandths of a pixel, the
  number a declared length has; the slow path's quantisation is gone. `CompactNodeProps` is 108 bytes (was 72).
- test: `compact_test::a_box_length_slot_holds_the_declared_thousandths`,
  `audit_tests::a3_core::an_inset_is_one_number_at_rest_and_hovered_and_costs_no_lookup_beyond_the_slot`.
- guards whose numbers changed or can move: listed in the message of 10fece2b9.
- status: fixed 10fece2b9

### A3-core-13 properties without a compact answer were asked of the cascade per node (A3-solver 36b, 36c)
- file:line: `layout/src/solver3/getters.rs` `compute_all_zooms`, `get_aspect_ratio_property`,
  `get_vertical_align_property`, the `text-transform` read, `inherited_color_override`; `sizing.rs` (flex-wrap).
- class: waste (one or more cascade lookups per node per layout). severity: medium
- fix: three `DOM_HAS_*` bits (text-transform, vertical-align, zoom-or-aspect-ratio: the word is full); `flex-wrap`
  from its tier-1 slot; `CssPropertyCache::text_color_override_seen` gates the walk for a colour override.
- test: `audit_tests::a3_core::{a_dom_that_declares_no_zoom_ratio_alignment_or_transform_is_not_asked_for_them,
  the_used_colour_walks_to_the_root_only_for_an_override_of_colour}`.
- status: fixed 27632f452

### A3-core-13b `propagated_text_decoration` walks to the root per text block and text node (36c, second half)
- evidence: one compact bit per ancestor (`has_text_decoration`), no cascade lookup at rest: about 12 ns per call at
  depth 4, 12,400 calls per layout of the Berlin view = 0.15 ms. Linear, bounded by depth. A DOM-level bit would
  need a 33rd `DOM_HAS_*` flag (the word is full).
- severity: low
- status: checked: fine

### A3-core-14 `FloatValue::new` truncated to three decimals (A2-cpu-24)
- file:line: `css/src/props/basic/length.rs`.
- class: sign/offset (rounding). severity: medium
- fix: rounds to the nearest thousandth (half away from zero); NaN and infinities as before.
- test: the pinned examples in `length.rs`, `angle.rs`, `pixel.rs`, `props/layout/*.rs` (listed in the commit).
- status: fixed fb8751e41

### A3-core-15 the uniform text background is taken from ancestors only (A3-window's "possible wrong output")
- file:line: `layout/src/solver3/display_list.rs` `compute_uniform_text_bg`; reader `cpurender/raster.rs`
  `render_single_item` (Text arm).
- evidence: the claim can be false (an earlier positioned sibling, a tile image, lies under the text), but its one
  reader keeps it only when every pixel the run covers is that colour (`backdrop_is_colour`, cef7aaa9d, pixel test
  `text_over_a_positioned_layer_keeps_the_layers_colour_around_its_glyphs`); all four list walks go through it; the
  GPU path does not read the hint. A proof in the builder would cost labels x items or give the fast path up for
  every page with a positioned box. The comments said "proven"; they say hint now (5f29eb550).
- severity: medium (had it been wrong)
- status: checked: fine

### A3-core-16 `enter_scroll_chain` copied three lists per positioned box (A3-window)
- file:line: `layout/src/solver3/display_list.rs` `enter_scroll_chain`.
- class: waste (an allocation or two per label per list). severity: low
- fix: the lists are compared where they lie; nothing is allocated without a detour.
- test: `a_shared_scroll_chain_is_the_chain_of_every_box_that_shares_it` (existing) holds the result.
- status: fixed 7426fd1f9

### A3-core-17 selection rects carried no layout tag (A3-window-11)
- file:line: `layout/src/solver3/display_list.rs` `EmitPhase`, `paint_in_flow_descendants`.
- class: enabler for a patch of the drag highlight. severity: medium
- fix: `EmitPhase::Selection` (added last); `PatchState::build` leaves it out as it does the walk's items.
- test: `audit_tests::a3_core::the_rects_of_a_selection_carry_their_blocks_layout_index_and_the_selection_phase`.
- note: A3-window's `node_patch.rs:290` matches `EmitPhase` exhaustively: it needs `EmitPhase::Selection => {}`.
- status: fixed 505c953ee

### A3-core-19 `add_forced_page_break` scanned and sorted the list per break (A3-window)
- class: quadratic in the forced breaks of a paged document. severity: low
- fix: bisection + insert. test: the two existing tests in the file.
- status: fixed 73914ce4d

### A3-core-20 no way to tell whether a styled DOM was composed with `append_child` (A3-window-18)
- file:line: `core/src/styled_dom.rs` `append_child_with_index`, `recompute_inheritance_and_compact_cache`.
- class: enabler (the shell rebuilds the compact cache of every window DOM a second time). severity: medium
- fix: `CssPropertyCache::composed_since_recompute`, `StyledDom::needs_recompute_after_append()`.
- test: `styled_dom_test::audit_a3_core_tests::a_styled_dom_owes_the_recompute_only_after_an_append`.
- status: fixed 5000f480c

### A3-core-21 nothing kept `node_patch.rs` in step with the builder (A3-window)
- fix: a builder-side test of what a build emits between a box's background run and its content run.
- test: `audit_tests::a3_core::between_a_boxes_background_run_and_its_content_run_a_build_emits_only_what_the_patch_asks_about`.
- severity: medium
- status: fixed f87ab2acd

### A3-core-22 the builder asked the font manager for the loaded fonts per ellipsis and prompt (A3-solver 42)
- class: waste (two locks and a fold over all faces per call). severity: low
- fix: `DisplayListGenerator::loaded_fonts` (once per list build).
- status: fixed 73914ce4d

### Checked: fine (do not check again)
- `:nth-child`, `:first`, `:last`: O(1) per node from `CascadeInfo`; the tree is built in one pass (02).
- Descendant / child combinators: ancestor walk per candidate, O(depth). Adjacent sibling `+`: O(gap of text nodes).
- `get_parents_sorted_by_depth`: BFS, O(n). `subtree_len`: O(depth). `convert_dom_into_compact_dom`: one pass.
- `StyledDom::append_child`: O(children of root) count + a sort of `non_leaf_nodes` per call; twice per window DOM.
- `restyle` inheritance walk, `apply_ua_css_in_tree`, `hand_ua_defaults_down`, `build_compact_cache_with_inheritance`,
  `generate_tag_ids`, `sync_backdrop_state`, `hide_contentless_generated_nodes`: one pass each, O(n x props).
- `insert_generated_nodes`: returns early without `content` rules.
- Hover restyle in core (`restyle_nodes_state`): touches the changed nodes and their parts only (answer 3).
- Display list: `collect_stacking_contexts` / `file_into_context` visit each box once; z-order = 3 filters + 2 stable
  sorts per context; `push_item` O(1) except `PopOpacity` (`group_paint_bounds` walks the group).
- a11y: no quadratic (`ScrollChains::compute` is the linear trie; the parent walk stops at the parent).
- status: checked: fine

## The four answers (as of this round)

1. **Styling one view render of 12.5k nodes, step by step** (`StyledDom::create_from_dom_inheriting`): `Dom` walks
   (linear; 11b), parse (memoised: 09), `construct_html_cascade_tree` (was quadratic flat: 02), match (was nodes x
   rules: 04), inheritance walk + `dedup_runs` (07), UA pass + `compute_inherited_values` (06), compact cache (12,
   13), tag ids (twice: 11a). Superlinear before this branch: the cascade tree when flat; matching, dedup and
   inherited values in nodes x styles.
2. **A flat list of 12k siblings under one parent is safe in this area after this branch**, except for a sheet that
   uses the general sibling combinator `~` (11e).
3. **A paint-only restyle of ONE node (hover colour)**: `restyle_nodes_state` re-matches nothing; it compares the
   hover properties the node declares before / after. It re-runs the cascade only for a rule with `:hover` left of
   its subject, and the inheritance + compact cache only for an inherited property. What follows (list rebuild or
   patch) is A3-window's.
4. **a11y**: built always (default feature), diffed per layout pass. With 10b + 10c nothing is built while no
   reader is attached; with 10a a reader's pass reads only the DOMs that changed.

## For area A2-gpu / A3-window / A6 / A1 / A3-solver (writers of `GpuValueCache` maps: A3-core-08)

Every direct write of a value map must be followed by `cache.note_value_write()` (once per batch is enough) before
any reader may skip work on an unchanged `value_generation()`. Sites at the base commit (`git grep -nE` for
`(current_transform_values|current_opacity_values|current_color_values|scrollbar_[vh]_opacity_values|sheen_light)`
followed by a mutating call or `=`):
- `layout/src/managers/gpu_state.rs`: 1356, 1660-1672, 1708-1709, 1737-1743 (A2-gpu).
- `layout/src/managers/mod.rs`: 481-483.
- `layout/src/window.rs`: 9996, 10006, 10029, 11903, 11907 (A3-window); 18517, 18519, 18760, 18783, 18793, 19024,
  19026, 19046, 19048, 19311, 19356 (A3-window); 21767, 21787 (A6).
- `layout/src/solver3/mod.rs`: 3850 (A3-solver). `layout/src/cpurender/raster.rs`: 10886, 10995, 10997 (A1; tests?).
- Also any `get_mut` / `entry` / `values_mut` / whole-map assignment the pattern does not catch: re-grep at merge.

## For area A1 and A4 (readers: A9's H4, H6, H7)

Once the writers above bump the generation: keep, per DOM, the `value_generation()` last read. A frame whose every
DOM cache has the generation it had at the previous frame AND whose display lists are the same `Arc`s has the same
bound values: skip the copy, the comparison and the transaction. A DOM cache that was replaced (remount) must be
compared by `(DomId, generation)` plus the cache's presence.

For A1 (A3-core-15): the rect `render_single_item` verifies before it trusts the background hint is
`item.visual_bounds()` (for `Text`: the pen positions grown by 1 em left / right, 1.5 em up, 0.75 em down, cut to
the item's clip). A glyph tile that reaches beyond that box (ink wider than 1 em right of the last pen position:
an icon font's wide glyph, a long ligature; plus the LCD filter's pad) is stamped over pixels that were not
compared. If that can happen, verify the union of the tile rects instead.

## For area A10: the accessibility update built on demand (A3-core-10), macOS

The layout side is in place and inert (`A11yManager::on_demand` is `false`): commits `a611ca2bd`, `43ff05b24`.
- `lw.a11y_manager.on_demand = true`: `update_a11y_tree()` and `update_a11y_tree_incremental()` then only note that a
  pass is due (`stale`); nothing is walked, nothing is parked, the retained tree does not move.
- `lw.a11y_update_pending() -> bool`: a pass is due (or an update is parked).
- `lw.a11y_update_on_demand(full: bool) -> accesskit::TreeUpdate`: runs the pass that is due and returns what the
  adapter is to get. `full = true`: the whole tree (for an adapter that holds none). `full = false`: the patch, or a
  patch without a node (same focus) when nothing changed. It always returns an update the adapter can take.

Why macOS needs more than the closure: `init_accessibility` ACTIVATES the adapter itself (`macos/mod.rs` 10792-10802:
it sends `accessibilityChildren` to the view, `request_initial_tree` answers `None`, the adapter is a placeholder;
10809-10812 then hand it the whole tree). From then on `update_if_active` runs its closure on every call, VoiceOver or
not: building inside the closure saves nothing while that forced activation stays. The saving comes from letting the
adapter become active when a client really asks.

Exact change, all behind a switch (RULES 8.5; it cannot be checked without VoiceOver): `AZ_A11Y_EAGER=1` keeps
today's path (forced activation, eager pass, `update_tree`); the default is the lazy path below. Also keep today's
path whenever an E2E script or the debug server is attached: they read `lw.a11y_manager` (`full_tree()`,
`last_rejection`, `layout/src/e2e/full.rs` 12258, 12783, 12828) without an adapter and need the eager tree.

1. `common/accessibility.rs`, `impl A11yTreeFeed`: add
   ```rust
   /// Built on demand: the adapter holds no tree (not active yet, just activated, or it missed an update).
   pub fn wants_full_tree(&self) -> bool { self.phase() != ACTIVE }
   /// Built on demand: the adapter was activated and waits for its first tree.
   pub fn awaiting_tree(&self) -> bool { self.phase() == AWAITING_TREE }
   /// Built on demand: the adapter took an update built inside its closure.
   pub fn built_and_taken(&self) { self.set_phase(ACTIVE); }
   ```
   On the lazy path `next_update` / `initial_tree` are not called: the feed keeps no copy of the tree (it cannot,
   nothing is built), only the phase.
2. `macos/accessibility.rs`:
   - `TreeActivationHandler` gets `window: usize` (the `MacOSWindow` address `schedule_a11y_delivery` uses as its
     token) and `MacOSAccessibilityAdapter::new(view, window)` passes it. `request_initial_tree` as it is
     (`placeholder_requested(); None`) plus, after it: `super::after_turn::a11y_delivery().request(self.window);`
     so that the turn after an activation hands the tree over although no layout ran (the handler runs on the main
     thread, inside the client's query).
   - new, beside `update_tree` (which stays for the eager path):
     ```rust
     pub fn awaiting_tree(&self) -> bool { self.feed.awaiting_tree() }

     /// `build(full)` runs INSIDE `update_if_active`, i.e. only while an assistive technology is active.
     pub fn update_tree_on_demand(&mut self, build: impl FnOnce(bool) -> TreeUpdate) {
         ax_language::install();
         let full = self.feed.wants_full_tree();
         let mut ran = false;
         let result = crate::desktop::recoverable_panic::catch(|| {
             self.adapter.update_if_active(|| {
                 ran = true;
                 let update = build(full);
                 if let Some(language) = update.nodes.iter().find(|(id, _)| id.0 == 0)
                     .map(|(_, root)| root.language().unwrap_or_default().to_string())
                 {
                     ax_language::set(&language);
                 }
                 update
             })
         });
         match result {
             Ok(events) => {
                 if ran { self.feed.built_and_taken(); }
                 if let Some(events) = events { events.raise(); }
             }
             // The closure may have run (the retained tree advanced) and the adapter did not take it.
             Err(_) => self.feed.missed(),   // + the warning `update_tree` logs
         }
     }
     ```
     `missed()` only moves ACTIVE to AWAITING_TREE; a panic while AWAITING stays AWAITING: the next call asks for
     the whole tree either way.
3. `macos/mod.rs`:
   - `init_accessibility` (10723): pass the window address to `new`; set `lw.a11y_manager.on_demand = true`; DROP the
     forced activation (10792-10802) and the `resend_full_tree()` + `update_accessibility()` after it (10809-10812);
     keep `update_view_focus_state(true)`. The placeholder -> active transition (the one that posts the focus
     notification VoiceOver needs) then happens at the first delivery after a client's first query instead of at
     window creation.
   - `update_accessibility` (10830), body:
     ```rust
     let (Some(adapter), Some(lw)) = (self.accessibility_adapter.as_mut(), self.common.layout_window.as_mut())
     else { return };
     lw.update_a11y_tree();                      // on demand: notes that a pass is due
     if lw.a11y_update_pending() || adapter.awaiting_tree() {
         adapter.update_tree_on_demand(|full| lw.a11y_update_on_demand(full));
     }
     ```
   - `deliver_a11y_now` (10872): the `a11y_dirty` branch and `rebuild_a11y_after_scroll_if_due()` as they are; the
     drain (10880-10888) becomes the same `if pending || awaiting { update_tree_on_demand(..) }`.
   While no client is active `a11y_update_pending()` stays `true` (the pass that is due never runs) and each delivery
   costs one `update_if_active` that returns at once: no node is visited.
   `refill_a11y_tree_unless_refreshed` (`common/event.rs` 5143) compares `a11y_tree_updates`, which does not move on
   the lazy path, and calls `update_a11y_tree()` once more: that only sets the flag again.

What to look at on the real window (also listed under that heading below): with the default (lazy) path, start
VoiceOver AFTER the editor is up: the focused control is announced and the VoiceOver cursor walks the toolbar and
the map's labels; start the editor with VoiceOver already running: same; Accessibility Inspector shows the tree and
follows a hover. `AZ_A11Y_EAGER=1` must behave as before.

Other platforms: noted, not to be changed now (RULES 8.4), see A3-core-10d.

## For area A3-window

- `EmitPhase::Selection` (A3-core-17): `display_list/node_patch.rs:290` needs the arm `EmitPhase::Selection => {}`.
- `StyledDom::needs_recompute_after_append()` (A3-core-20) is there for `dll/.../common/layout.rs` about 1079:
  `if styled_dom.needs_recompute_after_append() { styled_dom.recompute_inheritance_and_compact_cache(); }`.
- The a11y replay (A3-core-10a) reads `LayoutWindow::dom_generation`: the line for `update_a11y_tree_now` is in the
  finding. What the replay needs of the generation: equal values promise the same node types, texts, attributes,
  a11y info and callback kinds, the same hierarchy, boxes and positions (what the doc of `DomGenerations` says).
- `layout/src/window.rs:13065`: a tag lookup by scanning `tag_ids_to_node_ids` (as A3-core-01); one lookup per call.

## For area A3-solver

- `getters.rs`, `sizing.rs`, `taffy_bridge.rs`, `fc/table_borders.rs` were touched for A3-core-12 and -13 (see
  Claims): a few lines each inside existing functions, and `get_aspect_ratio_property` became a function around the
  macro-made `aspect_ratio_from_the_cascade`.
- `DisplayListGenerator::loaded_fonts` (A3-core-22) can read `self.ctx.loaded_fonts()` once that exists here.

## api.json

No change. (`Dom::css` as a shared sheet was allowed and is not needed: A3-core-09b.)

## What to look at on the real window

- A3-core-12: label and tile positions of a map are no longer rounded to tenths of a pixel: a label can sit up to
  0.05 px from where it sat. Nothing should jump on hover.
- A3-core-10c (once A10 has made the change): the VoiceOver checks listed under "For area A10".
- A3-core-17: selecting text (drag) highlights as before.

## What was not read

- `core/src/dom.rs` outside the builder / flatten paths, `xml*.rs`, `icon.rs`, `custom_property_cascade.rs`,
  `ua_css.rs` beyond the tables named above, `compact.rs` beyond the builder loop and the slot writers, `diff.rs`
  (A3-window), `events.rs` / `hit_test.rs` (A6), `transform.rs` (A2-gpu), `resources.rs`, `gl*.rs`, `svg.rs`.
- `css/src`: only `css.rs` (parse entry points, memo), the codecs and slots of `compact_cache.rs`, `length.rs`; not
  the parser (`parser2.rs`), the other property parsers, `dynamic_selector.rs`, `system.rs`.
- `layout/src/solver3/display_list.rs`: read the generator's walk; NOT read: pagination / slicer, tables, scrollbars,
  inline box fragments and ellipsis, SVG masks, `compute_patch_move_summary`, `FrameIndex` (A1), most item methods.
- `layout/src/managers/a11y.rs`: not `build_content` / `build_node` / the role maps.
- The shells other than what "For area A10" names (read for the a11y hand-over only).
