# Reftest Status Report & Next Session Plan

**Date**: 2026-03-28 (updated session 5)
**Score**: 23/44 passing (started at 17/44, session 1: 21/44, session 2: 22/44, session 3-4: 22/44, session 5: 23/44)
**Branch**: layout-debug-clean

---

## Session 5 Fixes

### FIXED: white-space:nowrap not enforced in break_one_line()
**File**: `layout/src/text3/cache.rs` (line ~7299)
- **Root cause**: `break_one_line()` received `white_space_mode` but NEVER checked for `Nowrap`/`Pre` to prevent line breaking. All content was always broken at soft wrap opportunities.
- **Fix**: Added a `no_wrap` check at the top of `break_one_line()`. When `white_space_mode` is `Nowrap` or `Pre`, all items are consumed onto a single line without breaking (only explicit `<br>` breaks honored).
- **Result**: text-overflow-001: 56730→13171 (-43559). Both nowrap boxes now render as single-line clipped text.

### FIXED: inline-block intrinsic sizes double-counted box-model extras
**File**: `layout/src/solver3/sizing.rs` (line ~435)
- **Root cause**: InlineBlock intrinsic sizing added margin+padding+border to min/max content widths (`h_extras`). But `calculate_used_size_for_node` also adds padding+border when converting to border-box, and the parent's `calculate_block_intrinsic_sizes` adds margin+padding+border as `cross_extras`. This caused triple-counting of box-model dimensions.
- **Fix**: Removed the h_extras/v_extras addition from InlineBlock intrinsic sizing. The content-level intrinsic sizes are now correct, and callers add box-model extras as needed.
- **Result**: inline-block-text-001: 23485→19128 (-4357). "Auto A/B/C" boxes now match Chrome's compact sizing.

### FIXED: table intrinsic sizing was a stub returning zeros
**File**: `layout/src/solver3/sizing.rs` (line ~920)
- **Root cause**: `calculate_table_intrinsic_sizes()` was a stub returning `IntrinsicSizes::default()` (all zeros). This caused table elements with `width: auto` to get zero width, which made column width distribution compute zero-width columns.
- **Fix**: Implemented proper table intrinsic sizing: walk table structure (row groups → rows → cells), measure cell content via IFC intrinsic sizing, aggregate per-column min/max widths, sum for table totals.
- **Result**: cascade-ua-defaults-001: 43430→43371 (-59). TABLE text now renders horizontally instead of vertically (one char per line). Small diff reduction because main diff is from heading font metrics.

### FIXED: Vec::insert corruption in position_out_of_flow_elements
**File**: `layout/src/solver3/positioning.rs` (line ~594)
- **Root cause**: `calculated_positions.insert(node_index, final_pos)` used `Vec::insert()` which SHIFT-INSERTS, pushing all subsequent positions to the right. This corrupted positions of all nodes after the first absolute/fixed element. For example, node 6 (second static-box) was written correctly to (40,240) but then shifted to index 7, while index 6 became (20,140) from the shifted data.
- **Fix**: Replace `calculated_positions.insert(node_index, final_pos)` with `super::pos_set(calculated_positions, node_index, final_pos)` which correctly indexes into the vec.
- **Result**: block-positioning-complex-001: 32800→0 (new PASS! 22→23/44)

---

## Session 5 — Bug Analysis & Fix Plan

### Current Failing Tests (all 22, sorted by diff)

| Diff | Test | Root Cause |
|-----:|------|------------|
| 90182 | block-margin-collapse-complex-001 | margin collapse height overshoot |
| 67254 | cascade-display-block-001 | font metrics / cascade |
| 56730 | text-overflow-001 | **white-space:nowrap not enforced in break_one_line()** |
| 49045 | cascade-multiple-classes-001 | font metrics |
| 46300 | cascade-font-weight-inherit-001 | font metrics |
| 45519 | cascade-specificity-001 | font metrics |
| 43430 | cascade-ua-defaults-001 | **table column widths = 0 (vertical text)** |
| 42760 | cascade-nested-selectors-001 | font metrics |
| 41659 | cascade-inheritance-001 | font metrics |
| 40946 | block-padding-border-001 | block padding/border |
| 40691 | absolute-non-replaced-height-001 | abs-pos height |
| 37919 | cascade-inline-style-001 | font metrics |
| 37385 | cascade-global-star-001 | font metrics (color fixed s4) |
| 32800 | block-positioning-complex-001 | **paint coordinate offset in stacking ctx** |
| 23485 | inline-block-text-001 | **shrink-to-fit uses wrong containing block** |
| 22739 | font-properties-001 | font weight / remaining metrics |
| 16080 | text-basic-001 | font metrics |
| 15607 | table-basic-001 | **table cell height / text position** |
| 15132 | inline-background-001 | inline padding (partially fixed s4) |
| 13238 | display-none-visibility-001 | font metrics only |
| 12856 | inline-elements-001 | **minor inline spacing** |
| 10386 | inline-block-text-002 | font metrics |

