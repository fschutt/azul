# TABLES (wave 5, 2026-10-02) - progress

Branch `wt/tables` from `2e92c759b`. Brief: scratchpad/wave5/TABLES.md. Report: scripts/TABLES_2026_10_02.md.

## Tools (scratchpad/tables/)
- `probe.py FILE.json` - Chrome vs prebuilt azul (AzPaint debug server, capped, fresh app per case) rects
  of id'd elements. `shot.py`, `tree.py` - screenshot / layout tree / display list of a mounted page.
- The prebuilt layout test binary `target/release/deps/all-978bc4fd9634d150` (base engine) runs existing
  tests through run_capped.sh: `... all-978bc4fd9634d150 table --test-threads 2` -> 166 passed at base.
- CAUTION: the debug server reports inline-block positions inside an IFC wrongly (all at the line start);
  use display-list rects for them.

## DONE
- f120ecc14 F23: prose_cells_wrap_inside_a_220px_table restored per cell (2 baselines each, shared,
  3.9.3 widths from measured min/max; two-word cells so it holds for any face). Engine already right
  (probe: azul 131.8/82.2 vs Chrome 131.5/82.5 Times; Arial identical).

- e4a6554a5 F24 RED (ignored, OPEN): text_beside_an_italic_or_bold_box_keeps_a_font (font bug, handed off).
- d4dc39299 F24: width-cap tests back on fixed-size boxes (`words()` = non-italic span boxes).
- c6530d7e4 RED a_rows_stray_child_sits_in_an_anonymous_cell (mailgun container td display:block).
- (this) FIX anonymous table objects in cache::reconcile_recursive (reconcile_table_children),
  builder agrees (process_anonymous_table_box_children, anon cell FC TableCell),
  fc::layout_cell_for_height takes DOM-less cells.

## IN PROGRESS
- F24 / E-INLINE: root cause is NOT table or intrinsic-sizing code. Probe matrix (prebuilt engine):
  - `<td><i ib/> <i ib/> <i ib/></td>` in a `width:1px` table: 306 wide (Chrome 106); spaces not painted.
  - same with `<span>` boxes (no italic): 106 = Chrome; the space paints (text item, 1 glyph).
  - `<span style="font-style: italic">` boxes: broken again; `<i style="font-style: normal">`: fine.
  - adding `<p>x</p>` (regular text) anywhere: fixed; adding bold or italic text: still broken;
    adding Courier New regular text: fixed.
  - same family of bug: `<p><b>A</b> tail</p>` / `<p><i>A</i> tail</p>` lose " tail" entirely unless
    the document has other regular text; `<p><span>A</span> tail</p>` is fine.
  => a text run next to an element of another font-style/weight is dropped (no face for it) unless
     some other regular text loads one: font collection / chain resolution
     (getters.rs collect_font_stacks_from_styled_dom / resolve_font_chains_fast, text3 shaping), owner:
     text/font engine. Mechanism not pinned without a run.
- Decision: RED tests for the font bug (ignored, OPEN, exact repro); the two TABLE_A tests restored to
  their original intent (fixed-size boxes, font-independent) with non-italic `<span>` boxes.

## NEXT
1. Newsletters end to end: cerberus (first diverging: div azr-1 height), postmark, 04_receipt, 02_gmail.
2. Newsletters end to end (mail corpus, boxes.json in main checkout target/refci/mail-baseline-2e92c759b).

## Decisions
- F23 test uses two-word cells (font-robust "exactly two baselines").
- E-INLINE's font mechanism is not fixed blind (cannot run; non-table engine area); handed off.

## Notes for others
- line-height normal: azul 18.4px vs Chrome 18px for 16px Times/Arial (Chrome rounds ascent, descent,
  lineGap separately) - every text line 0.4px taller; owner TEXTENG.
- lines of only atomic inlines get no strut in azul (Chrome: 18px line for a 10px inline-block).
