# WPT8 progress (wave 8, branch wt/wpt8, base 45c6bf98b)

Brief: scripts/waves/wave8/PLAN.md section "WPT8". Items in order:
1. body background not propagated to the canvas
2. inline boxes paint no margin/border/background; inline-block breaks its line
3. counters / `inline list-item`
4. single-stop gradients
5. background-clip
6. border-width keywords
7. box-shadow
8. reftest budget: per-page fuzzy budget in doc/src/reftest/pipeline.rs

## DONE
- 23e4f686a progress file
- item 1 (canvas background):
  - d23a3ea2f RED `layout/tests/the_root_background_covers_the_whole_canvas.rs` (4 tests) + the shared
    helper `layout/tests/common/painted.rs` (`painted(doc, w, h) -> Painted { rgb, is, count }`,
    document loader `parse_xml_to_styled_dom`, CPU render). Both registered at the END of layout/tests/all.rs.
  - 524ec0c8b getters.rs: `body_background_propagated_to` (one helper; an explicit transparent root
    propagates), `own_background_layers`, `background_layers_paint_nothing`; get_background_color /
    background_contents_as_declared rewritten on it.
  - 81ab4bd6e display_list.rs: `background_tiles` (free fn near the top), builder
    `push_background_layer` (the one layer->item match, used by box + inline painters) and
    `push_background_layer_tiled`; step 0 of generate_display_list_impl paints every root layer on the
    canvas (tile = root paint rect, repeat from `get_background_repeats` in getters.rs);
    generator field `canvas_painted: [Option<NodeId>; 2]` -> paint_node_background_and_border_inner
    skips the background of the root and the propagating body.

- a3c2c5027 WPT runner `render_xml` uses `parse_xml_to_styled_dom` (core's tree loader drops `<html>`
  attributes - XML8's file core/src/xml.rs `str_to_dom_unstyled`, REPORT it, not edited)

## IN PROGRESS - item 2 (inline boxes), ROOT CAUSES FOUND by probing the prebuilt AzPaint
(probe tool: /tmp/wpt8/probe.py + pages in /tmp/wpt8/pages, may be gone after a reboot)
- (a) a span's border/padding vanish (no paint, no pen shift) unless it also has a background or a
  text-decoration: text3 stage-1 cache key `calculate_id(&content)` (text3/cache.rs ~8187) hashes
  StyleProperties via its `Hash` impl (cache.rs ~4853) which OMITS border (InlineBorderInfo) and
  background_content. The intrinsic-sizing collector (solver3/sizing.rs
  `collect_inline_content_recursive`) styles span text with the TEXT node's own style (no border); its
  logical items are cached first and reused by the final layout (same hash). Fix: hash border +
  background_content in `impl Hash for StyleProperties`; layout_hash must include the inline insets
  (they move the pen: inline_offsets in position_one_line ~12590).
- (b) a span's background is painted TWICE (display_list `push_inline_backgrounds_and_border`: the
  `background_color` rect AND the Color layer of `background_content`) - translucent doubles.
- (c) inline margins are never applied (InlineBorderInfo has no margin; inline_offsets = border+padding).
- (d) sizing collector should give text inside an inline span the span's style (like fc.rs CASE 1).
- (e) nested inline chrome / an outer inline's bg spanning an inner one's margin
  (inline-formatting-context-002) needs per-element inline fragments - bigger; maybe left.
- (f) inline-block-baseline-001: `span {display:inline-block; overflow:visible}` with text sits ABOVE
  the line (own line) - not yet investigated.

## NEXT
- RED test file layout/tests/an_inline_box_paints_its_border_padding_and_margin.rs (crate::painted):
  border-only span draws blue pixels; margin+border+padding push a following red inline-block by 55px vs
  a chrome-less twin; a span's margin is not covered by its background; translucent span bg painted once.
  Then GREEN (a), (b), (c), (d) in that order, one commit each.

## Decisions / open questions
- Tests use the document loader (`parse_xml_to_styled_dom`), which keeps `<html>` attributes.
- The canvas ignores the root's border-radius (as the colour path always did).
- background-size / background-position are still not honoured anywhere (only repeat, only on the
  canvas); noted for E-GRAD tiled-gradients.
