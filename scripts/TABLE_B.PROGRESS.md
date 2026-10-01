# TABLE_B progress (branch `wt/table-b`, base `39092feee`)

Task: table parity with Chrome, part B (borders, spans, fixed layout, cell sizing rules).
Brief: scratchpad `TABLE_B_go.md`; house rules `wave4_common.md`. Nothing is compiled here.

## DONE
- `d2d3e41c3` RED / `6ec2d9131` FIX: a cell's specified width sets its column
  (`cell_width_contribution`, `measure_cell_widths`; sizing.rs table cells).
  Shared fixture `layout/tests/common/table_harness.rs`.
- `72e10c268` RED (15 WPT lines out of the expectations) / `aa2fa8076` FIX: collapsing
  border model (`analyze_table_structure` = the one grid placement, `resolve_collapsed_borders`,
  `apply_collapsed_table_borders` run first in `calculate_intrinsic_sizes`, painter rewritten,
  column backgrounds, table layer 1 paints its own border/backgrounds/shadows,
  `used_border_spacing` resolved once).
- `bfc46b3c5` RED: separated model (spacing in the intrinsic width, spanning cell covers inner
  spacing, empty-cells: hide).

## IN PROGRESS
- Coordinator item (item 5): remove the `cell_is_ifc` early return in
  `layout_formatting_context` (the parent reverted it on main), make the table's MEASUREMENT of a
  prose cell use its IFC min-content explicitly, un-ignore `prose_cells_wrap_inside_a_220px_table`.

## NEXT
- FIX for the separated-model RED (`bfc46b3c5`).
- Row heights: `tr` height, `td` height (box-sizing, IFC branch too), table height spread over
  rows, `vertical-align: baseline` across a row (block content too). WPT: collapsing-border-model-003/009.
- `table-layout: fixed`: only with a non-auto width; `col` widths; percentages; padding/border in
  the column width. WPT: fixed-table-layout-025..027.
- Spans: single-span first, then by span; spread by max-content; rowspan heights incl. spacing.
- Report `scripts/TABLE_B_2026_10_01.md`.

## Decisions (unattended)
- Table UA defaults (`border-spacing: 2px`, `box-sizing: border-box`) are left to TABLE-A
  together with the `cellspacing` mapping: adding the 2px without the mapping would widen
  every `cellspacing="0"` mail table.
- The collapsed table's own border = half of the WIDEST edge on each side (all four sides;
  WPT border-collapse-006 needs the inline sides that way too). An empty collapsed table keeps
  its own border.

## Open questions
- (none yet)
