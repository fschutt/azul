# TABLE_B progress (branch `wt/table-b`, base `39092feee`)

Task: table parity with Chrome, part B (borders, spans, fixed layout, cell sizing rules).
Brief: scratchpad `TABLE_B_go.md`; house rules `wave4_common.md`. Nothing is compiled here.

## FIX on merged main (branch `wt/table-b-fix` from `f9aba9e00`) - IN PROGRESS
Six table failures on main (layout --test all): (1) table_cell_width single/two cells +
flex_intrinsic_text::table_cell_padding_offsets_text_from_the_cell_top - whole table has no rects;
(2) real_table_cells_center_their_text_vertically - header baseline 24.8 vs 28.2; (3) fixed table
first-row percentage - 6300 px differ; (4) prose_cells_wrap_inside_a_220px_table - only 2 lines.
- NEXT: root-cause (1) from the code.

## REPLAY onto TABLE-A (branch `wt/table-b-on-a` from `1964f561e`) - DONE
All of wt/table-b replayed in order; report section 9 "Replayed onto TABLE-A" written
(`e8968026a`). Skipped picks: 5de5cfd0a (folded), 51c16a9df / bc087a71f / 349d1418e (alignment,
redundant or obsolete on A's base). Added: 1cc1d38df (rows / groups / cols carry no border),
50c476b81 (fixed layout through table_width), fe268416f + 3043c201f (one span rule in
table_width, the spanning cell's width included), f37ab69fa (three WPT lines out), bc2c7328f
(column_element_widths pub(crate)).
- NEXT: nothing; waiting for the parent's compile and suite run (report sections 6, 7, 9).
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

- `5de5cfd0a` restored `white_space_runs` (a bulk edit had touched it).
- Report started: `24fcb60bc` (summary), `fad03f6cd` (per item).
- Alignment with TABLE-A (wt/table-a, integrated first): `51c16a9df` (spacing = A's
  `resolve_table_border_spacing` verbatim), `bc087a71f` (layer 1 = A's generic box painting
  verbatim), `349d1418e` (A's intrinsic-pass cell/table block verbatim; cell width rule moved into
  `calculate_table_intrinsic_sizes`).

- Report `scripts/TABLE_B_2026_10_01.md` complete (`7e8a1f22b`): summary, per item, merge guide onto
  TABLE-A, WPT list with confidence, commits, compile risks, commands, what is left.

## IN PROGRESS
- (none) - task complete; waiting for the parent's compile / suite run.

## NEXT (if resumed after the parent's run)
- Fix whatever the compile shows (start with report section 6), then re-bless the WPT list.
- Anonymous table objects (report section 8) if the parent assigns them.

## Decisions (unattended)
- Table UA defaults (`border-spacing: 2px`, `box-sizing: border-box`) are left to TABLE-A
  together with the `cellspacing` mapping: adding the 2px without the mapping would widen
  every `cellspacing="0"` mail table.
- The collapsed table's own border = half of the WIDEST edge on each side (all four sides;
  WPT border-collapse-006 needs the inline sides that way too). An empty collapsed table keeps
  its own border.

## Open questions
- (none yet)
