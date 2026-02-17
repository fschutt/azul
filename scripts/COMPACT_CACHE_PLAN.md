# Compact Layout Cache — Implementation Plan

## Goal

Replace BTreeMap-based CSS property lookups (67% of CPU time) with a three-tier
compact cache that fits in CPU caches. Target: **256 KB L2 cache** (Skylake,
Apple M1 E-cores, ARM Cortex-A55).

## Expected Speedup

### Flamegraph breakdown (from cargo flamegraph, 64,429 total samples)

| Category | Samples | % of total | After compact cache |
|----------|---------|-----------|-------------------|
| BTreeMap::get (pointer chasing) | 26,273 | 40.8% | **→ ~0.5%** (array index) |
| get_property_slow (cascade walk) | ~4,000 | ~6.2% | **→ ~0%** (pre-resolved) |
| Getter wrapper overhead | ~6,500 | ~10.1% | **→ ~1%** (inline bit ops) |
| FontFamilyVec clone+drop | ~1,700 | ~2.6% | **→ ~0%** (u64 hash compare) |
| Alloc/realloc (BTreeMap nodes) | ~5,000 | ~7.8% | **→ ~0%** (no heap alloc) |
| **Subtotal eliminatable** | **~43,500** | **~67.5%** | **→ ~1.5%** |
| build_compact_cache() (new cost) | — | — | **+~3%** (one-time build) |
| Everything else (unchanged) | ~20,929 | 32.5% | 32.5% |

### Why speedup is LARGER for bigger DOMs

The superlinear scaling we measured comes from cache pressure — as DOM grows,
BTreeMap nodes scatter across more memory pages, and each pointer-chase
increasingly misses cache. With compact arrays, access is sequential and
prefetchable → the superlinear penalty disappears.

| DOM size | Current ms/node | Expected ms/node | Speedup |
|----------|----------------|-----------------|---------|
| 2-5K     | 0.060-0.066    | ~0.030          | ~2× |
| 20-30K   | 0.157-0.188    | ~0.035          | ~4-5× |
| 72K      | 0.320          | ~0.035          | **~9×** |

### Predicted benchmarks

| Benchmark | Current | Expected | Speedup |
|-----------|---------|----------|---------|
| deserialize.rs (72K nodes) | 60s | 6-10s | 6-10× |
| printpdf total (28 files) | 117s | 30-45s | 2.5-4× |
| Average 5K DOM | 0.33s | 0.15s | ~2× |

The property lookup goes from **~100ns/access** (BTreeMap + cascade) to
**~1-2ns/access** (array index + bit shift). That's 50-100× per access,
but the overall speedup is limited by other costs (text layout, display
list, tree building).

Conservative estimate: **3-5× overall**, **6-10× for large DOMs**.

## Budget

| DOM size | L2 budget per node | L3 budget per node |
|----------|-------------------|--------------------|
| 5,000    | 52 B              | ~3 KB              |
| 20,000   | 13 B              | ~800 B             |
| 72,000   | 3.6 B             | ~220 B             |

## Property Access Frequency (from grep of layout/src/)

```
 44×  display          (every node, tree walk + sizing + display list)
 37×  position         (every node, tree walk + sizing + positioning)
 23×  overflow_y       (tree walk + sizing)
 22×  overflow_x       (tree walk + sizing)
 12×  float            (tree walk + sizing)
  8×  vertical_align   (sizing)
  7×  text_align       (IFC text layout)
  6×  width            (sizing)
  6×  height           (sizing)
  5×  z_index          (positioning + display list)
  4×  justify_content  (flex layout)
  4×  font_size        (IFC text + em resolution)
  4×  direction        (IFC text)
  3×  white_space      (IFC text)
  3×  border_*_width   (sizing, ×4 sides)
  2×  font_weight      (text style)
  2×  font_style       (text style)
  2×  font_family      (text style)
  2×  line_height      (text style)
  1×  everything else
```

## Tier Design

### Tier 1: `Vec<u64>` — ALL enum properties bitpacked (8 bytes/node)

Pack every enum-type layout property into a single u64 per node:

```
Bit layout of u64 (51 bits used, 13 spare):
  [4:0]    display          5 bits  (20 variants)
  [7:5]    position         3 bits  (5 variants)
  [9:8]    float            2 bits  (3 variants)
  [12:10]  overflow_x       3 bits  (5 variants)
  [15:13]  overflow_y       3 bits  (5 variants)
  [16]     box_sizing       1 bit   (2 variants)
  [18:17]  flex_direction   2 bits  (4 variants)
  [20:19]  flex_wrap         2 bits  (3 variants)
  [23:21]  justify_content  3 bits  (6 variants)
  [26:24]  align_items      3 bits  (6 variants)
  [29:27]  align_content    3 bits  (6 variants)
  [31:30]  writing_mode     2 bits  (4 variants)
  [33:32]  clear            2 bits  (4 variants)
  [37:34]  font_weight      4 bits  (11 variants)
  [39:38]  font_style       2 bits  (3 variants)
  [42:40]  text_align       3 bits  (6 variants)
  [43]     visibility       1 bit   (2 variants)
  [46:44]  white_space      3 bits  (4 variants)
  [47]     direction        1 bit   (2 variants)
  [50:48]  vertical_align   3 bits  (5 variants)
  [63:51]  (spare)          13 bits
```

| DOM size | Tier 1 total | Fits in   |
|----------|-------------|-----------|
| 5,000    | 40 KB       | L1 cache! |
| 20,000   | 160 KB      | L2 cache  |
| 72,000   | 576 KB      | L3 (but sequential → prefetchable) |

**Why u64 for ALL enums instead of u16 for top-5:**
- For 5K DOM → 40 KB still fits L1 (32-64KB typical)
- Eliminates ALL enum fields from Tier 2, making it purely numeric
- One `Vec<u64>` read per node gives you every enum property
- 13 spare bits for future enums
- Even at 72K (576KB → spills L2), sequential Vec access is perfectly
  prefetchable — CPU hardware prefetcher handles linear sweeps. Compare
  to BTreeMap: random pointer chasing, zero prefetch benefit.

### Tier 2: `Vec<CompactNodeProps>` — Numeric dimensions only

All dimension/sizing properties. No enums (those are all in Tier 1).

**MSB/LSB Sentinel Encoding — use the full numeric range:**

Instead of wasting bits on flags, reserve a few values at the extreme
end of the range as sentinel codes. This gives nearly full range:

```
u16 encoding for resolved-px values (×10):
  65535 (0xFFFF)  = SENTINEL → look up in Tier 3 / slow cache
  65534           = Auto
  65533           = None
  65532           = Inherit
  65531           = Initial
  65530           = MinContent
  65529           = MaxContent
  0..65528        = unsigned value × 10
                    range: 0..6552.8 px at 0.1px precision

i16 encoding for signed values (margins, offsets):
  32767 (0x7FFF)  = SENTINEL → look up in Tier 3
  32766           = Auto
  32765           = Inherit
  -32768..32764   = signed value × 10
                    range: -3276.8..+3276.4 px at 0.1px precision

u32 encoding for dimension properties needing unit info:
  0xFFFFFFFF      = SENTINEL → look up in Tier 3
  0xFFFFFFFE      = Auto
  0xFFFFFFFD      = None
  0xFFFFFFFC      = Inherit
  0xFFFFFFFB      = Initial
  0xFFFFFFFA      = MinContent
  0xFFFFFFF9      = MaxContent
  0x00000000..
  0xFFFFFFF8      = [4-bit SizeMetric | 28-bit signed fixed-point ×1000]
                    range: ±134,217.727 px at 0.001px precision
                    (28 bits signed = ±134M, /1000 = ±134K px)
```

The u32 encoding with MSB sentinels is strictly better than the previous
flags-in-top-4-bits approach: 28 bits instead of 24 bits for the value,
range jumps from ±8,388 to ±134,217 px. Practically overflow-proof.

```rust
#[repr(C)]
pub struct CompactNodeProps {       // Total: 64 bytes/node
    // --- Dimensions needing unit (u32 MSB-sentinel) ---
    pub width: u32,                 //  4 B
    pub height: u32,                //  4 B
    pub min_width: u32,             //  4 B
    pub max_width: u32,             //  4 B
    pub min_height: u32,            //  4 B
    pub max_height: u32,            //  4 B
    pub flex_basis: u32,            //  4 B
    pub font_size: u32,             //  4 B   (needs em/rem/% for cascade)

    // --- Resolved px values (i16 MSB-sentinel, ×10) ---
    pub padding_top: i16,           //  2 B
    pub padding_right: i16,         //  2 B
    pub padding_bottom: i16,        //  2 B
    pub padding_left: i16,          //  2 B
    pub margin_top: i16,            //  2 B   (can be negative)
    pub margin_right: i16,          //  2 B
    pub margin_bottom: i16,         //  2 B
    pub margin_left: i16,           //  2 B
    pub border_top_width: i16,      //  2 B
    pub border_right_width: i16,    //  2 B
    pub border_bottom_width: i16,   //  2 B
    pub border_left_width: i16,     //  2 B
    pub top: i16,                   //  2 B
    pub right: i16,                 //  2 B
    pub bottom: i16,                //  2 B
    pub left: i16,                  //  2 B

    // --- Flex (u16 MSB-sentinel, ×100) ---
    pub flex_grow: u16,             //  2 B   (0..655.34, sentinel=65535)
    pub flex_shrink: u16,           //  2 B

    // --- Other ---
    pub z_index: i16,               //  2 B   (range ±32766, sentinel=32767)
    pub _pad: [u8; 2],              //  2 B   (alignment to 64)
}                                   // = 64 bytes
```

