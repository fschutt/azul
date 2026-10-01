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
- FIX items 1/4/5. Done in the working tree / last commit: fc.rs `resolve_table_border_spacing` (twins
  replaced), layout_table_fc reads the table width from `used_size` and gives the columns the content
  width minus the spacing (`columns_width`).
- Next step (b): sizing.rs `calculate_table_intrinsic_sizes` via table_width (cell warm intrinsic,
  spacing, caption, col widths) + TableCell exempt from the px override + Table keeps MIN.
- Then (c): `calculate_used_size_for_node`: auto table = clamp(MIN, avail, MAX), border box floored at MIN.
- Then (d): fc.rs `calculate_column_widths_auto_with_width` Step 2 via `distribute_to_columns` with
  percent/fixed from single-span cells and `<col>`s.

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
