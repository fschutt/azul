# Compact CSS Property Cache — Status Report V2

**Date:** 2025-01-XX  
**Git HEAD:** `54a4e134` (master)  
**Key commits:** `3981fb5d` (compact_cache.rs), `d9b9bee2` (getters fast-paths), `64c8343f` (solver routing)

---

## 1. Architecture Overview

The compact cache replaces BTreeMap-based `CssPropertyCache` lookups with cache-friendly
arrays for O(1) layout property access. It uses a three-tier encoding:

| Tier | Storage | Per-Node | Contents |
|------|---------|----------|----------|
| **Tier 1** | `Vec<u64>` | 8 B | 21 enum properties bitpacked into u64 (53 bits used, 11 spare) |
| **Tier 2** | `Vec<CompactNodeProps>` | 96 B | Numeric dimensions, padding, margin, border, flex, z-index, border colors/styles |
| **Tier 2b** | `Vec<CompactTextProps>` | 24 B | text_color, font_family_hash, line_height, letter/word spacing, text_indent |
| **Tier 3** | `Vec<Option<Box<BTreeMap>>>` | 8 B | Overflow: calc(), out-of-range values, rare properties |

**Total per-node:** 136 B (vs. ~2-4 KB per node for BTreeMap-based cache with many properties)

### File Locations

| File | Lines | Purpose |
|------|-------|---------|
| `css/src/compact_cache.rs` | 1,831 | Data structures, encode/decode helpers, sentinels, tests |
| `core/src/compact_cache_builder.rs` | 537 | `build_compact_cache()` — populates cache from CssPropertyCache |
| `layout/src/solver3/getters.rs` | 3,965 | Centralized getters with `compact =` fast-path macros |
| `core/src/prop_cache.rs` L468 | 1 | `compact_cache: Option<CompactLayoutCache>` field on CssPropertyCache |
| `core/src/styled_dom.rs` L918-921 | 4 | Wiring: `build_compact_cache()` called after restyle pipeline |

---

## 2. Fast-Path Activation

All compact fast-path getters check `StyledNodeState::is_normal()` before accessing
the compact cache. This returns `true` when **no** pseudo-state is active (no hover,
active, focused, disabled, checked, focus_within, visited, backdrop, dragging, drag_over).

For the initial layout pass (before any user interaction), **100% of nodes** are in
the normal state, so the compact cache handles every property lookup.

When a node enters a pseudo-state (e.g. `:hover`), getters fall through to the slow
path (BTreeMap cascade resolution in `CssPropertyCache`).

---

## 3. Property Coverage by Tier

### Tier 1 — Bitpacked Enums (21 properties, all have fast-path getters)

| Property | Bits | Getter in getters.rs | Fast-Path |
|----------|------|---------------------|-----------|
| `display` | 5 | `get_display_property_internal` (L1877) | `compact = get_display` |
| `position` | 3 | `get_position` (L927) | `compact = get_position` |
| `float` | 2 | `get_float` (L895) | `compact = get_float` |
| `overflow-x` | 3 | `get_overflow_x` (L911) | `compact = get_overflow_x` |
| `overflow-y` | 3 | `get_overflow_y` (L919) | `compact = get_overflow_y` |
| `box-sizing` | 1 | `get_css_box_sizing` (L935) | `compact = get_box_sizing` |
| `flex-direction` | 2 | `get_flex_direction` (L943) | `compact = get_flex_direction` |
| `flex-wrap` | 2 | `get_wrap` (L871) | `compact = get_flex_wrap` |
| `justify-content` | 3 | `get_justify_content` (L879) | `compact = get_justify_content` |
| `align-items` | 3 | `get_align_items` (L951) | `compact = get_align_items` |
| `align-content` | 3 | `get_align_content` (L959) | `compact = get_align_content` |
| `writing-mode` | 2 | `get_writing_mode` (L847) | `compact = get_writing_mode` |
| `clear` | 2 | `get_clear` (L903) | `compact = get_clear` |
| `font-weight` | 4 | `get_font_weight_property` (L967) | `compact = get_font_weight` |
| `font-style` | 2 | `get_font_style_property` (L975) | `compact = get_font_style` |
| `text-align` | 3 | `get_text_align` (L887) | `compact = get_text_align` |
| `visibility` | 2 | `get_visibility` (L983) | `compact = get_visibility` |
| `white-space` | 3 | `get_white_space_property` (L991) | `compact = get_white_space` |
| `direction` | 1 | `get_direction_property` (L999) | `compact = get_direction` |
| `vertical-align` | 3 | `get_vertical_align_property` (L1007) | `compact = get_vertical_align` |
| `border-collapse` | 1 | `get_border_collapse` (L3431, NO compact) | ❌ slow path only |

