# A3-core: quadratics and waste in `core/`, `css/`, the display list builder, a11y

Area agent A3-core of the blind audit (2026-11-21). Nothing here was compiled or run.
Branch `audit/A3-core`, base `audit/base` (`f48bcc7b4`).

## Claims

Functions this area rewrote or restructured (final):

- `core/src/style.rs`: `construct_html_cascade_tree`; `ScopedRuleIndex` + `MergedRules` are gone, replaced by
  `SubjectRuleIndex` (+ `subject_class`).
- `core/src/prop_cache.rs`: `InheritedValues` (`set_node_near`, `bucket_of`, `push_bucket`, `by_hash`, `append`),
  `CssPropertyCache::compute_inherited_values`, `store_if_changed`, `FlatVecVec::dedup_runs`, the rule loop of
  `CssPropertyCache::restyle` (candidates only).
- `core/src/styled_dom.rs`: `take_css_from_dom` (new), its two callers; `collect_css_from_dom` / `strip_css_from_dom`
  are test-only now.
- `core/src/gpu.rs`: `GpuValueCache::value_generation` / `note_value_write`, and the bodies of
  `apply_transform_events`, `apply_opacity_events`, `refresh_transform_value(s)_of`, `refresh_opacity_value_of`
  (they only gained the generation; events and values are as before).
- `css/src/css.rs`: `Css::from_string` (memo), `ParsedStyles` (`kept_bytes`, `keep`).
- `layout/src/solver3/display_list.rs`: `get_tag_id` -> `DisplayListGenerator::tag_id_of` / `tags_by_node`,
  `enter_scroll_chain` (`box_chain_key`, `box_chain_links`), `paint_selections`, `paint_cursor`.
- Not touched although claimed at the start: `layout/src/managers/a11y.rs`, `window.rs` 19632-19900 (findings only).

## Findings

Real sizes used below: Berlin view 12,470 DOM nodes (about 6,200 labels = span + text, 1 text style: 3 rules in the
view's sheet), Tuebingen view 2,192 nodes / 17,495 display list items / 2,118 reference frames / 3,703 hit-test areas
(26 text styles + 21 point styles: up to ~75 rules). Costs are estimates from reading (steps x a guessed ns), never
measured: the ranking is by steps.

### A3-core-01 the display list scans the DOM's tag list once per box
- file:line: `layout/src/solver3/display_list.rs` `get_tag_id` (12202 at base), asked by `paint_node_content_inner`
  (9327) for every box, `generate_for_stacking_context` (6870), `paint_inline_shape_of` (11871).
- class: quadratic (boxes x tags).
- evidence: `tag_ids_to_node_ids.iter().find(node)`: a tagged box reads the list up to its entry, an untagged one (text
  node, group div, anonymous box with a DOM node) all of it. Berlin: 6,200 tagged spans x ~3,100 + 6,200 text nodes x
  6,200 = about 54 million entry compares per display list of the view (est. 40-80 ms); Tuebingen 2,192 x ~1,500 = 3
  million. Runs at every build of the view's list: settle render, pan render, every hover that rebuilds the list (Z3).
- fix: `DisplayListGenerator::tag_id_of` reads a per-generation table by node index (`tags_by_node`, first entry per
  node = what `find` returned; out-of-table ids still scan). Identical answers.
- test: `audit_tests::a3_core::a_display_list_reads_the_tag_list_once_not_once_per_box` (counter `TAG_LIST_READS`, 250
  vs 1,000 labels, flat and groups of 64), `every_hit_test_area_carries_the_tag_of_its_node`.
- status: fixed `4720e5091`.

### A3-core-02 the cascade tree walks back over the siblings of every node that has children
- file:line: `core/src/style.rs` `construct_html_cascade_tree` (447 at base).
- class: quadratic (k^2 / 2 for k siblings that have children).
- evidence: per non-leaf node `preceding_siblings(..).count()`. A label is span > text, so every label is a non-leaf:
  flat, 6,200 labels = 19 million sibling steps (each reads a `NodeData`) per styling; in groups of 64 it is 200,000.
  This is the third flat-sibling quadratic (after Z9, Z10): it alone would have kept the flattened editor slow.
  Called once per `StyledDom` creation, and a second time by `generated_content::insert_generated_nodes` when a sheet
  has `content` rules.
- fix: the child walk records each child's count of earlier element siblings; a parent reads it (the old walk stays
  for the root and for a node listed before its parent).
