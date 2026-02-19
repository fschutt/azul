# CssPropertyCache Optimization Plan

**Date:** 2025-02-19  
**Goal:** Eliminate data duplication between BTreeMap-based `CssPropertyCache` and
compact cache; replace BTreeMap internals with sorted `Vec` for cache-friendly access.

---

## 0. Baseline Benchmark (2025-02-19)

| File | LOC | DOM nodes | str_to_dom | layout_doc | pdf_render | TOTAL |
|------|-----|-----------|-----------|-----------|-----------|-------|
| utils.rs | 116 | 2,434 | 86 ms | 178 ms | 300 ms | 1.8 s |
| units.rs | 298 | 6,068 | 201 ms | 312 ms | 636 ms | 1.3 s |
| ops.rs | 415 | 7,731 | 382 ms | 492 ms | 996 ms | 1.7 s |
| font.rs | 806 | 18,576 | 1,042 ms | 885 ms | 2.3 s | 2.9 s |
| lib.rs | 901 | 17,515 | 702 ms | 895 ms | 1.9 s | 2.6 s |
| serialize.rs | 1,578 | 34,672 | 2,099 ms | 2,196 ms | 5.0 s | 6.0 s |
| image.rs | 1,895 | 42,699 | 2,353 ms | 3,231 ms | 6.7 s | 7.7 s |
| graphics.rs | 1,899 | 32,695 | 1,312 ms | 1,905 ms | 4.0 s | 6.2 s |
| deserialize.rs | 3,771 | 77,209 | 9,357 ms | 11,056 ms | 23.7 s | 24.8 s |

### StyledDom::create Breakdown (str_to_dom) — deserialize.rs 77K nodes

| Phase | Time | % |
|-------|------|---|
| restyle | 2,717 ms | 29% |
| ua_css | 888 ms | 9% |
| inherit | 2,456 ms | 26% |
| resolved_cache | 2,176 ms | 23% |
| compact_cache | 920 ms | 10% |
| **total** | **9,312 ms** | |

### layout_document_paged — deserialize.rs 77K nodes

| Phase | Time | % |
|-------|------|---|
| tree_build | 99 ms | 1% |
| layout_loop | 5,892 ms | 53% |
| display_list | 3,562 ms | 32% |
| paginate | 176 ms | 2% |
| **total** | **11,056 ms** | |

---

## 1. Current Architecture Analysis

### 1.1 Data Flow Pipeline

```
CSS file + DOM →  restyle()           →  apply_ua_css()     →  compute_inherited_values()
                  populates:              populates:             populates:
                  css_props               cascaded_props         computed_values
                  (Vec<Vec<               (adds UA defaults)     dependency_chains
                   StatefulCssProperty>>)                        (all BTreeMap-based)
                                                                      ↓
                                      build_resolved_cache()  ←──────┘
                                      populates: resolved_cache
                                      Vec<Vec<(CssPropertyType, CssProperty)>>
                                                  ↓
                                      build_compact_cache()
                                      populates: compact_cache
                                      CompactLayoutCache (Tier 1/2/2b/3)
```

### 1.2 Current Memory Layout per Node (77K nodes)

| Field | Type | Per-node size | Total (77K) | Notes |
|-------|------|--------------|-------------|-------|
| `user_overridden_properties` | `Vec<BTreeMap<..>>` | ~48 B (empty BTreeMap) | 3.7 MB | Almost all empty |
| `cascaded_props` | `Vec<Vec<StatefulCssProperty>>` | ~24 B (Vec header) + N×entries | varies | UA CSS + inherited |
| `css_props` | `Vec<Vec<StatefulCssProperty>>` | ~24 B + N×entries | varies | From CSS rules |
| `computed_values` | `Vec<BTreeMap<CssPropertyType, CssPropertyWithOrigin>>` | ~48 B + entries | ~15 MB | Font/inherit resolution |
| `dependency_chains` | `Vec<BTreeMap<CssPropertyType, CssDependencyChain>>` | ~48 B + entries | ~8 MB | Most nodes: 1-2 entries |
| `resolved_cache` | `Vec<Vec<(CssPropertyType, CssProperty)>>` | ~24 B + N×entries | ~25 MB | **DUPLICATES** all above |
| `compact_cache` | `CompactLayoutCache` | 136 B | 10.5 MB | **DUPLICATES** layout props |
| **TOTAL** | | | **~63+ MB** | For 77K nodes |

### 1.3 Data Duplication Problem

The same property value (e.g., `display: inline`) is stored up to **4 times**:

