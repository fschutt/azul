# MAILENG6 progress (wave 6, 2026-10-03)

Branch `wt/maileng6` from `25d78e309`. Brief: scripts/waves/wave6/MAILENG6.md.

Probe tooling (scratchpad, not committed): `m6/probe2.py <cases.json> <logdir>` mounts each case
in a FRESH prebuilt AzPaint (headless, debug server) under run_capped.sh and prints node rects,
display-list text items and computed props. NOTE: the scratchpad root is shared with other
agents - keep files under `m6/` (a `msg.txt` at the root got overwritten).

## DONE
- item 1 (the font bug): RED `291873c42` (getters.rs `text_node_font_stack_tests` + TABLES'
  three ignored tests un-ignored), FIX `2b5bae827`. Root cause: `NodeId::from_usize(i)` (the
  1-based FFI decoder) on a 0-based index in `collect_font_stacks_from_styled_dom` - every
  text node took the font of the node before it.

## IN PROGRESS
- item 3 + 7 (Chrome's rounded font metrics + the Times/Helvetica/Courier ascent hack):
  NEXT: RED test, then one helper on `LayoutFontMetrics` (Blink rounding), used by
  `LineHeight::resolve*`, `get_item_vertical_metrics*`, the fc.rs strut.

## NEXT
- item 4 (verify line-height 20px pitch / unset lines `normal` on the prebuilt - probe)
- item 5 tables (td display:block, span-only cells text-align, receipt -8px, RTL collapse)
- item 6 (inline-flex/grid/table + img in spans; strut x/cap height from OS/2)
- item 2 (percentage heights vs auto-height parents; grep list in m6/h100.txt: 37 sites)

## Decisions
- Item 1: rewrote Phase 1's key to (family hash, FcWeight as u16, FontStyle as u8) read from the
  text node's own getters (one encoding) instead of patching only the decoder.
- Not fixed (report): a shaping result with a font deficit is cached in `per_item_shaped`
  (text3/cache.rs ~9230) - the text stays invisible after its font loads; `bolder`/`lighter`
  map statically to 900/300 (CSS Fonts 4: relative to the parent - `<b>` should be 700);
  dll shell2 common/layout.rs:1555 has the same `from_usize(i)` off-by-one.
- Disk: the machine ran out of space at ~02:45 (other agents); 286 MB free after. Keep probes small.