**Coverage: 20/21** — `border-collapse` at L3431 uses the slow-path variant (no `compact =`).
Note: border-collapse IS in Tier 1 bits AND populated by the builder, but the getter
at L3431 doesn't use the fast path.

### Tier 2 — CompactNodeProps (96 B/node, numeric dimensions)

#### u32-encoded dimension properties (with unit info)

| Property | Encoding | Getter | Fast-Path |
|----------|----------|--------|-----------|
| `width` | u32 (SizeMetric + ×1000) | `get_css_width` (L855) | `compact_u32_dim = get_width_raw` |
| `height` | u32 | `get_css_height` (L863) | `compact_u32_dim = get_height_raw` |
| `min-width` | u32 | `get_css_min_width` (L2418) | `compact_u32_struct = get_min_width_raw` |
| `min-height` | u32 | `get_css_min_height` (L2426) | `compact_u32_struct = get_min_height_raw` |
| `max-width` | u32 | `get_css_max_width` (L2434) | `compact_u32_struct = get_max_width_raw` |
| `max-height` | u32 | `get_css_max_height` (L2442) | `compact_u32_struct = get_max_height_raw` |
| `flex-basis` | u32 | — | ❌ No getter uses compact fast-path |
| `font-size` | u32 | `get_element_font_size` (L58, handwritten) | ❌ Handwritten, no compact fast-path |

#### i16-encoded resolved pixel values (×10)

| Property | Getter | Fast-Path |
|----------|--------|-----------|
| `padding-top` | `get_css_padding_top` (L2404) | `compact_i16 = get_padding_top_raw` |
| `padding-right` | `get_css_padding_right` (L2398) | `compact_i16 = get_padding_right_raw` |
| `padding-bottom` | `get_css_padding_bottom` (L2410) | `compact_i16 = get_padding_bottom_raw` |
| `padding-left` | `get_css_padding_left` (L2392) | `compact_i16 = get_padding_left_raw` |
| `margin-top` | `get_css_margin_top` (L2378) | `compact_i16 = get_margin_top_raw` |
| `margin-right` | `get_css_margin_right` (L2372) | `compact_i16 = get_margin_right_raw` |
| `margin-bottom` | `get_css_margin_bottom` (L2384) | `compact_i16 = get_margin_bottom_raw` |
| `margin-left` | `get_css_margin_left` (L2366) | `compact_i16 = get_margin_left_raw` |
| `border-top-width` | `get_css_border_top_width` (L2463) | `compact_i16 = get_border_top_width_raw` |
| `border-right-width` | `get_css_border_right_width` (L2457) | `compact_i16 = get_border_right_width_raw` |
| `border-bottom-width` | `get_css_border_bottom_width` (L2469) | `compact_i16 = get_border_bottom_width_raw` |
| `border-left-width` | `get_css_border_left_width` (L2451) | `compact_i16 = get_border_left_width_raw` |
| `top` | `get_css_top` (L2352) | `compact_i16 = get_top` |
| `right` | `get_css_right` (L2346) | `compact_i16 = get_right` |
| `bottom` | `get_css_bottom` (L2358) | `compact_i16 = get_bottom` |
| `left` | `get_css_left` (L2340) | `compact_i16 = get_left` |
| `border-spacing-h` | `get_border_spacing` (L3620, handwritten) | ❌ No compact fast-path |
| `border-spacing-v` | `get_border_spacing` (L3620, handwritten) | ❌ No compact fast-path |
| `tab-size` | — | ❌ No getter uses compact fast-path |