| DOM size | Tier 2 total | Fits in   |
|----------|-------------|-----------|
| 5,000    | 320 KB      | L2 cache (with Tier 1: 360 KB) |
| 20,000   | 1.28 MB     | L3 cache  |
| 72,000   | 4.5 MB      | L3 cache  |

### Tier 2b: `Vec<CompactTextProps>` — IFC/text-only properties

Only accessed for nodes that participate in inline formatting contexts.
Separate Vec so sizing-only loops don't touch this data.

```rust
#[repr(C)]
pub struct CompactTextProps {       // Total: 24 bytes/node
    pub text_color: u32,            //  4 B   (RGBA as 0xRRGGBBAA)
    pub font_family_hash: u64,      //  8 B   (FxHash of font-family list, 0 = sentinel)
    pub line_height: i16,           //  2 B   (px × 10, sentinel = 0x7FFF)
    pub letter_spacing: i16,        //  2 B   (px × 10)
    pub word_spacing: i16,          //  2 B   (px × 10)
    pub text_indent: i16,           //  2 B   (px × 10)
    pub vertical_align: u8,         //  1 B   (enum, 8 variants — could also be in Tier 1)
    pub _pad: [u8; 3],              //  3 B   (alignment)
}                                   // = 24 bytes
```

### Tier 3: `Vec<Option<Box<FxHashMap<CssPropertyType, CssProperty>>>>` — Overflow

For nodes with:
- `calc()` expressions
- Values that hit the sentinel cutoff (>6552px for u16, >134Kpx for u32)
- Grid properties, transforms, rare CSS

Use `Option<Box<>>` — 8 bytes/node (null pointer optimization).
Expect <1% of nodes to have a non-None entry.

## Total Memory Budget

| Tier | Per node | 5K nodes | 72K nodes | Cache level |
|------|----------|----------|-----------|-------------|
| 1: All enums (u64) | 8 B | 40 KB | 576 KB | L1 / L3 |
| 2: Dimensions | 64 B | 320 KB | 4.5 MB | L2 / L3 |
| 2b: Text props | 24 B | 120 KB | 1.7 MB | L2 / L3 |
| 3: Overflow | 8 B | 40 KB | 576 KB | — |
| **Total** | **104 B** | **520 KB** | **7.4 MB** | |

For 5K DOM: **Tier 1+2 = 360 KB → fits L2 (256-512KB typical).**
For 72K DOM: **7.4 MB → fits L3 (16-128MB), sequential prefetch.**

Compare: current BTreeMap → scattered heap allocations, zero prefetchability.

## Sentinel Encoding (MSB approach)

Use the **full numeric range**, reserve only the top few values as codes.
The "corruption zone" at the top is values you'd never see in real CSS:

```
u16:  65529..65535 reserved  →  max usable value = 6552.8px (at ×10)
i16:  32765..32767 reserved  →  max usable = ±3276.4px
u32:  0xFFFFFFF9..0xFFFFFFFF →  max usable = ±134,217.727px (at ×1000)
```

Getter pattern:

```rust
const U16_SENTINEL: u16 = 0xFFFF;
const U16_AUTO: u16 = 0xFFFE;

#[inline(always)]
fn get_display(tier1: &[u64], node_idx: usize) -> LayoutDisplay {
    // Pure bitwise decode, ~1ns
    unsafe { std::mem::transmute(((tier1[node_idx] >> 0) & 0x1F) as u8) }
}

#[inline]
fn get_padding_top(tier2: &[CompactNodeProps],
                   tier3: &[Option<Box<FxHashMap<..>>>],
                   node_idx: usize) -> f32 {
    let v = tier2[node_idx].padding_top;
    if v < 32765 {  // fast path: >99.99% of cases
        return (v as f32) / 10.0;  // arithmetic only
    }
    match v {
        32766 => 0.0,  // Auto → 0 for padding
        32767 => {     // Sentinel → slow path
            // look up in tier3, then CssPropertyCache
            ...
        }
        _ => 0.0,
    }
}

#[inline]
fn get_width(tier2: &[CompactNodeProps],
             tier3: &[Option<Box<FxHashMap<..>>>],
             node_idx: usize) -> LayoutWidth {
    let v = tier2[node_idx].width;
    if v <= 0xFFFFFFF8 {  // fast path
        return decode_u32_to_width(v);  // extract metric + value
    }
    match v {
        0xFFFFFFFE => LayoutWidth::Auto,
        0xFFFFFFFA => LayoutWidth::MinContent,
        0xFFFFFFF9 => LayoutWidth::MaxContent,
        0xFFFFFFFF => { /* sentinel: tier3 lookup */ ... }
        _ => LayoutWidth::Auto,
    }
}
```

## Which Tier Does Each Property Belong To?

Static compile-time mapping — each getter knows its tier:

```rust
fn property_tier(prop: CssPropertyType) -> Tier {
    match prop {
        // Tier 1: ALL enum properties (u64 bitfield)
        Display | Position | Float | OverflowX | OverflowY |
        BoxSizing | FlexDirection | FlexWrap | JustifyContent |
        AlignItems | AlignContent | WritingMode | Clear |
        FontWeight | FontStyle | TextAlign | Visibility |
        WhiteSpace | Direction | VerticalAlign => Tier::One,

        // Tier 2: numeric dimensions
        Width | Height | MinWidth | MaxWidth | MinHeight | MaxHeight |
        FlexBasis | FontSize |
        PaddingTop | PaddingRight | PaddingBottom | PaddingLeft |
        MarginTop | MarginRight | MarginBottom | MarginLeft |
        BorderTopWidth | BorderRightWidth | BorderBottomWidth | BorderLeftWidth |
        Top | Right | Bottom | Left |
        FlexGrow | FlexShrink | ZIndex => Tier::Two,

        // Tier 2b: text/IFC properties
        TextColor | FontFamily | LineHeight | LetterSpacing |
        WordSpacing | TextIndent => Tier::TwoB,

        // Tier 3: everything else
        _ => Tier::Three,
    }
}
```

## Implementation Steps

### Step 1: Define the compact structs

Create `core/src/compact_cache.rs`:
- `CompactNodeProps` (64 B)
- `CompactTextProps` (24 B)
- `CompactLayoutCache` holding `Vec<u64>`, `Vec<CompactNodeProps>`,
  `Vec<CompactTextProps>`, `Vec<Option<Box<FxHashMap<...>>>>`
- All MSB-sentinel encode/decode helper functions
- `build_compact_cache()` that iterates nodes and calls `get_property_slow()`

### Step 2: Wire up `build_compact_cache()` in styled_dom.rs

After `restyle()` + `apply_ua_css()`, call `build_compact_cache()` and
store the result on `CssPropertyCache` (or alongside `StyledDom`).

### Step 3: Update getters in `layout/src/solver3/getters.rs`

For each getter, add fast-path that reads from compact cache.
Start with the hottest (display, position, overflow), then dimensions,
then text props. ~15-20 getters total.

### Step 4: Benchmark and validate

Run git2pdf self-benchmark before and after. Targets:
- deserialize.rs (72K nodes): 60s → 6-10s (**6-10×**)
- printpdf total (28 files): 117s → 30-45s (**2.5-4×**)
- Average 5K DOM: 0.33s → 0.15s (**~2×**)

### Step 5: Remove `get_property_slow()` from hot paths

Once all layout getters use the compact cache, BTreeMap cascade is
only needed during `build_compact_cache()` and for paint properties.

## Files to Modify

| File | Change |
|------|--------|
| `core/src/compact_cache.rs` | **NEW** — all compact structs + builder |
| `core/src/lib.rs` | Add `pub mod compact_cache;` |
| `core/src/prop_cache.rs` | Add `compact_cache: Option<Box<CompactLayoutCache>>` field |
| `core/src/styled_dom.rs` | Call `build_compact_cache()` after restyle |
| `layout/src/solver3/getters.rs` | Update ~15-20 getters with fast path |
| `core/Cargo.toml` | Add `rustc-hash` dep (for FxHashMap in Tier 3) |
