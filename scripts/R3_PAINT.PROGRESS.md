# R3-PAINT progress (wave 9 round 3, branch wt/r3-paint from 6a39b7f1a)

## Diagnosis (done, no cargo)
- svg_paint x5: TEST wrong. An inline `<svg>` alone on a line sits on the strut (user ruling 2026-10-03,
  1adee2882): the 8/16 px frames overflow, the viewport scrolls, its thumb #c1c1c1 covers the frame.
  Fix: the fixture lays the svg out as a block (the ruling's own remedy).
- svg_mask_memo_tests (lib): TEST wrong. `parse_xml_to_styled_dom` is the DOCUMENT loader (fast arena
  path), which builds no SVG geometry / fill / viewBox at all; the tree loader (`parse_xml` +
  `dom_from_parsed_xml`) does. Fix: fixture uses the tree loader. Gap reported (not fixed).
- the_incremental_raster...::a_turned_box_is_repainted_turned: TEST wrong. The harness renders with
  no live GPU values, so it reads the display list's fallback matrix, baked before layout from the
  previous pass's sizes (none: rotate about the corner). Every live CPU backend renders with
  `CpuRenderState::from_gpu_cache` (values refreshed against the laid-out box after the solve).
- a_border_defaults...::a_border_style_alone_draws_a_medium_border: CODE wrong. The compact cache
  defaults an undeclared border width to 0 px (not `initial`), so `used_border_width` never sees
  "no width" on the normal-state fast path. Fix: default I16_INITIAL (css/src/compact_cache.rs).
- an_hr...::an_authors_single_border_keeps_the_rule_one_pixel: CODE wrong. `border: none` (and
  `border-*: none`, `border-style: none`, `border-*-style: none`) parse as the CSS-wide keyword
  `CssPropertyValue::None`, which the compact cache skips: the UA's `inset` stays, the width reads
  as undeclared -> medium: 1 + 3 px. Fix: `none` is border-style's typed value (css property.rs).

## DONE
(commits below)

## NEXT
- commit the 5 fixes in order above, then scripts/R3_PAINT_2026_10_05.md
