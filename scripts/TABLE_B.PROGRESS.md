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
  ef2673e6b -> a73764fe0, 6df89fe9b -> 7e948631a.
- Last commit: see `git log -1`.
- NEXT: cherry-pick 960298cfc (sizing intrinsic: take A's body, port: placement through
  fc::analyze_table_structure, spanning after single cells via fc::distribute_cell_width_across_columns,
  rowspan height growth), then d7235edda, 9cd2ca799(skip if conflict), 572733e5f, 36b59205f,
  da2c0a9bc, b62b4bf22, bef5901f0, 7b7f1c1f4, e38be3f8e (fold column_box_widths into A's
  table_width::column_element_widths: extend it with bare <col> and span), e3975795c, 5de5cfd0a
  (skip: already applied), 51c16a9df / bc087a71f / 349d1418e (alignment: likely empty -> skip),
  then report section "replayed onto A".
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

## IN PROGRESS (last commit 8a4ae1cc3)
- next: GREEN for `bfc46b3c5` - (1) sizing.rs `calculate_table_intrinsic_sizes` adds
  (cols+1)*h / (rows+1)*v spacing via `fc::used_border_spacing`; (2) `layout_table_fc` hands the
  column algorithms the content width minus (cols+1)*h; (3) `distribute_cell_width_across_columns`
  and the sizing.rs spanning loop subtract the inner spacing; (4) `calculate_row_heights` gives a
  spanning cell its inner spacing; (5) `empty-cells: hide` in display_list (cell bg + border).

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
