# LAYOUT7 progress (wave 7, 2026-10-03)

Branch `wt/layout7` from `2e55eef06`. Brief: `scripts/waves/wave7/LAYOUT7.md`. Never compile.
Probe (not committed, worktree `target/layout7/probe.py <cases.json>`): lays a body snippet out in
headless Chrome and the prebuilt azul (AzPaint `mount`, capped runner), prints rects per id.

## DONE
- item 1: RED 21853ac30 (layout/tests/an_absolutely_positioned_child_does_not_split_its_parents_line.rs);
  GREEN 92877ff2e (mask, fresh tree), fbef5dcd7 (mask, reconciler), 87cc1c6ae (no marker box for none,
  item's own type), 116feca4d (marker rides the first line: fc::marker_line_host / markers_on_first_line /
  is_marker_on_a_line, layout_bfc filter), 3eea318ee (sizing + process_out_of_flow_children skip).
- item 2: RED e861edfae (layout/tests/a_block_inside_an_inline_splits_the_inline_around_it.rs);
  GREEN 3995cb27f (layout_tree::inline_holds_a_block / split_inlines_around_blocks, used by the fresh
  tree, the reconciler, has_only_inline_children; fc.rs twin inline_children_hold_a_block deleted).
  Not built: the split inline's fragment boxes (background/border/padding of the inline beside the block).
- items 3+4: RED a7f933552 (a_border_box_min_width_bounds_the_border_box.rs, a_fit_content_width_shrinks_to_its_content.rs);
  GREEN d6016967a (border-box min/max clamp on auto widths + intrinsic min-width floor), cc44b5c48
  (fit-content keyword parses as fit-content(100%), argument = available space, content size; IFC root of
  only atomics measured; intrinsic-keyword boxes are STF).
- item 5: RED ff7f62126 (an_anonymous_table_cell_keeps_its_blocks_margins.rs); GREEN dd3412ea4
  (establishes_new_bfc: FormattingContext::TableCell, anonymous cells included).
- item 7: RED a4746426a (core diff_test a_leaf_does_not_follow_its_subtree_into_a_container_with_another_id;
  the layout RED test now installs pages via begin_reconciliation + layout_new_generation +
  finish_reconciliation like the shells); GREEN 3ccc3c8c5 (core/src/diff.rs A2 gate = nearest terminal
  ancestor). Touched core (unowned this wave).
- item 8: VERIFIED already fixed (TEXTENG wave 5: measure_atomic_inline in the span arm; pinned by
  layout/tests/an_inline_block_inside_a_span_is_sized_by_its_own_css.rs). Probe: px / % / auto widths
  in spans = Chrome. Ledger item stale. No change.
- item 10: RED a7d099bec (a_block_taller_than_a_page_is_split_across_pages.rs); GREEN 035ee5157
  (StructuralBreak::line_path, spine_line_split_at_y, child_index_path; PaginationSnapshot::
  break_line_path / break_line_start_run / break_line_start_byte - api.json additions). Engine splits
  (paragraph between lines; monolith sliced). Consumer change for WIDGETS7 (page_doms with
  (block, run, byte) starts) / OFFICE7 (starts_from_breaks) - report it.
- item 9: VERIFIED already done: f120ecc14 (TABLES wave 5) restored the per-cell 2-baseline + 3.9.3
  assertion; the inline-block min-content bug 56b105f60 dodged was the font-stack off-by-one
  (MAILENG6 2b5bae827), pinned by text_beside_an_italic_or_bold_box_keeps_a_font; probe: table of
  `<i>` boxes 106 = Chrome, narrow prose table columns 131.8/82.2 vs Chrome 131.6/82.5. No change.
  Seen: lines of only atomic inlines get no strut (Chrome 18px line around a 10px box, azul 10) -
  candidate extra item (fc.rs strut).