- test: `style_test::audit_a3_core_tests::{a_parent_of_many_parents_is_walked_once,
  the_cascade_tree_is_the_one_the_walk_back_built}` (counter `SIBLING_STEPS`; result against a copy of the old walk).
- status: fixed `a2ff3cc60`.

### A3-core-03 selection and caret painters copy every text block's lines before looking for a selection
- file:line: `layout/src/solver3/display_list.rs` `paint_selections` (5986), `paint_cursor` (6232).
- class: waste (per text block per display list: one `Arc<UnifiedLayout>` made from the dense clusters).
- evidence: `materialized_inline_layout_for_node` first, `is_text_selectable` / selection lookup / caret lookup after.
  Berlin: 6,200 allocations and ~50,000 cluster conversions per list build (est. 2-4 ms), for labels that are
  `user-select: none`; `paint_cursor` paid it whenever any caret existed in the window.
- fix: existence check first, the copy only in the branch that paints.
- test: `audit_tests::a3_core::a_block_without_a_selection_or_a_caret_copies_no_lines` (counter
  `INLINE_LAYOUT_COPIES`; needs the system fonts, like the caret tests).
- status: fixed `e1a9d8435`.

### A3-core-04 selector matching was nodes x rules for one sheet on a subtree root
- file:line: `core/src/prop_cache.rs` `restyle` (rule loop, 2232-2326 at base), `core/src/style.rs` `ScopedRuleIndex`.
- class: quadratic-like (nodes x rules; rules grow with the map's text styles, not with nodes).
- evidence: `Dom::with_component_css` scopes every rule of the sheet to the whole subtree, so every node was tried
  against every rule in its pass (`matches_html_element_in`, ~50 ns), and `candidates()` rebuilt + sorted the active
  list per node. Tuebingen 2,192 nodes x 75 rules = 164,000 matches; Berlin 12,470 x 3 = 37,000; a Berlin-sized view
  with Tuebingen's styles 935,000 (est. 45 ms).
- fix: `SubjectRuleIndex`: rules whose subject compound names a class are kept per class (scoped ones entered /
  left by the same scope walk) and offered only to nodes carrying the class; other rules as before. The dropped
  candidates are exactly those `selector_group_matches` rejects on `has_class`; order = rule order (sorted).
- test: `prop_cache_test::audit_a3_core_tests::{a_node_is_tried_against_the_rules_of_its_own_classes,
  the_rules_of_a_nodes_classes_apply_in_sheet_order_and_scope}`; the existing
  `a_restyle_tries_each_node_only_against_the_rules_scoped_around_it` still bounds the scope walk.
- status: fixed `d08a602d4`.

### A3-core-05 a scroll chain walk per positioned box
- file:line: `layout/src/solver3/display_list.rs` `enter_scroll_chain` (7097 at base).
- class: waste (O(depth) cascade lookups per label per list; identical answers).
- evidence: `ScrollChain::of` per stacking context / positioned box / absolute in-flow child: `box_anchor` +
  `establishes_containing_block` + `paints_as_a_group` + `chain_link` per level, ~10 slow-path lookups per label
  (est. 1 us): 6,200 per Berlin list.
- fix: memo by `(parent layout index, absolute | fixed | other)` (`box_chains`), which is all `box_anchor` reads of
  the box itself.
- test: `audit_tests::a3_core::{the_positioned_boxes_of_one_parent_share_one_scroll_chain_walk,
  a_shared_scroll_chain_is_the_chain_of_every_box_that_shares_it}`.
- status: fixed `1a172f584`.

### A3-core-06 the inheritance pass was nodes x distinct inherited values
- file:line: `core/src/prop_cache.rs` `compute_inherited_values` (6060), `InheritedValues::{values_for, set_node}`.
- class: quadratic-like (nodes x buckets x log nodes), plus one deep copy of the parent's values per node.
- evidence: per node `values_for(parent)` binary-searched every bucket's node list and cloned the hits; `set_node`
  compared each of the node's ~8 values with every bucket. Buckets = distinct (type, value, origin): ~15 for Berlin,
  150-200 for Tuebingen's 26 text styles (about 3,000 steps per node there).
- fix: an ancestor stack hands a node its parent's stored vector (borrowed; the same content `values_for` rebuilt);
  `set_node_near` places a value in the parent's bucket when equal, else by hash (`by_hash`).
- test: `prop_cache_test::audit_a3_core_tests::the_inheritance_pass_does_not_read_every_distinct_value_for_every_node`
  (+ the existing `compute_inherited_values_*` and `core/tests/css_inheritance.rs`).
- status: fixed `736f93547`. Still open: `InheritedValues::get` (the read of ONE inherited value by
  `get_property_slow`) scans all buckets and binary-searches those of the type: O(buckets) per read of an inherited,
  non-compact property that the node does not declare. A per-type bucket list would make it O(buckets of the type).

### A3-core-07 `FlatVecVec::dedup_runs` compared every node's run with the table from the start
- file:line: `core/src/prop_cache.rs` `dedup_runs` (641).
- class: quadratic-like (nodes x distinct rule sets, capped at 1,024), three calls per styling.
- fix: the matched run is swapped to the front (nodes of a style come in a row). Exact (a run is recorded once).
- test: `prop_cache_test::audit_a3_core_tests::nodes_of_one_rule_set_do_not_each_read_the_table_of_runs`.
- status: fixed `ffd50bd86`; open for interleaved styles (a hash per run costs Berlin more than it saves).

### A3-core-08 write generation on `GpuValueCache` (for A9 H4/H6/H7)
- file:line: `core/src/gpu.rs`.
- class: waste (enabler; no reader yet).
- fix: `value_generation` + `note_value_write()`; core's own writers bump by value (not per event: A2-gpu-08's
  event count is untouched).
