# TABLES (wave 5, 2026-10-02) - progress

Branch `wt/tables` from `2e92c759b`. Brief: scratchpad/wave5/TABLES.md. Report: scripts/TABLES_2026_10_02.md.

## Tools (scratchpad/tables/)
- `probe.py FILE.json` - Chrome vs prebuilt azul (AzPaint debug server, capped, fresh app per case) rects
  of id'd elements. `shot.py`, `tree.py` - screenshot / layout tree / display list of a mounted page.
  `boxes.py MAIL`, `outline.py MAIL [from] [to]` - the baseline mail_boxes report per box / with markup.
- The prebuilt layout test binary `target/release/deps/all-978bc4fd9634d150` (base engine) runs existing
  tests through run_capped.sh: `... all-978bc4fd9634d150 table --test-threads 2` -> 166 passed at base.
- CAUTION: the debug server reports inline-block positions inside an IFC wrongly (all at the line start);
  use display-list rects for them.

## DONE
- f120ecc14 F23: prose_cells_wrap_inside_a_220px_table restored per cell (engine already right).
- e4a6554a5 F24 RED (ignored, OPEN): text_beside_an_italic_or_bold_box_keeps_a_font (font bug, handed off).
- d4dc39299 F24: width-cap tests back on fixed-size boxes (`words()` = non-italic span boxes).
- c6530d7e4 RED / b05d55558 FIX: anonymous table rows/cells (reconciler; builder agrees; DOM-less cells).
  -> mailgun x3 (80 missing boxes).
- 1624bc60d RED / 8502418d9 FIX: atomic inline contributions (max-width clamp in stored intrinsic sizes,
  margin-box shapes, separate min-content scan). -> cerberus hybrid/responsive/fluid tables.
- bd750b08b RED / 2db332ee0 FIX: cell vertical-align counts last child's bottom margin. -> postmark.
- 303d81844 RED / 22661a53e FIX: <br> ends a line of the intrinsic measurement (sizing). -> 04_receipt.
- 73195eff6 RED / 963ba093c FIX: a cell's inline-blocks get the positions their line gave them
  (layout_cell_for_height publishes; vertical-align shift moves them). -> postmark button labels.
- b998b2bb0 RED / 03ba7769c FIX: whitespace beside a table's inline children kept
  (layout_tree::table_relevant_children, both builders); WPT whitespace-001, anonymous-table-ws-001 out.
- 5e81932cd RED / 66af7e730 FIX: table baseline of a row w/o baseline cells; caption's own caption-side.

## IN PROGRESS
- more feature probes (height attr, align attr floats, nowrap, % nested).

## NEXT
1. Look at 01_newsletter azr-31 and leemunroe azr-18 (table width too narrow), 04_receipt azr-6.
2. WPT expectations (tests/wpt/reftest_expectations.txt, css/CSS2/tables) - anything my fixes should flip.
3. Report scripts/TABLES_2026_10_02.md.

## Decisions
- F23 test uses two-word cells (font-robust "exactly two baselines").
- E-INLINE's font mechanism is not fixed blind (cannot run; non-table engine area); handed off.
- Atomic-inline intrinsic contributions (sizing.rs) fixed here although near TEXTENG's area: it is what
  sized the cerberus tables; TEXTENG's brief is "inline-blocks in spans" (layout), not the measurement.

## Notes for others
- MAILHTML: `max-height` is ignored on auto-height blocks (probe: `max-height: 0; overflow: hidden`
  preheader 18.4px tall, Chrome 0) - the cerberus preheader, +97px for every box after it.
- TEXTENG: line-height normal 18.4px vs Chrome 18px (16px Times/Arial; Chrome rounds ascent, descent,
  lineGap separately); Helvetica 16px lines 16 vs 18 (postmark); no strut on lines of only atomic inlines.
