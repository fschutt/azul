# Font Invalidation & Solver3 Memory Layout Analysis

## Part 1: `font_stacks_hash` and Font Invalidation

### 1.1 Overview

`font_stacks_hash` is a **window-global** XOR fingerprint of all per-node font family hashes.
It enables skipping the entire 5-step font resolution pipeline when fonts haven't changed between frames.

### 1.2 Hash Computation

**Per-node `font_family_hash`** (`core/src/compact_cache_builder.rs:351–357`):

```rust
let mut hasher = DefaultHasher::new();
families.hash(&mut hasher);
let h = hasher.finish();
result.tier2b_text[i].font_family_hash = if h == 0 { 1 } else { h };
```

Input: the complete `StyleFontFamilyVec` for each node. `0` is reserved as "unset" sentinel.

**Global `font_stacks_hash`** (`layout/src/window.rs:687–697`):

```rust
let current_font_hash: u64 = styled_dom
    .css_property_cache.ptr.compact_cache.as_ref()
    .map(|cc| cc.tier2b_text.iter().fold(0u64, |acc, t| acc ^ t.font_family_hash))
    .unwrap_or(0);
```

### 1.3 Skip Decision

(`layout/src/window.rs:701–703`):

```rust
let font_requirements_unchanged = current_font_hash != 0
    && current_font_hash == self.font_stacks_hash
    && !self.font_manager.font_chain_cache.is_empty();
```

If all three conditions hold → **skip Steps 0–5** (collect, resolve, diff, load, cache).

### 1.4 Invalidation Chain

```
CSS property change
  → restyle() + compute_inherited_values()
    → build_compact_cache()
      → per-node font_family_hash = FxHash(StyleFontFamilyVec)
        → XOR all hashes → current_font_hash
          → compare with stored font_stacks_hash
            → if different: run full font resolution pipeline
              → collect_and_resolve_font_chains()
                → collect_font_ids_from_chains()
                  → compute_fonts_to_load()
                    → load_fonts_from_disk()
                      → set_font_chain_cache()
                        → IFC layout_flow() uses new chains
                          → shape_visual_items() resolves FontChainKey → shaping
```

### 1.5 User's Three Points

#### Point 1: "A font can only change if a CSS property changed"

**Correct.** The `font_family_hash` is derived from `StyleFontFamilyVec` in the `CompactCache`,
which is only rebuilt after `build_compact_cache()`, which only runs after `restyle()` processes
CSS property changes. No CSS change → no compact cache rebuild → no hash change → fonts unchanged.

#### Point 2: "Changing the font stack invalidates IFC text layout"

**Partially implemented.** When `font_stacks_hash` changes, the entire font resolution pipeline
runs and `font_chain_cache` is rebuilt. Then in `layout_ifc()` (`layout/src/solver3/fc.rs:2197`),
the new font chains are passed to `text_cache.layout_flow()`, which calls `shape_visual_items()`
with the updated `font_chain_cache`.

**However:** There is no incremental IFC invalidation. The stub at `layout/src/solver3/fc.rs:2155–2173`
notes that Phase 2c/2d (incremental IFC relayout) is not yet implemented. Currently, IFC always
does a full relayout for every node that enters the IFC path.

#### Point 3: "Global flag is unclean vs granular dirty flags"

**Correct — this is the main weakness.** The current design:

| Flag | Scope | Granularity |
|------|-------|-------------|
| `font_stacks_hash` (XOR) | Window-global | All-or-nothing |
| `DirtyFlag` (None/Paint/Layout) | Per-node | Layout geometry only |
| `dirty_text_nodes` (BTreeMap) | Per-node | Text content edits only |
| `TextConstraintsCache` | Per-node | Constraint caching for text edits |

**Missing:** Per-node font-dirty tracking. If one node's `font-family` changes, the global XOR
changes and **all** font chains are re-resolved for **all** nodes, not just the affected one.

**XOR collision risk:** Adding and removing the same font in the same frame produces an unchanged
XOR hash, causing a false-negative (missed invalidation). In practice this is unlikely but
theoretically unsound.

**Proposed improvement:** A per-node `font_family_hash` dirty bit in `CompactTextProps` (which
already stores the hash), combined with a `BTreeSet<NodeId>` of nodes-with-changed-fonts,
would allow re-resolving only affected font chains.

---

## Part 2: `solver3::layout_document` Memory Layout

### 2.1 Function Signature

(`layout/src/solver3/mod.rs:402–420`):