#### u16-encoded flex values (×100)

| Property | Getter | Fast-Path |
|----------|--------|-----------|
| `flex-grow` | `get_flex_grow_prop` (L3920, get_css_property_value!) | ❌ No compact |
| `flex-shrink` | `get_flex_shrink_prop` (L3921, get_css_property_value!) | ❌ No compact |

#### Other Tier 2 fields

| Property | Getter | Fast-Path |
|----------|--------|-----------|
| `z-index` | `get_z_index` (L1150, handwritten) | ❌ No compact |
| `border-styles` (packed u16) | `get_border_info` (L1334, handwritten) | ❌ No compact |
| `border-colors` (4×u32) | `get_border_info` (L1334, handwritten) | ❌ No compact |

### Tier 2b — CompactTextProps (24 B/node)

| Property | Getter | Fast-Path |
|----------|--------|-----------|
| `text_color` (u32) | `get_background_color` / selection_style (handwritten) | ❌ No compact |
| `font_family_hash` (u64) | Font resolution (handwritten) | ❌ No compact |
| `line_height` (i16) | `get_line_height_value` (L3489, handwritten) | ❌ No compact |
| `letter_spacing` (i16) | `get_style_properties` (L1919, handwritten) | ❌ No compact |
| `word_spacing` (i16) | `get_style_properties` (L1919, handwritten) | ❌ No compact |
| `text_indent` (i16) | `get_text_indent_value` (L3502, handwritten) | ❌ No compact |

### Tier 3 — Overflow (BTreeMap per node, usually None)

Currently serves as the fallback for:
- `calc()` expressions
- Numeric values exceeding encoding range (i16: ±3276.7 px; u32: ±134,217.727)
- Any property not covered by Tier 1/2/2b

