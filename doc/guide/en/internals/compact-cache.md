---
slug: compact-cache
title: Compact Property Cache
language: en
canonical_slug: compact-cache
audience: contributor
maturity: mature
guide_order: null
topic_only: false
prerequisites: []
tracked_files:
  - css/src/compact_cache.rs
  - core/src/compact_cache_builder.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:43:38Z
---

The compact cache is a four-array, fixed-layout encoding of the ~50 layout-hot CSS properties. Layout reads them by node index in O(1), with no `BTreeMap` lookups and no cascade walks. Built once per restyle by `build_compact_cache_with_inheritance` (see [Cascade, Inheritance, Restyle](cascade.md)); read on every layout pass.

## File map

| File | Role |
|---|---|
| `css/src/compact_cache.rs:1411` | `CompactLayoutCache` — the four-array container |
| `css/src/compact_cache.rs:1138` | `CompactNodeProps` — Tier 2 layout-hot dimensions (68 B/node) |
| `css/src/compact_cache.rs:1180` | `CompactNodePropsCold` — Tier 2 cold paint props (28 B/node) |
| `css/src/compact_cache.rs:1368` | `CompactTextProps` — Tier 2b text/IFC props (24 B/node) |
| `css/src/compact_cache.rs:67`   | Tier 1 bit layout (u64 per node, 8 B/node) |
| `css/src/compact_cache.rs:40`   | Sentinel constants |
| `core/src/compact_cache_builder.rs` | The cascade-side encoder |

## Memory budget

For a 1000-node DOM:

| Array | Per node | 1000 nodes |
|---|---|---|
| `tier1_enums: Vec<u64>` | 8 B | 8 KB |
| `tier2_dims: Vec<CompactNodeProps>` | 68 B | 68 KB |
| `tier2_cold: Vec<CompactNodePropsCold>` | 28 B | 28 KB |
| `tier2b_text: Vec<CompactTextProps>` | 24 B | 24 KB |
| **Total** | **128 B** | **128 KB** |

Properties that don't fit (background, box-shadow, transform, filter, …) live on the slow `CssPropertyCache::get_property_slow` path and are not duplicated here.

## `CompactLayoutCache`

```rust,ignore
pub struct CompactLayoutCache {
    pub tier1_enums: Vec<u64>,
    pub tier2_dims: Vec<CompactNodeProps>,
    pub tier2_cold: Vec<CompactNodePropsCold>,
    pub tier2b_text: Vec<CompactTextProps>,
    pub font_dirty_nodes: Vec<usize>,
    pub prev_font_hashes: Vec<u64>,
    pub font_hash_to_families: BTreeMap<u64, Vec<StyleFontFamily>>,
    // …a few DOM-level flags described below
}
```

(`css/src/compact_cache.rs:1411`)

All four per-node arrays have length `node_count` and are indexed by `NodeId::index()`. Allocation happens in `CompactLayoutCache::with_capacity(n)` once per restyle.

`font_dirty_nodes` lists indices whose `font_family_hash` differs from `prev_font_hashes` — see the cascade page for how that drives incremental font resolution.

## Tier 1 — all enums in one u64

Every `enum`-valued layout property fits in a few bits. Tier 1 packs 21 of them into a single `u64`:

```text
[4:0]    display          5 bits  (22 variants)
[7:5]    position         3 bits
[9:8]    float            2 bits
[12:10]  overflow_x       3 bits
[15:13]  overflow_y       3 bits
[16]     box_sizing       1 bit
[18:17]  flex_direction   2 bits
[20:19]  flex_wrap        2 bits
[23:21]  justify_content  3 bits
[26:24]  align_items      3 bits
[29:27]  align_content    3 bits
[31:30]  writing_mode     2 bits
[33:32]  clear            2 bits
[37:34]  font_weight      4 bits
[39:38]  font_style       2 bits
[42:40]  text_align       3 bits
[44:43]  visibility       2 bits
[47:45]  white_space      3 bits
[48]     direction        1 bit
[51:49]  vertical_align   3 bits
[52]     border_collapse  1 bit
[55:53]  align_self       3 bits
[58:56]  justify_self     3 bits
[60:59]  grid_auto_flow   2 bits
[62:61]  justify_items    2 bits
[63]     TIER1_POPULATED  1 bit
```

(`css/src/compact_cache.rs:67`)

Bit 63 is a "this node has tier-1 data" flag. The bit layout treats `0` as "all defaults" (Display::Block, Position::Static, etc.) so an all-zero `tier1_enums[i]` with bit 63 set is semantically the same as a fresh `Default::default()` node — but `tier1_enums[i] == 0` (no bit 63) means "not yet populated" and forces a slow-path lookup.