### Bug S5-A: white-space:nowrap not enforced (text-overflow-001, diff=56730)

**Visual**: All 3 boxes show multi-line wrapped text. Chrome shows boxes 1+2 as single-line
(clipped/ellipsis) because they have `white-space: nowrap; overflow: hidden`.

**Root cause**: `break_one_line()` in `text3/cache.rs:7263` receives `white_space_mode` but
NEVER checks for `WhiteSpaceMode::Nowrap` to prevent line breaks. The code always tries to
break/hyphenate when text overflows. The `text_wrap` parameter isn't passed at all.

**Fix**: In `break_one_line()`, when `white_space_mode == Nowrap|Pre`, accumulate all items
on a single line without breaking. Also need overflow:hidden clipping in display list.

### Bug S5-B: inline-block shrink-to-fit uses wrong containing block (inline-block-text-001, diff=23485)

**Visual**: "Auto A/B/C" boxes are wider than Chrome. Should shrink-wrap to text content.

**Root cause**: In `fc.rs:6643`, `collect_and_measure_inline_content` passes
`constraints.containing_block_size` (the IFC root's parent's content-box) instead of the
IFC root's own content-box width. This makes `available_width` too large for shrink-to-fit,
so the result is `max_content` width (too wide).

**Fix**: Compute the IFC root's content-box width and pass that as containing block for
inline-block children.

### Bug S5-C: table column widths = 0 (cascade-ua-defaults-001, diff=43430)

**Visual**: TABLE text renders vertically (one char per line) = zero-width cells.

**Root cause**: UA CSS correctly sets `display:table/table-row/table-cell`. The table layout
IS triggered. But `calculate_column_widths_auto_with_width()` or `calculate_column_widths_fixed()`
may not set `computed_width` on columns, resulting in zero-width cells.

**Fix**: Debug column width calculation in `fc.rs:5055+`.

### Bug S5-D: block-positioning paint offset (block-positioning-complex-001, diff=32800)

**Visual**: Red static-box shifted left (x≈0 instead of x≈20). Green relative-box overlaps wrong.

**Root cause**: Layout calculated_positions are correct (verified s4). Bug is in display list
coordinate space — possibly double-offset when stacking context bounds are applied, or
position:relative offsets not reflected in paint coordinates.

**Fix**: Investigate `generate_for_stacking_context()` coordinate handling.

### Bug S5-E: table cell height/text positioning (table-basic-001, diff=15607)

**Visual**: Table rows slightly taller in azul. Text position within cells slightly different.

**Root cause**: Cell padding or row height calculation gives slightly different metrics.
Table structure and colors are correct.

### Bug S5-F: inline element spacing (inline-elements-001, diff=12856)

**Visual**: Very close to Chrome. Minor spacing differences in inline spans. Down from 16309
after s4 inline padding fix.

**Root cause**: Remaining differences likely from font metrics or minor padding calculation.

### Session 5 Fix Priority

1. **S5-A white-space:nowrap** — clear bug, high diff (56730), single code location
2. **S5-B inline-block sizing** — clear bug, single line fix at fc.rs:6643
3. **S5-C table column widths** — high diff (43430), needs debugging
4. **S5-D block-positioning paint** — medium diff, complex coordinate issue
5. **S5-E table cell height** — lower diff, incremental
6. **S5-F inline spacing** — lowest diff, may improve from other fixes

---

## Session 4 Fixes

### FIXED: Cascade — `*` selector overwriting inherited text color on Text nodes
**File**: `core/src/compact_cache_builder.rs` (line ~639)
- **Root cause**: The `*` selector properties were applied to ALL nodes including Text nodes. Text nodes are not CSS elements — per spec, `*` should only match elements. This caused `* { color: #666 }` to overwrite the inherited `color: red` on a Text child of `<p style="color:red">`.
- **Fix**: Skip global `*` properties for `nd.is_text_node()` in the compact cache cascade.
- **Result**: cascade-global-star-001: 37499→37385 ("Paragraph" text now renders RED as expected)

### FIXED: Inline padding not affecting layout positioning
**Files**: `layout/src/text3/cache.rs` (positioning loop, ~line 7943)
- **Root cause**: The IFC text layout engine positioned glyph clusters without accounting for inline border/padding. Padding values were correctly retrieved into `InlineBorderInfo` but never used to offset the text pen position.
- **Fix**: Pre-compute span-boundary offsets by comparing `Arc<StyleProperties>` pointers between consecutive clusters. Only the first cluster of each inline span gets `left_inset`, and only the last gets `right_inset`. Apply these as pen offsets in the positioning loop.
- **Result**: inline-background-001: 18022→15132, inline-elements-001: 16309→12856

