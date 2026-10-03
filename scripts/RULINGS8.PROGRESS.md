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
- 60a535026 STRUT GREEN (a): text3 cache.rs perform_fragment_layout - no-text lines measured by their line box
- 9fdf0e0d8 STRUT GREEN (b): solver3 getters.rs collect_font_stacks_from_styled_dom keys parents of inline boxes
- bb434c120 STRUT GREEN (c): text3 cache.rs baseline_shift helper (line box + placement), strut in pass 1,
  middle raises
- 1e5ec4293 BASELINE RED: layout/tests/an_atomic_inline_sits_on_the_baseline_of_its_content.rs (Chrome numbers)
- d06b7c8b6 BASELINE GREEN (d): fc.rs line_baseline(LineEdge) + atomic_inline_content_baseline in
  measure_atomic_inline (flex/grid First, block Last, overflow rule inline-block only)
- bb575b00f RED + 72ef0a634 GREEN (e): IFC baseline = positioned last line baseline
  (PositionedItem::baseline_y, UnifiedLayout::first_line_baseline_y / last_line_baseline_y; fc.rs layout_ifc
  both exits + line_baseline)
- MAILREF8 SYNC (coordinator 2026-10-03: MAILREF8 landed inline_block_baseline + UnifiedLayout::last_line_baseline
  + glyph-line line boxes in the same regions):
  - 92bb530ce reverted my (e) 72ef0a634 and (d) d06b7c8b6 (duplicates of MAILREF8's helpers)
  - d79e4b720 (d'): fc.rs layout_flex_grid sets output.baseline = first_line_baseline (MAILREF8's
    measure_atomic_inline reads it for Flex/Grid); overflow rule exempted for flex/grid via the overflow_x/y lines
  - 1adee2882 (a'): reverted 60a535026, re-landed as separate blocks (atomic_line_box_top/_bottom recorded
    before `line_index += 1`, union after the existing line-box block) - clear of MAILREF8's lines
  - 55ee0c7db docs; `git merge-tree --write-tree HEAD wt/mailref8` (and every wt/*8 branch): only
    layout/tests/all.rs (append-only) conflicts.
  - Inline-block baseline tests in an_atomic_inline_sits_on_the_baseline_of_its_content.rs pass only with
    MAILREF8 merged (noted in the file).
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
- 2. GREEN (a) DONE 60a535026; GREEN (b) DONE 9fdf0e0d8; GREEN (c) DONE bb434c120.
- 3. Ripple review: 33 layout/tests files use inline-blocks (table_markup::block(w) = 10px-high inline-blocks;
     table tests asserted 10px rows) - grep height / origin.y assertions on lines holding only inline-blocks
     and update them to Chrome's numbers (or note them for the parent).
- 3b. Ripple so far: read row_groups, a_rows_stray_child, an_inline_tables_baseline, abspos line, max-height,
     vertical_align viewport, text3_baseline_exact (sub/super/text-top unchanged; middle only asserts != 0) -
     all still hold. Known side effect: an EMPTY inline span (fc.rs collect_inline_span_recursive emulates it
     as a Shape of line-height height ON the baseline) alone on a line: 18 -> 22 (Chrome 0 phantom / 18);
     report it (fc.rs owner), not changed.
- 4. Widgets: scan tools (not committed) target/rulings8/scan.py <App> <out.json> (dumps layout tree +
     node rects + hierarchy of every dom, capped) and analyze.py <dump> (IFC roots with atomics but no text).
     AzWidgets: only __azul-native-dialog (block) > .dialog-invoker (inline-block button, 31px, has text
     baseline -> line unchanged). Widgets use NO vertical-align. TODO: analyze.py misses inline-flex children
     (layout FC "Flex") - extend scan.py to query get_node_css_properties for Flex children of all-inline
     parents; then scan a few apps (AzMail, AzWriter, AzDrive, AzCalendar...). Also grep widgets: flat.rs / flora.rs + widget files for display: inline-block / inline-flex / img
     icons in block containers; add line-height: 0 / display: block / vertical-align where the look must
     stay; list each in the report.
- 4b. FOUND (scan + probe target/rulings8/baseline.json): flex/grid (layout_flex_grid) and BFC (layout_bfc sets
     output.baseline = None) report no baseline -> inline-flex buttons / block-holding inline-blocks sit on the
     baseline by their bottom; with the strut they would grow by the strut descent (dialog invoker 31 -> ~35).
     DONE d06b7c8b6. (was: GREEN (d) in solver3/fc.rs measure_atomic_inline (~10500): when layout_result.output.baseline is
     None, walk the laid-out subtree: generalize first_line_baseline (~9220) into line_baseline(index, tree,
     depth, LineEdge::First|Last) (First = the old code exactly; Last = items.last() / children reversed);
     flex/grid -> First and ignore the overflow rule; block -> Last. Subtract padding.top+border.top
     (first_line_baseline is from the border-box top; atomic_inline_baseline_offset wants content-top).
- 4c. Scanned (no atomic-only lines found): AzWidgets (only the dialog invoker: has text, label-baseline),
     AzMail (--sample), AzWriter, AzDrive, AzCalendar (--data), AzNotes, AzSheets, AzShow, AzPhoto.
     NEXT: AzTasks (--data), AzContacts, AzPaint, AzDashboard, AzMeet, AzReview, AzBuilder, AzCalculator,
     AzSetup, AzVideoCut, AzShells, AzMaps (no --data-dir: run without --sample). Then grep widgets for
     image icons in block containers. (was: scan more apps one at a time (scan.py <App> target/rulings8/<app>.json; analyze.py) - lines of
     only atomics WITHOUT text inside (icons, swatches, images) are what grows (by the strut descent, and to
     the strut ascent if shorter); boxes with text sit on their label baseline now (unchanged height).
- 5. Report scripts/RULINGS8_2026_10_03.md.

## Decisions / open questions
- FOCUS test location: inside the crate (layout/src/e2e/), because the e2e runner's keep-the-runner entry point is
  crate-private; the click-to-focus rule is a pure function also unit-tested in managers/hover.rs.