**Currently no getter reads from Tier 3.** The builder doesn't write to Tier 3 either
(values that can't be encoded are silently dropped → sentinel → slow path fallback).

---

## 4. Getter Category Summary

| Category | Count | With Compact | Without Compact |
|----------|-------|-------------|-----------------|
| `get_css_property!` (Tier 1 enums) | 22 | **21** | 1 (border-collapse) |
| `get_css_property!` (Tier 2 u32_dim) | 2 | **2** | 0 |
| `get_css_property!` (Tier 2 u32_struct) | 4 | **4** | 0 |
| `get_css_property_pixel!` (Tier 2 i16) | 16 | **16** | 0 |
| `get_css_property!` (no compact) | 6 | 0 | **6** (text_justify, hyphens, table_layout, border_collapse, caption_side, cursor) |
| `get_css_property_value!` (taffy bridge) | 18 | 0 | **18** (flex/align/grid passthrough) |
| Handwritten (complex logic) | ~30 | 0 | **~30** |
| **TOTAL** | **~98** | **43** | **~55** |

---

## 5. What's Working

1. **Pipeline wiring is complete.** `build_compact_cache()` is called after
   `restyle()` → `apply_ua_css()` → `compute_inherited_values()` in `styled_dom.rs` L918.

2. **All 21 Tier 1 enum properties** are encoded in the builder and have decode
   functions. 20 of 21 getters use the fast path.

3. **All layout-critical numeric properties** (width, height, min/max widths/heights,
   padding 4×, margin 4×, border-width 4×, position offsets 4×) have compact
   fast-path getters (30 properties total).

4. **Sentinel fallback is correct.** Non-px units (em, %, rem, vw, vh) encode as
   `I16_SENTINEL` or `U32_SENTINEL`, causing the getter to fall through to the
   slow path, which has the full cascade resolution context (parent font size, viewport, etc.).

5. **CompactNodeProps size is validated** by a compile-time test: `assert_eq!(size_of::<CompactNodeProps>(), 96)`.

6. **Build compiles cleanly** (`cargo build -p azul-dll --features build-dll`).

---

## 6. What's NOT Working / Incomplete

### 6.1 Missing Fast-Path Getters

The following properties are **stored in the compact cache** (builder encodes them)
but **no getter reads the compact value**:

| Property | Compact Field | Why No Getter |
|----------|--------------|---------------|
| `flex-basis` | `tier2_dims[i].flex_basis` (u32) | taffy_bridge uses `get_flex_basis_prop` → `get_css_property_value!` (returns raw CssPropertyValue) |
| `font-size` | `tier2_dims[i].font_size` (u32) | `get_element_font_size` is handwritten with parent resolution logic |
| `flex-grow` | `tier2_dims[i].flex_grow` (u16) | `get_flex_grow_prop` → `get_css_property_value!` |
| `flex-shrink` | `tier2_dims[i].flex_shrink` (u16) | `get_flex_shrink_prop` → `get_css_property_value!` |
| `z-index` | `tier2_dims[i].z_index` (i16) | `get_z_index` is handwritten |
| `border-styles` | `border_styles_packed` (u16) | `get_border_info` is handwritten |
| `border-colors` | `border_*_color` (4×u32) | `get_border_info` is handwritten |
| `border-spacing` | `border_spacing_h/v` (i16) | `get_border_spacing` is handwritten |
| `tab-size` | `tab_size` (i16) | No getter at all |
| `text_color` | `tier2b_text.text_color` (u32) | `get_style_properties` is handwritten |
| `font_family_hash` | `tier2b_text.font_family_hash` (u64) | Font resolution is handwritten |
| `line_height` | `tier2b_text.line_height` (i16) | `get_line_height_value` is handwritten |
| `letter_spacing` | `tier2b_text.letter_spacing` (i16) | `get_style_properties` is handwritten |
| `word_spacing` | `tier2b_text.word_spacing` (i16) | `get_style_properties` is handwritten |
| `text_indent` | `tier2b_text.text_indent` (i16) | `get_text_indent_value` is handwritten |
| `border-collapse` | Tier 1 bits | `get_border_collapse` at L3431 uses slow-path variant |

**16 properties** are stored but not read via compact fast paths.

### 6.2 `get_css_property_value!` Getters (Taffy Bridge)

The 18 `get_css_property_value!` getters return `Option<CssPropertyValue<T>>` rather than
`MultiValue<T>`. They're used by the taffy bridge (`taffy_bridge.rs`) which needs the
raw `CssPropertyValue` wrapper (to distinguish Auto, Initial, Inherit, None, Exact).

These bypass the compact cache entirely. To add fast paths, the taffy bridge would need
to be refactored to consume `MultiValue<T>` instead, or a separate compact-aware
`get_css_property_value!` variant must be created.

### 6.3 Handwritten Getters (~30 functions)

Complex getters like `get_border_info`, `get_z_index`, `get_style_properties`,
`get_element_font_size`, `get_background_color`, etc. are handwritten with bespoke
logic (parent traversal, font cascade, ColorU composition). Adding compact fast paths
requires per-function changes.

### 6.4 Tier 3 is Empty

The builder never writes to `tier3_overflow`. Values exceeding the compact encoding
range simply get sentinel values and fall through to the slow path. This is correct
behavior but means Tier 3 exists as allocated-but-unused overhead (8 B × node_count
for `Vec<Option<Box<BTreeMap>>>`).

---

## 7. Edge Cases Analysis

### 7.1 Inheritance

**Handled correctly.** The builder runs AFTER `compute_inherited_values()`, so inherited
properties (font-size, color, visibility, etc.) are already resolved in `CssPropertyCache`
before encoding. The compact cache stores the post-inheritance values.

### 7.2 Hover / Active / Focus (Dynamic Pseudo-States)

**Handled correctly.** All compact fast-path getters check `node_state.is_normal()` first.
If any pseudo-state flag is active, they fall through to the slow path which does full
cascade resolution including `:hover`, `:active`, `:focus` rule matching.

**Limitation:** After a hover event, the compact cache is NOT invalidated or rebuilt.
This is fine because the slow path is still correct — but it means that during interaction,
the compact cache provides zero benefit for affected nodes.

### 7.3 Runtime Style Overrides (`set_css_property()`)

**Potentially stale.** If a user callback modifies CSS properties via
`set_css_property()` on a node, the compact cache is not rebuilt. The next layout
pass would use stale compact values for the modified node.

**Mitigation:** The compact cache should be rebuilt after any `set_css_property()` call,
OR `set_css_property()` should also update the compact cache entry for the affected node.
The latter is more efficient (O(1) per property change vs. O(N) rebuild).

### 7.4 Non-Pixel Units (em, %, rem, vw, vh, etc.)

**Handled correctly.** The i16 encoder only encodes `SizeMetric::Px` values. All other
units get `I16_SENTINEL`, falling through to the slow path which has the resolution
context (parent font size, viewport size, containing block). The u32 encoder preserves
the `SizeMetric` in the low 4 bits, so Tier 2 dimension properties DO handle non-px
units for width/height/min/max.

### 7.5 Font Resolution

**Not compact-cached.** `get_element_font_size()` has complex logic involving parent
traversal and default font size. `font_family_hash` is stored but no getter uses it.
The font_family_hash could accelerate font chain lookups (skip `FxHash` if hash matches
the previous node's hash).

### 7.6 `calc()` Expressions

**Fall to slow path.** `LayoutWidth::Calc(...)` / `LayoutHeight::Calc(...)` encode as
`U32_SENTINEL`, falling through to the slow BTreeMap path. This is correct.

### 7.7 Memory Usage

For a DOM with N nodes:
- Tier 1: `8 × N` bytes
- Tier 2: `96 × N` bytes  
- Tier 2b: `24 × N` bytes
- Tier 3: `8 × N` bytes (all None pointers)

**Total: 136 × N bytes.** For 72K nodes: ~9.5 MB. This is acceptable for the
performance gains when iterating properties linearly.

---

## 8. Compact Fast-Path Hit Rate Estimate

### Initial Layout (all nodes normal)

| Getter Type | Count | Fast-Path? | Notes |
|-------------|-------|-----------|-------|
| Tier 1 enums | 20 | ✅ | All except border-collapse |
| Tier 2 dimensions | 6 | ✅ | width, height, min/max W/H |
| Tier 2 pixel (i16) | 16 | ✅ | padding, margin, border-width, position offsets |
| Tier 2 flex/z/border (compact stored) | 6 | ❌ | flex-basis, flex-grow/shrink, z-index, border-styles/colors |
| Tier 2b text (compact stored) | 6 | ❌ | text_color, font_family_hash, line_height, letter/word spacing, text_indent |
| Taffy bridge passthrough | 18 | ❌ | Need CssPropertyValue wrapper |
| Handwritten complex | ~30 | ❌ | Special logic required |
| Rare properties (6 no-compact macros) | 6 | ❌ | text-justify, hyphens, etc. |

**42 of ~98 getters have compact fast paths ≈ 43% coverage.**

However, the 42 fast-path getters cover the **most frequently called** properties
during layout (display, position, width/height, padding, margin, border-width,
flex-direction, etc.). By call frequency, the hit rate is likely **70-80%+** for
the initial layout pass.

---

## 9. Path Forward — Prioritized Next Steps

### Phase 1: Low-Hanging Fruit (estimated: 1-2 hours)

1. **Add `compact = get_border_collapse` to the `get_border_collapse` getter at L3431.**
   It's already in Tier 1 bits and the builder populates it. Just needs the macro variant change.

2. **Add compact fast-paths to `get_z_index` (handwritten, L1150).**
   Simple: check `is_normal()`, read `cc.get_z_index(node_id.index())`, decode sentinel.

3. **Add compact fast-paths to `get_border_spacing` (handwritten, L3620).**
   Read `cc.get_border_spacing_h_raw()` and `cc.get_border_spacing_v_raw()`.

### Phase 2: Inline Fast-Path Additions (estimated: 2-4 hours)

4. **Add compact fast-paths to `get_border_info` (handwritten, L1334).**
   Border widths already have i16 compact. Add reads for border styles (packed u16)
   and border colors (4×u32) in the normal-state branch.

5. **Add compact fast-paths to `get_style_properties` (handwritten, L1919).**
   This returns `StyleProperties` struct with text_color, letter/word spacing, etc.
   Add normal-state branch reading from Tier 2b.

6. **Add compact fast-paths to `get_line_height_value` and `get_text_indent_value`.**
   Both are simple: read i16 from Tier 2b, decode.

7. **Add compact fast-path to `get_element_font_size` (L58).**
   Read `cc.get_font_size_raw()`, decode u32 → `PixelValue`. For non-px units,
   fall through. This one is tricky because of parent font size dependency.

### Phase 3: Taffy Bridge Refactor (estimated: 4-8 hours)

8. **Refactor `get_css_property_value!` to support compact fast paths.**
   Either: (a) create a `get_css_property_value_compact!` variant, or
   (b) change taffy bridge to consume `MultiValue<T>` where possible.
   Targets: flex-grow, flex-shrink, flex-basis, align-self, gap.

### Phase 4: Optimization (estimated: 2-4 hours)

9. **Remove Tier 3 allocation if not used.** Replace `Vec<Option<Box<BTreeMap>>>`
   with a single global `BTreeMap<(usize, CssPropertyType), CssProperty>` to avoid
   N × 8 bytes of None pointers.

10. **Add `set_css_property()` → compact cache update.** When a property is modified
    at runtime, update the corresponding compact cache entry in O(1) instead of
    rebuilding the entire cache.

11. **Benchmark.** Profile `build_compact_cache()` cost vs. layout savings with
    a large DOM (72K+ nodes) to quantify the actual speedup.

### Phase 5: Long-Term (weeks)

12. **Deprecate raw `CssPropertyCache` access.** Gradually route ALL property access
    through getters.rs so the compact cache becomes the single source of truth for
    resolved properties, with `CssPropertyCache` only used for cascade computation
    (author/UA/inline sources) and pseudo-state resolution.

13. **Incremental compact cache updates.** Instead of full rebuild on restyle, only
    update compact entries for nodes that changed (dirty-flag based).

---

## 10. Should CssPropertyCache Be Kept?

**Yes, for now.** The compact cache cannot fully replace CssPropertyCache because:

1. **Pseudo-state cascade:** `:hover`, `:active`, `:focus` rules stored in CssPropertyCache
   source maps are needed for the slow-path fallback.

2. **Runtime style overrides:** `set_css_property()` writes to CssPropertyCache.

3. **Non-layout properties:** ~155 getters on CssPropertyCache cover all CSS properties
   (backgrounds, transforms, shadows, filters, etc.), not just the ~55 layout properties
   in the compact cache.

4. **Cascade computation:** The compact cache is a **read-only snapshot** built from
   CssPropertyCache after cascade. It doesn't store cascade sources.

**Long-term vision:** CssPropertyCache remains the cascade computation engine
(author/UA/inline → specificity → inheritance). The compact cache becomes the
**read-optimized projection** for the layout hot path. Over time, more getters
gain compact fast paths until the BTreeMap is only hit for pseudo-states and
rare properties.

---

## 11. Summary Table

| Metric | Value |
|--------|-------|
| Total getters in getters.rs | ~98 |
| Getters with compact fast-path | 43 (43%) |
| Properties stored in compact cache | ~55 (T1: 21, T2: 28, T2b: 6) |
| Properties with both storage AND getter fast-path | 42 |
| Properties stored but NOT read via fast path | 16 |
| Tier 3 utilization | 0% (allocated but empty) |
| Build compiles | ✅ |
| Tests pass | Unit tests for encode/decode roundtrips ✅ |
