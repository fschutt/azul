# TABLE_A progress

Branch `wt/table-a` from `39092feee`. House rules: scratchpad `wave4_common.md` (no cargo, RED first).
Brief: scratchpad `TABLE_A_go.md` (widths, row groups, presentational attributes, nested tables).

## DONE
- 6e3e48548 progress file
- e719d122e RED: layout tests (5 files + common/table_markup.rs) + core/tests/xml_attributes.rs
- ebcd5c936 table_width.rs: ColumnConstraint, specified_width, clamp_percentages, table_min_max,
  distribute_to_columns (+unit tests), registered in solver3/mod.rs
- bc6361f65 FIX item 3: presentational attributes (attribute table keeps them, StyledDom creation maps them), UA table defaults

## IN PROGRESS
- Items 1/4/5 code complete (58f7ccd60, c06e62f4a, 7e00f8d0d, c1266420f, + the Step 2 commit).
- NEXT STEP: item 2 (row groups) in fc.rs: analyze_table_structure visual order (thead first, tfoot
  last), position_table_cells -> positions for row groups / direct rows / caption relative to the
  table, rows relative to their group, cells relative to their row (warm.relative_position), col /
  colgroup rects; cache.rs table cache hit positions only; display_list: the table paints its own
  background + border (generic path), then paint_table_items layers 2-6.

## NEXT
- FIX item 2: row groups / rows / cells positioned hierarchically; thead first, tfoot last; table paints
  its own background + border, then the table layers; cache-hit path for tables positions only.
- (optional) legacy `align=center` block centering; report.

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
