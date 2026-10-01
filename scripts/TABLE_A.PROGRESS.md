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
- Items 1-5 code complete. Last code commit: "row groups, rows, columns and the caption are boxes".
- Review pass of fc.rs layout_table_fc/caption done; expectations: 8 lines removed (commit "wpt
  expectations").
- f3c28b1b9 RED legacy center; next commit FIX (fc.rs centers_blocks_the_legacy_way + layout_bfc).
- NEXT STEP: the report
  scripts/TABLE_A_2026_10_01.md.

## NEXT
- (optional) legacy `align=center` block centering (-webkit-center) in layout_bfc.

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