1. **`css_props`** or **`cascaded_props`** — as `StatefulCssProperty` (source of truth)
2. **`computed_values`** — as `CssPropertyWithOrigin` (after inheritance)
3. **`resolved_cache`** — as `(CssPropertyType, CssProperty)` tuple (pre-resolved)
4. **`compact_cache`** — as bitpacked u64/i16/u32 (numeric encoding)

### 1.4 `StatefulCssProperty` Size

```rust
pub struct StatefulCssProperty {
    pub state: PseudoStateType,   // 1 byte (enum, aligned to 4-8 bytes)
    pub prop_type: CssPropertyType, // 1 byte (enum, aligned to 4-8 bytes)
    pub property: CssProperty,     // variable, largest variant ~72 bytes
}
```

Due to alignment, actual size: **~80 bytes** per entry. For 77K nodes × ~8 props/node 
average = 616K entries × 80 B = **~49 MB** in `css_props` + `cascaded_props` alone.

### 1.5 `CssProperty` Enum Size

The `CssProperty` enum has ~130+ variants. Its size is dominated by the largest variant
(likely `BackgroundContent` or `Transform` which contain `Vec`s). Even small properties 
like `Display(CssPropertyValue<LayoutDisplay>)` pay the full enum size (~40-80 bytes).

### 1.6 BTreeMap Overhead

Each `BTreeMap<CssPropertyType, _>` allocates:
- 48 bytes for the BTreeMap struct itself (even when empty)
- Internal B-tree nodes: 128+ bytes per allocation, with 11 key-value pairs per leaf
- For a BTreeMap with 5 entries: ~48 (struct) + 128 (1 leaf node) = 176 bytes
- For 77K empty BTreeMaps (`user_overridden_properties`): 48 × 77K = 3.7 MB of nothing

---

## 2. Optimization Strategy

### 2.1 Core Principle: Single Source of Truth

Instead of 4 copies of the data, we want:

- **Primary storage**: `resolved_cache` (sorted `Vec<(CssPropertyType, CssProperty)>` per node)
  - This replaces `computed_values`, `css_props`, `cascaded_props` for normal-state lookups
  - During initial layout/PDF: 100% of lookups go here
- **Fast-path cache**: `compact_cache` for layout-hot numeric properties (already exists)
- **Pseudo-state storage**: Only needed for interactive (non-PDF) use cases
  - Keep `css_props` / `cascaded_props` but ONLY for non-Normal pseudo states
  - For PDF mode: skip storing pseudo-state props entirely

### 2.2 Overview of Changes

```
BEFORE:  css_props(all states) + cascaded_props(all states) + computed_values + 
         dependency_chains + resolved_cache + compact_cache
         = 6 storage layers, ~63 MB for 77K nodes

AFTER:   resolved_cache (only) + compact_cache (subset) + 
         pseudo_state_props (sparse, optional)
         = 2-3 storage layers, ~20-25 MB for 77K nodes
```

---

## 3. Phased Implementation Plan

### Phase 1: Eliminate `user_overridden_properties` BTreeMap (Easy, ~1 hour)

**Problem:** `Vec<BTreeMap<...>>` wastes 48 B per node even when empty (99%+ nodes).

**Change:** Replace with `Vec<Vec<(CssPropertyType, CssProperty)>>`, initialized to 
empty `Vec` (0 bytes when empty, only 24 B for the Vec header on the outer Vec slot).

**Files:**
- `core/src/prop_cache.rs`: Change type, update `get_property_slow()` to use 
  `binary_search_by_key` instead of `BTreeMap::get`
- Update any `user_overridden_properties[i].insert()` calls to use sorted-vec insert

**Impact:** Saves ~2.4 MB for 77K nodes (48 → 0 bytes for empty entries).
Improves drop time (no BTreeMap destructors).

**Risk:** Low. Very localized change.

### Phase 2: Eliminate `computed_values` and `dependency_chains` BTreeMaps (~2 hours)

**Problem:** These two fields use BTreeMap per node, costing ~96 B each even when 
empty. The `computed_values` content is already fully captured in `resolved_cache`.

**Change:**
- Remove `computed_values` field entirely
- Remove `dependency_chains` field entirely  
- Modify `compute_inherited_values()` to write results directly into a temporary 
  `Vec<Vec<(CssPropertyType, CssProperty)>>` that becomes `resolved_cache`
- Merge `compute_inherited_values()` + `build_resolved_cache()` into a single pass

**Key insight:** `build_resolved_cache()` calls `get_property_slow()` for every node, 
which reads from `computed_values`. If we merge the two passes, we never need to 
store `computed_values` as a separate field — we build the resolved cache directly 
during inheritance traversal.

