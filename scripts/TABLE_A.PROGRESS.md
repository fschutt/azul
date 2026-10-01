# TABLE_A progress

Branch `wt/table-a` from `39092feee`. House rules: scratchpad `wave4_common.md` (no cargo, RED first).
Brief: scratchpad `TABLE_A_go.md` (widths, row groups, presentational attributes, nested tables).

## DONE
- 6e3e48548 progress file
- e719d122e RED: layout tests (5 files + common/table_markup.rs) + core/tests/xml_attributes.rs
- bc6361f65 FIX item 3: presentational attributes (attribute table keeps them, StyledDom creation maps them), UA table defaults

## IN PROGRESS
- FIX items 1/4/5 (table width, column distribution, nested tables)

## NEXT
1. RED: layout tests (width cap, row groups, attributes, percent columns, nested tables) + core test
2. FIX item 3: presentational attributes kept by the attribute table, mapped to CSS at StyledDom creation
3. FIX item 1/4/5: table used width = clamp(MIN, available, MAX); column constraints + CSS Tables 3
   width distribution, shared by intrinsic sizing and layout; nested table intrinsic from the cell
4. FIX item 2: row groups / rows / cells positioned hierarchically; thead first, tfoot last; table
   paints its own background + border, then the table layers
5. report

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
