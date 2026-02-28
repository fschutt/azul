# Reftest Bug Analysis Report

## Summary

6 failing reftests were investigated. The bugs fall into **5 distinct root causes**:

| # | Root Cause | Affected Tests | Severity |
|---|-----------|---------------|----------|
| 1 | Spurious scrollbar heuristic in display list | showcase-flexbox-complex-001, grid-alignment-001, grid-template-areas-001 | HIGH |
| 2 | `calc()` CSS function not supported | showcase-flexbox-complex-001 | HIGH |
| 3 | `z-index` getter is a stub returning 0 | block-positioning-complex-001 | HIGH |
| 4 | Grid item placement (`grid-column`/`grid-row` span) not forwarded to Taffy | grid-auto-flow-dense-001 | HIGH |
| 5 | `grid-template-areas` CSS property not implemented | grid-template-areas-001 | HIGH |
| 6 | Inline-block shrink-to-fit width ~2x too wide | inline-block-text-001 | MEDIUM |

---

## Bug 1: Spurious Scrollbar Heuristic

**Files:** `layout/src/solver3/getters.rs` lines 1364–1420

**Problem:** `get_scrollbar_info_from_layout()` has a fallback path when `node.scrollbar_info` is `None`. This fallback uses a heuristic that **forces overflow detection** for any node with more than 3 children:

```rust
let num_children = node.children.len();
if num_children > 3 {
    // Likely overflows - assume we need scrollbars
    LogicalSize::new(
        container_size.width,
        container_size.height * 2.0, // Force overflow detection
    )
}
```

This causes pink/magenta scrollbars on ANY container with >3 children, even when `overflow: visible` (the default). In the tests:

- **showcase-flexbox-complex-001**: sidebar (4 items), cards container (6 cards), footer (4 items) all get unwanted scrollbars
- **grid-alignment-001**: the `.wrapper` flex container with 4 grid containers gets a scrollbar
- **grid-template-areas-001**: the `.container` grid with 5 items gets a scrollbar

**Root cause:** For flex/grid children laid out by Taffy, `scrollbar_info` is only set in the Taffy bridge's `measure_function` callback. Nodes that are **parents** (not measured via `measure_function`) may not get `scrollbar_info` set, triggering the fallback. The fallback should return "no scrollbars" when overflow is `visible`, not assume overflow.

**Fix:** The fallback should check the node's actual overflow CSS property. If `overflow: visible`, return `ScrollbarRequirements::default()` (no scrollbars). Only apply overflow heuristics for `overflow: auto/scroll`.

---

## Bug 2: `calc()` CSS Function Not Supported

**Files:** CSS parser in `css/src/`

**Problem:** The showcase test uses `width: calc(33.333% - 10px)` for card elements. There is no `calc()` parser in the azul CSS codebase. The property is silently ignored, resulting in 0-width cards.

**Evidence:** In the display list output, card-header/content/footer rects have **0px width** (`bounds: 0x40`, `bounds: 0x295`, `bounds: 0x30`).

**Impact:** Any CSS using `calc()` expressions is broken. This is a fundamental CSS feature needed for many layouts.

**Fix options:**
1. Implement `calc()` parsing and evaluation (complex, proper fix)
2. For this test: rewrite CSS to avoid `calc()` (workaround only)

---

## Bug 3: `z-index` Getter Returns Constant 0

**File:** `layout/src/solver3/getters.rs` lines 771–775

**Problem:** The `get_z_index()` function is a stub:

```rust
pub fn get_z_index(styled_dom: &StyledDom, node_id: Option<NodeId>) -> i32 {
    // TODO: Add get_z_index() method to CSS cache, then query it here
    let _ = (styled_dom, node_id);
    0
}
```

All positioned elements get `z-index: 0` regardless of their CSS. The stacking context tree in `display_list.rs` (lines 1648–1770) sorts children by z-index, but since all values are 0, the paint order is wrong.

**Affected test:** `block-positioning-complex-001` uses z-index values of 5, 10, 15, 20, 30 on various positioned elements. All render at z-index 0 → wrong stacking order.

**Secondary issue for block-positioning-complex-001:** The pink box (`position: fixed; bottom: 30px; right: 200px`) is positioned too low. This may be related to how `position: fixed` with `bottom`/`right` is resolved relative to the viewport.

**Fix:** Add `get_z_index()` to `CssPropertyCache` in `core/src/prop_cache.rs`, then query it in `getters.rs`.

---

## Bug 4: Grid Item Placement Not Forwarded to Taffy

**File:** `layout/src/solver3/taffy_bridge.rs` function `translate_style_to_taffy()` (lines ~388–595)

**Problem:** The following Taffy style fields are **never set**:
- `grid_column` (start/end)
- `grid_row` (start/end)

The CSS parser correctly parses `grid-column: span 2` and `grid-row: span 2` into `GridLine::Span(2)` (verified by unit tests in `css/src/props/layout/grid.rs` lines 1061–1079). The `CssPropertyCache` has `get_grid_column()` and `get_grid_row()` methods. But `translate_style_to_taffy()` never reads these properties and sets them on the Taffy style.

**Result:** All grid items occupy exactly 1×1 cells regardless of CSS `grid-column: span N` / `grid-row: span N`.