## NOTES item 1 (done)
- item 1: abspos child treated as in-flow + ::marker with list-style-type none.
  Measured (Chrome / azul prebuilt, 16px Arial, line-height 20):
  - check item `li{list-style:none;position:relative}` text + abspos div: li 20 / 40
  - `<div>one<div abs/> two</div>`: 20 / 40 (abspos block splits the line)
  - `<li>Item<div>block</div></li>`: li 40 / 60; `<li><p>a</p><p>b</p></li>`: 40 / 60;
    `<li><div><p>a</p></div></li>`: 20 / 40; `<li><div h50/></li>`: 50 / 70; empty li 20 / 20.
  Root causes found:
  (a) `layout_tree::is_block_level` counts an abspos/fixed block as a block child: mixed content,
      anonymous wrappers split the line (builder `process_block_children`,
      `process_anonymous_table_box_children`; reconciler cache.rs `reconcile_recursive` ~2182/2298,
      `reconcile_table_children` ~1682, `layout_relevant_child_count` ~1252) and
      `has_only_inline_children` says "not an IFC".
  (b) the `::marker` pseudo node (dom id = the li) of a BFC list item is laid out by `layout_bfc` as an
      in-flow IFC; `collect_and_measure_inline_content_impl` then walks the LI's DOM children (its
      loose text) -> extra line, text twice. Also "Case 2" puts a marker on EVERY IFC whose parent is
      a list item (`<li><p>a</p><p>b</p></li>` two markers), and the anonymous wrapper never gets one.
  (c) `generate_list_marker_text` returns " " for list-style-type none (format! adds a space), the
      marker box is created anyway; it reads the list CONTAINER's type before the item's own
      (inherited) value.
  Plan: one mask `in_flow_block_level_mask` (layout_tree.rs) used by every split site; markers ride
  the IFC of the item's first line box (`marker_line_host` in fc.rs), layout_bfc / intrinsic sizing
  skip a marker that has a host, a marker IFC collects only its marker; no marker box for none.

## DONE (item 6)
- item 6 (CSS zoom). DESIGN (decided): zoom = a multiplier on the subtree's used lengths (Chrome's
  model: effective zoom = product of `zoom` over the node and its ancestors; absolute lengths (px pt
  in cm mm) and rem scale, em follows the zoomed font size, % / vw / vh do not). Steps, one commit each:
  Z1 RED layout/tests/css_zoom_scales_the_lengths_of_its_subtree.rs; Z2 the property (css: StyleZoom
  in css/src/props/style/effects.rs as {inner: PercentageValue}, property.rs ~25 spots copying
  Opacity's, props/macros.rs css_property_from_type, codegen format.rs impl_percentage_value_fmt +
  lower_types.rs lists, core prop_cache impl_get_prop get_zoom); Z3 core CssPropertyCache
  `resolved_zooms: OnceLock<Vec<f32>>` (cleared with resolved_font_sizes_px) + getters
  `get_effective_zoom` + `zoom_length` helper; Z4 get_element_font_size x zoom; Z5 BoxProps
  (ResolutionParams.zoom, UnresolvedBoxProps::resolve); Z6 sizing (Px width/height arms, min/max
  constraints, intrinsic overrides, image natural size); Z7 StyleProperties line-height / letter /
  word spacing px; Z8 positioned offsets. Not in reach (PAINT7 display_list): radius, shadow, outline.
  DONE: Z1 541da3fe8, Z2 f2a1095d8 + 40b1ab2a3, Z3+Z4 35609cc69, Z5 aa4a3f9e1, Z6 3cf0f66c5.
  Z7 42e291414, Z8 aa2ddf96f. ITEM 6 DONE. Left (PAINT7 / later): border-radius, box-shadow, outline,
  text-decoration thickness, background size/position px, calc() px terms, gaps / flex-basis in taffy.
  Note for OFFICE7: AzMail can now put `zoom: <n>` on the reading pane / paper.

## DONE (extras)
- (11) text-indent: RED 5a0c781f7, GREEN 64d3cb633. (12) vertical-align vw/vh/rem: RED ca60d66fb, GREEN 5ede221e7
  (get_vertical_align_for_node gained a `viewport: PhysicalSize` arg - Rust-only pub).

## DONE (end)
- (13) strut of atomic-only lines: NOT changed - text3 measures such lines by their items on purpose
  (cache.rs ~11570); reported for TEXT7.
- Report: scripts/LAYOUT7_2026_10_03.md.

## IN PROGRESS
- (none)

## NEXT
- (none - the parent compiles and runs the suites; see the report)
- extra (from TEXT7 via coordinator, RED first if room, else report): (11) text in an ANONYMOUS block
  after a nested block is text-indented although CSS 2.1 s16.1 indents only the first formatted line of
  the block container (fc.rs); (12) `vertical-align` in vw / vh has no viewport to resolve against.
  TEXT7 edited getters.rs (one text-indent resolver), fc.rs + sizing.rs (min/max-content count
  text-indent; one font-weight reader): keep edits local to the functions changed, do not re-implement.

## Decisions / open questions
- no-host marker (`<li><div h50/></li>`, Chrome 50 azul 70): kept as today (own line); note in report.
- floats still split inline runs (azul's IFC has no in-line float placement) - unchanged.
