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

(to be filled)
