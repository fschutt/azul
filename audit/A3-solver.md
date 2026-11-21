# A3-solver: quadratics and waste in `layout/src/solver3/*` (without `display_list.rs`) and the node graph widget

Branch `audit/A3-solver`, base `audit/base` (`f48bcc7b4`). Blind: nothing here was compiled or run. Every ms figure
below is an estimate from reading (cost model: comparisons x a guessed ns), never a measurement. Paths are relative
to `layout/src/` unless they start with `core/`, `css/`, `dll/`. Line numbers are those of the base commit.

How it was read: `solver3/mod.rs` (`layout_document`), `positioning.rs`, the layout half of `cache.rs`
(1160-2130, 2330-3520, 3517-5740), `fc/bfc.rs` 140-1465, `fc/dispatch.rs`, `fc/block_rules.rs` 466-568,
`fc/list_markers.rs` 76-245, `fc/ifc.rs` 340-770 by me in full; `getters.rs`, `layout_tree.rs`, `taffy_bridge.rs` +
`fc/flex_grid.rs`, `sizing.rs` + `fc/ifc.rs` + `fc/inline_content.rs` + `fc/text_constraints.rs` +
`fc/atomic_inline.rs` + `fc/white_space.rs`, `widgets/node_graph.rs` by five read-only helper agents (all non-test
code, in full); every finding of theirs that became a commit was read again by me at the cited lines.

## Claims

Functions this area rewrote or restructured (all in files it owns): `cache::process_out_of_flow_children` (head),
`cache::try_reuse_anon_wrapper`, `cache::reconcile_node` (fingerprint block), `fc::is_first_in_flow_child`,
`getters::get_scrollbar_style` (head, up to the fast path), `getters::memoised_font_size`,
`taffy_bridge::TaffyBridge::{new, get_layout_children, child_count, get_child_id}`,
`fc::collect_and_measure_inline_content_impl` (the child lookup), `sizing::calculate_block_intrinsic_sizes` /
`calculate_flex_intrinsic_sizes` (the child lookup), `layout_tree::LayoutTreeBuilder::create_node_from_dom`.
Nothing outside owned files was touched.

## The three answers

**1. Is a flat list of 12k absolutely positioned siblings under one parent linear in the solver now?**
After this branch: yes by reading, for both shapes, with the one open item below.
- Labels as SPANS (what the editor emits: `Dom::create_span_with_text` + class with `position: absolute`): the
  parent holds inline content only, so it is itself an IFC root (`layout_tree::has_only_inline_children`). The scan
  that was left: A3-solver-07 (`inline_content.rs:636`, n^2/2 = 72 M steps at 12k, per collection of the layer).
  Fixed. Z10's check is unchanged (one look per out-of-flow label).
- Labels as DIVS: the parent is a block container of blocks. Left were A3-solver-01 (`cache.rs:4602`, 72 M index
  compares per layout of the parent) and A3-solver-09 (`sizing.rs:1147`, 72 M pair compares per intrinsic pass).
  Fixed.
- MIXED (span labels with abs DIVS between them - the editor's selection boxes `boxes.push(Dom::create_div()..)`
  are such divs): each run of inline-display abs children between two block-display abs children gets its own
  anonymous inline wrapper (`layout_tree::in_flow_block_level_mask` + `cache::reconcile_recursive` mixed branch).
  Per relayout each run looked for "its" old wrapper from the old parent's first child: A3-solver-02, fixed.
- Guards: `audit_tests::a3_solver::a_flat_layer_of_absolute_labels_takes_position_lookups_in_proportion_to_its_labels`
  (spans and divs, with transforms, position lookups at 400 / 1,600), plus one counter test per fixed scan (01, 02,
  03, 07, 09) and Z10's `in_flow_probe_tests`.
