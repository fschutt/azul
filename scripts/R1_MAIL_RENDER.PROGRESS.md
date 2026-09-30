# R1_MAIL_RENDER progress

Branch `wt/r1-mail-render` from `abbba2408`. House rules: wave3_common.md (no cargo, RED first).

## DONE
- 5ea61159c RED: raster unit tests (unplaceable glyph pens, sub-pixel clips) in cpurender/raster.rs
- 1e4a14e02 FIX: raster guards `glyph_ink_reach` / `glyph_pen_reaches_pixmap` / `text_clip_pixel_box`
  in every glyph path (LCD sweep, LCD tiles, grayscale)
- 56e2efc87 RED: receipt price column (bis_D) + newsletter indented cell (sample 01) layout tests
- 3c8ac84cf FIX: table cell measured against a typed constraint; block-branch cell laid out at its
  column width, content height = content box; inline branch only for loose text / inline-only cells
- bb5e41e2c RED: quote-bar gradient test
- fb05834f9 FIX: E-GRAD (gradient_line, length stops / offset_px, shared resolver, CPU LUT, GPU path)
- 0d1f13626 FIX: compositor2 closure types
- report scripts/R1_MAIL_RENDER_2026_09_30.md

## IN PROGRESS
- nothing

## NEXT
- parent: compile, run the commands in the report, autofix api.json, AZ_BLESS the codegen goldens

## Open questions
- see "What is left" in the report
