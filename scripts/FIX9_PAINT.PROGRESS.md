# FIX9-PAINT progress (PKG 2 ENGINE-PAINT-FRAME-A11Y, wave 9, 2026-10-05)

Branch wt/fix9-paint (fast-forwarded to b454da215). Brief: scripts/waves/wave9/SMALL_FIXES.md "PKG 2".

## DONE
- 2.1 zoom -> radius / shadow / painted border widths: RED e633df1bd (getters inline test), RED 3e446e777
  (new file layout/tests/a_zoomed_box_paints_its_border_corners_and_shadow_zoomed.rs - parent registers),
  GREEN e64b16ce8. No CSS outline property exists. Notes: % radius in Normal state reads 0 from the compact
  cache (core/src/compact.rs stores only px) - round 2; inline boxes' InlineBorderInfo ignores zoom.

- 2.2 DEDUP live_color -> get_used_text_color: RED bf8067387, GREEN f4cb41a68.
- 2.3 vw/vh font chains: RED 696e24137, GREEN 191b6530f (viewport variants; window.rs + raster.rs pass it);
  resize RED fba427c9f, GREEN 307df77e7 (signature folds in the viewport for vw-sized docs).
  Round 2: text3/cache.rs:1109 + paged_layout.rs (2 sites) -> *_in_viewport variants.

## IN PROGRESS
2.4 CPU SVG stroke linecap / linejoin / dasharray

## NEXT
2.4 RED, GREEN; then 2.5 .. 2.14, then the suite failure (window.rs inline-flex test).

## Open questions
(none)
