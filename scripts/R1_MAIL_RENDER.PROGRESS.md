# R1_MAIL_RENDER progress

Branch `wt/r1-mail-render` from `abbba2408`. House rules: wave3_common.md (no cargo, RED first).

## DONE
- 5ea61159c RED: raster unit tests (unplaceable glyph pens, sub-pixel clips) in cpurender/raster.rs

## IN PROGRESS
- E-TABLE root cause: `measure_cell_content_width` (layout/src/solver3/fc.rs) hands the cell a FINITE
  sentinel (`f32::MAX / 2`), which `ContainingBlock::from_flattened_with_width_type` reads as a
  DEFINITE width, so a text-less cell (`<td colspan=2><hr></td>`) measures ~1.7e38 wide, both spanned
  columns get ~0.85e38, the second column's text lands at x ~ 0.85e38 and agg overflows.

## NEXT
- fix measurement CB typing + lay a cell's block children out at the column width
- harden render_glyphs_lcd / text paths against non-finite / off-i32 geometry
- REDs: receipt price column (bis_D), newsletter cell text (nl)
- E-GRAD: one shared stop/direction helper, CPU + GPU paths
- quote-bar RED + fix
- report scripts/R1_MAIL_RENDER_2026_09_30.md

## Open questions
- none yet
