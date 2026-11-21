# A3-core: quadratics and waste in `core/`, `css/`, the display list builder, a11y

Area agent A3-core of the blind audit (2026-11-21). Nothing here was compiled or run.
Branch `audit/A3-core`, base `audit/base` (`f48bcc7b4`).

## Claims

Functions this area is going to rewrite or restructure (others: minimal patches only, not inside these).

- `core/src/style.rs`: `construct_html_cascade_tree`, `ScopedRuleIndex` (`new`, `candidates`), `subject_range`.
- `core/src/styled_dom.rs`: `StyledDom::create_from_compact_dom`, `convert_dom_into_compact_dom`, `scope_inline_css`,
  `collect_css_from_dom`, `strip_css_from_dom`, `append_child_with_index`, `restyle_nodes_state`,
  `restyle_left_state_rules`, `determine_rendering_order`, `sort_children_by_position`.
- `core/src/prop_cache.rs`: `CssPropertyCache::restyle*`, the inheritance pass, the compact cache build.
- `core/src/dom.rs`: `Dom::with_css` / `set_css` / `with_component_css` (the per-node sheet parse), node count / estimate helpers.
- `css/src/css.rs`, `css/src/parser2.rs`: the sheet parse entry points (`Css::from_string`), a parse cache if one is added.
- `layout/src/solver3/display_list.rs`: the generator (`generate_display_list`, `DisplayListGenerator::*`, stacking-context
  collection and z-order sorting, hit-test area pushes). NOT `FrameIndex` / `reference_frame_extent` (A1).
- `layout/src/managers/a11y.rs` (all), `layout/src/window.rs` 19632-19900 (`update_a11y_tree*`, `build_a11y_snapshot`,
  `a11y_rebuild_after_scroll_due`).

(This list is narrowed as the reading goes on; see the findings below for what was actually touched.)

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

(more below as the work goes on)