`encode_tier1(display, position, float, …, border_collapse) -> u64` (`css/src/compact_cache.rs:858`) is the single producer; one `decode_*` function per field is the consumer:

```rust,ignore
#[inline(always)]
pub fn decode_display(t1: u64) -> LayoutDisplay {
    layout_display_from_u8(((t1 >> DISPLAY_SHIFT) & DISPLAY_MASK) as u8)
}
```

(`css/src/compact_cache.rs:900`)

The `_from_u8` / `_to_u8` pairs at `css/src/compact_cache.rs:163`-`784` are the per-enum codec. They use `match` rather than `transmute` so the compiler can prove every input maps to a defined output.

## Tier 2 hot — `CompactNodeProps`

```rust,ignore
#[repr(C)]
pub struct CompactNodeProps {
    // Dimensions with unit info: u32 MSB-sentinel
    pub width: u32, pub height: u32,
    pub min_width: u32, pub max_width: u32,
    pub min_height: u32, pub max_height: u32,
    pub flex_basis: u32,
    pub font_size: u32,

    // Resolved px × 10: i16 MSB-sentinel
    pub padding_top: i16,    pub padding_right: i16,
    pub padding_bottom: i16, pub padding_left: i16,
    pub margin_top: i16,     pub margin_right: i16,
    pub margin_bottom: i16,  pub margin_left: i16,
    pub border_top_width: i16, /* …4 sides */
    pub top: i16, pub right: i16, pub bottom: i16, pub left: i16,

    // Flex factors × 100: u16 MSB-sentinel
    pub flex_grow: u16,
    pub flex_shrink: u16,

    // Gap × 10: i16
    pub row_gap: i16,
    pub column_gap: i16,
}
```

(`css/src/compact_cache.rs:1138`)

68 bytes, layout-critical, accessed in every iteration of the constraint-solving loop. The integer encodings use the top of their unsigned/signed range as sentinels for "value doesn't fit, fall through to slow path".

## Tier 2 cold — `CompactNodePropsCold`

28 bytes of paint-only and rare-but-typed properties: border colors as `u32` RGBA, border radii as `i16`×10, `z_index`, `border_styles_packed` (4 bits per side), grid placement (`grid_col/row_start/end` as `i16` with `I16_AUTO` sentinel), `tab_size`, `border_spacing_h/v`, `opacity` (×254 with 255 = unset).

`CompactNodePropsCold` also carries two `u8` "has-X" flag bytes:

```rust,ignore
pub hot_flags: u8;
//   bit 0: has_transform
//   bit 1: has_transform_origin
//   bit 2: has_box_shadow
//   bit 3: has_text_decoration
//   bits 4-5: scrollbar_gutter (auto/stable/both-edges/mirror)
//   bit 6: has_background
//   bit 7: has_clip_path

pub extra_flags: u8;
//   bit 0: has_any_scrollbar_css
//   bit 1: has_counter
//   bit 2: has_break
//   bit 3: has_text_orientation
//   bit 4: has_text_shadow
//   bit 5: has_backdrop_filter
//   bit 6: has_filter
//   bit 7: has_mix_blend_mode
```

These are negative fast paths. When `hot_flags & HOT_FLAG_HAS_TRANSFORM == 0`, the renderer can skip the slow-cascade walk for `transform` entirely and use the identity matrix. The bit is set during cascade build only when the node actually declares the property.

`CompactLayoutCache` itself carries a parallel set of DOM-level flags (`DOM_HAS_TEXT_INDENT`, `DOM_HAS_LINE_HEIGHT`, …) at `css/src/compact_cache.rs:1263`. They mark "some node in this DOM declared this property" — when clear, the cascade walks for that prop are skipped across the whole DOM.

## Tier 2b — `CompactTextProps`

```rust,ignore
#[repr(C)]
pub struct CompactTextProps {
    pub text_color: u32,      // 0xRRGGBBAA
    pub font_family_hash: u64,
    pub line_height: i16,     // ×10, I16_SENTINEL = "normal"
    pub letter_spacing: i16,
    pub word_spacing: i16,
    pub text_indent: i16,
}
```

(`css/src/compact_cache.rs:1368`)

24 bytes of IFC/text-shaping inputs. The whole struct is inheritable as a unit, so the cascade builder copies `tier2b_text[parent]` to `tier2b_text[child]` in one move before running per-node CSS. `font_family_hash = 0` is the unset sentinel; the actual font-family list is looked up in `font_hash_to_families`, deduplicated across nodes.

