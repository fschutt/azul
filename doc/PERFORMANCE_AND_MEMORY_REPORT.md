# Performance & Memory Architecture Report

## Executive Summary

This report analyzes the azul layout/CSS architecture for performance bottlenecks
during resize and scroll operations, and proposes concrete improvements.
The analysis was done against the `scrolling.c` example with 500 DOM rows.

**Key findings:**

| Finding | Impact | Effort |
|---------|--------|--------|
| `CssProperty` enum is 1520 B (Scrollbar variant) — **removable** | ~11× memory bloat for all CSS data | Low (remove variant, already decomposed) |
| `BackgroundContent` / `BoxShadow` still large (~48 B / ~40 B) | Enum sized to largest remaining variant | Low (`BoxOrStatic<T>` wrapper) |
| Inline styles not compressed or deduplicated | 5.5 MB for 500 identical rows | Medium (CompactInlineProps + source dedup) |
| Font chain resolution runs every layout pass | 3.7% CPU on scroll frames | Low (dirty flag) |
| `tier3_overflow` duplicates all resolved properties | Doubles CSS memory | Medium (remove; use cascade fallback) |
| Resize always re-layouts from root | Slow resize for large DOMs | Already partially cached |

---

## 1. Layout Caching Architecture

### 1.1 Overview

