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
- nothing

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