### FIXED: Font-size em/percent resolution — inherited raw values instead of computed px
**File**: `core/src/compact_cache_builder.rs` (line ~717)
- **Root cause**: The compact cache inherited RAW font-size values (em, %, pt) from parent to child. When a child resolved `1.5em`, it used `DEFAULT_FONT_SIZE=16px` instead of the parent's actual computed size (14px), giving `24px` instead of `21px`. For `150%`, it resolved against `0.0` (percent_resolve=0) giving `0px` (invisible text).
- **Fix**: After all CSS properties are applied for each node, resolve font_size from em/percent/pt to px using the parent's already-resolved font_size (pre-order traversal guarantees parent is resolved first).
- **Result**: font-properties-001: "font-size: 150%" now renders (was invisible), "1.5em" now correct size. Diff increased 16528→22739 because more correct content is now displayed.

### INVESTIGATED: Block positioning (block-positioning-complex-001)
- Layout `calculated_positions` are CORRECT for all nodes (verified with debug logging)
- The bug appears to be downstream in display list generation or rendering
- Deprioritized — requires deeper investigation of the paint pipeline

---

## Session 4 — New Bug Analysis from Visual Comparison

### Current Failing Tests (sorted by diff)

| Diff | Test | Category |
|-----:|------|----------|
| 90182 | block-margin-collapse-complex-001 | margin collapse height |
| 67254 | cascade-display-block-001 | cascade / font metrics |
| 56730 | text-overflow-001 | text overflow |
| 49045 | cascade-multiple-classes-001 | cascade / font metrics |
| 46300 | cascade-font-weight-inherit-001 | cascade / font metrics |
| 45519 | cascade-specificity-001 | cascade / font metrics |
| 43430 | cascade-ua-defaults-001 | **TABLE vertical text bug** |
| 42760 | cascade-nested-selectors-001 | cascade / font metrics |
| 41659 | cascade-inheritance-001 | cascade / font metrics |
| 40946 | block-padding-border-001 | block padding/border |
| 40691 | absolute-non-replaced-height-001 | abs-pos height |
| 37919 | cascade-inline-style-001 | cascade / font metrics |
| 37499 | cascade-global-star-001 | **CASCADE: p vs * color** |
| 32800 | block-positioning-complex-001 | **Z-INDEX paint order** |
| 23485 | inline-block-text-001 | inline-block sizing |
| 18022 | inline-background-001 | **INLINE PADDING missing** |
| 16528 | font-properties-001 | **FONT-SIZE 1.5em/150%** |
| 16309 | inline-elements-001 | **INLINE PADDING missing** |
| 16080 | text-basic-001 | text / font metrics |
| 15607 | table-basic-001 | table paint |
| 13238 | display-none-visibility-001 | font metrics only |
| 10386 | inline-block-text-002 | font metrics only |

### NEW BUG A: Z-index paint order (block-positioning-complex-001, diff=32800)

**Visual evidence**: In the overlapping red/green rectangle section, azul shows a red sliver
to the left of the green rectangle. Chrome shows green completely on top of red. The z-index
ordering for positioned children is wrong — the red box (lower z-index) should be fully
behind the green box (higher z-index).

**Root cause hypothesis**: `paint_in_flow_descendants` in `display_list.rs` doesn't separate
children by stacking context status. Positioned children with explicit z-index are painted
in document order instead of z-index order.

**Priority**: HIGH — clear visual bug, single test but foundational for z-index correctness.

---

### NEW BUG B: Cascade — p selector color not overriding * selector (cascade-global-star-001, diff=37499)

**Visual evidence**: In cascade-global-star-001, the line "Paragraph inside box: p selector
sets color red, overrides *" is **RED in Chrome** but **gray in Azul**. The `p { color: red }`
rule is NOT overriding `* { color: #666 }` for `<p>` elements.

This is a REAL cascade bug (NOT just font metrics). Specificity of `p` (0,0,1) > `*` (0,0,0).

**Affected tests**: Likely contributes to diffs in ALL 9 cascade tests (37k-67k each).

**Root cause hypothesis**: In the compact cache cascade, per-node css_props for element type
selectors like `p { color: red }` may not be overriding the `* { color: #666 }` that was
applied earlier. Check if `apply_css_property_to_compact` for per-node rules (step 3) actually
overwrites the value set by global `*` rules (step 2.5).

**Priority**: CRITICAL — affects 9+ tests, ~400k total diff across all cascade tests.

---

### NEW BUG C: Table content renders vertically (cascade-ua-defaults-001, diff=43430)

**Visual evidence**: In cascade-ua-defaults-001, below "Second numbered item", the text
"TABLE: UA defaults Cell 2" renders as **vertical single characters** (one char per line:
T, A, B, L, E, :, ...) instead of horizontal text. Chrome shows it as a single horizontal line.

**Root cause hypothesis**: The `<table>` or `<td>` elements are getting zero or near-zero
width, forcing text to wrap after every character. The UA stylesheet may not be setting
`display: table` / `display: table-cell` correctly, or the table layout algorithm is
computing zero column widths.