- test: `gpu_test::audit_a3_core_tests::the_value_generation_moves_with_a_value_and_not_with_an_event`.
- status: fixed `b37653684` (core side). NOT usable by readers until the direct writers listed under "For area"
  bump it.

### A3-core-09 sheets copied and parsed per render
- file:line: `core/src/styled_dom.rs` `collect_css_from_dom` / `strip_css_from_dom` (3966), `css/src/css.rs`
  `Css::from_string` (194), `ParsedStyles::keep`.
- class: waste.
- evidence: every node sheet deep-cloned once more at styling (then the original dropped); the view's sheet
  (`ViewSheet`, up to 75 rules) parsed at every render although a pan render repeats the text; the memo's 4 MB bound
  counted text only.
- fix: `take_css_from_dom` (move); `PARSED_SHEETS` memo in `from_string`; `kept_bytes`.
- test: `styled_dom_test::audit_a3_core_tests::take_css_from_dom_moves_what_collect_listed_and_strips_the_tree`,
  `css::scoped_parse_tests::{a_sheet_is_parsed_once_per_text, the_bound_of_the_parsed_styles_counts_the_parsed_rules}`.
- status: fixed `806490941`. Open (A9's item): a memo hit still deep-copies the rules, because `scope_inline_css`
  writes each node's range into its rules' paths. Sharing needs `Dom::css` to hold `(Arc<Css>, scope)` (public `Dom`:
  api.json) with the cascade reading the scope beside the rule.

### A3-core-10 a11y: every layout pass walks every exposed node of every DOM, attached reader or not
- file:line: `layout/src/window.rs:3787` (call, after every `layout_and_generate_display_list`), `19632`
  `update_a11y_tree`, `layout/src/managers/a11y.rs:1292` `rebuild_retained`; scroll path
  `dll/.../common/event.rs:5193` `rebuild_a11y_after_scroll_if_due` (at most every 100 ms while an offset moves).
- class: waste (O(exposed nodes) per layout; no quadratic found: the `ScrollChains` trie and the per-node parent walk
  are linear).
- evidence: `a11y` is a default feature of azul-layout / azul-dll. `is_exposed_to_accessibility` is true for every
  div, span and non-blank text node, so all 12,470 view nodes are in the tree. A pass DIFFS (node signatures; an
  unchanged node is not rebuilt, an unchanged tree publishes nothing), but it still does per node: `dom_to_layout`
  lookup, a `ScrollChain` (a `Vec`) from the trie, 2 inserts into `own_origin` / `parent_of_entry`, a signature hash
  (type, text, attributes, a11y info, child texts), `retained.nodes` lookup, `parent_children_map` push, 2 set
  inserts in `enforce_child_invariants`, a map remove in the third pass: about 10 hash operations + 1-2 allocations,
  est. 0.4-1 us per node = 5-12 ms per pass at 12.5k nodes, whether or not a screen reader is attached (nothing in
  `update_a11y_tree` asks). Triggers: every successful layout pass of the window (a keystroke in a sidebar field
  relays out the window DOM and re-walks the unchanged 12.5k view nodes), every view re-render that goes through the
  layout tail, and every 100 ms while a scroll offset moves.
- fix (not made blind: a wrong gate leaves a screen reader without updates): (1) gate the pass on "a platform
  adapter has asked for the tree" - `A11yManager` gets `consumer_attached: bool` (default true = today's behaviour),
  the shells set it false at start and true from accesskit's activation handler (then `resend_full_tree()` + one
  pass); the e2e / debug-server tree readers set it true. (2) Skip a DOM whose inputs did not move since the last
  pass: needs a per-DOM generation bumped where `layout_results` entries are replaced or patched (A3-window).
  (3) Constant factors: sum the scroll offset per chain id once (`box_chain_id`) instead of materialising a
  `ScrollChain` per node.
