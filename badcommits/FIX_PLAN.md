# Azul Layout Engine: Regression Fix Plan

## Executive Summary

Three commits introduced visual regressions that make the Azul layout engine
produce incorrect output for CSS block formatting contexts. All three share a
common root cause: the introduction of **subtree layout caching** (`1a3e5850`)
changed the layout execution model from a two-pass (sizing → positioning) to a
single-pass approach, and subsequent commits attempted to fix symptoms of this
change without understanding the full implications.

This document analyzes each regression, proposes a W3C-conformant fix strategy,
and identifies constraints from downstream consumers (printpdf/git2pdf).

---

## Table of Contents

1. [Background: The Subtree Caching Commit](#1-background-the-subtree-caching-commit)
2. [Regression 1: `c33e94b0` – Whitespace Preservation Breaks Margin Collapsing](#2-regression-1-c33e94b0)
3. [Regression 2: `f1fcf27d` – Margin Double-Application Fix Breaks UA CSS](#3-regression-2-f1fcf27d)
4. [Regression 3: `8e092a2e` – Pass 1 Removal Breaks BFC Sizing](#4-regression-3-8e092a2e)
5. [W3C-Conformant Fix Strategy](#5-w3c-conformant-fix-strategy)
6. [Cache Architecture: Going Beyond Taffy](#6-cache-architecture-going-beyond-taffy)
7. [Downstream Consumer Constraints (printpdf / git2pdf)](#7-downstream-consumer-constraints)
8. [Already-Attempted Partial Fixes](#8-already-attempted-partial-fixes)
9. [Implementation Plan](#9-implementation-plan)
10. [Test Strategy](#10-test-strategy)
11. [IFC Incremental Layout Plan (Phase 2)](#11-ifc-incremental-layout-plan-phase-2)

---

## 1. Background: The Subtree Caching Commit

### Commit: `1a3e5850` — *"Add subtree layout caching to prevent O(n²) complexity"*

**Problem it solved:** Documents with ~300,000 DOM nodes (e.g., PDF output from
git2pdf where every `<p>` becomes a separate text node) took 3+ minutes to lay
out. The root cause was O(n²) complexity:

1. `layout_bfc` had a "Pass 1" that called `calculate_layout_for_subtree` for
   each child to compute their sizes.
2. `process_inflow_child` then called `calculate_layout_for_subtree` **again**
   for each child to set positions.
3. Same nodes were computed multiple times with identical inputs.

**Solution introduced:** A memoization cache keyed by `(node_index,
available_width, available_height)`. If a node was already laid out at the same
available size, the cached result is reused. This brought layout time down from
~3 minutes to ~2 seconds (60-100× speedup).

**What went wrong:** The cache fundamentally changed **when** sizing happens
relative to positioning. In the old model:

```
Pass 1 (sizing):  For each child: calculate_layout_for_subtree(child, pos=(0,0), cb_size)
                  → child.used_size is now populated
                  → grandchildren also laid out (with wrong positions, but sizes correct)

Pass 2 (positioning): For each child:
                  → Read child.used_size (already computed)
                  → Calculate position based on margins, floats, collapsing
                  → call calculate_layout_for_subtree again → cache hit (memoized)
```

In the new model (after `1a3e5850`), Pass 1 was removed and children are sized
"just-in-time" during positioning. But the just-in-time sizing only calls
`calculate_used_size_for_node()` which computes the **outer box** size from
intrinsic sizes — it does **not** recursively lay out grandchildren. This means:

- Float children may not have their sizes computed before normal-flow children
  need to know where floats are.
- Nested BFC nodes may have incorrect intrinsic sizes because their own children
  haven't been laid out yet.

### Timeline of all commits

```
1a3e5850  perf(layout): Add subtree layout caching to prevent O(n²) complexity  ← ROOT CAUSE
    │
c33e94b0  fix(layout): preserve whitespace-only text nodes                      ← REGRESSION 1
a017dcc2  Add xml perf test                                                     ← last good for margin test
    │
f1fcf27d  Fix margin double-application bug for body with margin: 15vh auto     ← REGRESSION 2
4bacfcac  Add UnresolvedBoxProps for lazy CSS box property resolution            ← last good for bg-color
    │
    ... (many unrelated commits) ...
    │
8e092a2e  fix(layout): prevent double margin subtraction for root nodes          ← REGRESSION 3
72ab2a26  fix(titlebar): use TitleBold instead of Title for window title font    ← last good for positioning
    │
2cb2cefa  Fix margin collapsing: use collapse_margins instead of child_margin_top only  ← partial fix
e3367e76  Fix InlineBlock to always use BFC, never IFC                           ← partial fix
2b11ca8e  Filter whitespace-only text nodes in Flex/Grid containers              ← partial fix
```

---

## 2. Regression 1: `c33e94b0`

### Test affected: `block-margin-collapse-complex-001`

### What the commit changed

**Goal:** Preserve whitespace-only text nodes so that `white-space: pre-wrap`
code blocks render correctly (spaces and indentation preserved).

**Changes:**

1. **XML Parser (`layout/src/xml/mod.rs`):** Changed `!is_whitespace_only` to
   `!text_str.is_empty()`. This means ALL non-empty text nodes (including `\n`
   and `\n    ` between block elements) are now preserved in the DOM.

2. **Whitespace collapsing (`layout/src/solver3/fc.rs` `split_text_for_whitespace`):**
   Added complex logic to preserve leading/trailing whitespace as spaces.

### Why it broke

Per CSS 2.2 § 9.2.2.1 (Anonymous Block Boxes): When a block container has mixed
block-level and inline-level children, the inline children are wrapped in
anonymous block boxes. This applies to whitespace text nodes too.

**Before the commit:**
```html
<div>
  <p>Text 1</p>
  <p>Text 2</p>
</div>
```
→ DOM: `div` → `[p("Text 1"), p("Text 2")]` (whitespace nodes filtered)
→ Margins between `<p>` elements collapse normally.

**After the commit:**
```html
<div>
  <p>Text 1</p>
  <p>Text 2</p>
</div>
```
→ DOM: `div` → `[text("\n  "), p("Text 1"), text("\n  "), p("Text 2"), text("\n")]`
→ The `\n  ` text nodes become anonymous inline boxes.
→ Anonymous inline boxes between block elements **prevent margin collapsing**
  (CSS 2.2 § 8.3.1: margins only collapse between *adjacent* block boxes).
→ Extra vertical space appears between blocks.

### W3C-conformant behavior

CSS 2.2 § 9.2.2.1 states: *"If a block container box has a block-level box
inside it, then we force it to have only block-level boxes inside it."* and
*"Any text that is directly contained inside a block container element (not inside
an inline element) that consists only of white space [...] is treated as if the
white space characters within were collapsed away."*

More specifically, CSS Text Level 3 § 4.1.1 (Phase I) says whitespace-only text
nodes that are between block-level siblings should be removed entirely in block
formatting contexts. The `white-space` property only matters for text *within*
inline formatting contexts.

### Fix

The XML parser change was directionally correct (we should preserve whitespace
for `pre`/`pre-wrap`), but the **layout tree builder** must filter whitespace-only
anonymous text nodes that appear **between block-level siblings** in a block
formatting context. Specifically:

1. Keep the XML parser change (`!text_str.is_empty()`).
2. In `reconcile_recursive()` (cache.rs), when building anonymous IFC wrappers
   for mixed content, **check if all inline children are whitespace-only text**.
   If so, skip creating the anonymous wrapper entirely.
3. Alternatively, in `layout_bfc()`, skip children whose only content is
   whitespace text when the parent is a block container with mixed content.

---

## 3. Regression 2: `f1fcf27d`

### Test affected: `block-positioning-complex-001` (body background color disappears)

### What the commit changed

**Goal:** Fix a bug where `margin: 15vh auto` on `<body>` was applied twice
(positioned at 230.4px instead of correct 115.2px).

**Changes:**

1. **UA CSS (`core/src/ua_css.rs`):** Removed `(NT::Html, PT::Height) =>
   Some(&HEIGHT_100_PERCENT)`. The rationale was that browsers don't set
   `height: 100%` on `<html>` in their UA stylesheets.

2. **Margin collapsing (`fc.rs`):** Changed `accumulated_top_margin =
   collapse_margins(parent_margin_top, child_margin_top)` to just
   `accumulated_top_margin = child_margin_top`. This breaks CSS 2.2 § 8.3.1.

3. **Escaped margin handling (`fc.rs`):** Removed the entire block that adjusted
   `child_main_pos` based on `child_escaped_margin` for first children.

### Why it broke

**Issue A — `height: 100%` on `<html>`:**

The commit was technically correct that browser UA stylesheets don't set
`height: 100%` on `<html>`. However, in browsers, the **Initial Containing Block
(ICB)** provides viewport-sized dimensions, and the `<html>` element gets its
height from the ICB through the "auto" height resolution mechanism:

Per CSS 2.2 § 10.6.3: *"If the element's position is 'relative' or 'static', the
'auto' value for height gives the element a used height that is the height of its
containing block."* — but this only applies to absolutely positioned elements.

For the root element specifically, CSS 2.2 § 10.6.2 says the root's height is
resolved against the ICB. The root element with `height: auto` in a continuous
media context would shrink to its content — but CSS 2.1 § 14.2 says:

*"The background of the root element becomes the background of the canvas. [...]
If the value of 'background' for the root element is 'transparent', the UA must
use the value of the 'body' element's background."*

So the fix should NOT be to set `height: 100%` on HTML (that's not W3C
conformant). Instead, Azul needs to implement **CSS 2.1 § 14.2 canvas
background propagation**: the body's background color should be propagated to the
canvas regardless of the HTML element's height.

**Issue B — Margin collapsing regression:**

Changing `collapse_margins(parent, child)` to just `child` violates CSS 2.2 §
8.3.1: *"When two or more margins collapse, the resulting margin width is the
maximum of the collapsing margins' widths."* The subsequent commit `2cb2cefa`
already partially fixed this.

**Issue C — Escaped margin removal:**

The escaped margin positioning was needed for correct parent-child margin collapse.
Without it, when a child's margin escapes through a parent without border/padding,
the child is positioned incorrectly within the parent's content box.

### Fix

1. **Canvas background propagation (CSS 2.1 § 14.2):** Implement in the display
   list generator or compositor — NOT in the layout engine. When the root element
   has `background: transparent`, check the `<body>` element's background and apply
   it to the full viewport/canvas. This is independent of `height: 100%`.

2. **Restore `collapse_margins`** for first-child escape case. Already done in
   `2cb2cefa`.

3. **Re-evaluate escaped margin positioning:** The escaped margin adjustment was
   removed because it caused double-application. The root cause of
   double-application is likely the caching commit changing when sizing happens.
   Rather than removing the adjustment, fix the underlying cause (see §5).

---

## 4. Regression 3: `8e092a2e`

### Test affected: `block-positioning-complex-001` (completely broken)

### What the commit changed

**Goal:** Prevent double margin subtraction for root nodes — the body's margin
was being subtracted once from `known_dimensions` (viewport minus margins) and
again by Taffy's margin calculation.

**Changes:**

1. **Removed Pass 1 entirely from `layout_bfc`**. Replaced with a comment saying
   "intrinsic sizes are already available from the bottom-up sizing pass."

2. **Added just-in-time `calculate_used_size_for_node()` calls** in the
   positioning pass when `child_node.used_size` is `None`. But this only computes
   the outer box size from intrinsic sizes — it does NOT lay out grandchildren.

3. **Added `InlineBlock` special handling** that conditionally used IFC or BFC
   based on children types. This violates CSS 2.2 § 9.4.1 (later fixed in
   `e3367e76`).

4. **Zeroed root margin in Taffy styles** (`taffy_bridge.rs`). This only affects
   Flex/Grid layouts, not BFC, creating inconsistency.

### Why it broke

The fundamental issue: **BFC layout needs child sizes BEFORE positioning them.**

In a BFC, child positioning depends on:
- **Float sizes** (to know where to place normal-flow content around floats)
- **Empty block detection** (requires knowing if a block has zero height)
- **Margin collapsing** (can depend on whether blocks are empty)
- **Auto width resolution** (margin: auto centering needs the child's width)

Without Pass 1 computing these sizes, the just-in-time sizing gives incorrect
results because `calculate_used_size_for_node()` uses `intrinsic_sizes` which
may not be populated yet (especially for deeply nested content).

### Fix

**Restore the two-pass architecture for BFC**, but make it cache-aware:

```
Pass 1 (Sizing): For each in-flow child:
    → Check subtree_layout_cache for (child, containing_block_size)
    → Cache hit: use cached.used_size, skip recursive layout
    → Cache miss: call calculate_layout_for_subtree(child, temp_pos, cb_size)
                  → This recursively sizes the child AND caches its result
    → After: child.used_size is populated

Pass 2 (Positioning): For each in-flow child:
    → Read child.used_size (guaranteed populated from Pass 1)
    → Apply margin collapsing, float positioning, centering
    → Set child.relative_position
    → process_inflow_child(child, ...) → sets absolute positions
      → This hits the cache for grandchildren (no redundant work!)
```

This preserves the O(n) complexity of the caching approach while restoring
correct sizing before positioning. The key insight is that Pass 1 can be
**cache-aware**: if a child was already computed at this available size, just read
`cached.used_size` without re-computing. Pass 1 only does work for cache misses.

---

## 5. W3C-Conformant Fix Strategy

### Principle: Separate concerns correctly

The CSS layout model has a clear separation:

1. **Sizing** (CSS 2.2 § 10): Determine box dimensions from CSS properties,
   intrinsic sizes, and containing block sizes.
2. **Positioning** (CSS 2.2 § 9): Place boxes in the flow based on formatting
   context rules.
3. **Painting** (CSS 2.1 § 14, Appendix E): Render visual output (backgrounds,
   borders, text).

The caching commit mixed sizing and positioning into one pass. The fix commits
then tried to work around the consequences. The correct approach is:

### A. Restore two-pass BFC with cache integration

```rust
fn layout_bfc(...) -> Result<BfcLayoutResult> {
    // === PASS 1: SIZE ALL IN-FLOW CHILDREN ===
    for &child_index in &node.children {
        // Skip out-of-flow
        if is_out_of_flow(child) { continue; }

        // Check cache first!
        let cache_key = LayoutCacheKey::new(child_index, children_containing_block_size);
        if let Some(cached) = ctx.subtree_layout_cache.get(&cache_key) {
            // Cache hit: just set used_size from cache
            if let Some(node_mut) = tree.get_mut(child_index) {
                node_mut.used_size = Some(cached.used_size);
            }
        } else {
            // Cache miss: full recursive layout to compute size
            let mut temp_positions = BTreeMap::new();
            calculate_layout_for_subtree(
                ctx, tree, text_cache, child_index,
                LogicalPosition::zero(),  // temp position
                children_containing_block_size,
                &mut temp_positions,
                &mut bool::default(),
                float_cache,
            )?;
            // After this, child.used_size and cache are populated
        }
    }

    // === PASS 2: POSITION ALL IN-FLOW CHILDREN ===
    // Now every child has .used_size populated.
    // Apply margin collapsing, float positioning, etc.
    for &child_index in &node.children {
        // ... existing positioning logic ...
        // child_node.used_size is guaranteed Some here
    }
}
```

### B. Implement CSS 2.1 § 14.2 canvas background propagation

This is NOT a layout concern. It belongs in the display list generator or
compositor:

```rust
// In display_list.rs or the compositor:
fn propagate_canvas_background(root_element: &LayoutNode, body_element: &LayoutNode) -> ColorU {
    // CSS 2.1 § 14.2: "The background of the root element becomes the background
    // of the canvas."
    let root_bg = get_background_color(root_element);
    if root_bg.is_transparent() {
        // "If the value of 'background' for the root element is 'transparent',
        // the UA must instead use the value of that element's first 'body' child."
        get_background_color(body_element)
    } else {
        root_bg
    }
}
```

The HTML element does NOT need `height: 100%`. The canvas background fills the
entire viewport regardless of the root element's dimensions. This is how all
browsers work.

### C. Fix whitespace filtering at the layout tree level

In `reconcile_recursive()` or in `layout_bfc()`:

```rust
// When building anonymous IFC wrappers for mixed content (block+inline siblings):
for inline_child in inline_run {
    // CSS 2.2 § 9.2.2.1: "Any text that is directly contained inside a block
    // container element (not inside an inline element) that consists entirely of
    // white space, is removed."
    if is_whitespace_only_text(inline_child) && context == BlockFormattingContext {
        continue; // Skip whitespace-only nodes between block siblings
    }
}
```

### D. Fix root node margin handling consistently

The "double margin subtraction" for root nodes should be fixed in ONE place:

**Option A (preferred):** The ICB (Initial Containing Block) provides the
available size. The root element's margin is resolved by `layout_bfc` just like
any other block. The root is just a normal block box whose containing block is
the ICB. No special cases needed.

**Option B (current approach, fragile):** `calculate_used_size_for_node()`
subtracts root margins from viewport size, then Taffy zeros out root margins.
This requires keeping two separate code paths in sync.

---

## 6. Cache Architecture: Going Beyond Taffy

### Current Problem: Global BTreeMap Cache

The current Azul subtree cache (`subtree_layout_cache` in `LayoutCache`) uses a
**global `BTreeMap<LayoutCacheKey, LayoutCacheValue>`** where the key is
`(node_index, available_width, available_height)`. This has several problems:

1. **Single slot per (node, size):** A node measured at `MinContent` width
   overwrites the entry for the same node at `Definite` width if the fixed-point
   representations collide. There's no slot differentiation.
2. **Stores child positions in the cache value:** The `child_positions: Vec<(usize, LogicalPosition)>`
   mixes sizing and positioning concerns. Positions are context-dependent
   (they depend on where the parent places the node) and should NOT be cached.
3. **Cache hit applies positions recursively:** When a cache hit occurs in
   `calculate_layout_for_subtree`, it recursively calls itself for all children
   to set absolute positions. This makes a "cache hit" still O(children) work.
4. **No separation of measure vs. layout:** Unlike Taffy, there's no distinction
   between "compute size only" and "perform full layout". Every cache entry stores
   the full layout result.

### What Taffy Does Right (and What It Lacks)

Taffy stores a `Cache` struct **on each node**:

```rust
pub struct Cache {
    final_layout_entry: Option<CacheEntry<LayoutOutput>>,   // 1 slot for full layout
    measure_entries: [Option<CacheEntry<Size<f32>>>; 9],     // 9 slots for size measurements
}
```

Key design decisions we adopt:

1. **9+1 slots per node** — deterministic slot index per constraint combination,
   entries never clobber each other.
2. **Separate ComputeSize / PerformLayout modes** — sizing queries are cheap.
3. **"Result matches request" cache hit** — Pass 1 measures a node, Pass 2
   provides the measured size as a known dimension → automatic cache hit.
4. **Dirty propagation stops early** — `mark_dirty()` propagates upward but
   stops at an already-dirty ancestor.

However, **Taffy has significant limitations** that we can improve on:

| Limitation in Taffy | Impact | Our Improvement |
|---------------------|--------|------------------|
| Cache lives ON the node | `Clone`-heavy nodes; mutating cache requires `&mut` to tree | External `Vec<NodeCache>` indexed by node index |
| No IFC awareness | Taffy only does Flex/Grid, not text layout | IFC-specific caching with per-item granularity |
| Binary dirty flag (clean/dirty) | Any CSS change = full subtree relayout | `RelayoutScope` enum: None/IfcOnly/SizingOnly/Full |
| No incremental IFC relayout | N/A for Taffy | Partial reflow for text-only changes |
| `BTreeMap` for positions | O(log n) per lookup | `Vec` parallel to tree for O(1) |

### Proposed Architecture: External Multi-Slot Cache

Instead of storing the cache on `LayoutNode` (Taffy's approach), we use an
**external `Vec<NodeCache>`** parallel to `LayoutTree.nodes`. This keeps
`LayoutNode` slim and allows cache manipulation without tree mutation.

```rust
/// External layout cache, parallel to LayoutTree.nodes.
/// cache_map.entries[i] holds the cache for LayoutTree.nodes[i].
/// Stored on LayoutCache (persists across frames).
pub struct LayoutCacheMap {
    entries: Vec<NodeCache>,
}

impl LayoutCacheMap {
    /// Resize to match tree after reconciliation.
    /// New nodes get empty caches. Removed nodes' caches are dropped.
    pub fn resize_to_tree(&mut self, tree_len: usize) {
        self.entries.resize_with(tree_len, NodeCache::default);
    }

    /// O(1) lookup by layout tree index.
    #[inline]
    pub fn get(&self, node_index: usize) -> &NodeCache { &self.entries[node_index] }
    #[inline]
    pub fn get_mut(&mut self, node_index: usize) -> &mut NodeCache { &mut self.entries[node_index] }

    /// Invalidate a node and propagate upward through ancestors.
    /// Returns early if an ancestor is already empty (Taffy optimization).
    pub fn mark_dirty(&mut self, node_index: usize, tree: &LayoutTree) {
        let cache = &mut self.entries[node_index];
        if cache.is_empty { return; } // Already dirty → ancestors are too
        cache.clear();

        // Propagate upward
        let mut current = tree.get(node_index).and_then(|n| n.parent);
        while let Some(parent_idx) = current {
            let parent_cache = &mut self.entries[parent_idx];
            if parent_cache.is_empty { break; } // Stop early
            parent_cache.clear();
            current = tree.get(parent_idx).and_then(|n| n.parent);
        }
    }
}

/// Per-node cache entry. NOT stored on LayoutNode.
#[derive(Debug, Clone, Default)]
pub struct NodeCache {
    /// 9 measurement slots (Taffy's deterministic scheme):
    /// - Slot 0: both dimensions known
    /// - Slots 1-2: only width known (MaxContent/Definite vs MinContent)
    /// - Slots 3-4: only height known (MaxContent/Definite vs MinContent)
    /// - Slots 5-8: neither known (2×2 combos of width/height constraint types)
    pub measure_entries: [Option<SizingCacheEntry>; 9],

    /// 1 full layout slot (with child positions, overflow, baseline).
    /// Only populated after PerformLayout, not after ComputeSize.
    pub layout_entry: Option<LayoutCacheEntry>,

    /// Fast check for dirty propagation.
    pub is_empty: bool,
}

impl NodeCache {
    pub fn clear(&mut self) {
        self.measure_entries = [None, None, None, None, None, None, None, None, None];
        self.layout_entry = None;
        self.is_empty = true;
    }

    /// Compute deterministic slot index from constraints (Taffy scheme).
    pub fn slot_index(
        width_known: bool,
        height_known: bool,
        width_type: AvailableWidthType,
        height_type: AvailableWidthType,
    ) -> usize {
        match (width_known, height_known) {
            (true, true) => 0,
            (true, false) => if width_type == AvailableWidthType::MinContent { 2 } else { 1 },
            (false, true) => if height_type == AvailableWidthType::MinContent { 4 } else { 3 },
            (false, false) => {
                let w = if width_type == AvailableWidthType::MinContent { 1 } else { 0 };
                let h = if height_type == AvailableWidthType::MinContent { 1 } else { 0 };
                5 + w * 2 + h
            }
        }
    }

    /// Check for sizing cache hit, including Taffy's "result matches request" opt.
    pub fn get_size(&self, slot: usize, known_dims: LogicalSize) -> Option<&SizingCacheEntry> {
        let entry = self.measure_entries[slot].as_ref()?;
        // Exact match on known dimensions
        if (known_dims.width - entry.available_size.width).abs() < 0.1
            && (known_dims.height - entry.available_size.height).abs() < 0.1
        {
            return Some(entry);
        }
        // "Result matches request" — if the caller provides the result size
        // as a known dimension, it's still a hit (common Pass1→Pass2 case)
        if (known_dims.width - entry.result_size.width).abs() < 0.1
            && (known_dims.height - entry.result_size.height).abs() < 0.1
        {
            return Some(entry);
        }
        None
    }
}

/// Cache entry for sizing (ComputeSize mode) — no positions stored
#[derive(Debug, Clone)]
pub struct SizingCacheEntry {
    pub available_size: LogicalSize,
    pub result_size: LogicalSize,
    pub baseline: Option<f32>,
    pub escaped_top_margin: Option<f32>,
    pub escaped_bottom_margin: Option<f32>,
}

/// Cache entry for full layout (PerformLayout mode)
#[derive(Debug, Clone)]
pub struct LayoutCacheEntry {
    pub available_size: LogicalSize,
    pub result_size: LogicalSize,
    pub content_size: LogicalSize,
    /// Relative positions within parent's content-box. NOT absolute.
    pub child_positions: Vec<(usize, LogicalPosition)>,
    pub escaped_top_margin: Option<f32>,
    pub escaped_bottom_margin: Option<f32>,
    pub scrollbar_info: ScrollbarRequirements,
}

/// Constraint classification for slot selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailableWidthType {
    Definite,
    MinContent,
    MaxContent,
}
```

### Context-Aware Dirty Flags: `RelayoutScope`

Taffy has a binary dirty flag: a node is either clean or dirty. But CSS
properties have vastly different impacts:

- Changing `color` on a text node: **zero layout impact** (repaint only)
- Changing `font-size` on a text node: **IFC relayout only** (reflow text lines,
  but sibling blocks don't move unless the IFC's height changes)
- Changing `width` on a block: **sizing relayout** (this node and ancestors)
- Changing `display`: **full subtree relayout**

The current `can_trigger_relayout()` in `CssPropertyType` returns a flat `bool`.
We replace it with:

```rust
/// Fine-grained dirty classification based on property + context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RelayoutScope {
    /// No relayout needed — repaint only (e.g., color, background, opacity)
    None,
    /// Only the IFC containing this node needs re-shaping.
    /// Block-level siblings are unaffected unless the IFC height changes,
    /// in which case this auto-upgrades to SizingOnly.
    IfcOnly,
    /// This node's sizing needs recomputation. Parent may need repositioning
    /// of subsequent siblings but doesn't need full recursive relayout.
    SizingOnly,
    /// Full subtree relayout required (e.g., display, position change).
    Full,
}

impl CssPropertyType {
    /// Context-dependent relayout scope.
    ///
    /// `node_is_ifc_member`: whether this node participates in an IFC
    /// (has ifc_membership set on its LayoutNode).
    pub fn relayout_scope(&self, node_is_ifc_member: bool) -> RelayoutScope {
        use CssPropertyType::*;
        match self {
            // Pure paint — never triggers relayout
            TextColor | Cursor | BackgroundContent | BackgroundPosition
            | BackgroundSize | BackgroundRepeat | BorderTopColor | BorderRightColor
            | BorderLeftColor | BorderBottomColor | BorderTopStyle | BorderRightStyle
            | BorderLeftStyle | BorderBottomStyle | BorderTopLeftRadius
            | BorderTopRightRadius | BorderBottomLeftRadius | BorderBottomRightRadius
            | ColumnRuleColor | ColumnRuleStyle | BoxShadowLeft | BoxShadowRight
            | BoxShadowTop | BoxShadowBottom | BoxDecorationBreak | Scrollbar
            | Opacity | Transform | TransformOrigin | PerspectiveOrigin
            | BackfaceVisibility | MixBlendMode | Filter | BackdropFilter
            | TextShadow => RelayoutScope::None,

            // Font/text properties — IFC-only if inside inline context,
            // otherwise no layout impact (block with only block children)
            FontFamily | FontSize | FontWeight | FontStyle
            | LetterSpacing | WordSpacing | LineHeight | TextAlign
            | TextIndent | TextTransform | WhiteSpace | WordBreak
            | OverflowWrap | TabSize | Hyphens => {
                if node_is_ifc_member {
                    RelayoutScope::IfcOnly
                } else {
                    // A block container with only block children:
                    // font properties are inherited but don't affect
                    // this node's own sizing. Children will pick up
                    // the change via inheritance and get their own
                    // dirty flags.
                    RelayoutScope::None
                }
            }

            // Sizing properties — only this node's size changes
            Width | Height | MinWidth | MinHeight | MaxWidth | MaxHeight
            | PaddingTop | PaddingRight | PaddingBottom | PaddingLeft
            | BorderTopWidth | BorderRightWidth | BorderBottomWidth
            | BorderLeftWidth => RelayoutScope::SizingOnly,

            // Everything else (display, position, float, margin, flex-*,
            // grid-*, overflow, etc.) — full relayout
            _ => RelayoutScope::Full,
        }
    }
}
```

This integrates into the existing `restyle_on_state_change` in `styled_dom.rs`:
instead of `if prop_type.can_trigger_relayout()` → `result.needs_layout = true`,
we compute `relayout_scope(prop_type, is_ifc_member)` and set granular dirty
flags that `reconcile_and_invalidate` can use to skip unnecessary work.

### How This Solves the Current Problems

1. **Two-pass BFC with O(1) cache hits in Pass 1:**
   - Pass 1 calls `cache_map.get(child).get_size(slot, constraints)` — if hit,
     returns `result_size` immediately (no recursion).
   - Pass 2 positions children using their cached sizes. No redundant work.

2. **No position contamination:**
   - `SizingCacheEntry` stores NO positions — only `result_size` and `baseline`.
   - `LayoutCacheEntry` stores relative positions (valid within parent's
     content-box coordinate space only).

3. **No collision between MinContent / MaxContent / Definite:**
   - Each goes to a different slot (0–8). A MinContent result (width≈0) never
     clobbers a Definite result.

4. **External cache = slim LayoutNode:**
   - `LayoutNode` does not grow by ~200 bytes for 9 `Option<SizingCacheEntry>`.
   - Cache can be cleared, resized, or serialized independently of the tree.
   - No `&mut tree` needed to read or write cache entries.

5. **O(1) lookup vs. BTreeMap O(log n):**
   - `Vec` index = layout tree index. No hashing, no float-to-fixed conversion.

6. **Performance preservation:**
   - Each node is measured at most 9 times + 1 final layout = O(10) per node.
   - Total: O(10n) = O(n). For git2pdf's 300K-node DOMs: ~2 seconds.

### Comparison: Current vs. Taffy vs. Our Proposed Architecture

| Feature | Current (BTreeMap) | Taffy (on-node) | Ours (external Vec) |
|---------|-------------------|-----------------|---------------------|
| Lookup cost | O(log n) | O(1) implicit | **O(1) indexed** |
| Slots per node | 1 | 9+1 | **9+1** |
| Sizing/Layout split | ✗ | ✓ | **✓** |
| Cache hit cost | O(children) recursive | O(1) | **O(1)** |
| LayoutNode size impact | 0 bytes | ~200 bytes | **0 bytes** |
| Needs `&mut tree` for cache | ✗ (global map) | ✓ | **✗** |
| IFC-specific caching | ✗ | N/A (no IFC) | **✓ (see §11)** |
| Dirty granularity | Binary (clean/dirty) | Binary | **4-level RelayoutScope** |
| "Result matches request" | ✗ | ✓ | **✓** |
| Dirty propagation stops early | ✗ | ✓ | **✓** |
| Incremental IFC reflow | ✗ | N/A | **✓ (see §11)** |

### Migration Path

1. Add `LayoutCacheMap` to `LayoutCache` (next to `tree`, `calculated_positions`).
2. After `reconcile_and_invalidate`, call `cache_map.resize_to_tree(tree.len())`.
3. Mark dirty nodes via `cache_map.mark_dirty(idx, &tree)`.
4. Replace the global `subtree_layout_cache: BTreeMap` usage in
   `calculate_layout_for_subtree` with `cache_map.get(node_index)` lookups.
5. Split `calculate_layout_for_subtree` into `ComputeSize` / `PerformLayout` modes.
6. Update `layout_bfc` to use `ComputeSize` in Pass 1, `PerformLayout` in Pass 2.
7. Remove the global `BTreeMap<LayoutCacheKey, LayoutCacheValue>` from `LayoutContext`.
8. Add `RelayoutScope` to `CssPropertyType` (keep `can_trigger_relayout()` as
   backward-compat wrapper: `self.relayout_scope(true) != RelayoutScope::None`).

---

## 7. Downstream Consumer Constraints (printpdf / git2pdf)

### How they use the layout engine

**printpdf** is the primary consumer. It calls `layout_document_paged_with_config()`
which invokes the full layout pipeline:

```
HTML string → parse_xml_string() → str_to_dom() → StyledDom
            → layout_document_paged_with_config()
              → Solver3LayoutCache (with subtree_layout_cache)
              → FragmentationContext (paged layout)
              → Vec<DisplayList>
            → display_list_to_printpdf_ops()
              → PDF operations
```

**git2pdf** generates HTML from source code files and feeds it through printpdf.
Each file generates HTML with `<pre class="code-block">` using `white-space:
pre-wrap` and monospace font at 6pt. Files with 1000+ lines create DOM trees with
thousands of nodes (3+ spans per line × syntax highlights).

### Critical constraints

1. **Performance MUST be preserved.** git2pdf processes files with 10,000+ lines.
   The O(n²) → O(n) improvement from the caching commit is essential. Any fix
   that reverts to O(n²) will make PDF generation unacceptably slow.

2. **`white-space: pre-wrap` MUST work.** git2pdf's code blocks use `pre-wrap`
   for line wrapping. The whitespace preservation commit (`c33e94b0`) was
   motivated by this use case. We must preserve whitespace in `pre`/`pre-wrap`
   contexts while filtering it in block contexts.

3. **Pagination MUST work.** printpdf uses `FragmentationContext` for page breaks.
   The layout cache interacts with pagination through different available sizes
   per page fragment — cache keys must account for this correctly.

4. **Flex layout MUST work.** git2pdf's title page uses `display: flex` with
   `justify-content: center`. The root margin fix must not break Flex/Grid
   layout.

5. **Text shaping pipeline is independent.** printpdf also has a standalone text
   shaping path (`shape.rs` → `render_unified_layout`) that bypasses the solver.
   This is unaffected by the regressions.

### Regression test strategy for printpdf/git2pdf

1. Add a test that lays out a 10,000-node DOM and verifies it completes in < 5
   seconds (performance regression guard).
2. Add a test that renders `<pre style="white-space: pre-wrap">  code  </pre>`
   and verifies whitespace is preserved.
3. Add a test with `<body style="margin: 15vh auto">` and verify correct
   positioning.
4. Run the existing `block-positioning-complex-001` and
   `block-margin-collapse-complex-001` reference tests.

---

## 8. Already-Attempted Partial Fixes

The following commits were applied after the regressions were identified. They
fix specific symptoms but do NOT address the root architectural issues:

### 8.1 Commit `2cb2cefa` — Restored `collapse_margins()` for First-Child Escape

**What it fixed:** Commit `f1fcf27d` had changed

```rust
accumulated_top_margin = collapse_margins(parent_margin_top, child_margin_top);
```

to just

```rust
accumulated_top_margin = child_margin_top;
```

This broke parent-child margin collapsing: the parent's top margin was ignored
when the first child escaped its margin upward.

**Status:** ✅ Correct fix. Should be KEPT.

**What remains broken:** The escaped margin is collapsed correctly now, but the
*positioning* based on the escaped margin is still wrong because Pass 1 (which
computed sizes BEFORE positioning) is missing. Without pre-computed sizes, the
BFC positioning loop can't correctly account for the vertical space consumed by
collapsed margins.

### 8.2 Commit `2b11ca8e` — Whitespace Filtering in Flex/Grid Containers

**What it fixed:** Added a filter in `LayoutTreeBuilder` that removes
whitespace-only text nodes when building children of Flex or Grid containers:

```rust
// In layout_tree.rs, during child collection for flex/grid:
if is_flex_or_grid && text_content.trim().is_empty() {
    continue; // skip whitespace-only text nodes
}
```

**Status:** ⚠️ Partial fix. Correct for Flex/Grid (CSS Flexbox § 4: "Each
in-flow child of a flex container becomes a flex item, and each contiguous
sequence of child text runs is wrapped in an anonymous block container flex
item. However, if the entire sequence of child text runs is only white space,
it is instead not rendered."). But this does NOT fix BFC containers where
whitespace-only text nodes between blocks must also be stripped (CSS 2.2 §
9.2.2.1).

**Recommendation:** Keep for Flex/Grid. Add equivalent filtering in
`reconcile_recursive()` for BFC mixed-content scenarios.

### 8.3 Commit `e3367e76` — InlineBlock Always Uses BFC

**What it fixed:** Commit `8e092a2e` had added an incorrect code path where
`display: inline-block` elements were treated as inline elements rather than
establishing their own BFC. This commit corrected it.

**Status:** ✅ Correct fix per CSS 2.2 § 9.4.1 ("Inline-blocks [...] establish
new block formatting contexts for their contents"). Should be KEPT.

### 8.4 Commit `3ac22ec1` — Fix IFC Intrinsic Sizing Heights

**What it fixed:** IFC (Inline Formatting Context) intrinsic height calculations
were returning incorrect values because they didn't account for line-box heights
correctly.

**Status:** ✅ Correct fix. Should be KEPT.

### Summary of Remaining Issues After Partial Fixes

| Problem | Root Cause | Fix Required |
|---------|-----------|--------------|
| BFC sizing is wrong without Pass 1 | `8e092a2e` removed Pass 1 | Restore Pass 1 with per-node cache |
| Whitespace between blocks creates spurious IFC | `c33e94b0` preserved all text nodes | Filter in `reconcile_recursive()` |
| Canvas background doesn't propagate | `f1fcf27d` removed `html { height: 100% }` | Implement CSS 2.1 § 14.2 propagation |
| Margin positioning depends on missing sizes | `8e092a2e` + `1a3e5850` | Per-node cache enables two-pass with O(n) |
| Root margin zeroed in Taffy bridge | `8e092a2e` workaround | Remove once BFC handles root correctly |

---

## 9. Implementation Plan (Updated with Per-Node Cache)

### Phase 1: Introduce External LayoutCacheMap

**Files:** `layout/src/solver3/cache.rs`, `layout/src/solver3/mod.rs`

1. Add `LayoutCacheMap`, `NodeCache`, `SizingCacheEntry`, `LayoutCacheEntry`,
   `AvailableWidthType` structs (see §6) to `cache.rs`.
2. Add `cache_map: LayoutCacheMap` field to `LayoutCache` (persistent across frames).
3. Add `ComputeMode` enum (`ComputeSize` | `PerformLayout`).
4. After `reconcile_and_invalidate`, call `cache_map.resize_to_tree(tree.len())`.
5. For each dirty node from reconciliation, call `cache_map.mark_dirty(idx, &tree)`.
6. Add `RelayoutScope` enum and `CssPropertyType::relayout_scope()` method.
7. Keep the old global `BTreeMap` cache temporarily for A/B comparison.

**Estimated effort:** ~250 lines of new code. No behavior change yet.

NOTE: `LayoutNode` is NOT modified — the cache is external. The existing
`taffy_cache: TaffyCache` field on `LayoutNode` stays as-is for Flex/Grid.

### Phase 2: Split `calculate_layout_for_subtree` into Two Modes

**Files:** `layout/src/solver3/cache.rs`

1. Add a `compute_mode: ComputeMode` parameter to `calculate_layout_for_subtree`.
2. In `ComputeSize` mode:
   - Check `node_cache.get_size(constraints)` first.
   - On miss: compute the layout (including positioning children), store the
     result size in `node_cache.store_size(constraints, result)` AND store the
     full layout in `node_cache.store_layout(constraints, full_result)`.
   - Return only `(used_size, baseline)`.
3. In `PerformLayout` mode:
   - Check `node_cache.get_layout(constraints)` first.
   - On hit: apply the cached child positions (no recursive descent needed —
     positions are relative to parent's content box).
   - On miss: compute and store as above.
4. Implement Taffy's "result matches request" optimization: if a cache entry's
   result size matches the requested known dimension, treat it as a hit.

**Estimated effort:** ~150 lines changed. This is the core architectural change.

### Phase 3: Restore Two-Pass BFC with Per-Node Cache

**Files:** `layout/src/solver3/fc.rs`

1. Re-add Pass 1 sizing loop in `layout_bfc()`, but using the new
   `calculate_layout_for_subtree(node, ComputeSize, constraints)`.
2. On cache hit, Pass 1 just reads the cached size — O(1) per child.
3. After Pass 1 completes, run Pass 2 (positioning) with full knowledge of all
   child sizes, margin collapses, and float placements.
4. In Pass 2, call `calculate_layout_for_subtree(node, PerformLayout, constraints)`
   which will be a cache hit (already sized in Pass 1 with same constraints).

**Estimated impact:** Fixes regression 3 (`8e092a2e`). Performance stays O(n)
because Pass 1 populates the cache and Pass 2 reads it.

### Phase 4: Fix Whitespace Filtering in BFC

**Files:** `layout/src/solver3/cache.rs` (reconcile_recursive)

1. In the mixed-content branch of `reconcile_recursive()`, before creating an
   anonymous IFC wrapper, check if ALL inline children in the run are
   whitespace-only text nodes.
2. If so, and if no ancestor has `white-space: pre | pre-wrap | pre-line`,
   skip creating the wrapper (CSS 2.2 § 9.2.2.1 whitespace stripping).
3. Keep the XML parser change (`!text_str.is_empty()`) — whitespace
   preservation for `pre`/`pre-wrap` is handled by the layout engine, not the
   parser.

**Estimated impact:** Fixes regression 1 (`c33e94b0`).

### Phase 5: Implement Canvas Background Propagation

**Files:** `layout/src/solver3/display_list.rs` or compositing code

1. In display list generation, check root element's background.
2. If transparent, propagate `<body>`'s `background-color` and
   `background-image` to the canvas/viewport.
3. This is purely visual — no layout change needed.

**Estimated impact:** Fixes regression 2 (`f1fcf27d`).

### Phase 6: Clean Up and Remove Global Cache

1. Remove the global `subtree_layout_cache: BTreeMap` from `LayoutCache`.
2. Remove the root margin zeroing hack from `taffy_bridge.rs`.
3. Review `2b11ca8e` whitespace filtering — keep for Flex/Grid, ensure BFC
   equivalent works via `reconcile_recursive()`.
4. Keep `e3367e76` (InlineBlock→BFC) and `3ac22ec1` (IFC height fix).

---

## 10. Test Strategy

### Reference Tests (Visual Regression)

| Test Name | What it validates |
|-----------|-------------------|
| `block-positioning-complex-001` | BFC block stacking, margin collapsing, float positioning |
| `block-margin-collapse-complex-001` | Margin collapsing between siblings, parent-child escape |
| `body-background-propagation-001` (new) | CSS 2.1 § 14.2 canvas background from body |
| `pre-wrap-whitespace-001` (new) | Whitespace preservation in pre-wrap blocks |
| `root-margin-flex-001` (new) | Root node with margin in flex container |

### Performance Tests

| Test Name | Constraint |
|-----------|------------|
| `large-dom-10k-nodes` | Must complete in < 5 seconds |
| `large-dom-300k-nodes` | Must complete in < 30 seconds |

### Unit Tests

| Test Name | What it validates |
|-----------|-------------------|
| `margin_collapse_sibling` | Sibling margin collapsing |
| `margin_collapse_parent_child` | Parent-child margin escape |
| `margin_collapse_empty_block` | Empty block self-collapse |
| `whitespace_filtering_block_context` | Whitespace text nodes filtered in BFC |
| `whitespace_preserved_pre_wrap` | Whitespace kept in pre-wrap |

---

## 11. IFC Incremental Layout Plan (Phase 2)

> **Priority:** This section describes improvements to be implemented AFTER the
> core regressions (§9 Phases 1–6) are fixed. The architecture from §6 is
> designed to support these optimizations without further structural changes.

### Problem: IFC Is Currently All-or-Nothing

The current `layout_ifc()` function in `fc.rs` calls `text_cache.layout_flow()`
which reshapes and repositions ALL inline items from scratch every time. There is
no mechanism for:

- "Only relayout items starting from the changed one"
- "Skip relayout if only a paint property (color) changed"
- "Reposition subsequent items without reshaping them"

For a paragraph with 100 inline spans, changing one span's font size reshapes
and repositions all 100 spans. Changing one span's color also reshapes all 100
spans (because `can_trigger_relayout()` returns `true` for most properties).

### The IFC Caching Architecture

The existing `CachedInlineLayout` on `LayoutNode` already stores:

```rust
pub struct CachedInlineLayout {
    pub layout: Arc<UnifiedLayout>,      // Full IFC result (all items positioned)
    pub available_width: AvailableSpace, // Constraint key
    pub has_floats: bool,                // Float state
    pub constraints: Option<UnifiedConstraints>,
}
```

We extend this with **per-item sizing metadata** that enables partial reflow:

```rust
pub struct CachedInlineLayout {
    pub layout: Arc<UnifiedLayout>,
    pub available_width: AvailableSpace,
    pub has_floats: bool,
    pub constraints: Option<UnifiedConstraints>,

    // === NEW: Per-item metadata for incremental relayout ===

    /// Cached advance width for each inline item (text run, inline-block, image).
    /// Index matches the item order in `layout.items`.
    pub item_metrics: Vec<InlineItemMetrics>,
}

/// Per-item metrics cached from the last IFC layout.
#[derive(Debug, Clone)]
pub struct InlineItemMetrics {
    /// Which LayoutNode this item came from (for dirty checking)
    pub source_node_index: usize,
    /// Advance width of this item (glyph run width, inline-block width, etc.)
    pub advance_width: f32,
    /// Advance height of this item's line contribution
    pub line_height_contribution: f32,
    /// Whether this item can participate in line breaking.
    /// `false` for items inside `white-space: nowrap` or `white-space: pre`.
    pub can_break: bool,
    /// Which line this item was placed on (0-indexed).
    pub line_index: u32,
    /// X offset within its line.
    pub x_offset: f32,
}
```

### Incremental IFC Relayout Algorithm

When a node inside an IFC is marked dirty, the IFC root checks what changed:

```
IFC Relayout Decision Tree:

1. Is the dirty node's RelayoutScope == None?
   → YES: Skip entirely. Only repaint.

2. Is the dirty node's RelayoutScope == IfcOnly?
   → Reshaping needed. Continue to step 3.

3. Did the item's advance_width change after reshaping?
   → NO: Only update glyph data in the UnifiedLayout. No repositioning.
         This handles font-weight changes that don't affect metrics.
   → YES: Continue to step 4.

4. Does item_metrics[changed_item].can_break == false
   AND all items on the same line also have can_break == false?
   → YES: "Nowrap fast path" — only shift x_offset for items
          AFTER the changed item on the same line. O(items_on_line).
   → NO: Continue to step 5.

5. Full line-breaking needed from the changed item's line onward.
   → Reshaping + repositioning from line_index[changed_item] to end.
   → Items on earlier lines are untouched.
   → If the LAST line's height changes, propagate height change to
     the IFC root's used_size → may trigger SizingOnly on parent.
```

### Example: Nowrap Fast Path

```
Line 0: ["Hello "] ["world"] [", this is"] [" a test"]
                    ^^^^^^^^
                    Font-size changed → reshaping needed
                    New advance_width: 50px → 60px (+10px)

All items on line 0 have can_break: false (assume nowrap).

Action:
  - Reshape "world" only → get new glyph positions
  - Shift ", this is" x_offset by +10px
  - Shift " a test" x_offset by +10px
  - Line width increased by 10px → check if it still fits
    → YES: done, no further reflow
    → NO: full line-break reflow from line 0
```

### Example: Color-Only Change

```
Line 0: ["Hello "] ["world"] [", this is"] [" a test"]
                    ^^^^^^^^
                    color: red → color: blue

RelayoutScope::None → skip IFC relayout entirely.
Only update the color in the display list. Zero layout work.
```

### Integration with `RelayoutScope`

The `RelayoutScope` enum from §6 drives the IFC decision tree:

| CSS Property Change | `RelayoutScope` | IFC Action |
|--------------------|-----------------|------------|
| `color`, `background` | `None` | No relayout, repaint only |
| `font-size`, `font-family` | `IfcOnly` | Reshape item → check width → maybe reposition |
| `font-weight` | `IfcOnly` | Reshape item → usually same width → no reposition |
| `letter-spacing` | `IfcOnly` | Reshape item → width changed → reposition subsequent |
| `white-space` | `Full` | Full IFC relayout (line-breaking rules changed) |
| `width` on inline-block | `SizingOnly` | Resize inline-block → reposition subsequent |

### Integration with External Cache (`LayoutCacheMap`)

The IFC incremental relayout interacts with the external cache as follows:

1. When a text node in an IFC has `RelayoutScope::IfcOnly`:
   - The IFC root's `NodeCache.layout_entry` is invalidated (positions changed).
   - The IFC root's `NodeCache.measure_entries` MAY be preserved if the
     overall IFC height didn't change (common for same-line reflows).
   - Parent's cache is invalidated only if IFC height changed.

2. When a text node has `RelayoutScope::None` (color change):
   - NO cache entries are invalidated. Zero layout work.
   - Only the display list is regenerated for the affected node.

3. When an inline-block has `RelayoutScope::SizingOnly`:
   - The inline-block's own `NodeCache` is invalidated.
   - The IFC root is notified to reposition items after the inline-block.
   - If the inline-block's height changed, the IFC root's sizing cache
     is also invalidated (line height may have changed).

### Performance Impact

| Scenario | Current | With IFC Incremental |
|----------|---------|----------------------|
| Color change on 1 span in 100-span paragraph | Full IFC relayout | **Zero layout work** |
| Font-size change, same line, nowrap | Full IFC relayout | **Reshape 1 item + shift N items** |
| Font-size change causing line reflow | Full IFC relayout | **Reflow from affected line only** |
| Width change on container | Full IFC relayout | Full IFC relayout (correct) |
| Typing in a text input (1 char) | Full IFC relayout | **Reshape 1 run + shift rest of line** |

For interactive UIs with frequent text edits (text inputs, contenteditable),
this reduces per-keystroke layout from O(all_items) to O(items_on_line).

### Implementation Phases

**Phase 2a:** Add `item_metrics: Vec<InlineItemMetrics>` to `CachedInlineLayout`.
Populate during `layout_ifc()`. No behavioral change yet.

**Phase 2b:** Add `RelayoutScope` to `CssPropertyType` (already in §6/§9).
Update `restyle_on_state_change` to use it. `can_trigger_relayout()` becomes a
wrapper: `self.relayout_scope(true) != RelayoutScope::None`.

**Phase 2c:** In `layout_ifc()`, before calling `text_cache.layout_flow()`,
check if the IFC root's `CachedInlineLayout` exists and if only specific items
are dirty. If the nowrap fast path applies, skip `layout_flow()` and just adjust
positions.

**Phase 2d:** For the general case (items with `can_break: true`), implement
partial line-breaking: feed only dirty items + items on affected lines to
`layout_flow()`, keeping earlier lines unchanged.

### CSS Specification Compliance

- CSS Text Level 3 § 4.1.1: White-space processing model — Phase I stripping
  happens before layout; our caching respects this by storing post-processed text.
- CSS Inline Layout Module Level 3 § 4: Line box construction — partial reflow
  preserves line-box stacking order and baseline alignment for unaffected lines.
- CSS Text Level 3 § 5.2: `white-space: nowrap` — the nowrap fast path is valid
  because nowrap text cannot break across lines, so width changes only affect
  the current line's horizontal extent.

---

## Appendix: CSS Specification References

- CSS 2.2 § 8.3.1: Collapsing margins
- CSS 2.2 § 9.2.2.1: Anonymous block boxes
- CSS 2.2 § 9.4.1: Block formatting contexts
- CSS 2.2 § 9.5: Floats
- CSS 2.2 § 10.3.3: Block-level, non-replaced elements in normal flow (margin: auto)
- CSS 2.2 § 10.6.3: Block-level non-replaced elements in normal flow, height: auto
- CSS 2.1 § 14.2: The canvas background
- CSS Text Level 3 § 4.1.1: The white-space processing model (Phase I)
- CSS Flexbox § 9: Flex Layout Algorithm
- CSS Inline Layout Module Level 3 § 4: Line box construction
- CSS Text Level 3 § 5.2: Breaking rules for `white-space: nowrap`
