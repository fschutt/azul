# RULINGS8 progress (wave 8, branch wt/rulings8, base 72d0d6639)

Brief: scripts/waves/wave8/PLAN.md section "RULINGS8".
1. FOCUS: a click on non-focusable content inside a VirtualView focuses the nearest focusable ancestor, across the
   VirtualView boundary into the host DOM.
2. INLINE-BLOCK LINE HEIGHT: a line holding only an inline-block includes the strut (CSS 2.1 s10.8), as Chrome; then
   adjust the widgets' icon CSS so they keep their look.

## DONE
- 80a1c3ccf progress file
- cffdaeffa FOCUS RED: layout/src/e2e/focus_across_virtual_view_tests.rs (child module of runner.rs, appended at
  its end; needs run_e2e_test_keeping_runner which is private, so not in layout/tests/)
- 6e64021b6 STRUT RED: layout/tests/a_line_holding_only_an_inline_block_is_as_tall_as_its_strut.rs (10 tests)
- 91928b72d FOCUS GREEN: managers::hover::focusable_under_pointer walks core::events::get_event_path (4th closure
  host_of); dll event.rs + runner.rs pass virtual_view_manager.host_of_nested_dom; hover.rs unit tests updated.

## IN PROGRESS
- INLINE-BLOCK LINE HEIGHT: analysis done, nothing written yet. Probe (not committed):
  target/rulings8/probe.py (copy of LAYOUT7's), cases target/rulings8/strut.json + middle.json.
  CHROME 154 (Arial; div p > span#b inline-block)            vs AZUL prebuilt today:
  - 10px box, 16px normal: p 18, b.y 4                           p 10, b.y 2.8
  - 10px box, 16px, line-height 20px: p 20, b.y 5                p 10, b.y 4.8
  - 10px box, 12px normal: p 14, b.y 1                           p 10, b.y 0
  - 10px box, line-height 0: p 10, b.y 0                         same
  - 24px box, 13px normal: p 27, b.y 0                           p 24
  - 40px box, 16px normal: p 44, b.y 0                           p 40
  - 10px middle 16px: p 18, b.y 4.84                             p 10, b.y 11.8 (SIGN BUG)
  - 10px top: p 18, b.y 0; two 10px boxes: p 18, y 4; img 10px: p 18, y 4
  - "x" + 10px box: p 18, b.y 4 (azul matches already: face loaded via the text)
  - middle 24px in 16px / 13px: p 24, b.y 0 (azul b.y 16 / 15.25 - outside its parent!)
  - middle 16px in 14px: p 17.38, b.y 1.38 (azul p 16, y 11.5); middle 40px: p 40 y 0 (azul y 24)
  - "x" + middle 24px: p 24 y 0 (azul p 30.15, y 16.15); bottom 24px: p 24 y 0 (azul y 3.2)
  - text-top 10px: p 18 y 0 (azul p 10); vertical-align -5px 10px: p 19 y 9 (azul p 10 y 7.8)
  - inline-flex 20px: p 24 (azul 20); inline-block with text + 4px padding: 26 (same)
  - text only 16/13/14px Arial: 18/15/16 (same)
  THREE ROOT CAUSES found:
  (a) text3 cache.rs perform_fragment_layout (~line 11600 + ~11650): the IFC height is the items' bounds; only
      lines with no item of height count their line box (line_box_extent). Fix: a line with no glyph Cluster
      counts its whole line box [line_top_y, line_top_y + band_height]; track line_box_top too and take the
      union rect with the items' bounds (keeps <br> cases: old None branch == union with top 0).
  (b) the strut face is only loaded when some TEXT node uses the IFC root's font: solver3/getters.rs
      collect_font_stacks_from_styled_dom Phase 1 keys only text nodes, so fc.rs ~5440 falls back to 0.8/0.2em
      and line-height normal = 1em (16 not 18). Fix (minimal solver3 edit): also key the PARENT of every
      non-text inline-level node (display inline / inline-block / inline-flex / inline-grid / inline-table).
  (c) text3 cache.rs position_one_line ~12640: vertical-align middle uses baseline + xh/2 (must be - xh/2);
      calculate_line_metrics (~10926) ignores the vertical-align shift of baseline-relative items and adds the
      strut AFTER the top/bottom pass (bottom 24px box -> 3.2 offset). Fix: ONE helper giving an item's
      shifted (ascent, descent) relative to the line baseline, used by calculate_line_metrics and the
      placement; strut joins pass 1; top/bottom only expand afterwards. Sub/super shift: from the strut
      (parent) font, not line_ascent (circular).

## NEXT
- 1. DONE (6e64021b6). RED test layout/tests/a_line_holding_only_an_inline_block_is_as_tall_as_its_strut.rs (use
     crate::table_markup::{body, near, rect}; append #[path]+mod to layout/tests/all.rs at the very end) with
     the Chrome numbers above (font-independent ones exact: line-height 20 -> 20, line-height 0 -> 10;
     normal: equal to a "x" text line in the same font, b.y 4 +-0.6; middle 24px -> p 24 b.y 0; bottom -> 0;
     -5px -> p 19 b.y 9 +-0.6; no-text-anywhere doc -> p 18 +-1). Commit.
- 2. GREEN (a), commit; GREEN (b), commit; GREEN (c), commit.
- 3. Ripple review: 33 layout/tests files use inline-blocks (table_markup::block(w) = 10px-high inline-blocks;
     table tests asserted 10px rows) - grep height / origin.y assertions on lines holding only inline-blocks
     and update them to Chrome's numbers (or note them for the parent).
- 4. Widgets: grep themes flat.rs / flora.rs + widget files for display: inline-block / inline-flex / img
     icons in block containers; add line-height: 0 / display: block / vertical-align where the look must
     stay; list each in the report.
- 5. Report scripts/RULINGS8_2026_10_03.md.

## Decisions / open questions
- FOCUS test location: inside the crate (layout/src/e2e/), because the e2e runner's keep-the-runner entry point is
  crate-private; the click-to-focus rule is a pure function also unit-tested in managers/hover.rs.
