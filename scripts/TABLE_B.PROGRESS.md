# TABLE_B progress (branch `wt/table-b`, base `39092feee`)

Task: table parity with Chrome, part B (borders, spans, fixed layout, cell sizing rules).
Brief: scratchpad `TABLE_B_go.md`; house rules `wave4_common.md`. Nothing is compiled here.

## REPLAY onto TABLE-A (branch `wt/table-b-on-a` from `1964f561e`) - CURRENT WORK
Coordinator job: cherry-pick wt/table-b's commits in order onto the integration tip, resolving per
report section 2 (A's structure and table_width kept, my logic ported, twins folded).
- Done (wt/table-b -> port): cc018cdde, d2d3e41c3 -> dbf1989f1 (table_harness folded into A's
  table_markup), 6ec2d9131 -> bc9d113fe (cell-width rule dropped: A's column model), 72e10c268 ->
  8c3c1a0ee + 43f9a2105, aa2fa8076 -> 1f116c397, extra 1cc1d38df (rows/groups/cols no border),
  bfc46b3c5 -> + 568aaafe9, b05da2bfb, 8a4ae1cc3 -> e58489580 (#[ignore] removed), 76da6ac30,
  ef2673e6b -> a73764fe0, 6df89fe9b -> 7e948631a, 960298cfc -> 4d7c617c8 (A's body + grid,
  span rule, rowspan heights), d7235edda -> 64c8288e9, 9cd2ca799, 572733e5f -> 4dc66b63c,
  36b59205f -> 4e8933a4b, da2c0a9bc -> e15b1c1ce, b62b4bf22 -> b1eb089b0, bef5901f0,
  7b7f1c1f4 -> ad1ffca85, e38be3f8e -> f3fba0b7e (picked clean BUT still calls the dropped
  cell_specified_border_box_width - does not compile until the next port commit).
- NEXT: port commit for f3fba0b7e: table_width::column_element_widths reads the grid's column
  boxes (bare <col>, span); fixed layout uses it + table_width::specified_width
  (fixed_layout_width rewritten), drop column_box_widths and the step-1 col loop (A's step 2
  has the cols), drop enclosing_table if unused. Then e3975795c, 5de5cfd0a (skip: done),
  51c16a9df / bc087a71f / 349d1418e (alignment: skip if empty), report section, tip to coordinator.
- Decisions: A's model wins on cell width (a width neither raises nor lowers a column's min);
  my table_harness.rs merged into A's tests/common/table_markup.rs.

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

- `8a4ae1cc3` FIX (coordinator item 5): early `cell_is_ifc` return in `layout_formatting_context`
  removed; `measure_cell_content_width` measures an IFC cell with `layout_ifc` under the min/max
  constraint (final pass untouched). Report: the parent removes the `#[ignore]` on
  `prose_cells_wrap_inside_a_220px_table` at merge (it is not in my base).

- GREEN for `bfc46b3c5`: `ef2673e6b` (spans after single cells, max-content spread, inner
  spacing), `6df89fe9b` (spacing off before distribution, `cell_span_width`), `960298cfc`
  (sizing.rs table intrinsic on the shared grid + spacing), `d7235edda` (empty-cells: hide).

- `572733e5f` RED row heights (7 WPT lines out) / GREEN `36b59205f` (baseline through blocks),
  `da2c0a9bc` (row/cell specified heights, baseline row growth), `b62b4bf22` (table height -> rows).

- `7b7f1c1f4` RED fixed layout (5 WPT lines out) / `e38be3f8e` GREEN (col widths, first-row cells
  with padding/border, percentages of the columns' share, fixed only with a definite width,
  content width clamped, col widths in auto layout).

## IN PROGRESS (last commit e38be3f8e)
- next: compile-free review of every changed region (`git diff 39092feee -- layout/src`), fix what
  does not type-check by reading; then the report `scripts/TABLE_B_2026_10_01.md`.

## NEXT
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