## Sentinel encoding

Three encodings, three sentinel schemes:

| Encoding | Purpose | Range | Sentinels |
|---|---|---|---|
| `u32` (dimensions with unit) | width, height, min/max-*, flex-basis, font-size | low 4 bits = `SizeMetric`, upper 28 = signed `×1000` | `U32_SENTINEL=0xFFFFFFFF`, `U32_AUTO`, `U32_NONE`, `U32_INHERIT`, `U32_INITIAL`, `U32_MIN_CONTENT`, `U32_MAX_CONTENT`. Threshold = 0xFFFFFFF9. |
| `i16 ×10` (resolved px) | padding, margin, border-width, top/right/bottom/left, gap, radii, line-height, letter/word-spacing, text-indent, grid-* | -3276.7 .. +3276.4 px | `I16_SENTINEL=0x7FFF`, `I16_AUTO=0x7FFE`, `I16_INHERIT=0x7FFD`, `I16_INITIAL=0x7FFC`. Threshold = 0x7FFC. |
| `u16 ×100` (flex factor) | flex-grow, flex-shrink | 0.00 .. 655.28 | `U16_SENTINEL=0xFFFF`. Threshold = 0xFFF9. |

Any value at or above the threshold means "doesn't fit; ask the slow path" — the getter on `CssPropertyCache` handles fallbacks.

`encode_pixel_value_u32(pv: &PixelValue) -> u32` (`css/src/compact_cache.rs:1026`):

```rust,ignore
let metric = size_metric_to_u8(pv.metric) as u32;
let raw = pv.number.number;            // FloatValue is value × 1000
if !(-134_217_728..=134_217_727).contains(&raw) {
    return U32_SENTINEL;
}
let value_bits = ((raw as i32) as u32) << 4;
value_bits | metric
```

The `FloatValue` ×1000 representation is what makes this work in `const` context — encoding/decoding uses no floats.

## Border styles packed into u16

```rust,ignore
pub fn encode_border_styles_packed(
    top: BorderStyle, right: BorderStyle,
    bottom: BorderStyle, left: BorderStyle,
) -> u16 {
    (border_style_to_u8(top)    as u16)
  | ((border_style_to_u8(right)  as u16) << 4)
  | ((border_style_to_u8(bottom) as u16) << 8)
  | ((border_style_to_u8(left)   as u16) << 12)
}
```

(`css/src/compact_cache.rs:789`)

`BorderStyle` has 10 variants — fits in 4 bits. Decoders `decode_border_top_style` / `_right_` / `_bottom_` / `_left_` (`css/src/compact_cache.rs:798`) extract a single side.

## Color encoding

`encode_color_u32(c: &ColorU) -> u32` packs RGBA as `0xRRGGBBAA`. `0x00000000` is the unset sentinel — meaning fully-transparent black (`rgba(0,0,0,0)`) collides with unset. The decoder returns `None` for `0`, and the renderer treats `None` as "use the parent's text color" or the property's initial value. In practice, fully-transparent black is rare enough that the collision is acceptable; callers who need to distinguish reach for the slow path.

## Reading the cache

The fast-path getters live on `CompactLayoutCache` (`css/src/compact_cache.rs:1261`-`1696`). Examples:

```rust,ignore
let cache: &CompactLayoutCache = ...;
let nid: usize = node_id.index();

// Tier 1 — single shift+mask, no branches
let display: LayoutDisplay = decode_display(cache.tier1_enums[nid]);

// Tier 2 — direct field
let pad_top_x10: i16 = cache.tier2_dims[nid].padding_top;

// Tier 2 — with sentinel handling
match cache.get_width_raw(nid) {
    Some(pv) => /* use pv */,
    None => /* slow-path */,
}

// Cold flag — short-circuit before the slow walk
if cache.tier2_cold[nid].hot_flags & HOT_FLAG_HAS_TRANSFORM != 0 {
    let transform = cache.get_transform_slow(node_data, nid, state);
}
```

`get_*_raw` returns the typed value directly; `get_*` returns `None` if the slot holds a sentinel (caller falls back to `get_property_slow`). The `is_*_auto` family (`is_margin_top_auto`, `is_grid_col_start_auto`, …) is a fast check for the `Auto` sentinel that doesn't require decoding.

## Encoding side

`build_compact_cache_with_inheritance` populates the four arrays in pre-order. Per-node encoder calls (`core/src/compact_cache_builder.rs:138`):

