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

- 2.4 SVG stroke caps/joins/dashes: RED 4c6e646a2, GREEN 03e377e56.
- 2.5 svg_render without PNG: RED d5c38eabd (also corrects the straight-alpha pin), GREEN 2b0d4e81e.
  New pub fn cpurender::render_svg_to_raw_image_over (Rust-only).

- 2.6 SVG mask memo: RED 61e096fb5, GREEN 108450ea2 (thread-local LRU in display_list.rs).
- 2.7 LCD tile clip: RED 09e16cdd4, GREEN ac1d92bae (text_clip_pixel_box(text_run_clip), not outward).

- 2.8 opacity tween values-only: RED e8b72669d, GREEN aabe3db1e (gpu.rs refresh_opacity_value_of +
  fingerprint; display_list binds the CSS key; window.rs Opacity arm + patch_compact_opacity).

- 2.9 Timer.node_id remap: RED 5358ef0a2, GREEN 1d77f15be.
- 2.10 GPU cache borrowed not cloned (2 sites, layout pass + regenerate_display_list_for_dom): 1bd06c5df.

- 2.11 lints skip unchanged DOM: instrumentation b617d6eb6 (pub dom_lint_runs), RED 5b796699f,
  GREEN 3b961da00 (per-dom arena stamp + new_generation / kept_doms).

- 2.12 lint dedupe: RED 62abe69c0 (3 tests), GREEN ef05aaf81 (core diagnostics::emit_once, cleared by
  clear(); dom_lint identity_key/report_once; has_readable_text_label twin folded).

## IN PROGRESS
2.13 redundant a11y pass in dll common/event.rs

## NEXT
2.13 (dll/src/desktop/shell2/common/event.rs refill_a11y_tree_after_regeneration ~4874); then 2.14,
then the suite failure.

## Open questions
(none)
