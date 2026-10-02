# PDFFIX progress (branch wt/pdffix, base dfa3e14b8)

Task: (1) column-count on a block container (issue #481) - children must flow across columns;
(2) paged layout: content clipped by overflow:hidden must not create extra pages.

User ruling (mid-task, 2026-10-02): "if we need refactoring, do that first, please don't hack it
just to make the one layout work".

## DONE
- e4b8b3b7f progress file
- item 2: 7488a015b RED (display_list unit tests + layout/tests/content_clipped_by_an_overflow_hidden_box_adds_no_pages.rs,
  pagination_dom_breaks::paginate made pub(crate) for reuse), 06b3b986b GREEN
  (calculate_display_list_height walks the clip stack; intersect_rects helper, intersect_or delegates).
  Root cause NOT in page_breaks.rs (it calls display_list::calculate_display_list_height).
  Twins seen (report them): cpurender/pixmap.rs rect_intersection, cpurender/compositor.rs
  intersect_logical_rects, managers/focus_cursor.rs intersect, display_list.rs intersect_rects.

## IN PROGRESS
- item 1 design

## Findings so far (item 1)
- The child <p>s do NOT see column-count. `get_property_slow` (core/src/prop_cache.rs) walks
  inline -> css_props (rules matched to THIS node) -> global `*` props -> cascaded_props (only
  is_inheritable() types are copied there, ~L2000-2090) -> computed_values (gated on
  is_inheritable) -> UA. ColumnCount is not inheritable, so a <p> resolves None.
  DOM_HAS_COLUMN_COUNT (core/src/compact.rs:1985) is a DOM-wide "some node declared it" bit that
  only gates the lookup in translate_to_text3_constraints; it is never a value.
- The issue's numbers fit NO columns at all: with Helvetica AFM widths, the ALPHA paragraph at the
  full 340pt width breaks into 3 lines, the last "nineteen twenty twentyone twentytwo
  twentythree." puts "twentythree." at x=242.1 (reported 242), line 3 at y~71 (reported 71), and
  BETA at 42+3*14.4+6 = 91.2 (reported 91). Two 160pt columns would give 7 lines, last word alone
  at x=220 and BETA at ~105. So the paragraphs were single-column, full width; "242" was the last
  word of line 3, not column 2. Real root cause: layout_bfc has no multicol handling; columns only
  reach text3 for an IFC root, which the div (block children) is not -> column-count ignored.
  (scratchpad helv.py has the computation.)

## Item 1 design (decided)
- NOT the K30b fragmentainer/token path: it re-lays per fragmentainer and a node split across two
  columns would need two positions in ONE display list (the position model is one per node; the
  page loop gets away with it because every page is its own display list). Fragment passes also
  never split IFCs (v1).
- Instead (azul's own pagination model, "continuous layout, then break analysis"): the multicol
  BFC lays its children out ONCE as a single column of width W (Pass 1/2 unchanged, only the
  children's containing block = W), then `multicol::plan_columns` (pure, unit-tested) picks the
  column breaks (between siblings, or between the LINES of a plain IFC child) with column-fill:
  balance (binary search on the column height, capped by a definite container height; overflow
  columns continue in the inline direction), and the children move to (column x, y - column start).
- An IFC child that straddles a break is re-laid with `LayoutConstraints.column_flow` ->
  `UnifiedConstraints.column_flow: Option<text3::cache::ColumnFlow { breaks: line indices,
  advance, column_top }>`: text3 starts a new column at those line indices (index-based, exact).
  Its used_size becomes its first fragment. Children that establish a BFC / replaced / tables move
  whole.
- Files: new layout/src/solver3/multicol.rs (ColumnGeometry resolver shared with
  translate_to_text3_constraints = the refactor-first step; plan_columns), fc.rs (layout_bfc hook,
  establishes_new_bfc: multicol container establishes a BFC, LayoutConstraints.column_flow),
  text3/cache.rs (ColumnFlow), cache.rs promote_layout_roots_to_containers (a layout root inside a
  multicol flow is laid out by the multicol container, like flex items).
- Limits: horizontal writing modes only; not inside K30b fragment passes (constraints.fragmentainer
  Some -> no columns); orphans/widows not honoured in columns; a split box paints its
  background/border on its first fragment only.

- item 1: 0e275ee03 RED (layout/tests/a_multicol_block_flows_its_children_through_its_columns.rs),
  41e7b532c refactor (solver3/multicol.rs: column_style + ColumnStyle::geometry + column_x/advance;
  translate_to_text3_constraints uses it)

- e66fddccb text3 ColumnFlow + LayoutConstraints.column_flow plumbing (None everywhere)
- 0aa475d5c guard test (loose text / anonymous box), 81a24e9d0 fix: anonymous IFC root takes no
  columns from its container (translate_to_text3_constraints gets `anonymous`)

- 4ca187646 multicol::plan_columns + unit tests
- 7f6d11759 establishes_new_bfc: multicol container -> BFC (is_multicol_container)
- 26129366b GREEN: layout_bfc multicol (block_columns, distribute_into_columns,
  splittable_lines, split_into_columns in fc.rs), promotion (cache.rs lift_to_slot_container +
  multicol lift; mod.rs caller), multicol::is_multicol_box

- 60ccc01c2 RED + cf4ab81b2 fix: paged TextLayout of a split paragraph (lines above its box)
  keeps them inside its bounds (display_list.rs paint_inline_content, `lines_above`)

- report scripts/PDFFIX_2026_10_02.md written

## NEXT
- nothing: task complete; the parent compiles and runs the commands in the report.
- NOTE: commit messages ALWAYS via -F file (backticks in -m got shell-evaluated once).

## Open questions / decisions
