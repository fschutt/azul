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

## IN PROGRESS
- item 7: AzMail wizard page 2 (RED cc7040ae5) - read the test, root-cause
- DECISION: item 6 (CSS zoom) moved after items 7-10: a new CssProperty touches property.rs (~30 spots),
  css codegen (format.rs, lower_types.rs ~9 lists), core prop_cache, 30 codegen golden files, and the
  used-length effect needs ~140 resolution sites or a paint transform in core/gpu.rs + display_list
  (PAINT7). Bounded bug fixes first.

## NEXT
- items 6..10 in brief order
- extra (from TEXT7 via coordinator, RED first if room, else report): (11) text in an ANONYMOUS block
  after a nested block is text-indented although CSS 2.1 s16.1 indents only the first formatted line of
  the block container (fc.rs); (12) `vertical-align` in vw / vh has no viewport to resolve against.
  TEXT7 edited getters.rs (one text-indent resolver), fc.rs + sizing.rs (min/max-content count
  text-indent; one font-weight reader): keep edits local to the functions changed, do not re-implement.

## Decisions / open questions
- no-host marker (`<li><div h50/></li>`, Chrome 50 azul 70): kept as today (own line); note in report.
- floats still split inline runs (azul's IFC has no in-line float placement) - unchanged.
