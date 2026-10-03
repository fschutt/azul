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

## IN PROGRESS
- item 1 leftover (decided, not yet done): the WPT runner `layout/tests/wpt/reftest.rs` `render_xml`
  uses core's `parse_xml` + `dom_from_parsed_xml` (core/src/xml.rs `str_to_dom_unstyled` DROPS the
  `<html>` element's attributes - `<html style="background:green">` in
  background-color-body-propagation-ref is lost). Plan: switch render_xml to
  `azul_layout::xml::parse_xml_to_styled_dom(xml)` (the document loader azul-doc reftest and the debug
  `mount` use) and update the module doc; report the core loader bug to XML8 (its file), do not edit
  core/src/xml.rs.

## NEXT
- item 2: inline boxes' margin/border/padding/background (WPT css/CSS2/linebox inline-formatting-context-002/
  004/006, empty-inline-001/003, split-inline-borders) and inline-block line breaking/baseline
  (css/CSS2/visudet inline-block-baseline-001/002/005/015). First read what wave 7 already fixed
  (3995cb27f inline box holding a block; fac8d1c5e texteng inline-blocks in spans), then the inline
  paint path: getters.rs ~3530 (inline bg/border info into glyph runs) and display_list.rs
  `push_inline_backgrounds_and_border` callers. RED test in layout/tests using crate::painted.

## Decisions / open questions
- Tests use the document loader (`parse_xml_to_styled_dom`), which keeps `<html>` attributes.
- The canvas ignores the root's border-radius (as the colour path always did).
- background-size / background-position are still not honoured anywhere (only repeat, only on the
  canvas); noted for E-GRAD tiled-gradients.