The layout pipeline in `layout_and_generate_display_list()` ([layout/src/window.rs](layout/src/window.rs#L562)) follows this flow:

```
layout_and_generate_display_list()
  └── layout_dom_recursive()
        ├── Font Resolution (Steps 0-5)      ← NO caching across frames
        ├── solver3::layout_document()        ← HAS incremental caching
        │   ├── reconcile_and_invalidate()    ← fingerprint-based diffing
        │   ├── calculate_intrinsic_sizes()   ← only dirty nodes
        │   ├── calculate_layout_for_subtree()← only dirty subtrees
        │   ├── reposition_clean_subtrees()   ← moves clean siblings
        │   └── generate_display_list()
        └── scan_for_iframes() + recurse
```

### 1.2 Reconciliation System

The layout engine uses `NodeDataFingerprint` ([core/src/diff.rs](core/src/diff.rs#L1408)) with 6 Highway-hash values per node for O(1) diffing:

```
content_hash  — NodeType (Text, Image, Div)
state_hash    — hover, focus, active pseudo-states
inline_css_hash — inline styles
ids_classes_hash — CSS selector inputs
callbacks_hash — event handlers
attrs_hash    — contenteditable, tab_index
```

This produces `DirtyFlag::{Layout, Paint, None}` per node. Nodes marked `None` can reuse cached layout results.

### 1.3 Taffy Cache Integration

Two cache layers work together:

1. **Taffy's built-in cache** on `LayoutNode.taffy_cache` — `CacheTree` trait implemented on `TaffyBridge` ([layout/src/solver3/taffy_bridge.rs](layout/src/solver3/taffy_bridge.rs#L1815))
2. **Azul's 9+1-slot NodeCache** in `LayoutCacheMap` ([layout/src/solver3/cache.rs](layout/src/solver3/cache.rs#L62)) — external cache parallel to the node array, inspired by Taffy's architecture

Dirty nodes have their caches cleared before layout ([layout/src/solver3/mod.rs](layout/src/solver3/mod.rs#L470)):
```rust
for &node_idx in &recon_result.intrinsic_dirty {
    if let Some(node) = new_tree.get_mut(node_idx) {
        node.taffy_cache.clear();
    }
}
```

### 1.4 Resize Behavior

When viewport size changes, `reconcile_and_invalidate()` marks the root as a `layout_root` ([layout/src/solver3/cache.rs](layout/src/solver3/cache.rs#L679)):

```
viewport.size != old_viewport.size → layout_roots.insert(0) → full top-down pass from root
```

**This means all 500 nodes are visited in the top-down pass.** However:

- Nodes with **fixed sizes** (`width: 100px; height: 50px`) hit the Taffy/NodeCache when their containing block constraints haven't changed
- Nodes with **percentage** or **auto** values must be recomputed
- The `scrolling.c` rows use fixed `height: 30px; min-height: 30px; flex-shrink: 0` which *should* cache well — but the container changes width on horizontal resize, forcing all rows with `auto` width to recompute

**Observation:** Resize is not smooth for 500 rows because:
1. The top-down pass visits every node even if only the root's width changed
2. Font chain resolution runs unconditionally (see Section 3)
3. Display list generation (~2800 lines in `display_list.rs`) iterates all nodes regardless of dirty state

---

## 2. CSS Property Cache Architecture

### 2.1 The Cascade Layers

`CssPropertyCache` ([core/src/prop_cache.rs](core/src/prop_cache.rs#L309)) stores properties in 5 layers, checked in priority order:

| Priority | Layer | Storage | Lookup |
|----------|-------|---------|--------|
| 1 (highest) | `user_overridden_properties` | `Vec<Vec<(CssPropertyType, CssProperty)>>` sorted | binary search |
| 2 | Inline CSS (`node_data.css_props`) | `CssPropertyWithConditionsVec` per node | linear scan with pseudo-state match |
| 3 | CSS Stylesheet (`css_props`) | `Vec<Vec<StatefulCssProperty>>` sorted | binary search |
| 4 | Cascaded/Inherited (`cascaded_props`) | `Vec<Vec<StatefulCssProperty>>` sorted | binary search |
| 5 | Computed inherited (`computed_values`) | `Vec<Vec<(CssPropertyType, CssPropertyWithOrigin)>>` sorted | binary search |
| 6 (lowest) | UA CSS | Static match table in `ua_css.rs` | function call |

### 2.2 Fast Path: CompactLayoutCache

After the initial cascade, `build_compact_cache()` ([core/src/compact_cache_builder.rs](core/src/compact_cache_builder.rs#L30)) flattens all resolved properties into a compact representation:

| Tier | Content | Size/Node | Lookup |
|------|---------|-----------|--------|
| Tier 1 | 21 enum properties in a `u64` | 8 B | array index + bitshift |
| Tier 2 | Numerical dimensions (`CompactNodeProps`) | 96 B | array index + sentinel decode |
| Tier 2b | Text properties (`CompactTextProps`) | 24 B | array index |
| Tier 3 | ALL resolved properties (sorted Vec) | variable | binary search |

**For Normal-state nodes, layout getters hit the compact cache directly:**
```rust
if node_state.is_normal() {
    if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
        return cc.get_display(node_id.index()); // O(1): array + bitshift
    }
}
```

This is already O(1) for the hot layout path. The slow 6-layer cascade is only hit for:
- Hovered/active/focused nodes
- Properties not in tier 1/2 (rare during layout)
- Before the first restyle (never in practice)

### 2.3 Tier 1: Bit-Packed Enum Properties

21 CSS enum properties packed into a single `u64` per node ([css/src/compact_cache.rs](css/src/compact_cache.rs#L80)):

```
Bits [4:0]   display          5 bit
Bits [7:5]   position         3 bit
Bits [9:8]   float            2 bit
Bits [12:10] overflow_x       3 bit
Bits [15:13] overflow_y       3 bit
Bits [16]    box_sizing       1 bit
Bits [18:17] flex_direction   2 bit
Bits [20:19] flex_wrap        2 bit
Bits [23:21] justify_content  3 bit
Bits [26:24] align_items      3 bit
Bits [29:27] align_content    3 bit
Bits [31:30] writing_mode     2 bit
Bits [33:32] clear            2 bit
Bits [37:34] font_weight      4 bit
Bits [39:38] font_style       2 bit
Bits [42:40] text_align       3 bit
Bits [44:43] visibility       2 bit
Bits [47:45] white_space      3 bit
Bits [48]    direction        1 bit
Bits [51:49] vertical_align   3 bit
Bits [52]    border_collapse  1 bit
Bits [63:53] spare            11 bit
```

**This is already the "u64 compression" technique** — it's implemented and in use.

---

## 3. Memory Analysis: StyledDom for 50,000 Nodes

### 3.1 Per-Node Base Cost

| Structure | Size | Source |
|-----------|------|--------|
| `NodeData` | 320 B | [core/src/dom.rs](core/src/dom.rs#L1308) |
| `NodeHierarchyItem` | 32 B | [core/src/styled_dom.rs](core/src/styled_dom.rs#L583) |
| `StyledNode` | 10 B | [core/src/styled_dom.rs](core/src/styled_dom.rs#L334) |
| `CascadeInfo` | 8 B | [core/src/style.rs](core/src/style.rs#L19) |
| **Subtotal (DOM core)** | **370 B** | |

### 3.2 CSS Property Cache Cost

| Structure | Size/Node | Notes |
|-----------|-----------|-------|
| Compact Tier 1 (`u64`) | 8 B | bit-packed enums |
| Compact Tier 2 (`CompactNodeProps`) | 96 B | numerical dimensions |
| Compact Tier 2b (`CompactTextProps`) | 24 B | text properties |
| Compact Tier 3 pointer (`Option<Box<...>>`) | 8 B | pointer to overflow Vec |
| `CssPropertyCache` Vec-of-Vec overhead (4 layers × 24 B) | 96 B | outer Vec slots |
| **Subtotal (compact)** | **232 B** | |

### 3.3 The CssProperty Size Problem

`CssProperty` is a `#[repr(C, u8)]` enum with **156 variants** and **1520 bytes** per instance.

The size is dominated by a single variant:

```
Scrollbar(ScrollbarStyleValue)    → 1512 bytes
  └── ScrollbarStyle              → 1504 bytes
       └── 2 × ScrollbarInfo     → 2 × 752 bytes
            └── 5 × StyleBackgroundContent → 5 × ~48 bytes each
```

**This variant is no longer needed.** Individual scrollbar sub-properties already
exist as separate `CssProperty` variants (`ScrollbarWidth`, `ScrollbarColor`,
`ScrollbarVisibility`, `ScrollbarFadeDelay`, `ScrollbarFadeDuration`). The layout
engine's `get_scrollbar_style()` ([layout/src/solver3/getters.rs](layout/src/solver3/getters.rs#L3341))
already resolves these individually (Steps 3–5), only falling back to the compound
`Scrollbar` variant in Step 2. Step 2 can be removed entirely.

After removing `Scrollbar`, the next-largest variants are:

| Variant | Payload Type | Payload Size |
|---------|-------------|------|
| `BackgroundContent(...)` | `CssPropertyValue<StyleBackgroundContentVec>` | ~32 B (Vec wrapper) |
| `BoxShadowLeft/Right/Top/Bottom(...)` | `CssPropertyValue<StyleBoxShadow>` | ~48 B |
| `Width/Height(...)` | `CssPropertyValue<LayoutWidth>` | ~16 B |
| Most variants | small enums / PixelValue | 8–16 B |

With `Scrollbar` removed, `CssProperty` shrinks from **1520 B → ~56 B** (tag + largest
remaining variant). However, `BackgroundContent` and `BoxShadow` are still uncommon
properties that inflate the enum. See Section 4.2 for the `BoxOrStatic<T>` solution.

**Projected sizes after removing Scrollbar:**

| Type | Current | After removal |
|------|---------|---------------|
| `CssProperty` | 1520 B | ~56 B |
| `StatefulCssProperty` | 1528 B | ~64 B |
| `CssPropertyWithConditions` | 1568 B | ~104 B |

### 3.4 Total Memory for 50,000 Nodes

Assuming 7 inline CSS properties per node (like `scrolling.c` rows):

| Component | 500 Nodes (current) | 50k Nodes (current) | 500 Nodes (after fixes) | 50k Nodes (after fixes) |
|-----------|-----------|--------------|------------|----------------|
| DOM core (370 B/node) | 185 KB | 18.1 MB | 185 KB | 18.1 MB |
| Compact cache (232 B/node) | 116 KB | 11.3 MB | 116 KB | 11.3 MB |
| Inline CSS (7 × 1568 B/node) | **5.35 MB** | **535 MB** | — | — |
| CompactInlineProps (est. ~16 B/node) | — | — | 8 KB | 0.8 MB |
| `tier3_overflow` clone | **5.18 MB** | **518 MB** | 0 (removed) | 0 (removed) |
| **Total** | **~11 MB** | **~1.08 GB** | **~0.3 MB** | **~30 MB** |

**With all fixes (remove Scrollbar variant, CompactInlineProps, source dedup, tier3 removal), 50,000 nodes drop from ~1 GB to ~30 MB — a 36× reduction.**

### 3.5 Inline Style Waste

In `scrolling.c`, 250 rows use the style `"height:30px; ... background:#e8e8e8"` and 250 use `"... background:#ffffff"`. That's only **2 unique inline style sets**, but they're stored as **500 independent allocations** of 7 × 1568 B each = 5.35 MB.

Two orthogonal problems:
1. **Representation**: each property is stored as a 1520 B `CssProperty` enum, even for a 4-byte `height: 30px`.
2. **Deduplication**: identical inline style strings produce independent per-node allocations.

Both are addressed in the plan below (Section 4.1 + 4.4).

---

## 4. Proposed Improvements (Ranked by Impact)

### Fix 1: Remove `CssProperty::Scrollbar` Variant Entirely (HIGH impact, LOW effort)

**Problem:** `CssProperty` is 1520 B because of `Scrollbar(ScrollbarStyleValue)`,
a compound property that packs 2 × `ScrollbarInfo` × 5 × `StyleBackgroundContent`
into a single enum variant.

**Fix:** Remove `CssProperty::Scrollbar(ScrollbarStyleValue)` and
`CssPropertyType::Scrollbar` entirely. The individual sub-properties already exist:

| Existing Variant | CSS Name | Payload |
|-----------------|----------|---------|
| `ScrollbarWidth` | `scrollbar-width` | `LayoutScrollbarWidth` (small enum) |
| `ScrollbarColor` | `scrollbar-color` | `StyleScrollbarColor` (~8 B) |
| `ScrollbarVisibility` | `-azul-scrollbar-visibility` | `ScrollbarVisibilityMode` (1 B) |
| `ScrollbarFadeDelay` | `-azul-scrollbar-fade-delay` | `ScrollbarFadeDelay` (~4 B) |
| `ScrollbarFadeDuration` | `-azul-scrollbar-fade-duration` | `ScrollbarFadeDuration` (~4 B) |

The compound `ScrollbarStyle` also exposed track/thumb/button/corner backgrounds
(`StyleBackgroundContent`). Instead of the compound struct, add **new flat variants**:

| New Variant | CSS Name | Payload |
|-------------|----------|--------|
| `ScrollbarTrack` | `-azul-scrollbar-track` | `CssPropertyValue<StyleBackgroundContent>` (~48 B) |
| `ScrollbarThumb` | `-azul-scrollbar-thumb` | `CssPropertyValue<StyleBackgroundContent>` (~48 B) |
| `ScrollbarButton` | `-azul-scrollbar-button` | `CssPropertyValue<StyleBackgroundContent>` (~48 B) |
| `ScrollbarCorner` | `-azul-scrollbar-corner` | `CssPropertyValue<StyleBackgroundContent>` (~48 B) |
| `ScrollbarResizer` | `-azul-scrollbar-resizer` | `CssPropertyValue<StyleBackgroundContent>` (~48 B) |

Key advantage: each is a **flat enum variant** with a ~48 B payload (same size as
`BoxShadow` variants). No compound struct needed → the enum doesn't bloat.
Full configurability is preserved — each part of the scrollbar is independently
stylable via CSS, and the values participate in the normal cascade.

The layout engine's `get_scrollbar_style()` ([layout/src/solver3/getters.rs](layout/src/solver3/getters.rs#L3341))
already resolves the existing sub-properties in Steps 3–5. Step 2 (the compound
fallback) is the only consumer of `CssProperty::Scrollbar` — it gets replaced by
new Steps that query the individual track/thumb/button/corner variants.
`scrollbar-color` remains as a shorthand that sets thumb + track colors.

**Effect:** `CssProperty` drops from **1520 B → ~56 B** (27× reduction).
**Cascading:** `StatefulCssProperty`, `CssPropertyWithConditions`, all Vec storage —
all shrink proportionally.

**Migration steps:**
1. Add new `CssPropertyType` variants: `ScrollbarTrack`, `ScrollbarThumb`,
   `ScrollbarButton`, `ScrollbarCorner`, `ScrollbarResizer`
2. Add corresponding `CssProperty` variants with `CssPropertyValue<StyleBackgroundContent>` payload
3. Add CSS parsing for `-azul-scrollbar-track`, `-azul-scrollbar-thumb`, etc.
4. Add getters: `get_scrollbar_track()`, `get_scrollbar_thumb()`, etc. to prop_cache
5. Update `get_scrollbar_style()` Step 2 → query new individual variants instead
   of the compound `ScrollbarStyle`
6. Remove `Scrollbar` from `CssPropertyType` and `CssProperty` enums
7. Remove `ScrollbarStyle`, `ScrollbarInfo` types
8. Remove `-azul-scrollbar-style` parsing, `as_scrollbar()`, `impl From<ScrollbarStyle>`
9. Update `macros.rs` match arms

### Fix 2: `BoxOrStatic<T>` for Remaining Large Variants (MEDIUM impact, LOW effort)

**Problem:** After removing `Scrollbar`, `BackgroundContent` (~32 B payload via Vec
wrapper) and `BoxShadow` (~48 B payload) become the largest variants.
They're uncommonly used but still inflate the enum to ~56 B. More importantly,
these are *uncommon* properties — most nodes never set a box-shadow or complex
background. It's wasteful to pay the size cost for every `CssProperty` instance.

**Fix:** Introduce a `BoxOrStatic<T>` enum:

```rust
#[repr(C, u8)]
pub enum BoxOrStatic<T: 'static> {
    /// Heap-allocated (parsed at runtime)
    Boxed(*const T),
    /// Compile-time constant (e.g. from `const` CSS defaults)
    Static(&'static T),
}
```

- Size: 1 (tag) + 7 (padding) + 8 (pointer) = **16 B** on 64-bit
- Preserves `const`-constructability: default/UA CSS values use `Static(&MY_CONST)`
- Runtime-parsed values use `Boxed(Box::into_raw(...))`
- `Drop` impl only frees `Boxed` variant
- `Clone` deep-copies `Boxed`, copies pointer for `Static`

Apply to:
- `BackgroundContent(CssPropertyValue<BoxOrStatic<StyleBackgroundContentVec>>)`
- `BoxShadowLeft/Right/Top/Bottom(CssPropertyValue<BoxOrStatic<StyleBoxShadow>>)`
- `TextShadow(CssPropertyValue<BoxOrStatic<StyleBoxShadow>>)`

**Effect:** After this, the largest `CssProperty` variant payload drops to ~16 B.
Total `CssProperty` size: **~24 B** (tag + `CssPropertyValue` discriminant + `BoxOrStatic` pointer).

| Type | Current | After Fix 1 | After Fix 1+2 |
|------|---------|-------------|---------------|
| `CssProperty` | 1520 B | ~56 B | ~24 B |
| `CssPropertyWithConditions` | 1568 B | ~104 B | ~72 B |

### Fix 3: Remove `tier3_overflow` — Use Cascade Fallback (HIGH impact, MEDIUM effort)

**Problem:** `build_resolved_cache()` clones every resolved property into
`tier3_overflow`, doubling the CSS memory footprint.

**Fix:** Remove `tier3_overflow` entirely. For non-compact properties (background,
transform, box-shadow, etc.) that are only read during display-list generation,
fall back to `get_property_slow()` which walks the existing cascade layers.

The compact cache tiers 1/2/2b already handle **all layout-hot properties**.
The slow cascade path is only invoked for paint-time reads where the cost
is amortized over the much larger display-list generation work.

**Where the CSS cache lives:** The `CssPropertyCache` with its 5 cascade layers
(`user_overridden_properties`, inline, `css_props`, `cascaded_props`,
`computed_values`) remains the single source of truth. It already lives on
`StyledDom.css_property_cache`. The compact tiers 1/2/2b are built on top
of it and stored in `compact_cache` on the same struct. No new storage needed.

**Effect:** Eliminates ~5 MB of cloned property data for 500 nodes (currently).
After Fix 1+2, the tier3 savings are smaller per-property but still meaningful
because the clone + allocation overhead itself is eliminated.

### Fix 4: CompactInlineProps + Source Deduplication (HIGH impact, MEDIUM effort)

**Problem:** Identical inline style strings produce independent
`Vec<CssPropertyWithConditions>` allocations per node: 500 rows × 7 × 1568 B
= 5.35 MB for just 2 unique style sets.

**Fix (two-part):**

#### Part A: CompactInlineProps Encoding

Compress any incoming `Vec<CssPropertyWithConditions>` from inline styles into
a compact binary representation, reusing the same technique as the
`CompactLayoutCache`:

```rust
/// Compact representation of a single node's inline CSS.
/// Produced once when `set_inline_style()` is called.
pub struct CompactInlineProps {
    /// Bit-packed enum properties (same encoding as compact tier 1)
    tier1_enums: u64,
    /// Bit mask: which of the ~156 property types are set
    set_mask: [u64; 3],  // 192 bits for 156 property types
    /// Compact numerical values (same encoding as CompactNodeProps)
    tier2_values: CompactNodeProps,  // 96 B
    /// Compact text values
    tier2b_text: CompactTextProps,   // 24 B
    /// Overflow: uncommon properties stored as small sorted Vec
    overflow: Option<Box<Vec<(CssPropertyType, CssProperty)>>>,
}
```

- Most nodes with simple inline styles (height, width, background-color, etc.)
fit entirely in tier1 + tier2 → **~128 B fixed** per unique style set, with no
heap allocation for the overflow.
- Decoded during cascading by the existing `get_property()` lookup: check
compact inline before falling through to stylesheet layers.

#### Part B: Source-Based Deduplication

Inline CSS is non-editable after parsing. Track the **source** (pointer +
length of the original style string, or a hash) so that nodes sharing the
same source share the same `CompactInlineProps`:

```rust
pub struct NodeData {
    // Before: css_props: CssPropertyWithConditionsVec  (7 × 1568 B)
    // After:
    inline_style_key: InlineStyleKey,  // 8 B — index into a dedup table
}

/// Global dedup table, lives on StyledDom or Dom
pub struct InlineStyleTable {
    /// source_hash → index
    lookup: HashMap<u64, usize>,
    /// Dense storage of unique compact inline props
    entries: Vec<CompactInlineProps>,
}
```

- When `set_inline_style("height:30px; ...")` is called, hash the string.
- If the hash is already in the table → return existing `InlineStyleKey`.
- Otherwise parse, compress to `CompactInlineProps`, insert, return new key.
- The cascade's inline-CSS layer reads from the dedup table via the key.

**Effect for `scrolling.c`:**

| | Before | After |
|--|--------|-------|
| Unique style sets | 2 | 2 |
| Inline CSS allocation per node | 7 × 1568 B = 10,976 B | 8 B (key) |
| Total inline CSS (500 nodes) | 5.35 MB | 256 B (2 entries) + 4 KB (500 keys) |
| Saving | — | **99.9%** |

### Fix 5: Cache Font Chain Resolution (MEDIUM impact, LOW effort)

**Problem:** `collect_and_resolve_font_chains()` walks all nodes every layout pass,
even on scroll-only frames where CSS hasn't changed.

**Fix:** Add `font_chains_dirty: bool` to `LayoutWindow`. Set `true` when CSS
properties change (restyle). Clear after resolution. Skip when `false`.

**Effect:** Eliminates 3.7% of active CPU time on scroll frames.
**Already documented in:** [scripts/TODO_PERF.txt](scripts/TODO_PERF.txt#L104)

### Fix 6: Incremental Display List Generation (LOWER impact, HIGH effort)

**Problem:** `generate_display_list()` iterates all nodes even when only a few changed.

**Fix:** Track paint-dirty nodes and only regenerate display list items for those
subtrees, merging with the cached display list.

**Risk:** High complexity — display list items have spatial dependencies (z-order,
stacking contexts, clipping).

**Recommendation:** Defer until simpler fixes are implemented and measured.

### Fix 7: Skip Layout for Width-Only Resize with Fixed-Height Children (LOWER impact, MEDIUM effort)

**Problem:** When only the window width changes, rows with `height: 30px;
flex-shrink: 0` don't change layout, but they're still visited top-down.

**Fix:** Already partially handled by Taffy's caching — if available width
reaching a child hasn't changed, the cached result is reused. The container
flex layout must still run to determine child constraints.

**Recommendation:** Measure cache hit rates during resize before investing effort.
The memory reduction fixes (1–4) improve cache line utilization which helps
resize performance indirectly.

---

## 5. Summary: Priority Order

| # | Fix | Memory Saving (500 nodes) | CPU Saving | Effort |
|---|-----|---------------------------|------------|--------|
| 1 | Remove `Scrollbar` variant | ~10 MB → ~0.4 MB | Faster restyle (27× less memcpy) | Low |
| 2 | `BoxOrStatic<T>` for large variants | ~0.4 MB → ~0.2 MB | Less cache pressure | Low |
| 3 | Remove `tier3_overflow` | ~5 MB → 0 | No clone allocations | Medium |
| 4 | CompactInlineProps + source dedup | ~5.35 MB → ~4 KB | No per-node parse/alloc | Medium |
| 5 | Cache font chains | — | 3.7% CPU on scroll | Low |
| 6 | Incremental display list | — | Variable | High |
| 7 | Width-only resize skip | — | Variable | Medium |

**Fixes 1–5 together reduce CSS memory from ~11 MB to ~0.3 MB for 500 nodes
(36× reduction) and from ~1 GB to ~30 MB for 50,000 nodes.**

---

## 6. Implementation Order

```
┌─────────────────────────────────────┐
│  Phase 1: Enum size reduction       │
│  Fix 1 (remove Scrollbar variant)   │ ← do first, biggest single win
│  Fix 2 (BoxOrStatic<T>)             │ ← builds on Fix 1
├─────────────────────────────────────┤
│  Phase 2: Cache cleanup             │
│  Fix 3 (remove tier3_overflow)      │ ← independent of Phase 1
│  Fix 5 (font chain dirty flag)      │ ← independent, quick win
├─────────────────────────────────────┤
│  Phase 3: Inline compression        │
│  Fix 4 (CompactInlineProps + dedup) │ ← benefits from smaller CssProperty
├─────────────────────────────────────┤
│  Phase 4: Display list (deferred)   │
│  Fix 6, Fix 7                       │
└─────────────────────────────────────┘
```
