User: so I did optimize the node data - is there anything more "data oriented" that I can do?

---

A few things that would be genuine wins beyond what you have:

**Split tier2 into layout-hot and paint-cold.** Your `CompactNodeProps` is 96 B but the flex/block layout solver only touches ~48 B of it per node (width/height/min/max, padding, margin, flex-grow/shrink/basis). Border widths, border colors, border styles, z-index, tab-size, border-spacing — those are only read during display list generation or table layout. Splitting into `CompactLayoutProps` (~48 B) and `CompactPaintProps` (~48 B) halves the working set during the hot constraint-solving loop. For 500 nodes, that's 24 KB vs 48 KB — the difference between fitting in L1 and spilling to L2.

**Guarantee DFS preorder in the node arrays.** If you already store nodes in depth-first preorder (which I suspect you do given `subtree_start..subtree_end` in the design doc), make it an explicit invariant. This means subtree ranges are contiguous slices, inheritance is a single forward pass, and the deferred cascade's `restyle_subtree` is just `&mut tier1[start..end]`. If you don't have this yet, it's the single highest-value structural change — it turns every tree operation into a linear scan.

**Bitset for "needs inheritance".** During `compute_inherited_values()`, most nodes inherit the same values as their parent (especially in GUI apps where you set `color` and `font-size` on the root and everything inherits). A `BitVec` marking nodes where inherited properties actually differ from parent lets you skip the copy for identical subtrees. Gecko does something similar with their `STYLE_INHERIT_MASK` flags — they track which property groups changed and skip inheritance for unchanged groups.

**Bump allocator for per-frame DOM construction.** Your `NodeData` contains `AttributeTypeVec`, `CoreCallbackDataVec`, `CssPropertyWithConditionsVec` — each with its own heap allocation. If the DOM is rebuilt per relayout, a bump allocator (`bumpalo` or a simple arena) for all these small Vecs means: one allocation for the entire frame's DOM data, zero individual frees, and better spatial locality since all the attribute strings and callback arrays are packed together. Drop the arena, everything's gone. This matters more than the per-node struct size once you have hundreds of nodes each with 2-3 small Vecs.

**Flatten `NodeHierarchyItem` to implicit encoding.** Your 32 B/node hierarchy (parent/first-child/last-child/prev-sibling/next-sibling as `usize` indices) could become ~4-8 B/node if you store just a `depth: u16` per node and rely on DFS preorder. Parent = scan backwards for depth-1. First child = next node if its depth is current+1. Next sibling = scan forward at same depth. This trades O(1) random access for O(1) amortized sequential access, which is exactly what tree walks do. Ladybird recently explored this. It's a more radical change but it's 500 × 24 B = 12 KB saved.

The tier2 split and the bump allocator are probably the highest ROI for your situation — the first because it directly halves cache pressure during layout, and the second because it eliminates the allocation overhead that's currently hidden inside every `NodeData` construction.

---

User: here's more perf analysis. I found out that the main structs are EdgeSizes and BoxProps (= ResolvedBoxProps). Could I get extra speed withs by using f16 instead of f32? Research.

---

## f16 for Layout: Not Worth It

**Precision is the dealbreaker.** IEEE 754 half-precision (f16) has 10 mantissa bits, giving ~3 decimal digits of precision. The precision degrades as values get larger:

| Value range | f16 ULP (smallest step) | Acceptable for layout? |
|---|---|---|
| 0–1 px | ~0.001 px | ✓ |
| 1–10 px | ~0.01 px | ✓ |
| 10–100 px | ~0.1 px | Borderline |
| 100–1000 px | ~1.0 px | ✗ |
| 1000–2000 px | ~2.0 px | ✗ |
| >2048 px | ≥2.0 px | ✗ |
| max representable | 65,504 | — |