```rust,ignore
result.tier1_enums[i] = encode_tier1(
    display, position, float, overflow_x, overflow_y, box_sizing,
    flex_direction, flex_wrap, justify_content, align_items, align_content,
    writing_mode, clear, font_weight, font_style, text_align,
    visibility, white_space, direction, vertical_align, border_collapse,
);

if let Some(val) = self.get_width(nd, &node_id, &default_state) {
    result.tier2_dims[i].width = encode_layout_width(val);
}
// …
```

Each `get_*` call cascades through `CssPropertyCache` (UA → global → cascaded → inline → user override). The result is an `Option<CssPropertyValue<T>>`; `None` leaves the slot at `Default::default()`.

`encode_layout_width` and friends live in `core/src/compact_cache_builder.rs` because they need access to the cascade's `CssPropertyValue` resolution; pure encode helpers (`encode_pixel_value_u32`, `encode_resolved_px_i16`, `encode_flex_u16`, `encode_color_u32`) are in `css/src/compact_cache.rs` and have no `core` dependency.

## Defaults

```rust,ignore
impl Default for CompactNodeProps {
    fn default() -> Self {
        Self {
            width: U32_AUTO,            height: U32_AUTO,
            min_width: U32_AUTO,        max_width: U32_NONE,
            min_height: U32_AUTO,       max_height: U32_NONE,
            flex_basis: U32_AUTO,       font_size: U32_INITIAL,
            padding_top: 0, /* …all 4 sides … */
            margin_top: 0,  /* …all 4 sides … */
            border_top_width: 0,
            top: I16_AUTO, right: I16_AUTO, bottom: I16_AUTO, left: I16_AUTO,
            flex_grow: 0,
            flex_shrink: encode_flex_u16(1.0),  // CSS default
            row_gap: 0,
            column_gap: 0,
        }
    }
}
```

(`css/src/compact_cache.rs:1291`)

`CompactNodePropsCold::default()` uses `I16_SENTINEL` for radii (no rounded corners → skip slow walk) and `I16_AUTO` for `z_index` and grid lines. `CompactTextProps::default()` uses `I16_SENTINEL` for `line_height` to mean "normal".

## When the slow path is faster

The compact cache is a worthwhile trade-off because the slow `get_property_slow` walks five sources per call (user override → inline → cascaded → computed → default). For ~50 properties read on every layout pass on every node, that's hundreds of thousands of walks per frame on a non-trivial DOM. The compact cache turns those into array indexing.

For uncommon properties (transform, box-shadow, filter, content, transitions), the per-frame call count is low enough that the slow path is fine — and adding them to compact tiers would inflate per-node memory without measurable speedup. The `HOT_FLAG_HAS_*` and `DOM_HAS_*` bits are the compromise: stay slow-path, but skip the walk entirely when the property is known unset.

## Adding a new compact-cached property

1. Pick the tier:
   - **Tier 1** if the property is an enum with ≤ 8 variants and the bit budget has room (3 spare bits at the top of `u64`).
   - **Tier 2 hot** if it's a numeric layout-critical dimension. Use `i16 ×10` for resolved px, `u32` if you need a unit (em/%, etc.).
   - **Tier 2 cold** if it's paint-only or rare. Add a `HOT_FLAG_HAS_*` bit if the value is usually unset.
   - **Tier 2b** if it's text/IFC and inheritable.
2. Add the field to the appropriate struct in `css/src/compact_cache.rs`. Update `Default`.
3. If new tier-1 bits, add `*_SHIFT` and `*_MASK` constants and update `encode_tier1` + the matching `decode_*`.
4. Add `encode_*` and `decode_*` helpers near the existing ones in `compact_cache.rs`.
5. Add encoder calls in `core/src/compact_cache_builder.rs` Step 3 and (if it's a global `*` rule target) Step 2.5.
6. Add inheritance handling in Step 1 if the property inherits.
7. Add a getter on `CompactLayoutCache` (`get_<prop>_raw`, `get_<prop>`, `is_<prop>_auto`).
8. Update the slow-path fallback in `CssPropertyCache::get_property_slow` so callers who don't use the compact cache still work.

## See also

- [Cascade, Inheritance, Restyle](cascade.md) — how `build_compact_cache_with_inheritance` populates these arrays.
- [CSS Parser](css-parser.md) — the source of the `CssProperty` values that get encoded.
- [DOM Internals](dom.md) — the `NodeData::css_props` are one input to the cascade.