- status: open (see "For area A10" / questions).

### A3-core-11 open, small: other per-node waste seen in the styling path (not fixed)
- `core/src/styled_dom.rs:1470` / `prop_cache.rs:2600`: `restyle` ends in `generate_tag_ids`, and every caller
  discards that result and generates the ids again after the compact cache exists (2 x O(nodes), ~1-2 ms at 12.5k).
  Fix: split `restyle` from the tag ids (tests read the returned ids, so the signature change touches tests).
- `core/src/styled_dom.rs:1255-1275 / 1680-1711 / 3897`: `fixup_children_estimated` x3 and `scope_inline_css` x2
  per styling of a `Dom` (plus 5 whole-tree pre-passes in `LayoutWindow::style_user_dom_for`: form controls, webview
  probe, translation, respin, icons): ~10 pointer-chasing walks of 12.5k `Dom` nodes (~1-2 ms). Fix: one walk.
- `core/src/dom.rs:4361` `copy_special_moving_complex` moves style / extra / node type but CLONES `callbacks`
  (3 `RefAny` clones + a `Vec` per label), `accessibility`, `fluent_args`, and the originals are dropped with the
  `Dom`: ~19,000 `RefAny` clone+drop pairs per Berlin render. Moving them is the same data; not done blind because the
  function carries web-lift (remill) constraints I cannot check.
- `layout/src/solver3/display_list.rs` `compute_uniform_text_bg` (5768): one ancestor walk with
  `get_background_contents` per text block (3-4 lookups per label); `paint_inline_content` (11281-11299): three
  `BTreeMap`s and an ancestor `Vec` per text block. Linear; a memo per parent would halve the lookups.