A 1920px-wide container with f16 has ±1px jitter. Two margins that should sum to exactly 960.5px might round to 960 or 961. You'd get visible 1-2px layout glitches on every frame, and `margin: auto` centering on a 1080p display would visibly shimmer.

**No native x86-64 arithmetic.** There is no f16 ALU on any mainstream x86-64 CPU. What exists:

- **F16C** (Ivy Bridge+, 2012): conversion only — `VCVTPH2PS` (f16→f32) and `VCVTPS2PH` (f32→f16). No add/mul/div in f16. Every operation is: load f16 → convert to f32 → compute → convert back → store f16. The conversion overhead kills any cache benefit.
- **AVX-512 FP16** (Sapphire Rapids, 2023): actual native f16 arithmetic. But this is server silicon only — not available on any consumer desktop/laptop CPU as of early 2026. And Azul targets desktop apps.
- **ARM**: ARMv8.2-A added native f16 ops, available on Apple M1+ and recent Cortex-A. But you'd need separate codepaths and your primary target is x86-64.

**Your i16×10 encoding is already better.** Look at what you already have in the compact cache:

```rust
// Your existing encoding: i16 with ×10 scaling
pub padding_top: i16,    // range ±3276.7px at 0.1px precision
pub margin_top: i16,     // same
pub border_top_width: i16, // same
```

This gives you 0.1px precision across ±3276px — **uniformly**, unlike f16 where precision degrades with magnitude. And i16 arithmetic is native on every CPU. You're already doing the right thing in the compact cache.

**The f32 EdgeSizes/BoxProps problem is in LayoutNode, not CompactLayoutCache.** Your `ResolvedBoxProps` uses f32 because the layout solver does actual arithmetic (margin collapsing, percentage resolution, auto margin calculation) that needs full precision during computation. Shrinking those to f16 would mean converting to f32 for every calculation and back — net slower, not faster.

## What Would Actually Help

The real issue from your analysis is that `LayoutNode` is ~550 B and the hot fields are spread across it. Instead of shrinking individual floats:

**1. Your `EdgeSizes` is 16 B (4×f32). Pack it to 8 B (4×i16) only in the compact cache output, keep f32 during computation:**

```rust
// Final resolved output (read-only during display list generation)
#[repr(C)]
pub struct PackedEdge {
    pub top: i16,    // ×10
    pub right: i16,
    pub bottom: i16,
    pub left: i16,
}

// Working copy during layout (full precision)
pub type EdgeSizes = [f32; 4]; // stays f32
```

This is what you're already doing with the compact cache — just extend the pattern to the layout output. The `calculated_positions` Vec that feeds display list gen could use packed i16 edges.

**2. The hot/cold split from your analysis doc is the real win.** `LayoutNode` at 550 B means ~8.5 nodes per cache line page. Splitting to ~80 B hot struct means ~51 nodes fit in the same cache working set. That's a 6× improvement in effective cache utilization during the constraint-solving loop, which dwarfs anything f16 could give you.

**3. If you want to squeeze `BoxProps` specifically**, consider that during layout you almost always need all 4 edges together. Store them as `[f32; 4]` instead of a struct — same size but the compiler can auto-vectorize operations like `margin_sum = margin[0] + margin[2]` into SIMD adds:

```rust
#[repr(C, align(16))]
pub struct ResolvedBoxProps {
    pub margin: [f32; 4],   // top, right, bottom, left — 16B, SIMD-aligned
    pub padding: [f32; 4],  // 16B
    pub border: [f32; 4],   // 16B
    pub margin_auto: u8,    // 4 bits packed
    pub _pad: [u8; 7],
}
// 56B, 16-byte aligned, SIMD-friendly
```

With `align(16)`, the compiler can use `ADDPS` / `MULPS` to process all 4 edges in one instruction when computing inner_size, margin collapsing, etc.

---

User: No, for storage. For calculation, I'd convert to f32, but it reduces memory

---

Fair point — but then just use your existing `i16×10` encoding, not f16. It's strictly better for your use case:

| | f16 | i16 ×10 |
|---|---|---|
| Bytes | 2 | 2 |
| Precision at 1000px | ~1.0 px | 0.1 px (uniform) |
| Conversion cost | `VCVTPH2PS` (4-5 cycles, needs F16C) | `i16 as f32 / 10.0` (1-2 cycles, always available) |
| Max range | 65,504 | ±3,276.7 px |
| Rust support | `f16` crate, no std support | Native |

The ±3,276.7px range covers any realistic box model value. No single margin/padding/border exceeds that in a desktop GUI.

**Concrete savings in LayoutNode's hot path:**

```rust
// Current: 52B
pub struct ResolvedBoxProps {
    pub margin: EdgeSizes,    // 4×f32 = 16B
    pub padding: EdgeSizes,   // 4×f32 = 16B
    pub border: EdgeSizes,    // 4×f32 = 16B
    pub margin_auto: MarginAuto, // 4B
}

// Packed: 26B
#[repr(C)]
pub struct PackedBoxProps {
    pub margin: [i16; 4],     // 8B, ×10
    pub padding: [i16; 4],    // 8B, ×10
    pub border: [i16; 4],     // 8B, ×10
    pub margin_auto: u8,      // 4 bits packed
    pub _pad: u8,
}
```

26B saved per node. But it only matters if it's in the hot struct — and your hot struct is what gets walked during constraint solving, where you immediately convert back to f32 anyway. The conversion happens on every access:

```rust
#[inline(always)]
fn unpack_edge(e: &[i16; 4]) -> [f32; 4] {
    [
        e[0] as f32 * 0.1,
        e[1] as f32 * 0.1,
        e[2] as f32 * 0.1,
        e[3] as f32 * 0.1,
    ]
}
```

**So the real tradeoff is:** 26B × N less memory vs 4 int-to-float conversions per edge access. For 10K nodes that's 260 KB saved. Whether that moves the needle depends on whether your hot struct is already fitting in L1/L2 or not.

If you do the hot/cold split first (550B → ~80B hot), the hot struct for 10K nodes is 800 KB — fits in L2 but not L1. Packing BoxProps drops it to ~54B hot, which at 10K nodes = 540 KB. Still L2 territory.

At 1K nodes (typical GUI): 80 KB hot (fits L1 easily) vs 54 KB hot (also fits L1). No difference.

**Verdict:** Do the hot/cold split first — that's the 6× win. The i16 packing of BoxProps is a secondary optimization that only matters at node counts you probably won't hit in a desktop GUI. If you do hit them, it's a mechanical change since you already have the `i16×10` pattern in `CompactNodeProps`.


---

User: Well I regularly hit 100k nodes in things like inline text layout for pagination - less so for regular DOM, more "html2pdf" cases. Analyze: what would then be the size of my layout node (see perf analysis md)?

---

This changes the math significantly. Let me look at the actual cache hierarchies:

## Cache Sizes: Desktop vs Mobile vs Embedded

| CPU | L1D | L2 | L3 | Year |
|---|---|---|---|---|
| Desktop (Zen 4) | 32 KB | 1 MB | 32 MB | 2022 |
| Apple M2 | 128 KB | 16 MB | — | 2022 |
| Snapdragon 8 Gen 3 (big) | 64 KB | 2 MB | 12 MB | 2023 |
| Snapdragon 8 Gen 3 (little) | 32 KB | 256 KB | 12 MB (shared) | 2023 |
| MediaTek Dimensity 700 | 32 KB | 128 KB | 512 KB | 2020 |
| **RPi 5** (Cortex-A76) | 64 KB | 512 KB | **none** | 2023 |
| **RPi 4** (Cortex-A72) | 32 KB | 1 MB | **none** | 2019 |
| **RPi Zero 2W** (Cortex-A53) | 16 KB | 512 KB | **none** | 2021 |
| **RPi Zero W** (ARM1176) | 16 KB | **none** | **none** | 2017 |
| Allwinner H3 (cheap SBCs) | 32 KB | 512 KB | **none** | 2015 |
| Rockchip RK3566 | 32 KB | 256 KB | **none** | 2021 |

