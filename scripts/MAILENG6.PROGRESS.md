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

## DONE (items 3/7 so far)
- RED `b3f6847fb` (layout/tests/a_normal_line_is_as_tall_as_chromes.rs), GREEN 1 `789331579`
  (`LayoutFontMetrics::line_metrics_px` + field `browser_ascent_boost`; LineHeight::resolve*),
  GREEN 2 `c847b4b6c` (font.rs `browser_ascent_boost(family)` replaces `browser_compat_ascent`).
- GREEN 3a `38ed88fae` (glyph boxes via `inline_box_px`, `split_leading` floor-above, strut split
  in `position_one_line`), 3b `047b32859` (fc.rs strut = rounded A/D, root `normal` = Px(A+D+G)),
  3c `1bed4d66a` (dense.rs: 4 copies of the run ascent -> `resolved_run_ascent` -> `inline_box_px`).
  Items 3 + 7 DONE.
- Seen, not changed: `UnifiedLayout::first/last_baseline` report a cluster's RAW ascent
  (`baseline_scaled`, no line y, no leading) as the IFC baseline (fc.rs 4551/4800) - inline-block
  baseline alignment reads it; `editing_host_strut_height` (empty editable) still 1.2em for
  `normal`.
- item 4 VERIFIED on the prebuilt wave-5 AzPaint (probe m6/f3): `font: 15px Arial; line-height:
  20px` 3 lines = 60.0 (Chrome 60), an unset `<p>` after it is `normal` (18.4, now 18 with the
  rounding) - TEXTENG's wave-5 fixes hold; nothing to change.
- item 5: RED `af58b319e` + FIX `ecdbafdf0` (a cell of only inline boxes is an IFC; guarded
  exception for block-in-inline). td display:block (TABLES 3.1), receipt -8px (TABLES 3.3) are
  wave-5 fixes - the parent's corpus re-measure confirms.
- FOUND (not fixed, big): block-in-inline in an IFC drops the block: `<div style="text-align:
  center"><a><span style="display:block;width:100px;height:40px"></span></a></div>` -> div 19.2px,
  the span gets no box (Chrome: 40px, span at x 0). Same in a cell with whitespace text. CSS 2.2
  9.2.1.1 splitting of an inline around a block is not implemented (reconciler + layout_ifc).
- item 5 RTL collapse: RED `a7fda9cc9` + FIX `4b9f52dcf` (start/end sides by direction,
  `CollapsedBorders.rtl` for the cell/table half-borders). Item 5 DONE.
- item 6: RED `429d3580f` + FIX `b1e6c7409` (span children: inline-flex/grid/table atomic,
  `<img>` via the new ONE helper `push_inline_image`); RED `b52c5f07c` + FIX `0896d5138`
  (OS/2 sxHeight/sCapHeight parsed; strut x/cap from the face). Item 6 DONE.
- (power cut + restart ~here; scratchpad /tmp wiped: m6/probe2.py, chrome_metrics.* are gone)
- item 2: RED `ceb4169a6` (layout/tests/a_percentage_height_in_an_auto_height_block_is_auto.rs)
  + FIX `188afe95a`: cache.rs `forwards_containing_block_height` (the ONE decision, used by
  `prepare_layout_context`; `layout_bfc` reads it back via constraints.available_size) +
  `is_percentage_height` (a % height against an indefinite cb is content-sized). All 48 app /
  widget `height: N%` sites checked (Explore agent): none affected. Caveat: CSD-wrapped windows
  (injected auto-height <html>): `body(no height) > div{height:100%}` now content-sized.
- IN PROGRESS: an Explore agent classifies the ~36 test files with % heights; NEXT = update the
  tests whose old expectations relied on the forwarded window height (commit per file).

## (older notes, items 3/7 plan)
- item 3 + 7 (Chrome's rounded font metrics + the Times/Helvetica/Courier ascent hack).
  VERIFIED against Chrome 154 (m6/chrome_metrics.py, 11 Mac families x 13 sizes = 143 cases,
  0 mismatches): `line-height: normal` = A + D + G with A = round(hhea asc * s),
  D = round(-hhea desc * s), G = round(lineGap * s); on macOS for family EXACTLY Times /
  Helvetica / Courier then A += floor((A + D) * 0.15 + 0.5) (Blink AscentDescentWithHacks,
  IS_APPLE; applied AFTER rounding, so MAILHTML's font-unit `browser_compat_ascent` is wrong
  once metrics round: 16px Helvetica would give 19, Chrome 18). `line-height: 20px` x3 = 60
  in Chrome for every family/size.
  NEXT STEP (exact): (a) RED test in a new `layout/tests/a_normal_line_is_as_tall_as_chromes.rs`
  (Arial 16px block = 18px, 2 lines = 36; skip where Arial is missing) - append to all.rs;
  (b) add a bool field `ascent_compat_boost` to `LayoutFontMetrics` (text3/cache.rs:2825; 10
  files build the literal: font.rs, font_traits.rs, text3/{cache,dense,glyphs,default}.rs,
  solver3/{layout_tree,display_list}.rs, tests text3_dense_equivalence.rs, text3/mod.rs) set in
  font.rs from the name-table FAMILY (not the PostScript prefix) and only on macOS, and a
  helper `LayoutFontMetrics::line_metrics_px(font_size) -> (A, D, G)`; (c) use it in
  `LineHeight::resolve_with_metrics`, `get_item_vertical_metrics(_approx)` (glyph A/D), fc.rs
  strut (~5391); half-leading split above = floor((L - (A + D)) / 2) (LayoutNG
  CalculateLeadingSpace); (d) drop `browser_compat_ascent` from the parse (keep fn name? no:
  replace by the flag) and update its two lib tests in font.rs.

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
