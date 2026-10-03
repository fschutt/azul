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

- item 2 (inline boxes):
  - 6bfe5ad17 RED layout/tests/an_inline_box_paints_its_border_padding_and_margin.rs (6 tests)
  - 753dbc393 text3 StyleProperties Hash covers border/padding/bg-content (stage-1 cache key; the sizing
    pass's border-less items were reused) + layout_hash covers horizontal advances; InlineBorderInfo
    margin_left/right + left_advance/right_advance/moves_the_pen; get_inline_border_info reads margins
  - 09e2d9d02 span background painted once (push_inline_backgrounds_and_border)
  - 6b55fa613 sizing.rs `text_run_style`: span text carries the span style in intrinsic sizing
    (MAILREF8 owns sizing.rs - minimal edit, say so in the report)
  - 83164df5e RED layout/tests/an_inline_block_sits_on_its_last_lines_baseline.rs (2 tests)
  - 90b7b1d8d text3 `baseline_in_layout`: first/last_baseline = item top + ascent (was ascent only)
  - 418ddb4c7 fc.rs `last_line_box_baseline`: layout_bfc reports its last line box baseline
  - LEFT for item 2: nested inline chrome / outer inline bg over an inner one's margin
    (inline-formatting-context-002/006 need per-element inline fragments); line breaking ignores
    inline insets; a span's several text children get insets each (Arc per text node in fc.rs CASE 1).

## IN PROGRESS
- item 3: counters / `inline list-item` (WPT css/CSS2/lists counter-*, css/css-lists counter-list-item,
  inline-list, li-list-item-counter-*)

## NEXT
- item 3 RED, then 4 single-stop gradients, 5 background-clip, 6 border-width keywords, 7 box-shadow,
  8 reftest budget (doc/src/reftest/pipeline.rs), then the report scripts/WPT8_2026_10_03.md.

## Decisions / open questions
- Tests use the document loader (`parse_xml_to_styled_dom`), which keeps `<html>` attributes.
- The canvas ignores the root's border-radius (as the colour path always did).
- background-size / background-position are still not honoured anywhere (only repeat, only on the
  canvas); noted for E-GRAD tiled-gradients.