The critical difference: **no L3 on any of these**. Your last line of defense is L2, and on RPi Zero 2W that's 512 KB shared across 4 cores.

## Your Working Sets vs These Caches

Assuming a compositor with ~500-2000 nodes (window chrome, taskbar, app switcher, notification overlays — not a browser):

### Current monolithic LayoutNode (550B)

| Nodes | Working set | RPi Zero 2W (512KB L2) | RPi 4 (1MB L2) | RPi 5 (512KB L2) |
|---|---|---|---|---|
| 500 | 275 KB | Fits L2 ✓ | Fits L2 ✓ | Fits L2 ✓ |
| 1,000 | 550 KB | **Spills to DRAM** | Fits L2 ✓ | **Spills to DRAM** |
| 2,000 | 1.1 MB | Spills | Spills | Spills |
| 5,000 | 2.75 MB | Spills badly | Spills badly | Spills badly |

On ARM without L3, "spills" means going straight to DRAM. DRAM latency on these SoCs is 80-120ns vs ~4ns for L1, ~10ns for L2. That's a 10-30× penalty per cache miss, and unlike desktop where L3 catches most misses, here every L2 miss is a DRAM hit.

### Hot struct at 48B

| Nodes | Hot working set | RPi Zero 2W L2 | RPi 4 L2 | RPi 5 L2 |
|---|---|---|---|---|
| 500 | 24 KB | Fits **L1** | Fits L1 | Fits L1 |
| 1,000 | 48 KB | Fits L2 ✓ | Fits L1 ✓ | Fits L1 ✓ |
| 2,000 | 96 KB | Fits L2 ✓ | Fits L2 ✓ | Fits L2 ✓ |
| 5,000 | 240 KB | Fits L2 ✓ | Fits L2 ✓ | Fits L2 ✓ |
| 10,000 | 480 KB | Tight L2 | Fits L2 ✓ | Tight L2 |

**This is the difference between "works" and "unusable" on embedded.** A compositor doing 60fps layout at 1000 nodes: with 550B/node, every frame thrashes L2 on RPi Zero 2W and you're doing ~50K DRAM round-trips per frame. At 100ns each that's 5ms just in memory stalls — already a third of your 16.6ms frame budget. With 48B hot struct, the entire hot array lives in L2 and you're doing ~0 DRAM accesses during the constraint loop.

## Additional ARM-Specific Concerns

**Memory bandwidth is brutal.** RPi 4 has ~4 GB/s DRAM bandwidth (shared with GPU). RPi Zero 2W is ~1.5 GB/s (LPDDR2). Desktop is 50-80 GB/s. At 1000 nodes × 550B × 60fps, you're streaming 33 MB/s just for layout nodes — 2% of RPi Zero 2W's total bandwidth every frame, competing with GPU compositing.

**ARM cache lines are 64B, same as x86.** Your 48B hot struct fits one cache line with 16B to spare. If you can get it to exactly 64B (adding a few useful fields), you get perfect one-line-per-node access. Going under 64B doesn't help — the CPU loads the full line anyway.

**Cortex-A53 (RPi Zero 2W) is in-order.** No out-of-order execution to hide memory latency. A cache miss stalls the pipeline completely. Desktop CPUs can execute other instructions while waiting for memory; A53 just waits. This makes cache locality literally 2-3× more important than on desktop.

**No F16C equivalent needed.** ARMv8 has native `FCVT` (f16↔f32 conversion) in base ISA and Cortex-A53 has it. But as we discussed, i16×10 is better anyway and `SXTL` + `SCVTF` (i16→f32) is one NEON instruction.

