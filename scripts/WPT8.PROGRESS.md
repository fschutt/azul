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

- item 3 (counters) PARTIAL:
  - 79da84eb6 RED + d85dbf66f fix: reversed list without start counts down into its first <li value>
    (cache.rs reversed_list_start)
  - FOUND: `::before` / `::after` generated content does not exist at all (parser2.rs
    pseudo_selector_from_str rejects "before"/"after" -> the whole rule is dropped; nothing creates
    NodeType::Before/After; css Content is "parsed but not consumed"). Every WPT counter test uses
    `::before { content: counter(..) }`. Deferred to the end (a feature: selector -> pseudo node ->
    cascade -> generated inline box -> content eval with counters).
  - `display: inline list-item` does not parse (needs LayoutDisplay::InlineListItem = api.json) - deferred.
- item 4: b9121f729 RED + 19fd42e92 fix: single-stop gradients (css background.rs normalizers)

- item 6 (border-width keywords -> really the border initial values; keywords already parsed):
  5bdbd02cf RED, 2ecd332e0 fix: getters `get_border_info` = used values over `declared_border_info`
  (`used_border_width`: none/hidden 0, declared, else medium; currentcolor via new
  `get_used_text_color` (extracted from get_style_properties); compact "no colour" vs transparent
  resolved through the cascade); layout_tree box props + taffy_bridge use the same rule; css
  StyleBorderSide.color_given -> shorthand without colour resets to Initial.
- item 7 (box-shadow): 00a7df789 RED, 26fe720a8 fix: `paint_box_decorations` (one painter for block
  boxes and inline-blocks: outer shadows, bg layers, inset shadows on the padding box, border);
  raster render_box_shadow casts inset shadows; exact ring hole on the pixel grid.

- item 8 (reftest budget): d90dfadfb RED (stub + 3 unit tests in doc/src/reftest/pipeline.rs),
  40643b675 fix: `pass_threshold_for` (wpt-* 2500 px or its fuzzy meta; others global), pipeline +
  debug.rs use it; `meta_content` reads one meta tag.
- item 5 (background-clip): f11afdf70 RED, 84d404d2b feat: new CSS property BackgroundClip (css type +
  parser + every CssProperty arm + codegen tables + core get_background_clip); getters
  `get_background_clip`; display_list `paint_box_decorations` clips bg layers; `inset_rect` /
  `inset_radius` free fns. API.JSON: StyleBackgroundClip, StyleBackgroundClipValue,
  CssProperty::BackgroundClip, CssPropertyType::BackgroundClip (+ consts) - list in report.

- 32245314b report scripts/WPT8_2026_10_03.md (api.json list, least-sure spots, test commands,
  expected reftest flips, what is left incl. the generated-content design).

## IN PROGRESS
- nothing. All 8 brief items handled (item 3 partial: ::before/::after generated content and
  `inline list-item` documented as features with a design, not built).

## NEXT
- (only if resumed with more time) background-size/position/origin painting, or generated content
  as its own task. Otherwise done.

## Decisions / open questions
- Generated content NOT built here: it needs DOM pseudo nodes before the cascade (core styled_dom /
  loaders = XML8 + parent territory); design in the report sec. 7.
- WPT expectations NOT edited blind: the parent blesses with AZ_WPT_BLESS=1 after the build.
- Tests use the document loader (`parse_xml_to_styled_dom`), which keeps `<html>` attributes.
- The canvas ignores the root's border-radius (as the colour path always did).
- background-size / background-position are still not honoured anywhere (only repeat, only on the
  canvas); noted for E-GRAD tiled-gradients.