- Still superlinear in principle, not hit by a flat label layer: A3-solver-08 (in-flow inline boxes of one IFC),
  -15 (blocks WITH in-flow children: a scratch position vector per block sized by the block's layout index), -16
  (an IFC that has both text and many abs children), -27 (nested shifted `position: relative`).
- Linear but with a large constant per label, per pass: A3-solver-14 (every abs box's interior is laid out twice
  on a cold pass and once more on EVERY later pass that is not a clean early exit), -19, -20, -21.
- So the groups of 64 can go as far as the solver is concerned. Not mine to answer: `window.rs`, the diff, the
  display list builder.

**2. Why does a 4k-node node graph cost 170 ms to lay out?** By reading, in this order:
- A3-solver-05 (fixed): every flex container laid out from outside taffy gets its own `TaffyBridge`, and each
  bridge allocated two `Vec<OnceCell<taffy::Style>>` with a cell per DOM node and per layout node of the whole
  document: about (4,006 + 4,1xx) x several hundred bytes = 4 MB allocated, initialised and dropped per bridge.
  A graph node holds several flex islands (the port wrappers `.node_input_wrapper` / `.node_output_wrapper` are
  `position: absolute; display: flex`, each field widget brings its own), and the abs ones get a second bridge from
  `positioning::position_out_of_flow_elements` (A3-solver-14). Hundreds of bridges x 4 MB. O(islands x nodes).
- A3-solver-17 (open): taffy measurement multiplicity on a cold pass: about 4 formatting-context layouts per leaf
  and 5 algorithm runs per container; width-only answers of a row are thrown away while the row's primary cache is
  empty (3^(d-1) runs for rows nested d deep).
- Per-node constants repeated per bridge / pass: style conversion (-18), child lists (-06, fixed), scrollbar base
  style (-04, fixed), text measured twice in the intrinsic pass (-19), `StyleProperties` builds (-20).
- Half of the 4,006 DOM nodes are CLOSED colour pickers: `widgets/color_input.rs:353 build` creates the whole
  142-node picker panel (96 of them checkerboard cells) per colour field, `display: none` for layout but built,
  cascaded, reconciled (A3-solver-34, open: opening the popup does not rebuild the DOM, so a lazy panel is a
  behaviour change).
- A pan does NOT relayout per mouse move: each visible node gets `set_css_property(transform)`, which is the
  GPU-value path. The relayout is one `Update::RefreshDom` at mouse-up, unconditionally
  (`widgets/node_graph.rs:3244`), because the offset is baked into every node's transform and culling may have
  changed. What makes a pan not relayout: return `RefreshDom` at release only when a node without DOM entered the
  cull box (keep the `ViewWindow` and the offset the DOM was built for in `NodeGraphLocalDataset`); better, move
  the offset to one transform on the two containers so that a rebuild after a pan yields identical node subtrees
  (A3-solver-35, open, with the exact functions). The cull box is 4 window areas (the editor passes the window
  size, the widget adds 50 % on every side): For area A7-editor.

**3. What does a settle render (12.5k nodes replaced by nodes of the same shape) recompute that it could keep?**
- A label whose `left` / `top` / `transform` changed has another `inline_css_hash`, so
  `core/src/diff.rs:2803` says `INLINE_STYLE_LAYOUT`, `cache::reconcile_node` builds the box fresh, and every
  per-node cache is empty for it: intrinsic sizes, the collected inline content, the line layout, the taffy cache.
  That is every label after a zoom or a pan. The text, the font and the width did not change; only real shaping is
  served from text3's content-keyed caches (and above 4,096 inline formatting contexts not even that: For A8).
  Keeping it needs the reconcile to treat an inset- or transform-only change of an absolutely positioned box as
  "move the box" (For area A3-window / A3-core, item 40).
- Per fresh label and pass, by reading: 2 intrinsic measures of the same text (-19), 1 full `layout_ifc`, 1
  re-entry of `layout_ifc` from the positioning pass with a dense->sparse expansion of the stored text (-14, -21),
  2-4 `StyleProperties` builds (-20), the fingerprint twice (-10, fixed), a scrollbar base style (-04, fixed), font
  size walks when the pointer is over the map (-12, fixed).
- Kept already: the layout tree of clean nodes (Z5), the structural display list cache for an identical DOM.

## Findings

Status vocabulary of RULES.md section 8: `fixed <hash>` | `open: <what is missing>` | `blocked (a): <what>` |
`blocked (b): <patch file>` | `noted (other platform)` | `checked: fine`. Severity: high | medium | low (ms x
frequency for the editor's loads). One row per finding id; the text blocks below give the evidence.

| id | class | where | severity | status | test |
|---|---|---|---|---|---|
| A3-solver-01 | quadratic | `cache.rs` `process_out_of_flow_children` | high | fixed 31304760a | `a_block_of_absolute_blocks_finds_each_of_them_among_its_children_by_bisection` |
| A3-solver-02 | quadratic | `cache.rs` `try_reuse_anon_wrapper` | medium | fixed f3362d04e | `a_block_of_many_inline_runs_lists_the_wrappers_of_its_old_box_once` |
| A3-solver-03 | quadratic | `fc/block_rules.rs` `is_first_in_flow_child` | low | fixed 8bdbb5237 | `blocks_behind_out_of_flow_siblings_look_back_to_the_nearest_in_flow_one` |
| A3-solver-04 | waste | `getters.rs` `get_scrollbar_style` | medium | fixed 79fc2d177 | `the_scrollbar_style_every_node_starts_from_is_made_once_per_context` |
| A3-solver-05 | quadratic | `taffy_bridge.rs` `TaffyBridge::new` | high | fixed 353d346a0 | `a_taffy_bridge_keeps_a_pointer_per_node_not_a_style` |
| A3-solver-06 | waste | `taffy_bridge.rs` `child_count` / `get_child_id` | low | fixed 6879e26e9 | `taffy_counts_and_indexes_a_containers_children_without_listing_them` |
| A3-solver-07 | quadratic | `fc/inline_content.rs` (element children of an IFC) | high | fixed 7167314df | `an_inline_root_of_absolute_spans_finds_each_of_them_among_its_children_by_bisection` |
| A3-solver-08 | quadratic | `fc/inline_content.rs` `collect_inline_span_recursive` | low | fixed 37d850480 | `ifc.rs` tests of the helper (see the commit) |
| A3-solver-09 | quadratic | `sizing.rs` intrinsic folds | high | fixed fcc148499 | `a_block_folds_its_childrens_intrinsic_sizes_in_one_walk` |
| A3-solver-10 | waste | `layout_tree.rs` + `cache.rs` fingerprint twice | medium | fixed e64bdae3a | `the_reconcile_hands_the_builder_the_fingerprint_it_computed` |
| A3-solver-11 | waste | `layout_tree.rs` `LayoutTreeBuilder::new` | low | fixed e64bdae3a | `reserve_nodes_makes_room_without_adding_a_node` |
| A3-solver-12 | waste | `getters.rs` `memoised_font_size` | medium | fixed 31f19e937 | `the_font_sizes_of_a_focused_page_are_answered_by_the_memo` |
| A3-solver-13 | guard | `positioning.rs` `get_position_type` counter | low | fixed 6cc8bf1ac | `a_flat_layer_of_absolute_labels_takes_position_lookups_in_proportion_to_its_labels` |
| A3-solver-14 | waste | `positioning.rs` `position_out_of_flow_elements`, `cache.rs` `calculate_layout_for_subtree_fragment` | high | fixed 93cf4a98a | `a_pass_for_a_paragraph_lays_out_no_interior_of_the_labels_beside_it` and three more |
| A3-solver-14b | waste | `positioning.rs` the solve of every absolute box in every pass | medium | fixed f8e1e7c73 | `a_pass_for_a_paragraph_solves_none_of_the_labels_beside_it`, `labels_that_moved_are_solved_again` |
| A3-solver-15 | quadratic | `fc/bfc.rs` Pass 1 position vector | low | fixed 4eabd302a | `the_first_pass_of_a_block_borrows_its_position_vector_from_the_pass` |
| A3-solver-15b | quadratic | `fc/table_rows.rs`, `fc/table_columns.rs`, `fc/table.rs`, `fc/bfc.rs` (fragmented child) temp position vectors | low | fixed 3f4e06488 | `the_cells_of_a_table_borrow_their_position_vectors_from_the_pass` |
| A3-solver-16 | quadratic | `fc/ifc.rs` `static_positions_of` | low | fixed 640fefd1b | `the_static_places_of_many_boxes_in_one_paragraph_are_read_off_one_listing_of_its_items` |
| A3-solver-16b | quadratic | `fc/ifc.rs` `inline_static_place_in_tree` (footnotes, columns) | low | fixed bd29ecca4 | `a3_solver::ifc::the_anchor_lines_of_many_boxes_of_a_paragraph_list_its_items_once` |
| A3-solver-17 | waste | `taffy_bridge.rs` width-only answers | high | fixed b27f65a88 | `a_nested_flex_row_answers_each_width_question_of_a_first_layout_once` |
| A3-solver-17b | waste | `taffy_bridge.rs` measure spill | medium | fixed 9d4637c90 | `the_measurement_a_second_store_evicts_is_found_in_the_second_way` |
| A3-solver-17c | waste | `taffy_bridge.rs` `set_final_layout_current(node, false)` on every measure | medium | blocked (a): a measure in the taffy bridge writes the tree's final state (sizes, offsets, inline layout, per-node caches); keeping the final layout current across a computed measure needs side-effect-free measures; design below | - |
| A3-solver-18 | quadratic | `taffy_bridge.rs` style conversion per bridge | medium | fixed 134b2fb7f | `the_cells_of_the_taffy_style_memo_grow_with_the_document_not_with_its_bridges` |
| A3-solver-19 | waste | `sizing.rs` a text measured for itself and for the box around it | medium | fixed dfe923716 | `sizing.rs` tests of the helper (see the commit) |
| A3-solver-19b | waste | `mod.rs` intrinsic pass per turn of the layout loop | medium | fixed a74109c49 | `a_later_turn_of_the_layout_loop_measures_no_label_whose_inputs_stayed` |
| A3-solver-20 | waste | `mod.rs` two style memos per pass | medium | fixed 75df5be39 | `the_layout_takes_over_the_styles_the_reconcile_resolved` |
| A3-solver-20b | waste | `getters.rs` one style build per label | medium | fixed f56165e9d | `equal_labels_take_their_style_by_its_inputs_instead_of_building_it` |
| A3-solver-21 | waste | `fc/ifc.rs` reuse exit expands the layout | medium | fixed 7eac1cf10 | `ifc.rs` tests of the helper (see the commit) |
| A3-solver-22 | waste | `layout_tree.rs` paint runs of measures | medium | fixed 318d1a97d | `a_measure_of_a_table_cell_builds_no_paint_runs` |
| A3-solver-23 | waste | `fc/white_space.rs` plain text | low | fixed 16395bc3e | `white_space.rs` tests of the helper (see the commit) |
| A3-solver-24 | waste | `layout_tree.rs` / `cache.rs` a kept node cloned twice | medium | fixed 7b39826e2 | `ifc.rs` count test (with 1ee264920: revert both) |
| A3-solver-25 | waste | `layout_tree.rs` `build` makes the DOM-to-box map a second time | low | fixed 99d7efff1 | `layout_tree::audit_a3_solver_tests::build_hands_the_dom_map_over_as_it_is` |
| A3-solver-25b | waste | `layout_tree.rs` `dom_to_layout: BTreeMap<NodeId, Vec<..>>`: one heap list per DOM node | low | blocked (a): the public type of `LayoutTree::dom_to_layout` (215 uses in 74 files of every area) becomes a dense table; design below | - |
| A3-solver-26 | waste | `sizing.rs` dirty closure as a hash set | low | fixed 4909c9d07 | `sizing.rs` tests of the helper (see the commit) |
| A3-solver-27 | quadratic | `positioning.rs` nested shifted relative boxes | low | fixed 8abfe10f9 | `nested_shifted_boxes_are_moved_by_one_walk`, `nested_shifts_are_added_one_after_the_other_outermost_first` |
| A3-solver-28 | n x depth | several (list below) | low | checked: fine | - |
| A3-solver-29 | wrong-output | `cache.rs` `reposition_block_flow_siblings` | medium | fixed b66f75aca | `a_clean_sibling_keeps_its_place_in_a_bordered_box_when_its_neighbour_is_laid_out_again` and two more |
| A3-solver-30 | waste | `cache.rs` `lift_to_slot_container` | medium | fixed 949321484 | `a_restyled_label_is_laid_out_without_its_layer` |
| A3-solver-31 | waste | `fc/bfc.rs` `centers_blocks_the_legacy_way` | low | checked: fine | - |
| A3-solver-32 | wrong-output | `sizing.rs` out-of-flow child in its parent's intrinsic sizes | medium | fixed 1e8fd3a32 | `[reveals]` d9e4f6da1 + two more in `a3_solver/sizing.rs` |
| A3-solver-33 | wrong-output | `getters.rs` font-stack memo by hash alone | low | fixed dc3e29f49 | `getters::audit_a3_solver_tests` |
| A3-solver-34 | waste | `widgets/color_input.rs` the panel of a closed picker | medium | blocked (a): the popup is opened by the engine without a rebuild (`set_transient_window_open`), so only the engine can ask for the panel when it opens: a content callback on `TransientWindowConfig` (core, shell, reconcile); decided again after the merge, design and reasons below | - |
| A3-solver-34a | waste | `widgets/color_input.rs` checkerboards | medium | fixed 0b94f3fdd | `a_checkerboard_is_one_node_where_it_was_a_div_per_cell` |
| A3-solver-34b | wrong-output | `widgets/color_input.rs` swatch click | medium | fixed a250251a5 | `clicking_toggles_the_picker_through_the_engine_and_never_touches_the_color` |
| A3-solver-35a | waste | `widgets/node_graph.rs` rebuild on every pan release | high | fixed 350ce0f23 | `the_end_of_a_pan_rebuilds_a_culled_graph_once` |
| A3-solver-35b | waste | `widgets/node_graph.rs` pan as one transform per container | medium | blocked (a): a reference frame nested in another still counts at its untransformed place (`GroupBounds::step` at `audit/A1`: no arm for `PushReferenceFrame`), so the containers' surfaces would not hold their nodes; decided again after the merge, design below | - |
| A3-solver-35c | waste | `widgets/node_graph.rs` drag writes width / height | high | fixed 429f0d444 | `a_step_of_a_node_drag_writes_no_layout_property_on_a_connection` |
| A3-solver-35d | wrong-output | `widgets/node_graph.rs` pan loop that does not advance | medium | fixed 516fce10f | `a_pan_passes_over_the_boxes_it_cannot_place_and_writes_the_rest` |
| A3-solver-35e | waste | `widgets/node_graph.rs` graph cloned per render | medium | fixed 2607ce45e | `a_render_moves_the_graph_into_its_dataset_and_copies_none_of_it` |
| A3-solver-35f | quadratic | `widgets/node_graph.rs` searches per connection, node, port | medium | fixed c721f24a3 | `the_wires_of_a_render_find_their_nodes_by_bisection` |
| A3-solver-35g | wrong-output | `widgets/node_graph.rs` no keys | medium | fixed 46667ed3c | `a_node_and_a_wire_keep_their_key_from_render_to_render` |
| A3-solver-35h | waste | `widgets/node_graph.rs` per-node property lists | low | fixed fe18a2172 | `the_box_of_a_node_declares_its_constants_from_one_slice_and_boxes_no_shadow` |
| A3-solver-41 | waste | `cache.rs` `reconcile_node` (inset-only moves; solver half of A3-window-08) | high | fixed 36b4c0248 | `labels_that_only_moved_and_turned_keep_their_boxes_and_stand_as_in_a_fresh_layout` |
| A3-solver-42 | waste | `mod.rs` / `fc/ifc.rs` / `sizing.rs` fonts asked per IFC (A8-13) | medium | fixed 58f61dce6 | `a_pass_asks_the_font_manager_for_its_fonts_once` (sizing half: f83fec4f7) |
| A3-solver-42b | waste | `regions.rs`, `display_list.rs` font asks | low | fixed 903e5fa66 | none of its own (needs a named-flow page); `a_pass_asks_the_font_manager_for_its_fonts_once` pins the function it now calls |
| A3-solver-43 | wrong-output | `positioning.rs` relative offset added again per incremental pass | medium | fixed 283a32482 | `a_relatively_positioned_box_beside_a_re_laid_one_is_not_shifted_again` and two more |
| A3-solver-44 | wrong-output | `positioning.rs` `adjust_sticky_positions` outside re-laid subtrees | medium | fixed e193ca69f | `a_sticky_box_beside_a_re_laid_one_keeps_its_static_place_for_later_scrolls` ([reveals] 9943af97e; with 88ad0aa7b) |
| A3-solver-45 | wrong-output | `positioning.rs` relative offset inside an absolute box | medium | fixed aeb67d400 | `a_relatively_positioned_box_inside_an_absolute_one_stands_at_its_offset` ([reveals] be2c9aff1) |
| A3-solver-46 | wrong-output | `positioning.rs` static position read back from the final one | low | fixed a3ddc3b26 | `a_box_at_its_static_position_in_an_rtl_block_does_not_drift_by_its_margin` ([reveals] 318ecf506) |
| A3-solver-47 | wrong-output | `sizing.rs` marker of an absolutely positioned list item | low | checked: fine | - |
| A3-solver-48 | wrong-output | `sizing.rs` inside markers not in intrinsic widths | low | fixed 352e773e8 | `a_shrink_to_fit_list_item_is_as_wide_as_its_inside_marker_and_its_text` |
| A3-solver-49 | wrong-output | `getters.rs` spacing fallback `Px(0)` vs `PxF(0.0)` | low | fixed 48f11a9ba | `getters::audit_a3_solver_tests::a_hovered_node_has_the_spacings_it_has_at_rest` |
| A3-solver-50 | waste | `sizing.rs` / `fc/ifc.rs` / `text3/cache/shaping_cache.rs` an IFC shaped twice per pass (A8-06 hand-over) | medium | fixed 011b0043c | `the_layout_of_a_label_takes_what_its_measure_shaped` |
| A3-solver-51 | wrong-output | `fc/table_rows.rs` overflow of a shifted cell | low | fixed b805defbd | `a3_solver::ifc::a_cell_moved_down_in_its_row_keeps_the_bounds_of_its_lines` |
| A3-solver-52 | wrong-output | `mod.rs` root loop / `cache.rs` `process_out_of_flow_children` (static position of a layout root's out-of-flow children) | medium | fixed d14e782a9 | `a_box_at_its_static_position_follows_the_flow_of_a_parent_laid_out_again` ([reveals] 5a31c9af8) |
| A3-solver-53 | wrong-output | `positioning.rs` a sticky box inside an absolutely positioned box | low | fixed 9019db74d | `a_sticky_box_inside_an_absolute_one_stands_at_its_shift_and_keeps_its_place` ([reveals] 6f54a959e) |
| A3-solver-54 | wrong-output | `positioning.rs` the boxes inside a sticky box do not move with its baked shift | medium | fixed 6f402e476 | `the_boxes_inside_a_sticky_box_go_with_it` and two more ([reveals] in the commit before) |
| A3-solver-55 | wrong-output | `widgets/node_graph.rs` the transforms a drag or pan writes outlive the gesture (`migrate_user_overrides_from`) | medium | open: the widget must clear its overrides when the gesture ends, and a clear costs a rebuild of the cascade tables of the whole DOM per box today; needs the fast path in `apply_node_css_change` written out under "For area A3-window (4)" | - |
| A6-13 | waste | `managers/scroll_registration.rs` `register_scroll_nodes`: the layout tree of a DOM copied per pass when its view raises a scrollbar (A6's row, built here) | medium | fixed 5ce5eae2f | `audit_tests::a3_solver::raised_bars::a_layout_pass_of_a_view_that_raises_a_bar_copies_no_layout_tree` and seven more |

### Fixed (details in the commit messages)

- **01** `children.contains(index)` for every recorded static position: k x k / 2. Sorted copy + binary search.
  Identical: membership in a set either way. Chosen over `nodes[index].parent == Some(node_index)` because that
  needs a tree invariant no test proves for every builder.
- **02** `.filter(is_wrapper).nth(ordinal)` per inline run -> the old parent's wrappers listed once per parent.
- **03** forward scan from the parent's first child -> binary search for the box, then backwards to the nearest
  in-flow sibling. Assumes a box is listed once among its parent's children.
- **04** node-independent part of the scrollbar style (context clone, UA rules, system scrollbar) memoised in a
  thread-local keyed by the VALUE of the context and of the four system scrollbar fields. Relies on
  `DynamicSelectorContext`'s `PartialEq` covering every field `CssPropertyWithConditions::matches` reads (it is
  the cascade's own "did anything change" test). Not covered by a test: two different contexts in a row.
- **05** `OnceCell<Style>` -> `OnceCell<Box<Style>>`. Still 8 bytes x document nodes per bridge (item 18).
- **06** `child_count` / `get_child_id` without building the filtered list.
- **07** stable-sorted `(dom node, layout child)` list per collection + `partition_point`.
- **09** cursor into `child_intrinsics` (built in child order), scan as fallback. The table's `stored` closure
  (`sizing.rs:1418`, asked in grid order) is unchanged: cells x children of the row, small.
- **10 / 11** `create_node_from_dom_fingerprinted`; `reserve_nodes(dom nodes x 9/8 + 16)`.
- **12** the memo answers when the asked state equals the node's stored state. The proof and its one assumption
  (memo == walk for normal nodes, which the code already relied on) are in the commit message.

### First round: what was open then (history - the table has each status)

- **08** `collect_inline_span_recursive` finds its own box with `parent_children.iter().find(dom == span)`
  (`inline_content.rs:1129`) and each element child with `span_layout_children.iter().find(..)` (`:1217`): a
  paragraph of s in-flow spans costs s^2 / 2. Fix: the caller in the main loop already has `child_index`; pass
  it (and a sorted index for `span_layout_children`). Signature change with 4 call sites; not done blind because
  the fallback `parent_children.to_vec()` for a span without a box must keep meaning the same list.
- **14** An absolutely positioned box's interior is laid out (a) by its parent's Phase 7
  (`cache::process_out_of_flow_children` -> `calculate_layout_for_subtree(PerformLayout)`, against the parent's
  content box) and (b) again, unconditionally, by `positioning::position_out_of_flow_elements`
  (`layout_formatting_context` against the solved box, `positioning.rs:829`), which loops over ALL nodes on every
  pass that is not the clean early exit - also when one unrelated node was dirty. For a label (b) is the reuse
  path of `layout_ifc` (item 21) plus `calculate_used_size_for_node`, `resolve_position_offsets` (3 font-size
  getters + 4 inset getters), a fresh `HashMap` and a subtree position walk; for an abs flex box it is a whole
  bridge. Fix direction: skip (b)'s interior run when the solved size equals the size (a) finalised and nothing
  in the subtree is dirty; or drop (a)'s layout and keep only its static position. Not provable blind: (a) also
  writes `scrollbar_info`, `overflow_content_size` and the per-node cache, (b) uses hand-built constraints
  (`text_align: Start`).
- **15** `layout_bfc` Pass 1 gives its children a fresh `temp_positions: PositionVec` (`fc/bfc.rs:252`), and
  `pos_set` grows a `Vec` indexed by GLOBAL layout index (`resize(idx + 1, UNSET)`): every block with an in-flow
  child that misses its cache fills a vector as long as its subtree's highest index. B blocks x n / 2 x 8 bytes: a
  page of 5,000 nested rows about 300 MB of fills. Absolutely positioned children are skipped before, so the label
  layer does not pay it. Same pattern: `fc/table_rows.rs:256`, `fc/table_columns.rs:394`, `fc/table.rs:909`. Fix
  direction: a scratch pool on `LayoutContext` reset per subtree, or positions keyed relative to the block's
  index. Needs a `LayoutContext` field (struct literal in 9 files of 3 areas).
- **16** `out_of_flow_static_positions` calls `inline_static_place` (a scan of all laid-out items plus a scan of
  the anchor's line) for every out-of-flow descendant: abs children x items. Zero for a layer of labels (no
  items). Fix: one sorted `(dom node, first item, last item)` table and per-line extents per call.
- **17** (taffy reader, not re-read by me) `width_only_answer_kept` returns `None` while `warm.taffy_cache` is
  empty and `cache_store` wipes `taffy_width_only` on the first primary store (`taffy_bridge.rs:2350`, `:3376`):
  directly nested flex rows re-run 2-3 times per level on a cold pass. The spill of a measure slot is created only
  at the second store (`:3388`), so the first key is recomputed. Proposed same-result fixes: a bridge-local
  `cold_width_only` map; allocate the spill at the first `ComputeSize` store. Spans to read once someone can run:
  `taffy_width_only_row` vs `_hit`, `taffy_cache_get_miss`, `taffy_final_layout_stale`.
- **18** After 05 a bridge still initialises (dom nodes + layout nodes) pointers and converts the styles of the
  nodes it reaches again (about 36 getters per node, `translate_style_to_taffy`). A memo owned by the pass (with
  `calc_storage`, whose boxes the calc pointers point into) removes both. Needs a `LayoutContext` field.
- **19** (sizing reader) The intrinsic pass measures a text layout node (`ancestor_is_stf` under an abs or flex
  box) and then its IFC root collects and measures the same text again; `calculate_intrinsic_sizes` runs inside
  the scrollbar / regrow loop of `layout_document` (`mod.rs:1382-1705`) with a dirty set that is never emptied.
  Fixes need proof that no reader uses a text node's own intrinsic sizes (`taffy_bridge.rs:2467/2785/3040`,
  `fc/bfc.rs:949/1108`), and that intrinsics are independent of the loop's changes (the comment at
  `mod.rs:1672` says so).
- **20** `StyleCache` is per `LayoutContext`, and `layout_document` makes two contexts (`ctx_temp` for the
  reconcile, `ctx` for the layout), so a text style built for `TextStyleKey::of` is built again; keys carry the
  node index, so 1,567 equal labels build 1,567 times (each: a family-list clone, a `Vec<FontSelector>` clone with
  a `String` per family, two unconditional cascade lookups, a SipHash of the result). Same-result steps: move
  `ctx_temp.style_cache` into `ctx`; a value-keyed front memo.
- **21** The reuse exit of `layout_ifc` calls `cached.materialized()` (`fc/ifc.rs:1143`): the dense text is
  expanded back into a `UnifiedLayout` only to read its extent, last baseline and object positions. Store those
  on `CachedInlineLayout`.
- **22** `CachedInlineLayout::new_with_constraints` builds glyph runs and the paint payload for min- and
  max-content MEASURES too (`fc/ifc.rs:1377` `should_store || is_measure`).
- **23** `white_space_runs`: 10-12 allocations for a 10-character text, 2-3 times per dirty pass. A fast path for
  text without breaks, bidi controls or collapsible runs is one `Arc::from(text)`.
- **24** (tree reader) A `LayoutNode` is about 1 KB and is copied 3-6 times per node per reconcile; a clean node
  is deep-cloned twice (`get_full_node` then `clone_node_from_old`), with its boxed inline layouts. Take the old
  node by value.
- **25** `dom_to_layout: BTreeMap<NodeId, Vec<..>>`: one heap `Vec` per DOM node, and `build` rebuilds the map.
- **26** `compute_dirty_ancestor_closure` returns a std `HashSet`, is built twice per pass and per loop
  iteration; a `Vec<bool>` would do (the return type is used by `mod.rs:1158`).
- **27** `adjust_relative_positions` shifts the whole subtree of every relatively positioned box with a non-zero
  offset; nested shifted boxes shift their descendants once per level. Summing the deltas first would change the
  f32 rounding order ((p + d1) + d2 vs p + (d1 + d2)), so it is not a same-result fix.
- **29** `reposition_block_flow_siblings` (run for the parent of every layout root below the tree root) re-stacks
  ALL children of the parent from `main_pen = 0` with their full margins: no margin collapsing, no escaped
  margins, `content_box_origin = parent_pos + padding` WITHOUT the border, and absolutely positioned children are
  stacked like blocks (healed afterwards by `position_out_of_flow_elements` only for boxes with insets; a box with
  `auto` insets takes the stacked place as its static position). By reading, a clean sibling of an edited
  paragraph in a parent with a border or with collapsing margins moves. I could not find what masks it; it needs
  a run (two `<p>` with margins in a bordered div, edit the first, compare the second's y).
- **30** `lift_to_slot_container` treats a box as inline-level when its OWN formatting context is `Inline` - which
  is also every block container that holds only inline content (the label layer, a paragraph). A dirty label
  therefore lifts its layout root through the layer to the first ancestor that is a block container of blocks, and
  the whole layer is laid out again (linear; every label's interior through item 14).
- **32** (sizing reader) an out-of-flow child's intrinsic is zeroed for its parent, but its margin, border and
  padding still enter the parent's min/max content and the margin chain (`sizing.rs:1156-1189`).
- **33** (getters reader) the font-stack memo is keyed by a 64-bit hash without an equality check.
- **34** see answer 2. Fix direction: build the picker panel when the popup opens (needs the open click to
  rebuild, or a transient child DOM); at least one background image instead of 96 checkerboard divs.
- **35** node graph, all in `widgets/node_graph.rs` (reader's lines): `RefreshDom` on every pan release (3244);
  three `set_css_property` per connection per mouse move while a node is dragged, two of them `width` / `height`
  = a relayout per move, for connection images that draw nothing (`draw_connection` returns a null image, 3095);
  `node_graph: self.clone()` per render (1130); `get_rect` does two linear node searches per connection (3194);
  no `with_key`, a fresh marker prefix per render (1263) and datasets hashed by pointer, so nothing is reused
  across a rebuild and pinned transforms can migrate to another node by sibling index; `continue` inside a `loop`
  without advancing (3457-3473): an infinite loop if a child's dataset is not a `NodeLocalDataset`. Not changed
  blind: 3,000 lines of widget tests pin the current DOM and callbacks.

### Checked: fine (do not check again)

- **28** O(n x depth) with a cheap step, linear at the depths of real pages: `cache::outermost_layout_roots` and
  `promote_layout_roots_to_containers` (one walk to the root per layout root; a cold pass makes every node a
  root, but each stops at its parent), `positioning::find_absolute_containing_block_rect` and the second walk for
  `cb_direction` (`positioning.rs:595`; one step when the parent is positioned),
  `cache::position_bfc_child_descendants` (each BFC level positions its whole subtree: one `pos_set` per node per
  block ancestor), `fc::centers_blocks_the_legacy_way` (31: up to 64 ancestors, only while `text-align: center`).
- `LayoutCacheMap::mark_dirty_all` (stops at nodes it cleared), `remapped` (n log n), `resize_to_tree`.
- `cache::alike_at_every_given_height` (memoised per node), `cache_key_block`, `prepare_layout_context`,
  `compute_scrollbar_info_core` (after 04), `check_scrollbar_change`.
- `cache::reconcile_recursive` / `reconcile_node` apart from 02, 10: per parent one `BTreeMap` of old children,
  two child `Vec`s, three block-level masks (n log n, allocations only for nodes with children).
- `cache::reposition_clean_subtrees` (each parent once), `compute_counters` (one pass; `list_item_numbers` is
  `core`'s), `css_dirty_reemit_set` (memoised walk), `mod.rs` patch-damage bookkeeping (O(n) vectors per pass).
- `positioning::adjust_sticky_positions`, `margin_note_place` (only boxes with all insets `auto`),
  `svg::text_scale` / `text_mapping` (node-type test first).
- `fc::markers_on_first_line`, `marker_line_host`, `is_marker_on_a_line`, `first_in_flow_child` (list items only).
- `fc::layout_bfc` / `bfc_prepare` / `bfc_place_children` up to line 1465: one pass over the children each;
  `static_positions` is a `BTreeMap` insert per abs child (n log n).
- `Probe::span` in my files: see "For area A11".
- Debug formatting: every `debug_*!` macro tests `debug_messages.is_some()` first; `log_child_positioning` gates
  first.
- From the readers (their "checked: fine" lists, not repeated here): the compact-cache getters of `getters.rs`,
  the `LayoutTree` accessors and builder appends (amortised O(1), no shifting), the taffy trait style getters
  (lent, not rebuilt), `calculate_intrinsic_recursive`, `calculate_used_size_for_node`.

### Recursion depth

`calculate_layout_for_subtree_fragment` -> `layout_formatting_context` -> `layout_bfc` -> (Pass 1) -> ... and
`reconcile_recursive`, `calculate_intrinsic_recursive`, `position_bfc_child_descendants`,
`shift_subtree_position`, `collect_inline_content_recursive` recurse per DOM level. The code already moves locals
out of the recursive frames (`bfc_prepare`, `reconcile_node` are `#[inline(never)]` for that reason). No deep
nesting sweep was added: a test of 1,000 nested blocks could overflow a debug test thread's stack and take the
whole test binary down in the verification pass.

## For area A3-core

- **36a** `get_css_property_pixel!` (`getters.rs:979-1035`) falls to the cascade for every inset, margin, padding
  or border width that is not px or is beyond +-3276.7 px (`css/src/compact_cache.rs:2542` i16 x 0.1 px): a map
  label's `left` / `top` beyond 3,276 px takes a cascade lookup on every read. The fast path also returns the value
  ROUNDED to 0.1 px while the slow path (any non-normal state, e.g. hover) returns it exactly: a hovered label can
  move by up to 0.05 px (sign / rounding class). Proposed: the lossless u32 encoding widths already use
  (`encode_pixel_value_u32`).
- **36b** No compact arm, so one cascade lookup per node per call: `get_aspect_ratio_property`
  (`sizing.rs:3217` in every used-size computation, `taffy_bridge.rs:861`), `get_zoom` in
  `getters::compute_all_zooms` (n lookups per new `StyledDom`), `get_text_transform` and
  `get_vertical_align_property` in every `StyleProperties` build, `get_flex_wrap_prop` (`sizing.rs:1351`).
  Proposed: a DOM-declared flag each (three bits are free in `dom_declared_flags`), as `DOM_HAS_FONT_VARIANT_NUMERIC`.
- **36c** `getters::inherited_color_override` walks to the root per call once any user override exists
  (`user_overridden_properties` is never shrunk); `propagated_text_decoration` walks to the root per IFC and text
  node. Proposed: counters / DOM flags.

## For area A3-window

- **40** `core/src/diff.rs:2803`: any change of `inline_css_hash` is `INLINE_STYLE_LAYOUT`, so a label whose
  `left` / `top` / `transform` moved is built fresh and loses every warm cache (answer 3). The tier-2 report
  (`diff.rs:2365`) already separates the changed properties; a transform-only change is paint, and an inset-only
  change of an absolutely positioned box moves the box without touching its interior.
- `#[derive(Clone)] LayoutTree` is a whole-tree deep copy; `Arc::make_mut` in `DomLayoutResult::layout_tree_mut`
  (`window.rs:1155/1171`) and `cache::unshare` (`mod.rs:839`, the skipped-reconcile path) pay it whenever the
  `Arc` is still shared. Not verified how often.
- `LayoutContext` needs two per-pass slots (items 15, 18): a scratch position pool and the taffy style memo. Its
  struct literal is in `mod.rs` (2), `paged_layout.rs` (5), `window.rs:26952`, `display_list.rs`, and tests.

## For area A8

- **38** `text3/cache/shaping_cache.rs:954`: once more than 4,096 entries exist, `begin_generation` runs on every
  `shape_content` call and keeps only what was touched since the previous call. The solver calls `shape_content`
  once in the intrinsic pass and once in `layout_ifc` per IFC, about n calls apart, so above 4,096 inline
  formatting contexts every text is really shaped twice per pass and nothing survives to the next pass (12k
  labels). Proposed: sweep only when `len > max(4096, 2 x len after the last sweep)`.
- **39** `FontManager::get_loaded_fonts` (`font_manager.rs:1080`): two mutex locks and a fingerprint fold over all
  loaded faces per call, called per intrinsic measure and per `layout_ifc`.
  `first_available_font_metrics` (`:1139`) builds a `FontChainKey` (a `Vec<String>` + a `String` per family) on
  every `layout_ifc` entry, reuse path included (`fc/text_constraints.rs:708`).

## For area A7-editor

- The node graph's cull box: `mapedit3/src/ui/mod.rs:27` passes the WINDOW size to `dom_in_view`
  (`ui/main/mod.rs:115`), and the widget adds 50 % on every side: DOM for about 26 nodes when 4-7 are visible.
  Pass the panel's box.
- `node_graph/mod.rs:343 translate_node_graph` translates all 270 nodes (about 2,000 fields, 4-5k `String`s) on
  every `layout()`, culled or not; a Layer node carries 10 translation fields (84 of its 116 DOM nodes).
- The selection boxes of marked labels are abs DIVS between abs SPANS: in a flat list each run of spans between
  two of them becomes an anonymous inline formatting context of its own (correct, and linear after 02, but the
  tree changes shape when a label is marked). A span for the box keeps one IFC.

## For area A11

- **37** `probe.rs:134`: with the `probe` feature (a default feature) `Probe::span` pushes the span name onto a
  thread-local `RefCell<Vec>` and pops it on drop even when recording is off; the doc at `probe.rs:528` says "one
  relaxed atomic load". The solver opens about 10 spans per `layout_ifc` and 3-6 per node elsewhere.

## Not read

- `paged_layout.rs`, `pagination.rs`, `page_breaks.rs`, `break_token.rs`, `footnotes.rs`, `regions.rs`,
  `multicol.rs`, `fc/columns.rs`: a window layout reaches them only through gates (`footnotes::announce`,
  `regions::lay_out_named_flows`, `multicol::dom_declares_columns` / `is_multicol_box`, the `fragmentainer`
  branches of `bfc_place_children`); the gates' own cost was not read.
- Tables: `fc/table*.rs`, `table_width.rs`. Floats: `fc/floats.rs`. `scrollbar.rs`, `scroll_chain.rs`,
  `calc.rs`, `counters.rs`, `geometry.rs`, most of `svg.rs`.
- `fc/bfc.rs` 1465-2666 (the second half of `bfc_place_children`: fragmentation, columns, the box's own height)
  was searched for scan patterns only. `cache.rs` 651-1160 (the cache struct), 2136-2330 (table children).
- `widgets/` other than `node_graph.rs` (reader) and the head of `color_input.rs`.
- All `#[cfg(test)]` modules. `text3/` (A8).

## Second round (after the coordinator's answers)

New commits since the first report, and what stays open with the reason. Helpers worked in the same worktree on
disjoint files (widgets; taffy; sizing + inline content; IFC + layout tree + white space); their commits carry the
finding ids below.

### Scaffolding

- `509295796` `LayoutContext::pass_memo: PassMemo` (`solver3/mod.rs`): ONE field for per-pass state, added to the
  twelve struct literals once. Slots are fields of `PassMemo` whose types live next to their users:
  `bfc: fc::BfcPassMemo` (item 15), `taffy: taffy_bridge::TaffyPassMemo` (item 18).
- `b4c8dc33c` test submodules `audit_tests/a3_solver/{ifc,sizing,taffy,widgets}.rs`.

### Fixed in the second round (mine)

- **29** fixed `b66f75aca`. Not a `[reveals]`: the defect is certain by reading (an existing unit test pinned the
  missing border in so many words). `reposition_block_flow_siblings` now shifts what follows a layout root by the
  root's growth (`sizes_before` from `layout_document`) and touches nothing else; a root of unknown former size
  keeps the old stacking for what follows it, from the real content box. Tests: three window tests (incremental
  pass == fresh layout: bordered box, padded box with collapsing margins, three siblings with a grown middle one)
  and three unit tests in `cache.rs`. Left as it was and NOT covered: a root whose first child's margin escapes
  through it changes the root's own place in its parent's flow, and the root is laid out in place (a limit of
  "laid out where it stood", not of the sibling pass).
- **15** fixed `4eabd302a`. `BfcPassMemo` lends the first pass's position vector and takes it back after unsetting
  the subtrees of the sized children (the only boxes `calculate_layout_for_subtree` writes positions for: the five
  `pos_set` sites of `cache.rs`). A test-only counter of entries left set is asserted 0 on a page mixing in-flow
  blocks, absolute boxes with and without insets, a flex row and an inline-block. Not changed: the table passes'
  vectors (`fc/table_rows.rs:256`, `fc/table_columns.rs:394`, `fc/table.rs:909`).
- **20** fixed `75df5be39` (the part that is exact): the layout's context takes over the reconcile context's
  `StyleCache`. Open rest: the memo is per pass, and equal labels build equal styles one by one (a value-keyed
  front memo needs the complete list of what `get_style_properties_for_state` reads as its key: about 25
  getters; a missed one serves a wrong font, and no test would show which).
- **33** fixed `dc3e29f49`: a font-stack memo entry carries its inputs and answers only for them.

### Second round: what was open then (history - the table has each status)

- **14** (every absolute box's interior is laid out by its parent's Phase 7 and again by the positioning pass).
  Not fixable as "the same result" by reading, in either direction:
  - The two runs are DIFFERENT layouts by construction, so no exact-key memo ever hits:
    (a) `process_out_of_flow_children` -> `calculate_layout_for_subtree`: containing block = the parent's content
    box, available height = that block's height or indefinite, `text_align` from the box's style;
    (b) `position_out_of_flow_elements` -> `layout_formatting_context`: containing block = the box's OWN solved
    inner box, available size definite on both axes, `text_align: Start`, writing mode horizontal.
  - Dropping (a) loses what only (a) writes: the static positions of the box's own absolutely positioned children
    (`layout_bfc`'s `output.static_positions` are ignored by (b)), `warm.scrollbar_info` and
    `overflow_content_size` of the box (read by the display list for scroll frames), the per-node cache entries,
    and the scrollbar reflow signal.
  - Skipping (b) for a clean box needs `b(b(S)) == b(S)` (laying the interior out again changes nothing), which
    does not hold in general: `bfc_prepare` reserves a scrollbar gutter from the PREVIOUS run's
    `scrollbar_info.needs_vertical`, i.e. a second run is how an `overflow: auto` box converges.
  - What reading does show: on a warm pass (a) is skipped for every child whose position is already in the cloned
    position vector (`pos_contains`), so only the COLD pass (and the settle render, where every label is fresh)
    runs both; and for a label the widths of (a) and (b) agree (`blockify_node_display` makes an absolute span a
    block, both shrink to the max-content width), so (b) takes `layout_ifc`'s reuse exit, which item 21 makes
    cheap. A run has to show: `overflow_content_size`, `scrollbar_info`, `inline_layout_result` and the children's
    relative positions of every absolute box with (a) skipped vs. not, on the reftests.
- **30** (`lift_to_slot_container` lifts a block that holds only inline content like an inline box). Changing
  the test to "sits in an inline formatting context" (`parent_fc == Inline`) is plausible but not provable: the
  own-context arm is what lifts a dirty absolutely positioned label to its layer today, and without it the label
  becomes its own root, whose size change makes `root_for_resized` climb the auto-height chain to the tree root
  (an out-of-flow box does not grow its ancestors, but that function does not know) - more layout, not less. It
  depends on 14: as long as (b) runs for every absolute box on every pass, the lift costs one collection of the
  layer (n log n after 07) on top.
- **27** (nested shifted `position: relative` boxes shift their descendants once per level): every linear
  variant adds the deltas in another order ((p + d1) + d2 vs p + (d1 + d2)), which is not bit-identical in f32;
  the number of additions per box IS the cost. Needs a decision on a tolerance, i.e. a run against the reftests.
- **34** (closed colour picker carries 142 nodes): STOP rule. 21 tests with about 72 assertions in
  `widgets/color_input.rs` take the panel from a freshly built closed input (list in the helper's report, kept
  below), `dll/tests/transient_window_layout.rs` clicks the swatch and expects the popup to measure and hold the
  plane and hue from the existing DOM, and two links are missing whatever the tests say: `build` cannot know
  the open state (the old `ColorPickerData` is merged in by the DOM diff AFTER the app's layout callback
  returned, `core/src/diff.rs:1723/1885`, and `ColorInput` has no open field in the public API), and opening
  must not rebuild (`clicking_toggles_the_picker_through_the_engine_and_never_touches_the_color` asserts
  `Update::DoNothing`). Tests that pin the closed panel: `dom_is_a_div_with_the_native_class_and_a_closed_popup_child`,
  `the_rendered_dom_flattens_with_exactly_one_transient_window`, six `on_plane_*` / `on_hue_*` tests, seven key
  tests through `picker_control(class)`, five `theme_tests`, `app_theme_tests::a_color_input_declares_its_structure_once_for_every_theme`.
- **35b** (the graph's offset as one transform on the two containers): not provable by reading. Every reference
  frame gets a raster surface sized by `reference_frame_extent`, which unions nested items at their untransformed
  bounds; the nodes are laid out at the container's origin and moved by their own transforms, so the container's
  surface would be one node big while its nodes paint thousands of px away. Whether nested frames are clipped to
  the parent surface is A1's code under edit. Plus: three hit-test / compositor implementations must agree on a
  scale about a node's centre under an outer translate, and one large layer may be slower than N small ones.

### Second round, part 2 (after the coordinator's second message and the usage limit)

Integration notes:
- **`pass_memo` and A3-window's branch.** A3-window's branch adds a SECOND `LayoutContext` struct literal in
  `window.rs` (its `node_patch` work, commit `25cf3821f`: `patch_restyled_decorations`, next to
  `regenerate_display_list_for_dom`), which this branch cannot see. The integration must add
  `pass_memo: Default::default(),` to that literal too (one line, as in the twelve here).
- **`bdecb2813`** is A3-window's `f57838478` cherry-picked (`core/src/diff.rs` only). Its tests in
  `core/src/diff_test.rs` conflicted with the other tests of that branch and were left out here; they arrive
  with A3-window's branch. The merge of the two branches sees the same `diff.rs` change twice.
- `PassMemo` is generic over the context's font type since `58f61dce6` (`PassMemo<T>`, hand-written `Default`
  and `Debug`): the literals still write `pass_memo: Default::default()`.

Fixed:
- **24** `1ee264920` + `7b39826e2` (two commits that belong together: a failed edit left the call site out of
  the first). A kept node is read without its child list and MOVED into the new tree
  (`get_full_node_without_children`, `adopt_node_from_old`). The builder half was the dead helper's uncommitted
  work, read in full and taken over.
- **41** `36b4c0248`: the solver's half of A3-window-08. An absolutely positioned / fixed box whose inset
  VALUES alone moved (`NodeChangeSet::INLINE_STYLE_INSETS`) is kept (clone path, `DirtyFlag::Paint`) and placed
  again by `position_out_of_flow_elements`; `paint_dirty` boxes are in the display-list patch's re-emit set.
  Count test: 200 labels moved and turned: 0 fresh boxes, at most 4 boxes sized anew, positions == fresh.
  **Not effective on the shell's path yet** (the test for it is `#[ignore]`d with the reason): see "For area
  A3-window (2)" below.
- **42** `58f61dce6` + `f83fec4f7` (A8-13): `LayoutContext::loaded_fonts` asks the font manager once per pass;
  used by `fc::layout_ifc`, `fc::flow_paragraph` and the intrinsic pass. Not switched: `regions.rs:1617` (named
  flows), `display_list.rs` (two asks per build, A3-core's file: `self.ctx.loaded_fonts()` is a drop-in).
  A8's 3-line patch in `load_fonts_from_disk` is on its branch and was not touched.
- **43** `283a32482` (found while doing 41): `adjust_relative_positions` added every relative box's offset to
  its position on EVERY pass, also to positions an incremental pass only cloned from the pass before (offset
  already in): a relative box outside the re-laid subtrees crept by its offset per pass. Now only the boxes
  below a laid-out root are shifted. Three window tests (incremental == fresh).

For area A3-window (2): **the css diff defeats `INLINE_STYLE_INSETS`.** `LayoutWindow::begin_reconciliation`
(`window.rs:17172-17260`) compares the computed styles of every paired node (`changed_styles`) and stages
`(node, worst scope)` as css dirt; `CssPropertyType::relayout_scope` answers `Full` for `top` / `right` /
`bottom` / `left` (the `_ =>` arm, `css/src/props/property.rs`). A node the css diff names with a scope above
`None` is built fresh by the reconcile whatever its fingerprint says (`ReconciliationResult::css_relayout`,
`cache.rs` `reconcile_node`: the solver cannot tell "only the insets moved" from a `(node, Full)` entry, and a
stylesheet-driven change of another property may hide behind the same entry). So on the shell's path a moved
label is still a fresh box. What is needed there: do not stage css dirt for a node whose changed properties
are ALL insets, that is absolutely positioned or fixed before and after, and whose `inline_inset_hash`
differs (then the reconcile's fingerprint reports it and 41 places it). An inset moved by a STYLESHEET (same
inline style) must keep its css dirt: the fingerprint does not see it. The same holds for a transform /
opacity value (`INLINE_STYLE_PAINT`): a css-dirt entry of scope `None` for it is harmless to the layout but
turns the display-list patch off (`css_dirty.is_empty()` in `structure_ok`). Test waiting for it:
`audit_tests::a3_solver::labels_that_only_moved_keep_their_boxes_when_the_shell_rebuilds_the_page`
(`#[ignore]`: remove the attribute with the fix).

Still open, found in this part (not fixed, reasons):
- **44** `adjust_sticky_positions` takes the cloned position of a sticky box outside the re-laid subtrees as
  its static position (the mistake of 43) and bakes a shift on top; skipping such a box is not enough, because
  the pass drops every `warm.sticky` constraint and re-adds only what the function returns. Needs the
  constraint of a skipped box to be kept; not done blind (the GPU sticky channel reads `baked`).
- **45** `position_out_of_flow_elements` rewrites the positions of everything inside an absolutely positioned
  box from `warm.relative_position` (`position_bfc_child_descendants`) AFTER the relative pass, which drops
  the offset of a relatively positioned box inside an absolute one. By reading; needs a run (a `relative`
  box with `top` inside an `absolute` box: is it drawn shifted today?).

### Second round: the helpers' items (each commit carries its own argument and tests)

Fixed:
- **35a** `350ce0f23` (`widgets/node_graph.rs`): the end of a pan returns `RefreshDom` only when the pan
  brought a node or connection WITHOUT DOM into the cull box (`CulledDom` record of what got DOM,
  `pan_needs_rebuild`, shared predicates `node_in_view` / `connection_in_view` / `drawn_connections` for
  builder and record). Count tests in `node_graph::audit_a3_solver_tests`; the existing
  `the_end_of_a_pan_rebuilds_a_culled_graph_once` was updated (it expected a rebuild for every culled pan).
  Behaviour that changes: a pending port click survives a pan that asks for no rebuild.
- **21** `7eac1cf10` (`fc/ifc.rs`, `layout_tree.rs`): the reuse exit of `layout_ifc` reads extent and last
  baseline from measures kept when the lines were stored; no dense -> sparse expansion per reused label.
- **19** `dfe923716` (`sizing.rs`): the box around ONE inline box takes that box's intrinsic measure
  (`last_ifc_measure`, exact conditions in the commit) - one measure per label instead of two. The text's own
  measure is NOT skipped: its stored sizes are read (the golden dumps print them, the folds and the taffy
  bridge read them).
- **26** `4909c9d07` (`sizing.rs`): a flag per node instead of a hash set for "is above a dirty node".
- **18** `134b2fb7f` (`taffy_bridge.rs`): the taffy styles and the calc storage live in `PassMemo::taffy`,
  converted once per pass. `build_taffy_style`'s per-node clone is gone: its cross-size suppression only ever
  set a size to `auto` that was `auto` (`should_suppress_cross_intrinsic` returns true only then; a
  `debug_assert` holds it), and the tree root's zeroed margin has a memo of its own.
- **17** `b27f65a88` (`taffy_bridge.rs`): a bridge serves the width-only answers it gave a flex row itself
  while the row's taffy cache was empty (`width_only_fresh`); everything older is still dropped. Nested rows:
  4 and 10 runs for 3 and 6 levels instead of 10 and 122.
- **23** `16395bc3e` (`fc/white_space.rs`): text the white-space processing leaves as it is becomes its run
  directly (`is_collapsed_plain_text`).
- **08** `37d850480` (`fc/inline_content.rs`): an inline box is handed its layout box and bisects for its
  element children (`BoxesByDom`).
- **32** `d9e4f6da1` `[reveals]`: test only
  (`an_absolutely_positioned_child_adds_nothing_to_its_parents_intrinsic_width`). By reading the float around
  a 100px block and an absolute child with `padding-left: 150px` comes out 150 wide; the fix (skip
  out-of-flow children in the block and flex folds) moves boxes and needs the goldens regenerated.

Open, with the reason:
- **19, the loop part** (`calculate_intrinsic_sizes` runs in every turn of `layout_document`'s scrollbar /
  regrow loop with the same dirty set): not provably identical to run it once. `fc::bfc_prepare` re-resolves
  the children's percentage margins and paddings against the block's content box during a turn
  (`tree.resolve_box_props`), and the intrinsic folds read `box_props`: the second turn measures dirty nodes
  with other values than the first. A run must show the goldens unchanged plus a page that reflows for a
  scrollbar with a percentage padding inside a shrink-to-fit box.

## Third round (after RULES.md section 8: nothing stays open that can be done)

### A3-solver-14, the interior of an absolutely positioned box (fixed 93cf4a98a)

- What the second-round note called not provable was decided the other way round: instead of proving that a
  second interior run changes nothing, the run is skipped only while a per-box entry stands
  (`NodeCache::abs_interior`), the entry is dropped by everything that can write into the box's subtree, and the
  remaining assumption (the positioning pass's run is a fixed point of itself) is CHECKED in every test of the
  crate: a kept interior is laid out anyway under `cfg!(test)` and compared bit for bit, and panics if it differs.
- Writers and how each is seen: a dirty node at or below the box clears the box's `NodeCache`
  (`mark_dirty_all` clears every ancestor); a visit of the box from its parent's flow goes through the new wrapper
  `calculate_layout_for_subtree_fragment`, which drops the entry when `LayoutCacheMap::tree_writes` moved during
  the visit (one count per computed layout, one per cache hit that changed a size, scroll geometry, escaped
  margin, baseline or a child's offset; the hit paths compare before they write); passes with paged layout,
  named flows or overlay content neither read nor write entries; boxes under a flex / grid / table parent and
  pseudo boxes are never tracked.
- The scrollbar-gutter objection of the second round: a box whose second run converges differently is one whose
  run is no fixed point; the check panics on it in tests, and such a box's `scrollbar_info` is written by the
  flow's visit (a counted write), not by the positioning pass.
- Switches: `AZ_NO_ABS_INTERIOR_REUSE` (any value) lays every interior out in every pass as before;
  `AZ_CHECK_ABS_INTERIOR_REUSE` (any value) makes a normal build lay kept interiors out and compare (panics on
  a difference). Default: reuse on, check off outside this crate's tests.
- Memory: `NodeCache` + 40 bytes (the commit message says 36: the `Option` tag pads it), `LayoutCacheMap` + 24.
- Still per absolute box per pass: the solve (A3-solver-14b) and one `position_bfc_child_descendants` walk.
- Corrected: the helper's observation that an absolutely positioned SPAN is laid out at the containing block's
  width by the flow and at its own width by the positioning pass is not so - `computed_style.display` is the
  blockified display (`layout_tree.rs` `blockify`, the two `node.computed_style.display = display_type` sites),
  so `prepare_layout_context` takes the `inner_size.width` arm for it. checked: fine.

### A3-solver-50, one shaping per inline formatting context per pass (fixed 011b0043c; A8's hand-over)

- `sizing::MeasuredParagraphs` in the pass's memo; the layout takes the measure's `(logical, shaped)` when its
  content compares equal. In A8's file one function was split (`measure_intrinsic_widths` =
  `shape_content` + new `intrinsic_widths_of_shaped`), nothing else; A8's branch does not touch that function
  (its hunks are the struct, the constructors, the sweep at the head of `shape_content` and a test module at the
  end of the file), so the merge is clean by lines.
- Depends on the two collectors producing equal content for a text label
  (`sizing::collect_inline_content_for_sizing`, `fc::collect_and_measure_inline_content`); the test says so if
  they do not.
- Bound: 65,536 shaped items kept at a time (`MEASURED_ITEMS_KEPT_AT_MOST`).

### The helpers' second assignments (each commit carries its argument, tests and mutations)

- 32 fix `1e8fd3a32` (next commit after the `[reveals]`): `adds_nothing_to_its_parent`; four golden dumps move in
  their `ic=` columns only (listed in the commit). 19b `a74109c49` relies on it: revert 19b before 32.
- 19b `a74109c49`: `BfcPassMemo::box_props_changed` (noted by `bfc_prepare` around `resolve_box_props`), later
  turns measure `sizing::dirty_nodes_to_measure_again`.
- 20b `f56165e9d` (+ `7c1f96414`, `9623b4f78`, `d06b35bd1`): `StyleCache::by_inputs`; every hit is built and
  compared under `cfg(any(test, debug_assertions))`. Not served: non-normal states, documents with viewport units
  or zoom or the font-variant flags (any document with a list marker), inline boxes with paint or edges.
- 22 `318d1a97d`, 16 `640fefd1b`, 17b `9d4637c90`: see the table.
- 35c-35h, 34a, 34b: see the table. 35d also changes an empty nodes container (connections still pan).
  35g: keys are DOM-wide - two graphs in one DOM with overlapping node ids share keys (needs a per-graph salt on
  `NodeGraph`, a public API field: for the widget's owner).

### New findings of this round (open rows of the table)

- **46** `positioning.rs`: the positioning pass reads a box's static position from `calculated_positions` and
  writes the final position into the same slot. A pass that does not re-derive the slot from the flow (the box is
  outside every re-laid subtree, or is a direct child of the layout root: `process_out_of_flow_children` skips a
  child that already has a position) solves from last pass's FINAL position. With all insets auto in LTR that is
  the same point; in RTL the solve is `x = static.x - margin_right`, so the box moves by its right margin on each
  such pass. Fix: re-derive the static position from the parent's content box and `warm.relative_position` when
  the flow did not place the box in this pass (as `position_bfc_child_descendants` does for boxes below a child).
- **47** `sizing.rs` (`adds_nothing_to_its_parent`, `process_layout_children`, `sole_inline_content_child`): the
  position is asked by the box's DOM node, and a `::marker` box carries its item's node, so the marker of an
  absolutely positioned list item counts as out of flow. Fix: `!is_marker_box(tree, idx) &&` in front, at all
  three sites.
- **48** `sizing.rs`: a marker's intrinsic sizes are 0/0 (its text is generated in the layout collection only),
  so a shrink-to-fit list item with `list-style-position: inside` is sized without its marker.
- **49** `getters.rs` `get_style_properties_for_state`: the cascade fallbacks for letter- and word-spacing end in
  `.unwrap_or_default()` = `Spacing::Px(0)`, the compact path gives `Spacing::PxF(0.0)`: a hovered node's style
  is unequal to its resting one with no rule involved.
- **51** `fc/table_rows.rs` (about line 1070): `overflow: cached_layout.layout.overflow.clone()` reads the
  retired sentinel's default overflow; a vertically shifted cell loses `unclipped_bounds` and `line_clamped`.
- **17c**, **14b**, **15b**, **16b**, **42b**: remainders of fixed items, see the table.

### A3-solver-34, the lazy picker panel: blocked (a), design

`build` cannot know `open`: `merge_picker_data` runs in the DOM diff, after the app's DOM is built, and opening
must not rebuild (`Update::DoNothing` is asserted). Done short of it: 34a (142 -> about 48 nodes closed), 34b.
1. core: `TransientWindowConfig::with_content(RefAny, fn(RefAny, LayoutCallbackInfo) -> Dom)`; the field goes
   into api.json.
2. shell: where a popup's DOM is taken from the transient subtree, call the content callback when it is set, and
   measure the popup by laying that Dom out before queuing it.
3. reconcile: `transfer_states` must also repoint the config's `RefAny` to the merged dataset.
4. widget: `ColorPickerData` gains `theme`; `build` makes the popup without children and
   `with_content(data.clone(), picker_content)`; `picker_content` calls `picker_panel` (through
   `follow_app_theme` when the theme is `None`). The panel's callbacks already work inside the popup's own DOM.
5. tests: the 21 tests that take the panel from a closed input (list in the second-round section) get a helper
   that builds the panel through `picker_content`; `dom_is_a_div_...` asserts a childless popup;
   `dll/tests/transient_window_layout.rs` keeps its intent.

### A3-solver-35b, the graph's offset on the two containers: blocked (a), design

Done short of it: 35a (no rebuild per pan release), 35c (a drag writes transforms only), 35d-35h.
1. engine first (A1's code): `reference_frame_extent` / `GroupBounds::step` must union a nested frame's extent
   moved by its matrix (today nested items count at their untransformed place). Pixel test: a child frame
   translated outside its parent frame's box is painted, on the CPU and the GPU path.
2. builder: both containers get `transform: translate(offset)` always (so the frame binding exists);
   `render_node` is called with offset (0, 0); a connection's transform is `translate(rect.origin)` plus scale.
3. pan: two `set_css_property` calls, one per container (`pan_writes(graph)` replaces `pan_node_writes`); a node
   drag writes `translate(position)` and `translate(rect.origin)` without the offset.
4. tests to change: the expected values in the 35c and 35d tests; the transform in the 35h test stays.
5. switch `AZ_NODEGRAPH_PAN=nodes` keeps per-node writes, and stays the default until step 1 is seen on screen.

## What to look at on the real window

- Absolutely positioned content after an unrelated change: map labels (text, rotation, selection boxes), popups,
  dropdowns, tooltips, the node graph's nodes while typing in a side panel. Run once with
  `AZ_CHECK_ABS_INTERIOR_REUSE=1` (a panic names the box) and compare with `AZ_NO_ABS_INTERIOR_REUSE=1`.
- The colour picker's alpha bar and preview, open, in light and dark mode (34a: one node per checkerboard;
  `AZ_COLOR_PICKER_CHECKER=cells` is the old build). A translucent colour's swatch opens its popup (34b).
- The node graph: drag a node (wires follow, 35c: their boxes keep the build-time size until the next rebuild,
  which matters once connections are drawn), pan across the culling edge (35a, 35d), undo a move after a pan
  (see "For area A3-window (3)").
- A hovered or focused label / inline box: the same font and colour as at rest (20b; debug builds assert it).

## For area A3-window (3) / A3-core

- wrong output (found by the widgets helper): `migrate_user_overrides_from` (`core/src/styled_dom.rs:3480`,
  called at `window.rs:17380`) carries every `set_css_property` override to the matched node, and an override
  outranks the rebuilt style. A node that was panned or dragged keeps that transform against an undo, a
  programmatic move or a zoom of the model. Keys (35g) make it the right node, not the right value. Proposal:
  carry an override of property P only when the new node's inline declarations of P equal the old node's; or
  batch all override writes of one callback into one `restyle_user_properties`.
- css owner: `From<CssPropertyWithConditionsVec> for Css` (`css/src/css.rs:351`) copies a const slice into an
  owned Vec, then allocates a rule block and a one-element declarations Vec per property. Proposal: one rule per
  run of equal conditions.

## For area A2-cpu

- `render_conic_gradient` uses agg's `GradientConic` (`|atan2|`, a mirrored half circle): a CSS conic gradient
  is drawn mirrored on the CPU renderer (found by the widgets helper while reading for 34a; not verified by me).

## For area A8 (2)

- `text3/cache/shaping_cache.rs`: `measure_intrinsic_widths` is split into its `shape_content` call and the new
  public `TextShapingCache::intrinsic_widths_of_shaped` (A3-solver-50, commit 011b0043c). No behaviour change.

## For the integration

- `LayoutContext` has the field `pass_memo` (`509295796`); every struct literal needs
  `pass_memo: Default::default(),` - A3-window's second literal in `window.rs` (its commit `25cf3821f`) too.
- `PassMemo` got `measured` (50); `BfcPassMemo` got `box_props_changed` (19b); `NodeCache` got `abs_interior`,
  `LayoutCacheMap` got `tree_writes` and `own_size_write` (14): all have defaults, no literal changes.
- `cache::calculate_layout_for_subtree_fragment` is now a wrapper; the former body is
  `calculate_layout_for_subtree_visit_counted`. A merge that brings another area's hunk into the old body lands
  in that function.

### Third round, part 2: the positioning passes on incremental layouts (44, 45, 46, 52, 30)

Each wrong output got a `[reveals]` commit (the test alone) and the fix as the next commit, so the verification
pass keeps or reverts the fix by what the test did before it.
- **45** `be2c9aff1` / `aeb67d400`: the loop body of `adjust_relative_positions_of` is now
  `shift_if_relatively_positioned`; the positioning pass calls it for the boxes below each absolute box it places
  (`shift_relative_descendants`). Changes FRESH layouts too (a relative box in an absolute one used to stand
  unshifted): reftests with that shape move by the box's offset - expected.
- **46** `318ecf506` / `a3ddc3b26`: `flow_static_position` in the `Rtl` all-auto arm only.
- **52** `5a31c9af8` / `d14e782a9`: the slots of a layout root's own out-of-flow children are emptied before the
  root is laid out (only when the root holds no layout entry; restored if the layout leaves one empty).
  Not covered: boxes taken out of an inline box of the root, children of a node laid out below a root that was
  served from its cache.
- **44** `9943af97e` / `e193ca69f` + `88ad0aa7b`: `adjust_sticky_positions_of(.., placed_now)`; the old tree's
  constraints are handed to the reconciled tree first (a reconciled tree had none: `LayoutNode::split`).
  Revert the two together.
- **30** `949321484`: narrow form - only an out-of-flow box whose css scope stayed below `Full`, under a block
  container. The general lift rule (a block holding only inline content lifts to its parent) is kept on purpose:
  it is what relays a table cell or a shrink-to-fit box around a restyled paragraph (cell -> row -> table goes
  through that one level), and the css-dirty channel lays a block out through its parent by design (scroll extent
  and auto height of the parent).
- New: **53** (sticky inside an absolute box), see the table.

### A3-solver-14b (fixed f8e1e7c73)

- `cache::AbsPlacement` inside the (now boxed) `AbsInterior`: a box is placed from its last solve when its
  containing block, the viewport, the containing block's direction, its box props and its static position (or
  its un-re-derived final position) are what they were. The box's own style is not compared: it relies on the
  dirty marks every per-node layout cache relies on, plus `layout_document` dropping the placement of every
  `paint_dirty` node (the inset-only moves of 41).
- Same switches and the same check as 14 (`AZ_NO_ABS_INTERIOR_REUSE`, `AZ_CHECK_ABS_INTERIOR_REUSE`, on in this
  crate's tests). Known ways the check can fire in the suite without the memo being the bug: an inset that is
  no multiple of 0.1 px on a box whose state changes between passes (A3-core 36a: the compact path rounds, the
  cascade path does not), a `rem` inset with a changed root font size if the reconcile does not mark the box.
  Both are inputs the dirty marks should carry; the panic names the box.
- The memory note of 14 is replaced: `NodeCache` + 8 bytes; about 110 bytes on the heap per absolutely
  positioned box with children.

### A3-solver-17c, the final layout after a computed measure: blocked (a), design

Every computation in the taffy bridge - a measure (`RunMode::ComputeSize`) or the final layout - writes the tree:
`compute_child_layout` stores the answered size as the node's `used_size`, `compute_non_flex_layout` re-flows
inline content and re-places children at the measure's constraints, and the per-node caches of the subtree are
stored at those constraints. So `NodeCache::final_layout_current` is cleared by every computation and a final
layout after a computed measure runs again (`taffy_bridge.rs` 2228, 2343, 2359, 3476). Done short of it: the
measures that are answered without computing (width-only answers 17, the spill 17b, pure-measure sizes, the
intrinsic-width answers) - each avoided measure is an avoided re-run of the final.
To keep the final current across a COMPUTED measure, a measure must leave the tree as it found it:
1. A journal on `TaffyBridge` (`measure_undo: Vec<(usize, SubtreeState)>`), opened by `compute_child_layout` when
   `run_mode == ComputeSize` and `final_layout_current(node)`, closed when that call returns.
2. `SubtreeState` = for every node of the subtree: `used_size`, the warm fields a layout writes
   (`relative_position`, `inline_layout_result`, `inline_measure_layouts`, `scrollbar_info`,
   `overflow_content_size`, `escaped_*_margin`, `baseline`, `measured_content_sizes`, `taffy_cache`) and its
   `NodeCache` (`layout_entry`, `measure_entries`, the taffy fields). All of them are `Clone`; the inline layouts
   are `Arc`s. Taken before the measure runs (a subtree walk), put back after it, then
   `set_final_layout_current(node, true)` again. The measure's ANSWER is kept in taffy's cache as now.
3. What must not be put back: the caches whose purpose is to remember the measure - `taffy_measure_spill`,
   `taffy_width_only`, `width_only_fresh`, `TaffyPassMemo::last_measures` - and `LayoutCacheMap::tree_writes`
   (the measure wrote; the restore is one more write for any `abs_interior` bookkeeping inside: bump it).
4. Cost model to decide by a run: the snapshot is O(subtree) per computed measure against one avoided final
   layout of the subtree per pass; only worth it where the final is the expensive one (text). Counters to add:
   finals re-run because a measure cleared the flag (`taffy_final_layout_stale` span exists) against measures
   computed.
5. Tests: the existing `flex_items_keep_the_size_their_container_gave_them` family (they pin what a measure must
   not leak into the final) plus a count test: a flex row of text items laid out twice with one probe in
   between runs each item's final once.

### Third round, part 3 (48, 49, 51, 42b; 47 closed; A8-13)

- **48** `352e773e8`: the intrinsic measure opens with the markers of `markers_on_first_line`, as the layout's
  collection does. Golden dumps: `ic=` of list items with `list-style-position: inside` grows by the marker.
- **49** `48f11a9ba`: `spacing_of_px`: zero spacing is `Spacing::default()` on both read paths.
- **51** `b805defbd`: one field (`cached_layout.overflow` for `cached_layout.layout.overflow`).
- **42b** `903e5fa66`: `regions.rs` reads `ctx.loaded_fonts()`.
- **47** checked: fine. The three predicates of `sizing.rs` that take a `::marker` box of an absolutely positioned
  list item for out of flow (`adds_nothing_to_its_parent`, `process_layout_children`,
  `sole_inline_content_child`) give the numbers they would give otherwise: a marker box holds no intrinsic size
  of its own (0 / 0; its content is its line's, 48) and no edges, a marker on a line is passed by the second arm
  of `adds_nothing_to_its_parent` anyway, and `sole_inline_content_child` answers `None` either way. If a marker
  box ever gets sizes or edges of its own, put `!fc::is_marker_box(tree, idx) &&` in front of the three tests.
- **A8-13** (the coordinator's second note): done as A3-solver-42 (`58f61dce6`, `f83fec4f7`, `903e5fa66`).
  `LayoutContext::loaded_fonts()` fills `pass_memo.loaded_fonts` at the FIRST ask of a context, so no refresh is
  needed after `FontManager::extend_chains_for_content`: its one caller (`window.rs`, the relayout after an edit)
  extends the chains before it builds the context that lays the paragraph out. The two asks left are
  `display_list.rs:9848` and `:10647` (`self.ctx.font_manager.get_loaded_fonts()`), A3-core's file: they can
  read `self.ctx.loaded_fonts()` at the merge.

## Hand-over (the context of this agent ran long; written for whoever continues)

### Open rows and where to start

See the table: 15b, 16b, 25, 27 (low, each with its starting point in the status cell) and 53 (low): a sticky box
inside an absolutely positioned box - `position_out_of_flow_elements` derives the positions below the box from
the flow after the sticky step ran; the fix is the shape of 45: after `shift_relative_descendants` call a
`shift_sticky_descendants` that runs the per-box body of `adjust_sticky_positions_of` (to be extracted like
`shift_if_relatively_positioned`) for the boxes below the placed box and RETURNS their constraints, which
`layout_document` must merge into the ones it stores (`warm.sticky`); the positioning pass has no channel for
that yet (it returns `()`): give it a `&mut Vec<(usize, StickyConstraint)>`.
Blocked (a), designs above: 34 (lazy picker panel), 35b (pan as one transform per container), 17c
(side-effect-free measures in the taffy bridge).

### What the merge into `audit/integration` must do

1. `core/src/diff.rs` conflicts: my branch's only change to that file is the cherry-pick `bdecb2813` of
   A3-window's `f57838478`. Take integration's file whole (`git checkout --theirs` in a merge of my branch INTO
   integration means: keep integration's side); nothing of mine is lost. `cache::reconcile_node` reads
   `NodeChangeSet::AFFECTS_LAYOUT` and `INLINE_STYLE_INSETS` from it: check both names still exist.
2. `LayoutContext` literals: every one needs `pass_memo: Default::default(),` - A3-window's second literal
   (`patch_restyled_decorations` in `window.rs`) and any literal another area added.
3. A3-core's `PxSlot` (box slots as `i32` thousandths): `getters::style_inputs_of` (20b, `f56165e9d`) tests
   `dims.padding_top / _right / _bottom / _left != 0` and `dims.margin_left / _right != 0` on the compact row:
   use the slot type's zero test. Its text slots (`line_height`, `letter_spacing`, `word_spacing`, `tab_size`
   as `i16`, `font_size` as `u32`) are tier-2b / cold fields: check they are unchanged. No other raw box-slot
   read was added on this branch (`git diff audit/base HEAD | grep _raw` is empty for sizing.rs and
   taffy_bridge.rs).
4. `DOM_HAS_*`: this branch adds no flag bit (it reads three existing ones in `style_inputs_of`).
5. `display_list.rs`: `DisplayListGenerator`'s two `get_loaded_fonts()` asks -> `self.ctx.loaded_fonts()`.
6. Un-ignore `labels_that_only_moved_keep_their_boxes_when_the_shell_rebuilds_the_page`
   (`audit_tests/a3_solver.rs`): A3-window's css-diff half is in integration (`072419824`).
7. A8's `0a88603d3` changed two resolvers in my files (four lines each): keep them; my `spacing_of_px` (49)
   and `style_inputs_of` (20b) are in other functions of `getters.rs`.
8. `text3/cache/shaping_cache.rs`: my split of `measure_intrinsic_widths` (50) is between A8's hunks; if git
   reports a conflict there, the resolution is A8's file with `measure_intrinsic_widths` ending in
   `Ok(Self::intrinsic_widths_of_shaped(shaped_items, constraints, loaded_fonts))` and the scan as the body of
   that new function.
9. Revert pairs, if the verification pass rejects one half: 24 (`1ee264920` + `7b39826e2`), 44 (`e193ca69f` +
   `88ad0aa7b`), 19b before 32 (`a74109c49`, then `1e8fd3a32`), 14b before 14 (`f8e1e7c73`, then `93cf4a98a`).
   Each `[reveals]` commit stays; its fix is the commit after it (45, 46, 52, 44, 32).
10. Expected to move in the golden dumps: `ic=` of four documents (32, listed in `1e8fd3a32`), `ic=` of list
    items with inside markers (48), positions of relatively positioned boxes inside absolutely positioned ones
    (45).


## After the merge (the successor's rounds; merge commit b38dee0e0)

The branch contains `audit/integration` (be978485e) since `b38dee0e0`; what the merge kept from each side and
what it adjusted is in that commit's message (diff.rs = integration's; `StyleInputs` for `PxSlot`; the second
`LayoutContext` literal; `DisplayListGenerator::loaded_fonts` reads the pass's memo; the shell-path test is not
ignored any more).

- **53** `6f54a959e` [reveals] / `9019db74d`: `shift_if_sticky` (the body of `adjust_sticky_positions_of`,
  moved) runs for the boxes below every absolute box the positioning pass places
  (`shift_sticky_descendants`, after `shift_relative_descendants`); the constraints come back through
  `position_out_of_flow_elements_with_sticky` and replace the earlier ones in `warm.sticky`. Asked for only in a
  document with a sticky box. (The mutation line of that commit says "keeps flow y 0" for `placed_now = false`;
  by the code it keeps -50: the test fails either way.)
- **54** (new, found while doing 53) [reveals] + `6f402e476`: the sticky step moved the box alone; the boxes
  below it take the distance it moved (the whole shift when the pass placed it, the change since the pass before
  when it did not). Golden values: boxes inside a sticky box with a non-zero baked shift move by it.
- **27** `8abfe10f9`: one walk per outermost shifted box carries the offsets down and adds them one after the
  other, outermost first - the same additions in the same order (bit for bit; a test pins `(p + d1) + d2`
  against `p + (d1 + d2)`). Per box one addition per shifted box around it stays; the visits are linear.
- **16b** `bd29ecca4`: `fc::TreeStaticPlaces` (expanded layout + `StaticPlaceTables` per context root, made at
  the first ask) for `footnotes::footnote_boxes` and `fc::columns::anchored_notes`.
- **15b** `3f4e06488`: the three table sites and the three fragmentainer sites of `fc/bfc.rs` lend from
  `PassMemo::bfc`; new test counter `solver3::POSITION_FILLS`.
- **25** `99d7efff1`: step 1. **25b** (step 2), blocked (a), design:
  1. `pub struct DomBoxes` in `layout_tree.rs`: `slots: Vec<Slot>` indexed by DOM node index, `enum Slot { None,
     One(LayoutNodeId), Many(u32 /* into `lists` */) }`, `lists: Vec<Vec<LayoutNodeId>>` (a node with several
     boxes: a `::marker`, a split preview, an inline box broken over lines), `len: usize` (nodes with a box).
  2. Its API is the one the 215 uses call today on the `BTreeMap`: `get(&NodeId) -> Option<&[LayoutNodeId]>`
     (covers the 90 `get(..).and_then(|v| v.first())` / `?.iter()` / `?.get(i)` sites unchanged),
     `contains_key`, `len`, `is_empty`, `keys()`, `values()` (slices), `iter()` and `IntoIterator for &DomBoxes`
     with `Item = (NodeId, &[LayoutNodeId])` (the six `for (&dom_id, indices) in &tree.dom_to_layout` loops lose
     the `&` in the pattern), `Index<&NodeId, Output = [LayoutNodeId]>`, `last_key_value`, `insert(NodeId,
     Vec<LayoutNodeId>)` (tests), `push(NodeId, LayoutNodeId)` for the builder's five `entry(..).or_default()
     .push(..)`, `PartialEq`, `Debug`, `Default`, `Clone`.
  3. Mechanical edits outside `layout_tree.rs`: 49 test literals `dom_to_layout: BTreeMap::new()` ->
     `Default::default()`; the sites that name `Vec` (`.map(Vec::as_slice)` x3, `v.capacity()` in the memory
     report, `assert_eq!(map[&k], vec![..])` x2 -> slices).
  4. Gain, by reading: per reconciling pass one allocation and one B-tree insert less per DOM node, and the frees
     of the tree before: about 5 ms per pass at 25,000 nodes.
  5. Test: allocation count of `LayoutTreeBuilder::build` for n and 4 n boxes (no allocation per DOM node).

## For area A1

- **wrong-output, medium: a CSS conic gradient is drawn over half a turn and mirrored on the CPU renderer.**
  `cpurender/raster.rs` `render_conic_gradient` (line 451 at `audit/A1`) fills with agg's `GradientConic`, whose
  parameter is `|atan2(y, x)| * d / pi` (`agg-rust-azul-1.1.4/src/span_gradient.rs:139`): 0 on the local +x
  axis, d at 180 degrees on EITHER side. The transform is right (rotation by `angle - 90deg`, then the centre:
  local +x is 12 o'clock for `from 0deg`, and the angle grows clockwise on screen), so the direction and the
  start are CSS's; the range is not: CSS runs the stops over the full turn, t = theta / 360.
  Hand-computed, `conic-gradient(red, blue)` on a 100 x 100 box, centre (50, 50):
  pixel (90, 50), 3 o'clock, theta = 90: CSS t = 0.25 -> rgb(191, 0, 64); drawn t = 0.5 -> rgb(128, 0, 128).
  pixel (50, 90), 6 o'clock, theta = 180: CSS t = 0.5 -> rgb(128, 0, 128); drawn t = 1.0 -> rgb(0, 0, 255).
  pixel (10, 50), 9 o'clock, theta = 270: CSS t = 0.75 -> rgb(64, 0, 191); drawn t = 0.5 -> rgb(128, 0, 128)
  (the mirror image of 3 o'clock).
  Proposed change, in `raster.rs` only (agg's trait `span_gradient::GradientFunction` is public and
  `pixmap::agg_fill_gradient_clipped` is generic over it):
  `struct ConicTurn; impl GradientFunction for ConicTurn { fn calculate(&self, x: i32, y: i32, d: i32) -> i32 {
  let mut a = f64::from(y).atan2(f64::from(x)); if a < 0.0 { a += core::f64::consts::TAU; }
  (a * f64::from(d) / core::f64::consts::TAU + 0.5) as i32 } }` and `ConicTurn` in place of `GradientConic` in
  the call. Test: the three pixels above (tolerance 2 per channel: the LUT has 256 steps).
  Replaces the unchecked note under "For area A2-cpu" above.

## Part 3 after the merge: the three `blocked (a)` rows, decided again

- **34 stays blocked (a).** Not a widget change, for one reason: the open state is the ENGINE's. A click calls
  `info.set_transient_window_open(popup, open)` and returns `Update::DoNothing`
  (`on_color_input_clicked`); the engine holds the popup open across rebuilds and takes its content from the
  transient node's subtree. `build` runs in the app's layout callback and cannot know that its picker is open:
  the flag lives in the old DOM's dataset, which the diff merges in after the callback returned
  (`merge_picker_data`), and `ColorInput` has no such field. So a panel that exists only while open has to be
  ASKED FOR by the engine when it opens the popup: the content callback of the design above
  (`TransientWindowConfig::with_content`: core + shell + `transfer_states`), three areas' files and an
  api.json addition. The widget-only variant - the click returns `RefreshDom` and `build` reads "open" from a
  registry keyed by the input's callback - was read and dropped: the popup would be opened on the panel-less
  subtree first, and whether an open transient window takes content and size again from a rebuilt subtree is
  shell code no test here covers; a wrong answer is a colour picker that opens empty. Tests that pin the
  panel in a freshly built closed input, counted again by their markers: at least 11 tests / 39 assertions in
  the main test module (`dom_is_a_div_with_the_native_class_and_a_closed_popup_child` 11,
  `the_rendered_dom_flattens_with_exactly_one_transient_window` 3,
  `clicking_toggles_the_picker_through_the_engine_and_never_touches_the_color` 5,
  `dismissal_closes_the_picker_state` 3, the seven key tests through `picker_control` 17), plus the five
  `theme_tests`, `app_theme_tests::a_color_input_declares_its_structure_once_for_every_theme` and
  `dll/tests/transient_window_layout.rs` (the predecessor's count: 21 tests, about 72 assertions).
- **35b stays blocked (a).** Checked at `audit/A1`'s tip (`layout/src/solver3/display_list.rs`,
  `GroupBounds::step`, the walk behind `reference_frame_extent` / `FrameIndex`): `PushReferenceFrame` /
  `PopReferenceFrame` have no arm, the items of a nested frame are unioned at their own, untransformed bounds.
  With the offset on the two containers every node box is laid out at the container's origin and moved by its
  own `translate(position)`: the containers' extents would be one node big at the origin, and the CPU
  renderer's surface for a frame is its extent - the nodes would be cut. Step 1 of the design (a nested frame
  counts moved by its matrix, with a pixel test on both paths) is A1's renderer core: extents, surfaces, damage
  and group culling all read that number. Edges and culling hold by reading (the wires get the same container
  transform; `ViewWindow` reads the graph, not the DOM); hit-testing under two nested transforms is A6-01's
  case; painting does not hold, so the change is not made. Also weighed: laying the nodes out by `left` / `top`
  with the pan on the container makes a node DRAG a layout write per move (35c) and one surface of up to four
  window areas of the container.
- **17c stays blocked (a)**: a measure that leaves the tree as it found it is the bridge's ownership model
  (every computation writes sizes, offsets, inline layouts and per-node caches of the subtree). The design above
  is complete; two points added: (i) the journal is a STACK - a measure of a row computes measures of its items
  - opened and closed by the same `compute_child_layout` call, restored innermost first; (ii) no journal when
  the measure runs at the constraints of the final layout the flag stands for (same known dimensions and
  available space): then the measure leaves what the final left, and the flag may simply stay.

## For area A3-window (4): the node graph's gesture transforms (A3-solver-55), Part 4 of the hand-over

Decided by reading, not fixed: no minimal patch is right, and no rule on values can be.
- What happens: `set_css_property(node, transform)` is `ContentChange::NodeCss { override_only: false }`:
  `apply_node_css_change` upserts the node's INLINE style, pins the value as a user override
  (`write_bound_gpu_properties` -> `set_user_property_override_fast`) and publishes it under the node's key.
  `begin_reconciliation` -> `StyledDom::migrate_user_overrides_from` copies every override to the matched
  node of every rebuild, and an override outranks the rebuilt inline style. The node graph writes one per node
  box and wire per mouse move of a drag or pan (`nodegraph_drag_graph_or_nodes`) and never clears one: after a
  gesture, a rebuild from a model that says otherwise (undo of a move, a programmatic move, a zoom of the
  model) shows the gesture's transform. 35g's keys make it the right node, not the right value.
- Why `migrate_user_overrides_from` cannot decide: the write keeps the old node's inline style equal to the
  override, so "the rebuilt inline value differs from the old one" IS "it differs from the override" - the case
  in which a toggle must persist (the ribbon's band, an open panel, a tooltip's opacity: the reason overrides
  migrate at all). `widgets/accordion.rs` documents the contract: the widget that hands a state to the host
  clears its override (`CssProperty::initial(ty)` through the override channel).
- So the node graph must clear its transforms when the gesture ends (the inline styles are in sync, so the
  picture stays, and the next rebuild is the host's). Cost today: a clear is a non-GPU write ->
  `restyle_user_properties` -> `recompute_inheritance_and_compact_cache` of the whole DOM, per box: 1 + its
  wires for a node drag, every box with DOM for a pan (about 26 nodes and their wires in the editor: the
  rebuild 35a removed, several times over).
- The exact change, engine half (`window.rs` `apply_node_css_change`, before the GPU path): when
  `override_only` and every property is `CssProperty::initial(ty)` of a GPU-only type (`is_gpu_only_property`)
  and the node's inline style declares for `ty` exactly the value the override holds, remove the entries with
  `set_user_property_override_fast` and return `ContentDirtyTier::Unchanged` - nothing any reader resolves
  changes (the cascade answers the inline value, the compact cache holds it, the published value stands), so
  no table is rebuilt and no list. (Or A3-window-15: one `restyle_user_properties` per dispatch for all its
  writes.)
- Widget half (`widgets/node_graph.rs`, this area, to do with it): a flag "a node was dragged" beside
  `graph_dragged`; in `nodegraph_unset_active_node` (and in the no-button branch of
  `nodegraph_drag_graph_or_nodes`, for a release outside the box) `info.override_css_property(box,
  CssProperty::initial(CssPropertyType::Transform))` for the dragged node's box and its wires, or for every
  node box and wire after a pan (the walks of the drag callback). Test: the changes of a mouse-up after a drag
  are one `OverrideNodeCssProperties { initial transform }` per written box and nothing else.
- With 35b the pan's part is two boxes.

## Hand-over (second; the context of the successor ran long too)

- Open: **55** (above; waits for the engine half), **25b** (design above; a decision whether a low row may
  change a field 74 files read), and the three `blocked (a)` rows 34, 35b, 17c.
- Not compiled, as everything: the commits since `b38dee0e0` in the order of risk are in the final message; the
  ones most likely to need a compiler's word are `8abfe10f9` (27: a generic iterator parameter, two scratch
  vectors), `9019db74d` (53: an `Option<(&BTreeMap, &mut Vec)>` parameter taken with `as_mut()`), `bd29ecca4`
  (16b: `or_insert_with` with `?` in the closure).
- Golden values expected to move since the merge: boxes inside a sticky box with a baked shift (54), sticky
  boxes inside absolutely positioned boxes (53). Nothing else places a box differently.

## The engine rows (third agent: A6-13, 25b, 17c, 34, 35b, 55)

The branch contains `audit/integration` (aeb5d0dfe) since `ceb7587af`.

## For area A6

- **A6-13: fixed 5ce5eae2f** (built by the solver area's third agent from A6's design and hand-over).
  What was built differs from the hand-over in three points, each found by reading the readers:
  1. The raised set lives as long as the layout RESULT: a `Weak` of the tree names it, AND a layout pass of the
     DOM drops it (`ScrollManager::forget_raised_bars` in `layout_dom_recursive_impl`). The tree's identity alone
     is not enough: a pass with nothing to lay out (the reconcile finds everything clean, or the structural
     display-list cache answers) leaves `LayoutCache::tree` in place and hands the SAME allocation to the new
     result; before A6-13 that result started from the cache's unraised tree.
  2. `display_list.rs` `paint_scrollbars` IS a reader after the registration: `regenerate_display_list_for_dom`
     builds from the result's tree outside a pass (caret blink, a text edit, a `set_css_property`), and re-derives
     only a view's extent, from the extent of that moment. So the set is folded into the result's own tree
     before such a build (`ScrollManager::settle_raised_bars`): one copy, the one the registration used to make
     after every pass, now only when a display-list-only rebuild follows the pass. To remove that one too the
     builder must take the raised flags as an input (A3-core's `generate_display_list`).
  3. No re-keying at the text-edit writes: `reshape_text_node` folds the set into the tree before its first
     write (the tree becomes the result's own there anyway), after which everything in that function reads and
     writes the tree as it always did, the plan's `old_info` and `warm.scrollbar_info = plan.now_reqs` included.
     With a `Weak` outstanding every `Arc::make_mut` moves the tree, so "follow the tree" cannot be done at the
     write.
- The registration writes into the tree as before when the result holds it alone (a hand-built result in a test,
  a result a text edit made its own): `Arc::get_mut`, no copy.
- A reader that stays on the tree by its contract: `focus_cursor::can_manually_scroll` in the layout-only model
  (`SpatialNavigationEnv::layout_only`: no scroll manager; the shells use the live model). It no longer sees a
  view's bar raised after layout.
- Golden dumps: `sb=` of a box whose bar only the registration raised is no longer in the dump of a shared tree.
  If a dump loses an `sb=` on a box that is no `VirtualView`, the registration raised a bar the layout pass did
  not (`apply_content_scroll_necessity` against the padding box vs. Phase 3 against the content box): by
  reading that cannot happen right after a pass; a dump that shows it is a finding.
- New test counter `solver3::cache::TREE_WRITE_COPIES` (copies made by `cache::own_mut`, which
  `DomLayoutResult::layout_tree_mut` now calls); `TREE_COPIES` counts what it counted.