## Revised Recommendation for Embedded Target

Given no-L3 + in-order cores + low bandwidth, I'd push harder:

```rust
#[repr(C, align(64))]  // one cache line exactly
pub struct LayoutNodeHot {
    pub margin: [i16; 4],         //  8B
    pub padding: [i16; 4],        //  8B
    pub border: [i16; 4],         //  8B
    pub children_start: u32,      //  4B  (arena index)
    pub children_len: u16,        //  2B
    pub dom_node_id: u32,         //  4B  (u32::MAX = anonymous)
    pub parent: u32,              //  4B
    pub used_width: u16,          //  2B  (×1, unsigned)
    pub used_height: u16,         //  2B  (×1, unsigned)
    pub formatting_context: u8,   //  1B
    pub margin_auto: u8,          //  1B  (4 bits)
    pub dirty_flag: u8,           //  1B
    pub flags: u8,                //  1B  (is_anonymous, has_inline_result, etc.)
    // 14B remaining in cache line
    pub intrinsic_min_w: u16,     //  2B  (×1 — moved from warm, needed in sizing)
    pub intrinsic_max_w: u16,     //  2B  (×1)
    pub intrinsic_min_h: u16,     //  2B
    pub intrinsic_max_h: u16,     //  2B
    pub baseline: i16,            //  2B  (×10)
    pub _pad: [u8; 4],           //  4B
}                                 // = 64B exactly = 1 cache line
```

At 64B/node: 1000 nodes = 64 KB (fits RPi Zero 2W L2 with 448 KB to spare for warm + children arena). 5000 nodes = 320 KB (still fits). The compositor case never leaves L2.

**For the html2pdf / 100K node case** on RPi: 100K × 64B = 6.4 MB. That won't fit in any embedded L2. But pagination means you're processing one page at a time (~500-2000 nodes), and if your layout is incremental, only dirty subtrees get walked. The 64B hot struct ensures each page's layout pass stays in L2.

The hot/cold split isn't just an optimization on these devices — it's a hard requirement for acceptable performance.

---

## Before vs After: Full Accounting

### Per-Node Memory Breakdown

| Component | Before | After | Δ |
|---|---|---|---|
| **NodeData** | 320 B | 184 B | −136 B (steps 1-5) |
| **CompactCache** (tier1+tier2+tier2b) | 128 B | 128 B | — (already optimal) |
| **NodeHierarchy** | 32 B | 6 B | −26 B (flatten to depth:u16 + DFS) |
| **LayoutNode** | 550 B | 428 B | −122 B (hot/warm/cold split + i16 pack) |
| ↳ hot | (monolithic) | 64 B | (cache-line aligned) |
| ↳ warm | | 84 B | (ComputedLayoutStyle deduped vs tier1) |
| ↳ cold | | 280 B | |
| **LayoutCacheMap** | 260 B | 260 B | — (not yet optimized) |
| **Children storage** | 24 B | 6 B | −18 B (Vec → arena start:u32+len:u16) |
| **Total per node** | **1,314 B** | **1,012 B** | **−302 B (−23%)** |

### Working Set (What the Layout Solver Actually Touches)

This is the number that matters for cache performance:

| Component | Before | After |
|---|---|---|
| LayoutNode | 550 B (all of it) | 64 B (hot only) |
| CompactCache | 128 B (all tiers) | 56 B (tier1 8B + tier2-hot 48B) |
| Hierarchy | 32 B | 6 B |
| **Working set per node** | **710 B** | **126 B** |
| **Reduction** | | **−82%** |

### Memory at Scale

**Total allocation:**

| Nodes | Before (1,314 B/node) | After (1,012 B/node) | Saved |
|---|---|---|---|
| 500 | 657 KB | 506 KB | 151 KB |
| 1,000 | 1.31 MB | 1.01 MB | 302 KB |
| 10,000 | 13.1 MB | 10.1 MB | 3.0 MB |
| 100,000 | 131 MB | 101 MB | 30 MB |