**Priority**: MEDIUM — affects 1 test directly, but indicates fundamental table layout issue.

---

### NEW BUG D: font-size 150% not parsed, 1.5em too large (font-properties-001, diff=16528)

**Visual evidence**:
1. The "font-size: 150%" line is **MISSING** entirely in azul. Chrome shows it at ~21px.
2. The "font-size: 1.5em" text is **much larger** in azul (~36px) vs Chrome (~21px).
3. The "font-weight: 100 (thin)" text at top has garbled/overlapping rendering.

**Root cause hypothesis**:
- 150%: The CSS parser may not handle percentage font-size values, so the declaration is
  dropped entirely and the element collapses or gets default size.
- 1.5em: The em calculation may be using the wrong base size (e.g., using a parent's computed
  font-size that is already enlarged, or doubling the multiplication).
- font-weight 100: May be mapped to wrong font face or not supported.

**Priority**: MEDIUM — affects font rendering correctness globally.

---

### NEW BUG E: Inline padding not affecting layout/paint (inline-background-001 + inline-elements-001)

**Visual evidence**: In both tests, inline `<span>` elements with padding (e.g., `padding: 2px 6px`)
render with backgrounds that are **tight to the text** with no visible padding. Chrome shows clear
padding gaps around the colored inline text spans.

In inline-elements-001, the "yellow span", "cyan bordered span", "Dashed border span" etc.
all lack the padding space that Chrome shows.

**Root cause hypothesis**: The inline formatting context (IFC) layout does not account for
padding on inline-level elements when:
1. Computing the background rectangle dimensions
2. Advancing the inline pen position after the inline element