**Files:**
- `core/src/prop_cache.rs`: Remove fields, merge functions
- `core/src/styled_dom.rs`: Update pipeline call order

**Impact:** Saves ~23 MB for 77K nodes. Eliminates one full pass over all nodes.
Estimated speedup: 2-3 seconds on deserialize.rs (eliminating `build_resolved_cache` 
pass + reducing `compute_inherited_values` overhead).

**Risk:** Medium. `compute_inherited_values` logic is complex. Needs careful testing
to ensure inheritance order is preserved.

### Phase 3: Shrink `css_props` and `cascaded_props` — Skip Normal-State Duplication (~3 hours)

**Problem:** After Phase 2, `resolved_cache` already contains ALL resolved Normal-state 
properties. But `css_props` and `cascaded_props` still store Normal-state entries
(for the cascade resolution that already happened). For PDF mode, these are never 
read again after the resolved cache is built.

**Change:**
- After `resolved_cache` is built, strip Normal-state entries from `css_props` and 
  `cascaded_props`, keeping only Hover/Active/Focus/Dragging/DragOver entries
- For PDF mode (`restyle_for_print()`): Don't store non-Normal entries at all
- Alternatively: use a flag to lazily drop the source data after resolved_cache is built

**Files:**
- `core/src/prop_cache.rs`: Add `strip_normal_state_props()` method
- `core/src/styled_dom.rs`: Call after `build_resolved_cache`

**Impact:** For PDF mode, reduces `css_props` + `cascaded_props` to near-zero.
For interactive mode, reduces them by ~80% (most props are Normal-state).

**Risk:** Low for PDF. Medium for interactive (need to verify that 
`invalidate_resolved_node()` path can re-read from remaining pseudo-state props).

### Phase 4: Move compact_cache properties OUT of resolved_cache (~2 hours)

**Problem:** Properties that are in `compact_cache` (display, position, padding, margin,
border-width, etc. — 42 properties) are ALSO stored in `resolved_cache` as full
`CssProperty` enum values (~40-80 bytes each). This is pure duplication.

**Change:**
- When building `resolved_cache`, SKIP properties that are already covered by
  `compact_cache` fast-path getters (the 42 properties with compact fast-paths)
- Modify `get_property()` to check compact cache first for these property types,
  then fall back to `resolved_cache` only for non-compact properties

**Implementation detail:**
```rust
// In CssPropertyType, add a method:
impl CssPropertyType {
    /// Returns true if this property is handled by the compact cache fast path
    pub fn is_compact_cached(&self) -> bool {
        matches!(self,
            CssPropertyType::Display | CssPropertyType::Position | 
            CssPropertyType::Float | CssPropertyType::OverflowX | 
            CssPropertyType::OverflowY | CssPropertyType::BoxSizing |
            // ... all 42 compact-cached properties
        )
    }
}

// In build_resolved_cache, skip compact-cached properties:
for prop_type in &prop_types {
    if prop_type.is_compact_cached() {
        continue; // Already in compact cache
    }
    // ... resolve and store
}
```

**Impact:** Reduces `resolved_cache` entries by ~42 per node.
For 77K nodes: saves ~77K × 42 × ~50 B = ~162 MB of `CssProperty` allocations avoided
(or more precisely, ~50% fewer entries per node in resolved_cache).

**Risk:** Medium. The `get_property()` dispatch needs to handle the split correctly.
The compact cache only handles normal-state — for pseudo-states, we need the 
resolved_cache to still have these properties. But for PDF mode (no pseudo-states),
this is a clean win.

### Phase 5: Replace remaining BTreeMap references in `get_property_slow` (~2 hours)

**Problem:** `get_property_slow()` iterates through `css_props` and `cascaded_props`
using `Vec::iter().find()` (already linear scan). But `computed_values` lookup was 
BTreeMap. After Phase 2, this is gone.

**Change:** Ensure ALL lookups in the cascade path use sorted-Vec + binary_search.

For `StatefulCssProperty` Vecs, the lookup pattern is:
```rust
// Current: linear scan
props.iter().find(|p| p.state == state && p.prop_type == prop_type)

// Better: sort by (state, prop_type), use binary_search
// This requires sorting css_props and cascaded_props after populating them
```

**Implementation:**
- Sort `css_props[i]` and `cascaded_props[i]` by `(state, prop_type)` after restyle
- Replace `find()` with `binary_search_by_key()` or `partition_point()`
- For the Normal-state-only case (PDF mode), this reduces lookup from O(n) to O(log n)
  where n is the number of properties per node

