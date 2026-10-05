# FIX9-PAINT progress (PKG 2 ENGINE-PAINT-FRAME-A11Y, wave 9, 2026-10-05)

Branch wt/fix9-paint (fast-forwarded to b454da215). Brief: scripts/waves/wave9/SMALL_FIXES.md "PKG 2".

## DONE
- 2.1 zoom -> radius / shadow / painted border widths: RED e633df1bd (getters inline test), RED 3e446e777
  (new file layout/tests/a_zoomed_box_paints_its_border_corners_and_shadow_zoomed.rs - parent registers),
  GREEN e64b16ce8. No CSS outline property exists. Notes: % radius in Normal state reads 0 from the compact
  cache (core/src/compact.rs stores only px) - round 2; inline boxes' InlineBorderInfo ignores zoom.

## IN PROGRESS
2.2 DEDUP live_color vs get_used_text_color

## NEXT
2.2 RED, GREEN; then 2.3 .. 2.14, then the suite failure (window.rs inline-flex test).

## Open questions
(none)