Inline padding should:
- Expand the background rect by padding amounts
- Add horizontal padding to the inline advance width
- NOT affect line height (only vertical margins/padding on inline elements don't affect line boxes)

**Affected files**: `layout/src/solver3/ifc.rs` (inline formatting context layout),
`layout/src/solver3/display_list.rs` (inline background painting)

**Priority**: HIGH — affects 2 tests directly (~34k total diff), plus likely affects other
inline-heavy tests.

---

## Session 4 Priority Order (bugs to fix)

1. **Bug B: Cascade p vs * color** — CRITICAL, affects 9+ tests (~400k total diff)
2. **Bug E: Inline padding** — HIGH, affects 2 tests (~34k diff), foundational inline layout
3. **Bug A: Z-index paint order** — HIGH, clear visual bug (32.8k diff)
4. **Bug D: Font-size 150%/1.5em** — MEDIUM, 16.5k diff, font correctness
5. **Bug C: Table vertical text** — MEDIUM, 43.4k diff, table layout fundamental

---

## Session 3 Fixes (diff reductions, no new passes yet)

### FIXED: CSS selector matching — descendant and child combinators
**Commit**: `569b2842` — Rewrote `matches_html_element()` in `core/src/style.rs`
- **Root cause**: Descendant combinator (space) only checked one parent level instead of searching all ancestors. Child combinator (`>`) didn't verify rightmost selector matched target node.
- **Fix**: Complete rewrite: collect all selector groups, require rightmost matches target, then walk ancestors/siblings with proper combinator logic (descendant searches all ancestors, child checks only parent).
- **Result**: cascade-nested-selectors-001: 103156→42760 (all backgrounds now correct)

### FIXED: Table row background paint order
**Commit**: `569b2842` — Skip `paint_node_background_and_border` for table-internal elements
- **Root cause**: TR elements' `calculated_positions` had wrong Y values (all at y=20 instead of correct row positions). The recursive paint descent re-painted row backgrounds at wrong positions, covering header cell content.
- **Fix**: Skip background painting for `TableRowGroup`, `TableRow`, `TableColumnGroup` in `paint_node_background_and_border` since `paint_table_items` already handles them correctly using cell bounding boxes.
- **Result**: table-basic-001: 27059→15607 (header text now visible with correct dark background)

### FIXED: Block intrinsic height margin collapse
**Commit**: `2fe02b4d` — Apply margin collapsing in `calculate_block_intrinsic_sizes`
- **Root cause**: `sizing.rs` added full margins (top+bottom) for each child in block intrinsic height calculation, ignoring CSS 2.2 §8.3.1 margin collapsing. This overstated `used_size` for auto-height containers by hundreds of pixels.
- **Fix**: Only include border+padding in block-axis extras (not margins). Add collapsed sibling gaps (max of adjacent margins). Omit first/last child margins (they may escape parent per parent-child collapse).
- **Result**: block-margin-collapse-complex-001: 373182→90182 (4x improvement)

---

## Session 2 Fixes

### FIXED: Bug 1 — display:none not hiding elements
**Commit**: `beec64bf` — filter display:none children in reconciler's collect_children_dom_ids()
**Result**: display-none-visibility-001 diff 62067→13238 (remaining diff is font metrics)

### FIXED: Bug 2 — Opacity not working + stacking contexts not found
**Commits**: `11ca0c64` — Two fixes:
1. Added `global_css_props` fallback in `get_property_slow()` for `*` selector properties
2. Fixed `collect_stacking_contexts` to recursively search non-SC descendants via `find_nested_stacking_contexts`
3. Fixed `paint_in_flow_descendants` to skip stacking context children (prevents double-painting)
**Result**: color-background-001 diff 36091→4012 (PASS). Opacity 0.5/0.2 now render correctly.

### PARTIALLY FIXED: Bug 4 — z-index ordering
The stacking context tree fix also improved z-index ordering. block-positioning-complex-001 diff 45300→32800.
Remaining: positioned children's paint order still has edge cases.

---

## Remaining Bugs (ordered by estimated impact)

### Bug 1: `display: none` not hiding elements — FIXED

**Tests**: display-none-visibility-001 (diff=62067)
**Impact**: HIGH — also probably affects cascade-display-block-001 (diff=67254)

**Screenshot evidence**: In the "display: none on Box 2" section, Box 2 (blue) is still
VISIBLE and takes space. Chrome hides it completely. `visibility: hidden` works correctly
(space preserved, element invisible).

**Root cause**: The compact cache encodes `display: None` as value 4 (`layout_display_to_u8`).
But during layout tree construction or the reconciler, nodes with `display: none` are likely
not being filtered out. The layout engine processes them as normal elements.

**Where to fix**:
- `layout/src/solver3/layout_tree.rs` — check `process_node` or `process_block_children` for
  display:none filtering
- `layout/src/solver3/cache.rs` — check reconciler for display:none handling
- `core/src/compact_cache_builder.rs` — verify the encoding `None = 4` is read correctly

**Verification**: display-none-visibility-001 should show Box 2 hidden in the "display:none" section.

---

### Bug 2: Global `*` CSS properties not reaching slow-path (opacity, etc.)

**Tests**: color-background-001 (diff=36091)
**Impact**: HIGH — affects ANY non-compact-cache property set via `*` selector

**Screenshot evidence**: `opacity: 0.5` and `opacity: 0.2` boxes are solid purple instead
of semi-transparent. `rgba()` alpha works fine. Background-color from the SAME `*` rule works.

**Root cause (CONFIRMED by agent)**: In `core/src/prop_cache.rs:694-706`, the `restyle()`
function collects global `*` rule properties into `self.global_css_props`. These are applied
to the COMPACT CACHE only (tiers 1/2/2b) in `compact_cache_builder.rs:640-648`. But properties
that aren't in the compact cache (like opacity, transform, filter, box-shadow, etc.) are
NEVER pushed into `self.css_props`. So `get_property_slow()` can never find them.

**Where to fix**: `core/src/prop_cache.rs` — in `restyle()`, after collecting `global_css_props`,
also push them into `css_props` for ALL nodes. Or: add `global_css_props` as a fallback
search location in `get_property_slow()`.

**Verification**: color-background-001 opacity boxes should be semi-transparent.

---

### ~~Bug 3: Text color mismatch in cascade tests~~ — RECLASSIFIED as font metrics

**Tests**: cascade-global-star-001 (diff=37499), cascade-inline-style-001 (diff=37919),
cascade-multiple-classes-001 (diff=44941), cascade-nested-selectors-001 (diff=123800),
cascade-display-block-001 (diff=67254)
**Impact**: LOW — screenshot comparison shows colors ARE correct. Diffs from font metrics.

**Screenshot evidence**: In cascade-global-star-001, "Box: .box overrides margin to 20px,
color to #003366" shows as muted/gray-blue instead of dark blue (#003366). "Paragraph inside
box: p selector sets color red" shows as gray instead of red.

**Root cause analysis**: The compact cache cascade order is:
1. Inherit from parent (line ~566) — copies tier2b_text including text_color
2. UA CSS (line ~592)
3. Global `*` author CSS (line ~640) — overwrites inherited text_color
4. Per-node css_props (line ~668) — should override `*`
5. Inline CSS (line ~688)

Per CSS spec, `*` (specificity 0,0,0) should override inherited values. Step 3 correctly
runs after step 1. BUT: the `*` rule's text_color might overwrite a color that was set by
a SPECIFIC rule on the parent. The child inherits the parent's color (step 1), then `*`
overwrites it (step 3), then per-node rules restore it IF the child has explicit rules (step 4).
For children with NO explicit color, `*` correctly wins over inheritance.

The visual diff suggests colors from specific selectors (`.box { color: #003366 }`) are
being rendered as the `*` color (#666) or vice versa. Need to verify if the issue is:
- (A) `*` color overwriting specific selector colors → check if css_props order is wrong
- (B) text_color encoding/decoding precision loss
- (C) the compact cache fast path in `get_style_properties` returning wrong color

**Where to fix**: `core/src/compact_cache_builder.rs` (cascade order),
`layout/src/solver3/getters.rs` (color fast path)

---

### Bug 4: Z-index ordering for children of non-stacking-context parents

**Tests**: block-positioning-complex-001 (diff=45300)
**Impact**: MEDIUM

**Screenshot evidence**: Second red rect renders IN FRONT of green rect. Chrome shows green
IN FRONT of red. The z-index ordering is reversed for positioned children.

**Root cause (CONFIRMED by agent)**: `paint_in_flow_descendants` in `display_list.rs:2113`
doesn't separate children by stacking context status. When a non-stacking-context element
(like `.container` with position:relative + z-index:auto) has children that ARE stacking
contexts (with explicit z-index), those children are painted in document order instead of
z-index order.

**Where to fix**: `layout/src/solver3/display_list.rs` — in `paint_in_flow_descendants`,
after separating by float status, also separate by stacking context status. Paint
non-stacking-context children in document order, then stacking-context children sorted by
z-index.

---

### Bug 5: Table row background z-order (header text hidden)

**Tests**: table-basic-001 (diff=43454)
**Impact**: MEDIUM

**Screenshot evidence**: Row backgrounds (red/green/blue) are visible but the header row
shows purple/blue background instead of black. Header cell text ("Header A") is hidden
behind the row background.

**Root cause**: `paint_table_row_and_cells` computes row rect from cell bounding boxes and
paints it. But the `<tr>` element in `<thead>` has no explicit background — only `<th>` cells
have `background: #333333`. The row rect picks up `bg_color.a == 0` (transparent) so it
should be skipped. The header appearing purple/blue might be from a different row's background
bleeding through due to incorrect rect positioning.

The deeper issue: `paint_table_items` runs inside `paint_node_background_and_border` BEFORE
`paint_in_flow_descendants`. Cell borders and text are painted AFTER row backgrounds, which
is correct. But the header cell backgrounds might be covered if `paint_table_row_and_cells`
paints ALL rows including the header row with some non-zero background.

**Where to fix**: `layout/src/solver3/display_list.rs` — verify `get_background_color` for
`<tr>` inside `<thead>` returns transparent. Check if the row rect bounding box is correct
for the header row.

---

### Bug 6: Margin collapse container height

**Tests**: block-margin-collapse-complex-001 (diff=373182)
**Impact**: LOW (visually close, container just extends too far)

**Screenshot evidence**: White container extends further down than Chrome's. The individual
box positions are correct. The diff is from the container being too tall.

**Root cause**: `content_box_height = main_pen - total_escaped_top_margin - escaped_bottom_margin`.
The escaped_bottom_margin calculation may not account for all cases. The container has no
bottom padding → last child's margin should escape → reduce height.

**Where to fix**: `layout/src/solver3/fc.rs` (~line 2260) — verify escaped_bottom_margin
calculation for the specific test case.

---

### Bug 7: Inline-block auto-sizing still slightly too wide

**Tests**: inline-block-text-001 (diff=23485)
**Impact**: LOW (text shows, boxes slightly wider than Chrome)

**Screenshot evidence**: "Auto A", "Auto B", "Auto C" purple boxes show text but are wider
than Chrome's compact shrink-wrapped boxes.

**Root cause**: `calculate_ifc_root_intrinsic_sizes` measures text correctly but the
max-content width includes padding/border extras that might be double-counted, or the
text measurement returns slightly wider bounds than Chrome's.

**Where to fix**: `layout/src/solver3/sizing.rs` (~line 427)

---

### Bug 8: Inline-block text positioning (inline-block-text-002)

**Tests**: inline-block-text-002 (diff=10386)
**Impact**: LOW (close to passing, text wrapping slightly different)

**Screenshot evidence**: Text inside colored inline-block boxes wraps correctly but minor
differences in line breaking and horizontal positioning remain.

**Where to fix**: Font metrics or text shaping differences — may need fine-tuning rather
than a bug fix.

---

### Bug 9: Various cascade test color mismatches — RECLASSIFIED

**Tests**: cascade-font-weight-inherit-001 (diff=29574), cascade-specificity-001 (diff=36733)

**CRITICAL UPDATE**: Agent screenshot comparison found that ALL 6 cascade tests render
**visually correct** — text colors, specificity, inheritance, inline styles, multi-class
selectors, and nested selectors all match Chrome. The pixel diffs come from font metrics
and text positioning differences, NOT from CSS cascade bugs.

This means Bug 3 (text color cascade) and Bug 9 are NOT color bugs — they're text
rendering/positioning issues. The cascade system works correctly. The diffs will decrease
as font metrics and text layout improve but don't need specific cascade fixes.

---

## Priority Order for Next Session

1. **Bug 1 (display:none)** — 5 filter locations identified, clear fix, HIGH impact
2. **Bug 2 (opacity/global props)** — one-line fix in get_property_slow, HIGH impact
3. **Bug 4 (z-index)** — clear fix in paint_in_flow_descendants, MEDIUM impact
4. **Bug 5 (table header)** — verify paint order, MEDIUM impact
5. **Bug 6 (margin height)** — test with/without escaped_bottom subtraction
6. **Bug 7 (inline-block sizing)** — fine-tuning
7. ~~Bug 3 (text colors)~~ — RECLASSIFIED: cascade works, diffs are font metrics
8. ~~Bug 9 (cascade tests)~~ — RECLASSIFIED: same as Bug 3

## Key Files for Next Session

| File | Purpose |
|------|---------|
| `core/src/prop_cache.rs` | restyle(), global_css_props, get_property_slow — Bugs 2, 3 |
| `core/src/compact_cache_builder.rs` | cascade order, text_color encoding — Bug 3 |
| `layout/src/solver3/display_list.rs` | paint_in_flow_descendants, paint_table — Bugs 4, 5 |
| `layout/src/solver3/layout_tree.rs` | display:none filtering — Bug 1 |
| `layout/src/solver3/cache.rs` | reconciler display:none — Bug 1 |
| `layout/src/solver3/fc.rs` | margin collapse height — Bug 6 |
| `layout/src/solver3/sizing.rs` | inline-block intrinsic sizes — Bug 7 |
| `css/src/compact_cache.rs` | display encoding (None=4) — Bug 1 |

## Implementation Instructions for Next Session

### Commit Order (dependencies matter!)

Fix bugs in this order — later fixes depend on earlier ones being correct:

```
1. Bug 1 (display:none)        — standalone, no dependencies
2. Bug 2 (global * slow-path)  — standalone, no dependencies
   → commit together: "fix(cascade): display:none filtering + global * slow-path properties"
3. Bug 3 (text colors)         — depends on Bug 2 being fixed (color from * might be the issue)
   → commit: "fix(cascade): text-color cascade order for inheritable properties"
4. Bug 4 (z-index ordering)    — standalone painting fix
   → commit: "fix(paint): sort stacking context children by z-index in paint_in_flow_descendants"
5. Bug 5 (table header)        — depends on Bug 4 (paint order)
   → commit: "fix(table): correct paint layering for table backgrounds"
6. Bug 6 (margin height)       — standalone
   → commit: "fix(margin): escaped_bottom_margin for container auto-height"
```

### Agent Investigation Details (from subagent output)

#### Bug 1 — display:none (FULLY INVESTIGATED)

Agent found **5 missing filter locations**:

| Location | Function | Fix needed |
|----------|----------|------------|
| `cache.rs:488-510` | `collect_children_dom_ids()` | Filter display:none before reconciliation |
| `cache.rs:860` | `reconcile_recursive()` | Uses unfiltered collect_children_dom_ids |
| `fc.rs:977` | `layout_bfc()` Pass 1 | `tree.children()` unfiltered |
| `fc.rs:1061` | `layout_bfc()` Pass 2 | `tree.children()` unfiltered |
| `taffy_bridge.rs:1238` | `layout_taffy_subtree()` | Uses `tree.children()` not `get_layout_children()` |

Encoding/decoding is CORRECT (`None = 4`). `process_block_children` in layout_tree.rs
line 1287 DOES filter display:none — but reconciler path bypasses this.

**Recommended fix**: Filter in `collect_children_dom_ids()` (Layer 1) since it's the entry
point. All downstream code then automatically gets filtered children. Also add guards in
BFC Pass 1/2 as defense-in-depth.

#### Bug 2 — global `*` properties not in css_props

**Agent confirmed root cause.** The fix is in `core/src/prop_cache.rs`.

Detailed agent finding:
- `restyle()` at lines 694-706 collects global `*` rule props into `self.global_css_props`
- These feed ONLY into `compact_cache_builder.rs:640-648` (`apply_css_property_to_compact`)
- Properties NOT in the compact cache (opacity, transform, filter, box-shadow, text-shadow,
  background-image, cursor, etc.) are NEVER stored in `css_props`
- `get_property_slow()` searches: user_overrides → inline → css_props → cascaded → computed → UA
- `global_css_props` is NOT in this search chain

**Fix option A** (simplest): In `get_property_slow`, after checking UA defaults and before
returning None, also check `self.global_css_props`:
```rust
// After UA default check, add:
for prop in self.global_css_props.iter() {
    if prop.get_type() == *css_property_type {
        return Some(prop);
    }
}
```

**Fix option B** (correct but more work): During restyle, push `global_css_props` into
`css_props` for every node. This is expensive for large DOMs but semantically correct.

**Recommendation**: Fix option A — minimal change, correct behavior.

#### Bug 3 — text color cascade

**Agent found the architectural issue** but it's complex. Summary:

The cascade order (inherit → UA → `*` → per-node → inline) is correct per CSS spec:
`*` selector (specificity 0,0,0) DOES override inherited values. But the visual evidence
shows specific selectors' colors being wrong.

Possible sub-causes:
1. `apply_css_property_to_compact` for global `*` might be overwriting colors set by
   per-node css_props if the order is: inherit → UA → per-node → `*` (wrong order).
   Verify: does step 2.5 (global `*`) run BEFORE or AFTER step 3 (per-node css_props)?
   Current code: `*` at line 640 runs BEFORE per-node at line 668. This is CORRECT.

2. The color might be wrong because `get_style_properties` reads text_color from the
   compact cache fast path, which might have stale/wrong values. The compact cache
   text_color is set during cascade and should be final. But check if the text color
   in the compact cache matches what Chrome expects.

3. IMPORTANT: After fixing Bug 2, re-check Bug 3. The `*` rule's color IS in the compact
   cache (it's a tier2b property). But the issue might be that per-node rules DON'T
   override it because they're also using the compact cache path incorrectly.

**Debugging approach**: Add a unit test in `core/tests/compact_cache_correctness.rs` that
creates a DOM with `* { color: gray }` and `.specific { color: red }`, then verify the
compact cache has the right color for the .specific element.

#### Bug 4 — z-index ordering

**Agent found exact location.** The fix is in `display_list.rs:paint_in_flow_descendants`.

Current code at lines 2136-2179 separates children into:
- `non_float_children`
- `float_children`
- `dragging_children`

But does NOT separate by stacking context status. Children with explicit z-index are
painted in document order instead of z-index order.

**Fix**: After the float separation, add:
```rust
let (stacking_ctx_children, regular_children): (Vec<_>, Vec<_>) =
    non_float_children.iter().partition(|&&idx| self.establishes_stacking_context(idx));

// Paint regular non-float children first (document order)
for &child_idx in &regular_children { ... }

// Then paint stacking context children sorted by z-index
let mut sorted_stacking: Vec<_> = stacking_ctx_children.iter().map(|&&idx| {
    let z = self.get_z_index(idx);
    (z, idx)
}).collect();
sorted_stacking.sort_by_key(|(z, _)| *z);
for (_, child_idx) in sorted_stacking {
    self.generate_for_stacking_context(builder, &self.collect_stacking_contexts(child_idx)?)?;
}
```

NOTE: This requires `collect_stacking_contexts` to be callable for arbitrary nodes, not
just the root. Check if it already supports this.

#### Bug 5 — table header background

**Agent found paint ordering issue.** The header `<th>` cells have `background: #333333`
but the `<tr>` row has no background. The `paint_table_row_and_cells` correctly checks
`bg_color.a > 0` and should skip painting transparent rows.

Check: is the header row's `get_background_color` returning non-zero? If `<tr>` has no
background CSS, it should return transparent (a=0). If it returns something else, there's
a UA CSS or cascade issue.

Also check: is the header row rect overlapping with data row rects? The bounding box
computation from cells might include padding/border that extends into adjacent rows.

#### Bug 6 — margin collapse height (INVESTIGATED)

Agent traced the calculation. The current code at line 2260:
```rust
content_box_height = main_pen - total_escaped_top_margin - escaped_bottom_margin.unwrap_or(0.0);
```

Agent analysis is conflicting — subtracting escaped_bottom_margin might be correct OR wrong
depending on how main_pen accounts for the last child's margin. Need to test both with/without
the subtraction and compare with Chrome's pixel output.

Also check lines 2159-2178 where escaped_bottom_margin is handled for root nodes — possible
double-counting.

### Debugging Tips for Agents

1. **Always zoom into screenshots** — use `Read` tool on `.webp` files to see pixel-level
   differences. Small color shifts or 1px position changes are often the clue.

2. **Check the `results.json`** — every test has debug warnings with `[PositionCalculation]`,
   `[IfcLayout]`, `[layout_bfc]`, `[paint_node]` etc. Filter by node index to trace a
   specific element.

3. **The compact cache is the source of truth for layout** — if a property value is wrong
   in the compact cache, everything downstream is wrong. Always verify compact cache values
   first.

4. **The `// +spec:` markers** reference W3C spec paragraphs. Prompt files in
   `doc/target/skill_tree/prompts/` contain the actual spec text for verification.

5. **Test with unit tests first** — `core/tests/compact_cache_correctness.rs` has 36+ tests.
   Add failing tests for cascade bugs BEFORE fixing them. This prevents regressions.

## Session Statistics

- **15 commits** this session
- **+4 tests passing** (17→21)
- **3 rounds of parallel agent investigations** (9 agents total)
- Key fixes: font hash reverse map, table double-text, text justification, flex child width,
  per-side borders, margin:auto BFC, abs-pos text layout, line-height px parsing, table row
  backgrounds, auto inline-block sizing
