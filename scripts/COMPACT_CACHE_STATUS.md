# Compact Cache Status Report

**Date:** 2026-02-17  
**Branch:** `master` (merged from `getter-migration`)  
**Build:** `cargo build -p azul-dll --features build-dll` ✅ passes

---

## 1. Current State Summary

### What Was Done (getter-migration, 5 commits)

1. **Centralized all CSS property access** into `layout/src/solver3/getters.rs`
2. **Removed direct `css_property_cache` access** from 6 solver files:
   - `fc.rs`, `taffy_bridge.rs`, `display_list.rs`, `cache.rs`, `positioning.rs`, `layout_tree.rs`
3. **3 macro types** handle the routine getters:
   - `get_css_property!` — enum properties → `MultiValue<T>` (13 invocations)
   - `get_css_property_pixel!` — dimension/spacing → `MultiValue<PixelValue>` (16 invocations)
   - `get_css_property_value!` — taffy bridge raw values → `Option<CssPropertyValue<T>>` (18 invocations)
4. **~37 handwritten getters** for complex cases (backgrounds, borders, fonts, scrollbar, etc.)
5. **84 public functions** total in getters.rs (3,473 lines)

### What Does NOT Exist Yet

- `core/src/compact_cache.rs` — **not created**
- No `CompactLayoutCache`, `CompactNodeProps`, `CompactTextProps` structs
- No `build_compact_cache()` function
- No fast-path in any getter — **all getters still go through the BTreeMap slow path**

---

## 2. Are the Getters Using a Fast Path?

**No.** Every getter currently calls:

```
styled_dom.css_property_cache.ptr.$method(node_data, &node_id, node_state)
```

Which internally calls `get_property_slow()` → walks the cascade:
1. `user_overridden_properties` (user JS/Rust overrides)
2. `css_*_props` per pseudo-state (hover/active/focus/dragging)
3. `cascaded_*_props` (CSS stylesheet)
4. `computed_values` (inherited)
5. UA CSS fallback

Each step does **BTreeMap::get** (O(log n) with pointer chasing per lookup).

The macros also add a second UA CSS check via `azul_core::ua_css::get_ua_property()`,
which is redundant with the cascade but harmless.

---

## 3. Remaining Direct `css_property_cache` Access Outside getters.rs

| File | Line | Usage | Can Be Moved? |
|------|------|-------|---------------|
| `layout/src/solver3/getters.rs` | 79× | All accesses (the canonical location) | N/A |
| `layout/src/solver3/layout_tree.rs` | L1484 | `dependency_chains` for font-size | ⚠️ Special — needs dependency chain, not a simple prop |
| `layout/src/hit_test.rs` | L119 | `get_cursor()` for hit testing | ✅ Could use `get_cursor_property()` |
| `layout/src/callbacks.rs` | L2420-2421 | Direct cache access for callbacks | ⚠️ May need full CssPropertyCache access |

**Verdict:** The migration is ~97% complete. The 3 remaining call sites are edge
cases (dependency chains, hit testing, callbacks) that don't benefit from compact
cache anyway.

---

## 4. Property Tier Assignment Audit

### Tier 1: Enum properties → `Vec<u64>` bitpacked (8 B/node)

All 20 enum properties from the plan have corresponding getters:

| Property | Getter | Bits | Status |
|----------|--------|------|--------|
| display | `get_display_property` / `get_display_property_internal` | 5 | ✅ Has getter |
| position | `get_position` | 3 | ✅ Has getter |
| float | `get_float` | 2 | ✅ Has getter |
| overflow_x | `get_overflow_x` | 3 | ✅ Has getter |
| overflow_y | `get_overflow_y` | 3 | ✅ Has getter |
| box_sizing | `get_css_box_sizing` | 1 | ✅ Has getter |
| flex_direction | `get_flex_direction_prop` | 2 | ✅ Has getter (via `get_css_property_value!`) |
| flex_wrap | `get_wrap` / `get_flex_wrap_prop` | 2 | ✅ Has getter |
| justify_content | `get_justify_content` / `get_justify_content_prop` | 3 | ✅ Has getter |
| align_items | `get_align_items_prop` | 3 | ✅ Has getter (via `get_css_property_value!`) |
| align_content | `get_align_content_prop` | 3 | ✅ Has getter (via `get_css_property_value!`) |
| writing_mode | `get_writing_mode` | 2 | ✅ Has getter |
| clear | `get_clear` | 2 | ✅ Has getter |
| font_weight | (accessed in `get_style_properties`) | 4 | ⚠️ No standalone getter — inlined in `get_style_properties()` |
| font_style | (accessed in `get_style_properties`) | 2 | ⚠️ No standalone getter — inlined in `get_style_properties()` |
| text_align | `get_text_align` | 3 | ✅ Has getter |
| visibility | `get_visibility` | 1 | ✅ Has getter |
| white_space | `get_white_space_prop` | 3 | ✅ Has getter |
| direction | `get_direction` | 1 | ✅ Has getter |
| vertical_align | `get_vertical_align_raw` / `get_vertical_align_for_node` | 3 | ✅ Has getter |