**Layout solver working set (the one that matters):**

| Nodes | Before (710 B) | After (126 B) | Ratio | RPi Zero 2W (512KB L2) | RPi 5 (512KB L2) | Desktop (32MB L3) |
|---|---|---|---|---|---|---|
| 500 | 355 KB | **63 KB** | 5.6× | L2 → **L1** | L2 → **L1** | L3 → L3 |
| 1,000 | 710 KB | **126 KB** | 5.6× | **DRAM** → **L2** | **DRAM** → **L2** | L3 → L3 |
| 2,000 | 1.4 MB | **252 KB** | 5.6× | DRAM → **L2** | DRAM → **L2** | L3 → L3 |
| 5,000 | 3.6 MB | **630 KB** | 5.6× | DRAM → DRAM | DRAM → DRAM | L3 → **L3** |
| 10,000 | 7.1 MB | **1.26 MB** | 5.6× | DRAM → DRAM | DRAM → DRAM | L3 → **L3** |
| 100,000 | 71 MB | **12.6 MB** | 5.6× | DRAM → DRAM | DRAM → DRAM | Thrashes L3 → **fits L3** |

### What Each Optimization Contributes

Ranked by impact on layout working set:

| Optimization | Working set Δ | Total memory Δ | Effort |
|---|---|---|---|
| **LayoutNode hot/cold split + i16 pack** | −486 B/node | −122 B/node | Medium (restructure solver) |
| **CompactCache tier2 hot/cold split** | −72 B/node | 0 B/node | Low (split array) |
| **NodeData 320→184B** | 0 B (not in solver path) | −136 B/node | Medium (5 refactoring steps) |
| **Hierarchy flatten** | −26 B/node | −26 B/node | Low |
| **Children arena** | −18 B/node | −18 B/node | Low (+ eliminates N heap allocs) |
| **ComputedLayoutStyle dedup** | included in split | −80 B/node | Low (read from tier1) |
| **Total** | **−584 B/node** | **−302 B/node** | |

### Comparison vs Browser Engines (After Optimizations)

| | Azul (before) | **Azul (after)** | Gecko | Blink | Ladybird |
|---|---|---|---|---|---|
| Total per-node | ~1,314 B | **~1,012 B** | ~800-1200 B | ~700-1000 B | ~1000-1500 B |
| Layout working set | ~710 B | **~126 B** | ~300-400 B¹ | ~200-300 B¹ | ~500-800 B |
| Style storage model | Flat SoA | **Flat SoA** | Arc-shared | Refcounted | Per-node (no sharing) |

¹ Gecko/Blink's numbers look lower but involve **pointer-chased** Arc'd ComputedValues — each cache miss during property lookup is a random memory access. Your 126B is a contiguous flat array scan. At 100K nodes, your cache miss rate will be dramatically lower despite similar byte counts.

### The Bottom Line

The **total memory** reduction is modest (23%) — you're not eliminating data, just reorganizing it. But the **layout performance** improvement is transformative:

- **5.6× smaller working set** during constraint solving
- **RPi Zero 2W**: 1000-node compositor goes from thrashing DRAM to fitting in L2
- **100K-node html2pdf**: goes from thrashing L3 on desktop to fitting in L3
- **RPi Zero 2W in-order A53**: each avoided DRAM miss saves ~100ns of complete pipeline stall. At 1000 nodes × ~10 property reads per node during layout = 10K reads. Before: ~50% miss rate × 100ns = 500µs wasted. After: ~0% miss rate = ~0µs. That's per layout pass at 60fps.

The optimizations we *didn't* quantify yet (bump allocator, LayoutCacheMap consolidation, per-node font dirty tracking) would shave another ~100-200 B/node off total and eliminate thousands of heap allocations per frame. But the hot/cold split alone gets you the critical cache behavior improvement.

---