**Affected test:** `grid-auto-flow-dense-001` - items with `.item-large` (span 2×2), `.item-wide` (span 3×1), `.item-tall` (1×span 2) all render as 1×1.

**Fix:** Add the following to `translate_style_to_taffy()`:
```rust
// Read grid-column and grid-row placement from CSS
if let Some(grid_col) = get_grid_column(styled_dom, dom_id, &styled_node_state) {
    taffy_style.grid_column = grid_placement_to_taffy(grid_col);
}
if let Some(grid_row) = get_grid_row(styled_dom, dom_id, &styled_node_state) {
    taffy_style.grid_row = grid_placement_to_taffy(grid_row);
}
```

And implement `grid_placement_to_taffy()` to convert `GridPlacement` → `taffy::Line<taffy::GridPlacement>`.

---

## Bug 5: `grid-template-areas` Not Implemented

**Files:** Entire codebase

**Problem:** There is **zero implementation** of `grid-template-areas` anywhere in the codebase:
- No CSS parser for `grid-template-areas`
- No `grid-area` resolution
- No mapping from named areas to grid line numbers
- The `taffy_style.grid_template_areas` field is never set

**Affected test:** `grid-template-areas-001` - the layout uses:
```css
grid-template-areas:
    "header header header"
    "sidebar main aside"
    "footer footer footer";
```
And items use `grid-area: header`, `grid-area: sidebar`, etc. Without this, all items just stack in default grid placement.

**Fix:** This requires:
1. Parsing `grid-template-areas` CSS property
2. Parsing `grid-area` shorthand property
3. Converting named areas to explicit `grid-column`/`grid-row` line numbers
4. Forwarding to Taffy's grid template areas support

---

## Bug 6: Inline-Block `width: auto` Shrink-to-Fit ~2× Too Wide

**File:** `layout/src/solver3/taffy_bridge.rs` lines ~1325–1366 and `layout/src/solver3/sizing.rs` lines ~288–326

**Problem:** Auto-width inline-block elements render approximately **2× wider** than Chrome:

| Element | Chrome width | Azul width | Ratio |
|---------|-------------|-----------|-------|
| Auto A  | 67px        | 137px     | 2.04× |
| Auto B  | 68px        | 138px     | 2.03× |
| Auto C  | 69px        | 139px     | 2.01× |

The shrink-to-fit logic in `taffy_bridge.rs` uses `intrinsic.max_content_width` as the preferred width. This value comes from `calculate_ifc_root_intrinsic_sizes()` in `sizing.rs`. The `max_content_width` appears to be including excessive space - possibly double-counting padding, or measuring text width incorrectly (e.g., using font metrics that are too wide, or not accounting for character-level width properly).

Chrome: "Auto A" text ~47px + 20px padding = 67px border-box.  
Azul: seems to report text ~117px + 20px padding = 137px border-box.  
The text measurement is ~2.5× off, suggesting the intrinsic text width calculation is using an incorrect font metric or measuring method.

**Fix:** Debug `calculate_ifc_root_intrinsic_sizes()` to verify what `max_content_width` returns for "Auto A" text at 16px sans-serif. Compare against expected advance width values. The issue is likely in `text3` text shaping/measurement.

---

## Recommended Debug Approach

For each bug, the approach depends on complexity:

| Bug | Can Fix Without Gemini? | Recommended Action |
|-----|------------------------|-------------------|
| Bug 1 (scrollbar heuristic) | **YES** - clear code fix | Fix `get_scrollbar_info_from_layout()` fallback to not assume overflow |
| Bug 2 (calc()) | **NO** - significant feature | `cargo run -p azul-doc -- debug showcase-flexbox-complex-001 calc() CSS function is not parsed, cards get 0 width` |
| Bug 3 (z-index stub) | **YES** - straightforward | Add `get_z_index()` to CSS cache, wire it up |
| Bug 4 (grid placement) | **YES** - clear missing code | Add grid-column/grid-row to `translate_style_to_taffy()` |
| Bug 5 (grid-template-areas) | **NO** - new feature | `cargo run -p azul-doc -- debug grid-template-areas-001 grid-template-areas and grid-area CSS properties are not implemented` |
| Bug 6 (inline-block width) | **MAYBE** - needs investigation | `cargo run -p azul-doc -- debug inline-block-text-001 max_content_width for shrink-to-fit inline-block is 2x too wide` |

### Quick Wins (fix manually, no Gemini needed):
1. **Bug 1**: Remove the `num_children > 3` heuristic, make fallback return no-scrollbars for overflow:visible
2. **Bug 3**: Add z-index to CssPropertyCache
3. **Bug 4**: Forward grid-column/grid-row placement to Taffy style

### Gemini Debug Commands:
```sh
# For calc() support investigation
cargo run --release -p azul-doc -- debug showcase-flexbox-complex-001 "Cards have 0 width because calc() CSS function is not supported in the parser"

# For grid-template-areas
cargo run --release -p azul-doc -- debug grid-template-areas-001 "grid-template-areas and grid-area CSS properties have no implementation"

# For inline-block width
cargo run --release -p azul-doc -- debug inline-block-text-001 "Auto-width inline-block elements are 2x too wide - max_content_width from text measurement is wrong"
```