```rust
pub fn layout_document<T: ParsedFontTrait + Sync + 'static>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    new_dom: StyledDom,                                    // Owned
    viewport: LogicalRect,                                 // 16 B
    font_manager: &FontManager<T>,                         // Ref
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,     // Ref
    selections: &BTreeMap<DomId, SelectionState>,          // Ref
    text_selections: &BTreeMap<DomId, TextSelection>,      // Ref
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,  // Ref
    gpu_value_cache: Option<&GpuValueCache>,               // Opt ref
    renderer_resources: &RendererResources,                // Ref
    id_namespace: IdNamespace,                             // Copy
    dom_id: DomId,                                         // Copy
    cursor_is_visible: bool,                               // 1 B
    cursor_location: Option<(DomId, NodeId, TextCursor)>,  // ~24 B
    system_style: Option<Arc<SystemStyle>>,                 // 8 B
    get_system_time_fn: GetSystemTimeCallback,              // 8 B
) -> Result<DisplayList>
```

17 parameters total. Internally constructs a `LayoutContext<'a, T>`.

### 2.2 Core Data Structures (Per-Node)

#### StyledDom (owned input)

```
StyledDom {
    root: NodeHierarchyItemId                          8 B
    node_hierarchy: Vec<NodeHierarchyItem>            24 B header, 32 B/node
    node_data: Vec<NodeData>                          24 B header, ~120 B/node
    styled_nodes: Vec<StyledNode>                     24 B header, ~10 B/node
    cascade_info: Vec<CascadeInfo>                    24 B header, 8 B/node
    css_property_cache: CssPropertyCachePtr           16 B (Box + bool)
    ...                                               ~72 B (misc index vecs)
}
```

#### CompactLayoutCache (SoA-optimized, inside CssPropertyCache)

```
CompactLayoutCache {
    tier1_enums: Vec<u64>                              8 B/node  (bitpacked enum props)
    tier2_dims:  Vec<CompactNodeProps>                 96 B/node  (numeric dimensions, repr(C))
    tier2b_text: Vec<CompactTextProps>                 24 B/node  (text props, repr(C))
}
Total: 128 B/node in 3 contiguous arrays
```

#### LayoutNode (AoS, the big one)

```
LayoutNode {                                          ~500-600 B/node
    dom_node_id: Option<NodeId>                        16 B
    children: Vec<usize>                               24 B (+ heap)
    parent: Option<usize>                              16 B
    dirty_flag: DirtyFlag                               1 B
    box_props: ResolvedBoxProps                        52 B
    unresolved_box_props: UnresolvedBoxProps           ~96 B
    computed_style: ComputedLayoutStyle                ~80 B
    node_data_fingerprint: NodeDataFingerprint          48 B
    inline_layout_result: Option<CachedInlineLayout>   ~72 B
    formatting_context: FormattingContext               16 B
    intrinsic_sizes: Option<IntrinsicSizes>            28 B
    used_size: Option<LogicalSize>                     12 B
    ...                                                ~60 B (misc optional fields)
}
```

#### NodeCache (Taffy-style 9+1 cache)

```
NodeCache {
    measure_entries: [Option<SizingCacheEntry>; 9]    ~180 B
    layout_entry: Option<LayoutCacheEntry>             ~80 B (+ heap for child positions)
    is_empty: bool                                       1 B
}
Total: ~260 B/node
```

### 2.3 Memory Footprint Per Node

| Structure | B/node | Access Pattern | SoA/AoS |
|-----------|-------:|----------------|---------|
| CompactLayoutCache tier1 | 8 | **Hot** — O(1) index | SoA ✓ |
| CompactLayoutCache tier2 | 96 | **Hot** — O(1) index | SoA ✓ |
| CompactLayoutCache tier2b | 24 | Warm — text nodes only | SoA ✓ |
| NodeHierarchyItem | 32 | **Hot** — traversal | SoA ✓ |
| NodeData | ~120 | Warm — fingerprinting | SoA ✓ |
| StyledNode | ~10 | Warm — state checks | SoA ✓ |
| **LayoutNode** | **~550** | **Hot — layout core** | **AoS ✗** |
| NodeCache | ~260 | Hot — cache lookup | SoA ✓ |
| calculated_positions | 8 | Hot — position output | SoA ✓ |

**Total per node ≈ 1,100–1,200 bytes**

| Document Size | Estimated Memory |
|--------------:|-----------------:|
| 100 nodes | ~120 KB |
| 1,000 nodes | ~1.2 MB |
| 10,000 nodes | ~12 MB |
| 100,000 nodes | ~120 MB |

### 2.4 SoA vs AoS Assessment