**Files:**
- `core/src/prop_cache.rs`: Sort Vecs after restyle, update lookup functions

**Impact:** Faster cascade resolution during `build_resolved_cache`. 
Each `get_property_slow` call goes from O(n×6 states) to O(log n).

**Risk:** Low. Straightforward data structure change.

### Phase 6: PDF-only fast path — `restyle_for_print()` (~3 hours) 

**Problem:** `restyle()` processes 6 pseudo-states (Normal, Hover, Active, Focus, 
Dragging, DragOver) when PDF only needs Normal. 5/6 of the work is wasted.

**Change:** Add `restyle_for_print()` that:
- Only evaluates Normal-state CSS selectors
- Skips tag ID generation (no hit-testing in PDF)
- Skips hover/active/focus pseudo-state code paths entirely
- Stores properties directly into a pre-sorted Vec (no BTreeMap intermediate)

**Files:**
- `core/src/prop_cache.rs`: New `restyle_for_print()` method
- `core/src/styled_dom.rs`: Call `restyle_for_print()` when in PDF mode
- Need a way to signal "PDF mode" (parameter or configuration flag)

**Impact:** ~800 ms savings on deserialize.rs restyle phase. Also reduces memory by
not allocating non-Normal property storage.

**Risk:** Medium. Need to ensure the print path produces identical results for 
Normal-state properties. Need a way to propagate "PDF mode" flag through the API.

### Phase 7: Fix `apply_ua_css` O(n²) scaling (~1 hour)

**Problem:** `apply_ua_css()` iterates 14 property types × n nodes, calling
`.iter().any()` on each node's props for each property type. For 77K nodes, this is
77K × 14 × scan = O(n × k × m) where k=14, m=avg props per node.

Currently costs 888 ms for 77K nodes (quadratic scaling observed: O(n^2.03)).

**Change:**
- Build a bitset per node of "which property types are already set" during restyle
- In `apply_ua_css`, only insert UA properties for types not in the bitset
- This eliminates the triple-nested loop

**Implementation:**
```rust
// During restyle, build a u128 bitmap per node:
let mut prop_set: Vec<u128> = vec![0u128; node_count];
// For each property stored, set bit: prop_set[node_id] |= 1 << (prop_type as u8)
// In apply_ua_css, check: if !(prop_set[node_id] & (1 << pt_bit)) { insert }
```

**Files:**
- `core/src/prop_cache.rs`: Add bitmap tracking, simplify `apply_ua_css`

**Impact:** `apply_ua_css` goes from 888 ms → ~10 ms for 77K nodes.

**Risk:** Low. Simple bitmap optimization.

---

## 4. Memory Impact Summary

| Layer | Before (77K nodes) | After Phase 1 | After Phase 2 | After Phase 4 |
|-------|-------------------|--------------|--------------|--------------|
| `user_overridden_properties` | 3.7 MB | 1.8 MB | 1.8 MB | 1.8 MB |
| `computed_values` | 15 MB | 15 MB | **0 MB** | 0 MB |
| `dependency_chains` | 8 MB | 8 MB | **0 MB** | 0 MB |
| `css_props` (normal) | ~20 MB | ~20 MB | ~20 MB | ~20 MB |
| `css_props` (after strip) | — | — | — | ~4 MB |
| `cascaded_props` (normal) | ~10 MB | ~10 MB | ~10 MB | ~10 MB |
| `cascaded_props` (after strip) | — | — | — | ~2 MB |
| `resolved_cache` | 25 MB | 25 MB | 25 MB | **12 MB** |
| `compact_cache` | 10.5 MB | 10.5 MB | 10.5 MB | 10.5 MB |
| **TOTAL** | **~92 MB** | **~90 MB** | **~67 MB** | **~30 MB** |

**PDF mode (Phase 3 + 6):** css_props and cascaded_props stripped of Normal-state 
entries → total drops to ~**25 MB** for 77K nodes.

---

## 5. Performance Impact Summary

**Target: deserialize.rs (77K nodes)**

| Phase | Component | Before | After (est.) | Savings |
|-------|-----------|--------|-------------|---------|
| Phase 1 | Drop time | ~2.4 s | ~1.5 s | ~0.9 s |
| Phase 2 | inherit + resolved_cache | 4.6 s | ~2.5 s | ~2.1 s |
| Phase 3 | Drop time (reduced data) | ~1.5 s | ~0.5 s | ~1.0 s |
| Phase 4 | resolved_cache build | incl above | -0.3 s | ~0.3 s |
| Phase 5 | get_property_slow | (amortized) | -0.2 s | ~0.2 s |
| Phase 6 | restyle | 2.7 s | ~1.9 s | ~0.8 s |
| Phase 7 | ua_css | 0.9 s | ~0.01 s | ~0.9 s |
| **TOTAL** | | **~24.8 s** | **~18.6 s** | **~6.2 s** |

