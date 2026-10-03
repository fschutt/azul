# LAYOUT7 progress (wave 7, 2026-10-03)

Branch `wt/layout7` from `2e55eef06`. Brief: `scripts/waves/wave7/LAYOUT7.md`. Never compile.
Probe (not committed, worktree `target/layout7/probe.py <cases.json>`): lays a body snippet out in
headless Chrome and the prebuilt azul (AzPaint `mount`, capped runner), prints rects per id.

## DONE
- (none yet)

## IN PROGRESS
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

## NEXT
- item 1: RED test file layout/tests/an_absolutely_positioned_child_does_not_split_its_parents_line.rs
- items 2..10 in brief order

## Decisions / open questions
- no-host marker (`<li><div h50/></li>`, Chrome 50 azul 70): kept as today (own line); note in report.
- floats still split inline runs (azul's IFC has no in-line float placement) - unchanged.
