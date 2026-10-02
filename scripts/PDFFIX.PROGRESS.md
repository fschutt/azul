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

## NEXT
- design multicol for block containers (refactor first: one column-geometry helper shared by the
  IFC path and the BFC path)

## Open questions / decisions