- `core/src/style.rs` general sibling combinator (`~`): `matches_html_element_in` walks ALL earlier siblings of a
  candidate until one matches: O(siblings) per node whose subject compound matches, so `.a ~ .b` over 12,000 `.b`
  siblings without an `.a` is 72 million steps. Only when a sheet uses `~` (the editor's does not). Fix: per parent,
  the first sibling index matching each left compound (one pass).

### A3-core-12 (from A3-solver 36a) compact pixel slots round to 0.1 px and overflow at +-3276.7 px
- file:line: `css/src/compact_cache.rs:1278` `encode_resolved_px_i16` (x10 in an `i16`), fields `top/right/bottom/
  left`, `margin_*`, `padding_*`, `border_*_width`, gaps of `CompactNodeProps`; readers `getters.rs:979-1035`.
- class: wrong-output (rounding) + waste (slow path beyond the range).
- evidence: the resting-state fast path answers `round(px * 10) / 10`, the slow path (any raised state, e.g. hover)
  the declared value (three decimals): a label at `left: 123.456px` is laid out at 123.5 at rest and at 123.456 if a
  relayout happens while it is hovered (0.044 px). The exact value is the right one (CSS has no such rounding).
  A value beyond 3,276.3 px (a 4K view, or a picture coordinate inside the overscan of a 2560 px view at the right
  edge: 2560 + 512 = 3,072 is inside, 3,840 is not) is the sentinel: a cascade lookup per read.
- fix (not made: 145 encode / decode sites in three files, one of them A3-solver's, and every layout number with a
  fraction moves by up to 0.05 px, which no guard can be reasoned through blind): store these slots in the lossless
  `u32` encoding the sizes already use (`encode_pixel_value_u32`, +2 bytes x 18 slots = +36 bytes per node), decode
  through the same accessors. Test to write with it: the same node read at rest and with `hover` raised gives the
  same number for `left: 123.456px` and for `left: 5000px`, and the compact read count for 5000 px is 0 slow reads.
- status: open.

### A3-core-13 (from A3-solver 36b/36c) properties without a compact arm or DOM flag
- `aspect-ratio`, `zoom`, `text-transform`, `vertical-align` (property getter), `flex-wrap` getter: one cascade
  lookup per node per call. Fix as proposed by A3-solver: a `DOM_HAS_*` bit each in
  `CompactLayoutCache::dom_declared_flags` (set in `update_dom_declared_flags`, `core/src/compact.rs`), the getter
  answering the initial value when the bit is clear. Three bits are said to be free; each getter is in
  `solver3/getters.rs` (A3-solver's), the flags in `css/src/compact_cache.rs` + `core/src/compact.rs` (mine).
- `inherited_color_override` / `propagated_text_decoration`: a root walk per node once any user override exists.
  Fix: a count of nodes with a `color` override on the cache (`user_overridden_properties` writers are in
  `styled_dom.rs`), the walk skipped at 0.
- status: open (not started: budget).

### Checked: fine (do not check again)
- `:nth-child`, `:first`, `:last`: O(1) per node from `CascadeInfo` (`style.rs` `match_nth_child`); the tree is built
  in one pass now (A3-core-02). The runner's `span:nth-child(25640)` is a printed path, not a scan.
- Descendant / child combinators: ancestor walk per candidate, O(depth). Adjacent sibling `+`: O(gap of text
  nodes). `~`: see A3-core-11.
- `get_parents_sorted_by_depth` (`id.rs:314`): BFS, O(n). `subtree_len`: O(depth).
- `convert_dom_into_compact_dom`: one recursive pass, O(n); no vector splices.
- `StyledDom::append_child`: O(children of root) count + a sort of `non_leaf_nodes` per call (quadratic only when used
  in a loop; `append_child_with_index` + `finalize_non_leaf_nodes` exist for that). The dll uses it twice per window
  DOM (titlebar + user DOM), not per view render.
- `restyle` inheritance walk (`prop_cache.rs:2396`): per parent, 10 states x its own props + its children; O(n x
  props). `apply_ua_css_in_tree`, `hand_ua_defaults_down`, `build_compact_cache_with_inheritance`,
  `generate_tag_ids`, `sync_backdrop_state`, `hide_contentless_generated_nodes`: one pass each, O(n x props).
- `insert_generated_nodes`: returns early without `content` rules (two O(n) scans of inline props).
- Hover restyle in core (`restyle_nodes_state`): touches the changed nodes and their parts only; see answer 3.
- Display list: `collect_stacking_contexts` / `file_into_context` visit each box once; z-order = 3 filters + 2
  stable sorts per context (O(c log c)); `establishes_stacking_context` / `paints_as_positioned_box` are O(1) lookups
  (called 3-4 times per box); `push_item` O(1) except `PopOpacity` (`group_paint_bounds` walks the group: nested
  opacity groups cost depth x size); glyphs are copied once per run per list (`to_vec_offset`), linear.
- a11y: no quadratic (`ScrollChains::compute` is the linear trie; the nearest-exposed-parent walk stops at the
  parent because every element is exposed).

## The four answers

1. **Styling one view render of 12.5k nodes, step by step** (`StyledDom::create_from_dom_inheriting`):
   (a) `Dom` walks: `fixup_children_estimated`, `scope_inline_css`, sheet collection (was a deep copy per node sheet:
   09), flatten (`convert_dom_into_compact_dom`, clones each node's callbacks: 11): linear.
   (b) parse: node strings are memoised by text (`parse_scoped`), the view's sheet was parsed per render (09).
   (c) `construct_html_cascade_tree`: linear in groups, QUADRATIC flat (02, fixed).
   (d) match (`restyle`): was nodes x rules-in-scope x passes (04, fixed: nodes x own-class rules).
   (e) inheritance walk + `sort_each_and_flatten` + `dedup_runs`: linear x props; dedup was nodes x rule sets (07).
   (f) UA pass, `compute_inherited_values`: was nodes x distinct inherited values (06, fixed).
   (g) compact cache, prune, tag ids (twice: 11): linear.
   Superlinear before this branch: (c) when flat; (d), (e-dedup), (f) in nodes x styles.
2. **A flat list of 12k siblings under one parent is safe in this area after this branch**: the cascade tree (02)
   was the one flat-sibling quadratic in `core/`; `:nth-child` is O(1); no vector splices in `StyledDom`
   construction; the display list builder files and sorts each box once (stable sort per context) and no longer
   scans the tag list per box (01). Not safe: a sheet using the general sibling combinator `~` (11). A3-solver
   reports the solver linear for 12k absolute siblings on its branch; `diff` / window code is A3-window's answer.
3. **A paint-only restyle of ONE node (hover colour) in this area**: `restyle_nodes_state` re-matches nothing. It
   sets the flag on the node (and its `part_of_parent` children), lists the property types that node declares for
   `:hover` (its sheet rules, inherited entries, inline conditions) and compares `get_property_slow` before / after
   for each: O(hover props of the node). It re-runs the whole cascade only if a rule names `:hover` LEFT of its
   subject (`left_state_mask`; the editor's `.tN:hover` does not), and the whole inheritance + compact cache (O(n))
   only if a changed property is inherited (`color`, `cursor`; the editor's hover changes `background`: not
   inherited). What follows the restyle (display list rebuild or patch) is A3-window's; a rebuild of the view's list
   was 54 M tag-list reads + 6,200 chain walks + 6,200 line copies on Berlin before 01 / 05 / 03.
4. **a11y**: built always (default feature), diffed per layout pass, never rebuilt per frame; O(exposed nodes) per
   pass with no reader attached: est. 5-12 ms at 12.5k nodes per layout pass and per 100 ms of scrolling
   (A3-core-10). Not fixed.

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

Once the writers above bump the generation: keep, per DOM, the `value_generation()` last read (CPU:
`WindowGpuValues::of_doms` and the previous-values copy; GPU: `synchronize_gpu_values`). A frame whose every DOM
cache has the generation it had at the previous frame AND whose display lists are the same `Arc`s has the same bound
values: skip the copy, the comparison (`window_value_damage` has no value damage) and the transaction. A DOM cache
that was replaced (remount) must be compared by `(DomId, generation)` plus the cache's presence. In debug builds
assert the skipped path against the full comparison until the verification pass has run the e2e scenarios.

## For area A10 (and the coordinator): a11y gate (A3-core-10)

Proposed `A11yManager::consumer_attached` (default `true`): `LayoutWindow::update_a11y_tree` returns at once while
it is `false`; each shell sets it `false` after creating its adapter and `true` in the adapter's activation /
initial-tree request (then `a11y_manager.resend_full_tree()` and one `update_a11y_tree()`), `false` again on
deactivation. iOS / Android (`build_a11y_snapshot`) are pull-based already. The e2e runner's a11y commands and the
debug server must set it `true` (or call `update_a11y_tree` themselves before reading).

## For area A3-window

- `layout/src/window.rs:13065`: a tag lookup by scanning `tag_ids_to_node_ids` (same pattern as A3-core-01); one
  lookup per call there, fine unless it sits in a per-node loop.
- `StyledDom::append_child` in `dll/.../common/layout.rs:2187-2188` is followed by
  `recompute_inheritance_and_compact_cache` (layout.rs:1079): a second whole-DOM compact build per window DOM
  regeneration (the cascade built one already). Not the view's path.
- A per-DOM "inputs changed" generation would let the a11y pass skip unchanged DOMs (A3-core-10 fix 2).

## Questions for the coordinator / the user

1. a11y: may the pass be gated on an attached assistive technology (A3-core-10)? It needs one line per shell and a
   test with VoiceOver / Orca that nobody can run blind.
2. A3-core-12 (compact pixel slots): widening them changes every fractional layout number by up to 0.05 px (towards
   the exact value). Do it in the verification pass, where the pixel-identity guards can be run?
3. `Dom::css` as `(shared sheet, scope)` (A3-core-09 rest): an api.json change of the public `Dom`; wanted?

## What was not read

- `core/src/dom.rs` outside the builder / flatten paths (node types, attributes, a11y info, menus), `xml*.rs`,
  `icon.rs` (`resolve_icons_in_dom_with_context` runs per styling: not read), `custom_property_cascade.rs`
  (only reached with `var()`), `ua_css.rs` (the table `get_ua_default` reads per node x UA type: not read),
  `compact.rs` beyond the builder loop, `diff.rs` (A3-window), `events.rs` / `hit_test.rs` (A6), `transform.rs`
  (A2-gpu), `resources.rs`, `gl*.rs`, `svg.rs`, widgets-side files.
- `css/src`: only `css.rs` (parse entry points, memo) and the i16 codec of `compact_cache.rs`; not the parser
  (`parser2.rs`), the property parsers (`props/**`), `dynamic_selector.rs`, `system.rs`, `rice.rs`.
- `layout/src/solver3/display_list.rs`: read the generator's walk (3869-5040, 5590-5900, 5966-6130, 6212-6330,
  6474-7430, 7575-8470, 9245-9690, 11040-11605, 11880-12220); NOT read: pagination / slicer (12300-14760), tables
  (8780-9210), scrollbars (9953-10515), inline box fragments and ellipsis (10515-10980), placeholder prompt, SVG
  masks (14956-15600), patching (`PatchState`, `compute_patch_move_summary`, 5117-5590), `FrameIndex` (A1), the
  item methods (`is_visually_equal`, `visual_bounds`: 2773-3480).
- `layout/src/managers/a11y.rs`: read 1-320, 760-1900; not `build_content` / `build_node` / role maps / tests.