**Total: 51 bits used, 13 spare.** All enum properties are covered.

### Tier 2: Numeric dimensions → `CompactNodeProps` (64 B/node)

| Property | Getter | Encoding | Status |
|----------|--------|----------|--------|
| width | `get_css_width` | u32 MSB-sentinel | ✅ |
| height | `get_css_height` | u32 | ✅ |
| min_width | `get_css_min_width` | u32 | ✅ |
| max_width | `get_css_max_width` | u32 | ✅ |
| min_height | `get_css_min_height` | u32 | ✅ |
| max_height | `get_css_max_height` | u32 | ✅ |
| flex_basis | `get_flex_basis_prop` | u32 | ✅ |
| font_size | `get_element_font_size` | u32 | ✅ |
| padding_top/right/bottom/left | `get_css_padding_*` | i16 ×10 | ✅ (4 getters) |
| margin_top/right/bottom/left | `get_css_margin_*` | i16 ×10 | ✅ (4 getters) |
| border_top/right/bottom/left_width | `get_css_border_*_width` | i16 ×10 | ✅ (4 getters) |
| top/right/bottom/left | `get_css_top/right/bottom/left` | i16 ×10 | ✅ (4 getters) |
| flex_grow | `get_flex_grow_prop` | u16 ×100 | ✅ |
| flex_shrink | `get_flex_shrink_prop` | u16 ×100 | ✅ |
| z_index | `get_z_index` | i16 | ✅ |

**All 25 dimension properties have getters. Ready for Tier 2.**

### Tier 2b: Text/IFC properties → `CompactTextProps` (24 B/node)

| Property | Getter | Encoding | Status |
|----------|--------|----------|--------|
| text_color | `get_style_properties` (inline) | u32 RGBA | ⚠️ No standalone getter |
| font_family | `get_style_properties` (inline) | u64 hash | ⚠️ No standalone getter |
| line_height | `get_line_height_value` | i16 ×10 | ✅ |
| letter_spacing | `get_style_properties` (inline) | i16 ×10 | ⚠️ No standalone getter |
| word_spacing | `get_style_properties` (inline) | i16 ×10 | ⚠️ No standalone getter |
| text_indent | `get_text_indent_value` | i16 ×10 | ✅ |

**4 of 6 text props lack standalone getters** — they're embedded in the 
monolithic `get_style_properties()` function (L1463-L1789). This function
builds a full `StyleProperties` struct with font resolution, color fallback, etc.

### Tier 3: Overflow / Rare Properties → `FxHashMap`

Everything else: grid props (7 getters), transforms, filters, box-shadow (4 sides),
backgrounds, borders (styles/colors), scrollbar, selection, caret, counters,
shape-inside/outside, fragmentation (break-before/after/inside, orphans, widows),
opacity, text-decoration, tab-size, etc.

Currently **18 `get_css_property_value!` getters** for taffy bridge (flex/grid/alignment)
would also fall into Tier 3 for the raw `CssPropertyValue<T>` wrapper.

---

## 5. Edge Cases & Challenges

### 5.1 Inheritance

The current `CssPropertyCache.computed_values` handles CSS inheritance
(font-size, color, line-height, etc. flow from parent to child). The compact cache
would be built **after** `compute_inherited_values()`, so it sees the final
resolved values. No change needed — compact cache is a read-only snapshot.

### 5.2 Dynamic State (hover, active, focus)

`get_property_slow()` checks pseudo-state maps: `css_hover_props`, `css_active_props`,
`css_focus_props`, etc. The compact cache must be **rebuilt** (or patched) when
node states change. Options:

- **Full rebuild** after state change — simplest. If build is fast enough (~3ms for 72K), acceptable.
- **Per-node patch** — only update changed nodes' rows. More complex but avoids full rebuild.
- **Stateless compact cache** — only cache the "normal" state, fall through to slow path
  for hover/active/focus. Since most nodes are in normal state, this still wins.

**Recommendation:** Start with option 3 (stateless cache for normal state only).
Nodes with active hover/focus state (~0.1% of nodes) use the slow path — negligible impact.

### 5.3 Runtime Property Overrides

`user_overridden_properties` is the highest-priority layer (set via JS/Rust API at runtime).
If the compact cache doesn't check it, runtime overrides would be invisible.

**Solution:** The compact cache builder includes user overrides (it calls `get_property_slow()`
which already checks that layer first). For **subsequent** overrides after build,
either rebuild or invalidate the affected node's row.