PERF2.md Analysis: What to Do Before the Agent Run

Current State of LayoutNode (~550B, monolithic)
The struct has hot/warm/cold tiers annotated but not split. It has 20+ fields accessed by every file in
solver3/. The children arena optimization is already done (LayoutTree.children_arena + children_offsets).

The Critical Question

Which optimizations would cause structural conflicts if done AFTER 800 agents produce patches?

┌────────────────────────┬────────────────────────────────────────────────────┬─────────────────────────┐
│      Optimization      │             Would break agent patches?             │         Verdict         │
├────────────────────────┼────────────────────────────────────────────────────┼─────────────────────────┤
│ LayoutNode hot/cold    │ Yes — every node.box_props.X becomes               │ Don't do before         │
│ split                  │ hot[idx].box_props.X                               │                         │
├────────────────────────┼────────────────────────────────────────────────────┼─────────────────────────┤
│ i16×10 BoxProps        │ Yes — changes f32 field types                      │ Don't do before         │
│ packing                │                                                    │                         │
├────────────────────────┼────────────────────────────────────────────────────┼─────────────────────────┤
│ CompactCache tier2     │ No — agents don't touch cache internals            │ Could do, but low       │
│ split                  │                                                    │ priority                │
├────────────────────────┼────────────────────────────────────────────────────┼─────────────────────────┤
│ Children arena         │ N/A — already done                                 │ Done                    │
├────────────────────────┼────────────────────────────────────────────────────┼─────────────────────────┤
│ Hierarchy flatten      │ Moderate — changes parent/child traversal          │ Don't do before         │
├────────────────────────┼────────────────────────────────────────────────────┼─────────────────────────┤
│ DFS preorder           │ N/A — already implicit in tree build               │ Done                    │
├────────────────────────┼────────────────────────────────────────────────────┼─────────────────────────┤
│ Bump allocator         │ No — NodeData, not solver3                         │ Irrelevant for agents   │
└────────────────────────┴────────────────────────────────────────────────────┴─────────────────────────┘

Recommendation: Do NOT do any structural refactoring before the agent run

Reasoning:

1. The hot/cold split is mechanical, not semantic. It changes WHERE data lives (node.box_props →
hot_nodes[idx].box_props), not WHAT the data means. After agents fix spec compliance logic, the split can be
applied as a mechanical find-and-replace across all files.
1. Agents fix logic, not data layout. Spec paragraphs say things like "if both margin-left and margin-right
are auto, center the element." Agents will modify control flow in fc.rs, sizing.rs, positioning.rs — the same
logic applies regardless of whether box_props lives in a monolithic struct or a hot struct.
1. Doing the split now would complicate agent prompts. Agents would need to understand the hot/warm/cold
separation, know which struct to add new fields to, and handle indirect indexing. The current monolithic
LayoutNode is much simpler to work with.
1. Agent patches that add new LayoutNode fields go cold anyway. Any new CSS property an agent adds (e.g.,
clear, page-break-before) would be cold tier. After the split, these just move to the cold struct — trivial.
1. Risk of introducing bugs. A massive refactoring right before running 800 agents means any bug in the split
would cascade into every agent's work.

What SHOULD be done before the run

Nothing structural. The codebase is in good shape for agents:
- Children arena: done
- DFS ordering: implicit
- Field access patterns: straightforward node.field — easy for agents to understand
- BoxProps/EdgeSizes API: clean, well-documented

After the agent run, the optimization order should be:

1. LayoutNode hot/cold split (biggest win: 550B → 64B hot, 5.6× cache improvement)
2. i16×10 packing (secondary: −26B/node in hot struct)
3. CompactCache tier2 split (−72B working set, low effort)
4. Hierarchy flatten (−26B, more radical)

All of these are mechanical refactorings that a single focused pass can do after agent patches are collected
and cherry-picked.

Bottom line: Run the agents now. Do the performance refactoring after.