**Well-implemented SoA patterns:**
- `StyledDom` parallel Vecs (hierarchy, data, styled_nodes, cascade) — all indexed by NodeId
- `CompactLayoutCache` — textbook SoA: 3 flat arrays (8 + 96 + 24 B/node), cache-line friendly
- `LayoutCacheMap` — external to tree, parallel Vec
- `calculated_positions` — separate flat Vec

**Problematic AoS patterns:**
- **`LayoutNode` (~550 B)** — monolithic struct, only ~80 B used on hot path per traversal pass.
  Cache line (64 B) loads 50+ unused bytes alongside hot fields.
- **`CssPropertyCache` inner `Vec<Vec<…>>`** — N separate heap allocations per field (4 fields × N = 4N allocs), causes pointer chasing on linear scans.

### 2.5 Hot Path Analysis

**Bottom-up intrinsic sizing:**
- Reads: `LayoutNode.{children, formatting_context, computed_style, box_props}`
- Reads: `CompactLayoutCache.{tier1_enums[i], tier2_dims[i]}`
- Writes: `LayoutNode.{intrinsic_sizes, used_size}`

**Top-down layout (`calculate_layout_for_subtree`):**
- Reads: `LayoutNode.{children, box_props, formatting_context, computed_style, intrinsic_sizes}`
- Reads/Writes: `LayoutCacheMap.entries[i]` (Taffy 9+1)
- Writes: `calculated_positions[i]`, `LayoutNode.{used_size, relative_position}`

**IFC text layout:**
- Reads: `CompactLayoutCache.tier2b_text[i]`
- Reads: `FontManager.{font_chain_cache, parsed_fonts}` (Mutex lock)
- Writes: `LayoutNode.inline_layout_result` (Arc)

### 2.6 Optimization Opportunities

#### Priority 1: LayoutNode Hot/Cold Split

Split `LayoutNode` (~550 B) into:
- **`LayoutNodeHot`** (~80 B): `children`, `parent`, `box_props`, `computed_style`, 
  `formatting_context`, `used_size`, `relative_position`, `dirty_flag`
- **`LayoutNodeCold`** (~470 B): `fingerprint`, `subtree_hash`, `inline_layout_result`,
  `scrollbar_info`, `ifc_id`, `ifc_membership`, `unresolved_box_props`, etc.

**Impact:** Reduces hot-path working set from ~5.5 MB to ~800 KB for 10K nodes.

#### Priority 2: CssPropertyCache Flat Allocation

Replace `Vec<Vec<StatefulCssProperty>>` (N heap allocs per field) with a single flat
`Vec<StatefulCssProperty>` + offset array `Vec<(u32, u32)>`.

**Impact:** Eliminates 4N heap allocations and pointer chasing.

#### Priority 3: LayoutNode.children Arena

Replace `Vec<usize>` per node (N heap allocs) with a global `Vec<usize>` + `(start, len)` per node.

**Impact:** Eliminates N heap allocations, improves locality.

#### Priority 4: Per-Node Font Dirty Tracking

Add `font_dirty: bool` to `CompactTextProps` and maintain a `Vec<NodeId>` of dirty-font nodes.
Re-resolve only affected font chains instead of all.

**Impact:** Avoids O(N) font chain resolution when one node's font-family changes.

#### Priority 5: BTreeMap → HashMap/Vec for LayoutCache

`scroll_ids`, `scroll_id_to_node_id`, `counters`, `float_cache` use `BTreeMap` (O(log N),
poor cache locality). For typical sizes (< 1000 entries), `HashMap` or dense `Vec` would be faster.

#### Priority 6: GpuValueCache Consolidation

14 separate `BTreeMap`s with largely overlapping key sets → consolidate into a single
`HashMap<NodeId, GpuNodeValues>` to reduce map overhead from x14 to x1.

---

## Summary Table

| Component | Status | Key Issue |
|-----------|--------|-----------|
| CompactLayoutCache | ✅ Excellent | SoA, O(1), cache-friendly |
| StyledDom Vecs | ✅ Good | Consistent SoA pattern |
| font_stacks_hash skip | ✅ Good | Effective for common case |
| LayoutNode | ❌ Poor | AoS monolith, ~550 B, wastes cache lines |
| CssPropertyCache fallback | ❌ Poor | Vec<Vec<…>>, pointer chasing |
| Font dirty tracking | ⚠️ Missing | Global-only, no per-node granularity |
| IFC incremental relayout | ⚠️ Stub | Always full relayout (Phase 2c/2d) |
| BTreeMap usage | ⚠️ Suboptimal | O(log N), poor locality |
