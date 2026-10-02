# TABLE_A progress

Branch `wt/table-a` from `39092feee`. House rules: scratchpad `wave4_common.md` (no cargo, RED first).
Brief: scratchpad `TABLE_A_go.md` (widths, row groups, presentational attributes, nested tables).

## DONE
- 6e3e48548 progress file
- e719d122e RED: layout tests (5 files + common/table_markup.rs) + core/tests/xml_attributes.rs
- bc6361f65 FIX item 3: presentational attributes (attribute table keeps them, StyledDom creation maps them), UA table defaults
- ebcd5c936 table_width.rs; c06e62f4a column_element_widths
- 58f7ccd60 border-spacing helper, columns get content minus spacing, width from used_size
- 7e00f8d0d intrinsic sizes via table_width (nested tables)
- c1266420f used width max(MIN, min(MAX, available)), floor at MIN
- 3d2988ec1 CSS Tables 3 distribution in the layout
- bb8963ee3 visual row order; aac1d3349 grid boxes, caption, cache-hit path, table border painting
- 5cff90ef5 WPT expectations (8 removed)
- f3c28b1b9 / f9816a688 RED / FIX legacy align=center block centering
- report scripts/TABLE_A_2026_10_01.md (this commit)

## IN PROGRESS
- Parent's run (main + this branch): compiled; 1791 pass, 5 of MY tests fail. Fixing one by one:
  - #5 columns_and_column_groups_span_their_columns: ROOT CAUSE reconcile_recursive (cache.rs, the
    tree builder the layout really uses) classifies children by `layout_tree::is_block_level`, which
    lacked table-column(-group): a table's [colgroup, tr] was "mixed content" and the colgroup got
    wrapped in an anonymous INLINE box under the table, invisible to the grid. FIX: is_block_level +=
    TableColumnGroup | TableColumn (commit "colgroup ... wrapped").
  - #3 cellpadding_pads_the_tables_own_cells_only: ROOT CAUSE the intrinsic pass's Inline arm
    (sizing.rs calculate_node_intrinsic_sizes) measured only text nodes and DOM elements with text;
    the ANONYMOUS inline wrapper reconcile builds around `<i>` beside the nested table has no DOM
    node -> (0, 0): the cell's intrinsic width lost the 100px box (cell came out 12 + 2x6 = 24).
    FIX: measure an anonymous inline wrapper like an IFC root (commit "anonymous inline wrapper").
  - #4 align_center_on_a_cell_centers_the_table_inside_it: ROOT CAUSE cache.rs
    prepare_layout_context re-derived a TABLE CELL's used size with calculate_used_size_for_node
    (TableCell arm: its intrinsic max-content, 600) instead of the column width the table wrote
    into used_size (800); layout_bfc takes `available_cross` from those constraints, so the 600px
    table had 0px to be centred in. FIX: a cell's table-given used_size is its used size there
    (commit "a table cell's children get the column").
  - #1/#2 width-cap tests: the table code is right (1000 / 500 are MIN; the cap and the floor
    hold). ROOT CAUSE upstream: the intrinsic min-content of inline-blocks separated by
    whitespace-only text = SUM of the boxes (spaces give no break and no width) - an inline
    intrinsic bug, reported, not fixed here. Tests now measure prose (`table_markup::prose`).
  - Report section 9 (the five failures + engine findings) written.
- NEXT: nothing; parent re-runs `cargo test --release -p azul-layout --test all` (the 5 tests).

## NEXT
- parent: compile, run section 5 of the report, bless the WPT lists, send mail_boxes numbers.

## Decisions (unattended, noted here)
- Presentational hints: the attribute table KEEPS the attribute on the node (AttributeType::Custom), and
  StyledDom creation maps it to CSS declarations PREPENDED to the node's inline style (the style attribute
  still wins). Precedence deviation from HTML noted in the report (an author stylesheet rule cannot beat a
  hint yet). Not a component argument (ruling A covers element arguments like href; a hint is style).

## Open questions
- none yet

## Notes
- Parent (after outage): on main the `cell_is_ifc` early return in layout_formatting_context was REVERTED;
  do not rely on it. TABLE-B owns prose-cell min-content measurement.