Note: Layout_loop (5.9 s) and display_list (3.6 s) are NOT addressed by this plan.
They require separate optimization (mostly in getters.rs and display_list.rs).

---

## 6. File Change Summary

| File | Changes | Phase |
|------|---------|-------|
| `core/src/prop_cache.rs` | Major restructuring of CssPropertyCache fields and methods | 1-7 |
| `core/src/styled_dom.rs` | Pipeline call order, merge passes | 2, 3, 6 |
| `core/src/compact_cache_builder.rs` | Read from resolved_cache instead of prop_cache | 2, 4 |
| `css/src/props/property.rs` | Add `is_compact_cached()` method | 4 |
| `css/src/compact_cache.rs` | No structural changes needed | — |
| `layout/src/solver3/getters.rs` | Update getter dispatch for Phase 4 | 4 |

---

## 7. Testing Strategy

### 7.1 Correctness Tests

1. **Existing reftest suite** — run before/after each phase to verify layout output is
   identical (pixel-perfect comparison)
2. **Property resolution tests** — for each phase, add unit tests verifying that
   `get_property()` returns the same value through the new code path as the old one
3. **Inheritance tests** — specifically test that `font-size: 2em` inheritance chain
   produces the same result after Phase 2 (merged passes)

### 7.2 Performance Tests

After each phase:
```bash
cd /Users/fschutt/Development/git2pdf
cargo build --release
bash run_bench.sh
# Compare bench_output/bench_all.log timings
```

### 7.3 Memory Tests

Add a debug print in `StyledDom::create` to report:
```rust
eprintln!("  [Memory] user_overridden_properties: {} entries",
    css_property_cache.user_overridden_properties.iter().map(|m| m.len()).sum::<usize>());
eprintln!("  [Memory] cascaded_props: {} entries",
    css_property_cache.cascaded_props.iter().map(|v| v.len()).sum::<usize>());
eprintln!("  [Memory] css_props: {} entries",
    css_property_cache.css_props.iter().map(|v| v.len()).sum::<usize>());
eprintln!("  [Memory] resolved_cache: {} entries",
    css_property_cache.resolved_cache.iter().map(|v| v.len()).sum::<usize>());
```

---

## 8. Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Inheritance order changes when merging passes | Medium | Compare output property-by-property for test DOMs before/after |
| Pseudo-state lookups break after Normal-state stripping | Low | Only strip in PDF mode; for interactive, keep all data |
| Compact cache getters return different values after resolved_cache split | Medium | Verify with property-resolution unit tests |
| `set_css_property()` stops working after removing `computed_values` | Low | Route through `user_overridden_properties` which stays |
| Layout regressions from PDF-only fast path | Medium | Guard behind `for_print` flag; keep standard path as fallback |

---

## 9. Implementation Order Recommendation

**Start with Phase 7 (ua_css fix) — lowest risk, highest bang-for-buck (900 ms → 10 ms).**

Then Phase 1 (user_overridden_properties) — also low risk, removes BTreeMap waste.

Then Phase 6 (restyle_for_print) — isolated new function, doesn't modify existing code.

Then Phase 2 + 3 (merge passes, strip data) — the big architectural change.

Finally Phase 4 + 5 (resolved_cache dedup, binary search) — refinement.

Suggested order: **7 → 1 → 6 → 2 → 3 → 4 → 5**

---

## 10. Long-Term Vision

Once Phases 1-7 are complete, the `CssPropertyCache` for PDF mode becomes:

```
CssPropertyCache {
    node_count: usize,
    // One sorted Vec per node with ONLY non-compact-cached properties
    resolved_cache: Vec<Vec<(CssPropertyType, CssProperty)>>,
    // Compact layout cache for 42 layout-hot properties  
    compact_cache: Option<CompactLayoutCache>,
    // Empty in PDF mode, sparse in interactive mode
    user_overridden_properties: Vec<Vec<(CssPropertyType, CssProperty)>>,
    // Only pseudo-state properties (empty in PDF mode)
    pseudo_state_props: Vec<Vec<StatefulCssProperty>>,
}
```

Total per-node cost: ~136 B (compact) + ~24 B (Vec header) + ~5 non-compact props × ~50 B 
= **~410 B per node** vs current **~1200+ B per node**.

For 77K nodes: ~32 MB vs ~92 MB = **65% memory reduction**.
