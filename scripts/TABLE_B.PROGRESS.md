# TABLE_B progress (branch `wt/table-b`, base `39092feee`)

Task: table parity with Chrome, part B (borders, spans, fixed layout, cell sizing rules).
Brief: scratchpad `TABLE_B_go.md`; house rules `wave4_common.md`. Nothing is compiled here.

## DONE
- (none yet)

## IN PROGRESS
- Reading the table code (`layout/src/solver3/fc.rs` 5600-8600, `display_list.rs` 6260-6900,
  `sizing.rs` table intrinsic sizes) and the WPT tests of my area.

## NEXT (plan, one RED + FIX pair each)
1. A cell's specified `width` sets its column (min = max(MCW, W), max = max(min, W)) in
   the column measurement AND in the table's intrinsic sizes (two twins today).
2. Collapsing border model: one grid placement, conflict resolution over cell / row /
   row group / column / column group / table, half borders in layout (table + cells, table
   padding 0) applied before the intrinsic pass, the grid painted once per edge.
3. Separated model: the table's own border is painted; border-spacing inside spanning
   cells; `empty-cells: hide`.
4. Row heights: `tr` height, `td` height (box-sizing), table height spread over rows,
   `vertical-align: baseline` across a row (block content too).
5. `table-layout: fixed`: only with a non-auto table width; `col` widths; percentages;
   cell padding/border in the column width.
6. Spans: single-span cells first, then by span; spread by max-content; spacing inside
   the span; rowspan heights.
7. Min-content of a cell holding a block child.

## Decisions (unattended)
- Table UA defaults (`border-spacing: 2px`, `box-sizing: border-box`) are left to TABLE-A
  together with the `cellspacing` mapping: adding the 2px without the mapping would widen
  every `cellspacing="0"` mail table.

## Open questions
- (none yet)