### 5.4 Font Resolution Chains

`dependency_chains` in `CssPropertyCache` tracks font-size resolution with
em/rem dependencies. This is accessed in `layout_tree.rs:1484` and
`get_element_font_size()`. The compact cache stores **resolved pixel values**
for font-size, so dependency chains are only needed during build.

### 5.5 Taffy Bridge (`get_css_property_value!` getters)

The 18 taffy bridge getters return `Option<CssPropertyValue<T>>` (the raw wrapper
including Auto/Initial/Inherit). The compact cache encodes Auto/Initial as sentinels,
so these can be reconstructed, but the mapping is more complex than simple value retrieval.

**Recommendation:** For taffy bridge getters, use Tier 2 encoding for flex/gap properties,
and Tier 3 fallback for grid properties (rarely used, complex types).

---

## 6. Recommended Path Forward

### Phase 1: Create compact_cache.rs (Tier 1 only)

**Impact: Eliminates ~40% of BTreeMap lookups** (display, position, overflow accessed 44+37+45 = 126× per node)

1. Create `core/src/compact_cache.rs` with:
   - `CompactLayoutCache { tier1_enums: Vec<u64> }`
   - Encode/decode functions for all 20 enum props
   - `build_tier1()` that iterates nodes, calls `get_property_slow()` for each enum

2. Add `pub compact_cache: Option<Box<CompactLayoutCache>>` to `CssPropertyCache`

3. Wire `build_tier1()` in `styled_dom.rs` after `compute_inherited_values()`

4. Update 12 `get_css_property!` getters + `get_display_property` to check Tier 1 first:
   ```rust
   if let Some(cc) = &styled_dom.css_property_cache.ptr.compact_cache {
       return cc.get_display(node_id.index());
   }
   // fallback to slow path
   ```

**Estimated effort:** ~400 lines of code, ~2 hours.

### Phase 2: Add Tier 2 dimensions

**Impact: Eliminates another ~20% of BTreeMap lookups**

1. Add `CompactNodeProps` (64 B/node) struct to `compact_cache.rs`
2. Add `tier2_dims: Vec<CompactNodeProps>` to `CompactLayoutCache`
3. Implement MSB-sentinel encoding/decoding
4. Update 16 `get_css_property_pixel!` getters + 4 `get_css_min/max_*` getters

**Estimated effort:** ~600 lines, ~3 hours.

### Phase 3: Add Tier 2b text props

**Impact: Eliminates font-family cloning and text property BTreeMap lookups**

1. Add `CompactTextProps` (24 B/node) struct
2. Extract standalone getters from `get_style_properties()` for text_color,
   font_family_hash, letter_spacing, word_spacing
3. Update `get_style_properties()` and IFC text getters

**Estimated effort:** ~300 lines, ~2 hours.

### Phase 4: Tier 3 overflow + taffy bridge

**Impact: Complete coverage, no BTreeMap on hot paths**

1. Add `tier3_overflow: Vec<Option<Box<FxHashMap<CssPropertyType, CssProperty>>>>`
2. Route 18 taffy bridge getters through compact cache
3. Grid properties stay in Tier 3

**Estimated effort:** ~200 lines, ~1 hour.

---

## 7. Key Design Decision: Keep CssPropertyCache

**Recommendation: Keep CssPropertyCache.** Don't replace it.

The compact cache is a **read-only acceleration layer** built on top of the existing
CssPropertyCache. Reasons:

1. **CssPropertyCache handles writes** — set_property(), user overrides, cascade
2. **CssPropertyCache handles the full cascade** — needed for build_compact_cache()
3. **CssPropertyCache handles all pseudo-states** — compact cache can start with normal-only
4. **Paint properties** (backgrounds, transforms, filters, etc.) don't need compact cache
   — they're accessed ~1× per node during display list generation, not in tight loops
5. **Incremental migration** — each phase independently speeds up the hot path without
   risking regressions

The goal is: **hot layout loops read from compact cache → cold/rare paths read from CssPropertyCache.**

---

## 8. File Change Summary

| File | Phase | Change |
|------|-------|--------|
| `core/src/compact_cache.rs` | 1 | **NEW** — CompactLayoutCache + Tier 1 |
| `core/src/lib.rs` | 1 | Add `pub mod compact_cache;` |
| `core/src/prop_cache.rs` | 1 | Add `compact_cache` field to CssPropertyCache |
| `core/src/styled_dom.rs` | 1 | Call `build_compact_cache()` after restyle |
| `layout/src/solver3/getters.rs` | 1-3 | Add fast-path checks in macros/getters |
| `core/Cargo.toml` | 4 | Add `rustc-hash` for FxHashMap (Tier 3 only) |
