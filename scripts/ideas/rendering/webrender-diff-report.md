# WebRender Vendored Fork — Full Diff Report

## Overview

The `webrender/` directory was vendored from `azul-2/webrender/` (original Mozilla
WebRender at commit `e1c924eb`) starting at commit `61d6b590` ("Add webrender/api
for hard-fork of WR at e1c924eb"). Since then, **111 files** were changed with
**~21k insertions and ~17k deletions** across **96 source files** in `webrender/api/src/`
and `webrender/core/src/`.

## Directory Mapping

| Original (`azul-2/webrender/`)   | Modified (`azul/webrender/`) |
|----------------------------------|------------------------------|
| `webrender_api/src/`             | `api/src/`                   |
| `webrender/src/`                 | `core/src/`                  |

## Category 1: PeekPoke Removal (display_item.rs, display_list.rs)

### What Changed

The original WR serialized display items and auxiliary data into byte streams using
the `peek_poke` crate (`poke_into_vec`, `peek_from_slice`, `skip_slice`, with
"red zones" for safety).

The modified version replaces this with **direct Vec storage**:

```
// ORIGINAL: 3 byte buffers
DisplayListPayload {
    items_data: Vec<u8>,    // serialized display items + inline aux data
    cache_data: Vec<u8>,    // serialized items for retained (cached) groups
    spatial_tree: Vec<u8>,  // serialized spatial tree items
}

// MODIFIED: 9 typed Vecs
DisplayListPayload {
    items: Vec<DisplayItem>,
    spatial_items: Vec<SpatialTreeItem>,
    glyphs: Vec<GlyphInstance>,
    stops: Vec<GradientStop>,
    filters: Vec<FilterOp>,
    filter_data: Vec<FilterData>,
    filter_primitives: Vec<FilterPrimitive>,
    clip_chain_items: Vec<ClipId>,
    points: Vec<LayoutPoint>,
}
```

### Marker Items Gained Count Fields

Original marker variants were unit:
```rust
SetGradientStops,
SetFilterOps,
SetFilterPrimitives,
SetPoints,
```

Modified markers carry a count so the iterator doesn't greedily consume:
```rust
SetGradientStops { stop_count: usize },
SetFilterOps { filter_count: usize },
SetFilterPrimitives { primitive_count: usize },
SetPoints { point_count: usize },
```

Additionally:
- `TextDisplayItem` gained `glyph_count: usize`
- `ClipChainItem` gained `clip_count: usize`

### push_iter / push_iter_impl are NO-OPs

```rust
fn push_iter_impl<I>(data: &mut Vec<u8>, iter_source: I) { /* empty */ }
pub fn push_iter<I>(&mut self, iter: I) { /* empty */ }
```

This is intentional — all specialized push methods (`push_stops`, `push_filters`,
`push_text`, etc.) directly extend the payload vecs. No code path relies on the
generic `push_iter` anymore.

### FilterData Handling Changed

Original stored `FilterData` as 5 separate `ItemRange`s inside `TempFilterData`:
```rust
struct TempFilterData<'a> {
    func_types: ItemRange<'a, ComponentTransferFuncType>,
    r_values: ItemRange<'a, f32>,
    ...
}
```

Modified stores the whole `FilterData` struct directly:
```rust
payload.filter_data.push(filter_data.clone());
```

The scene builder's `filter_datas_for_compositing()` changed from
`&[TempFilterData] → Vec<FilterData>` to `&Vec<&FilterData> → Vec<FilterData>`.
Both produce identical output.

### CachedDisplayItem (display_item_cache.rs)

Changed glyph serialization for the item cache:
```rust
// ORIGINAL: read bytes from the byte stream
data: item_ref.glyphs().bytes().to_vec(),

// MODIFIED: transmute_copy each GlyphInstance to bytes
data: item_ref.glyphs().iter().flat_map(|g| {
    let bytes: [u8; size_of::<GlyphInstance>()] = unsafe { transmute_copy(g) };
    bytes
}).collect(),
```

This is a potential issue if `GlyphInstance` has padding bytes, but only affects
the display item cache (Gecko's retained display list optimization), not the
normal rendering path used by Azul.

### Deserialize Path Bug (feature-gated, non-critical)

In the `#[cfg(feature = "deserialize")]` block, marker items are constructed
without counts:
```rust
Debug::SetGradientStops(stops) => {
    payload.stops.extend(stops);
    Real::SetGradientStops  // BUG: missing { stop_count: stops.len() }
}
```
Same for `SetFilterOps`, `SetFilterPrimitives`, `SetPoints`. However, this code
is behind feature flags (`serialize`/`deserialize`) which are **not enabled** in
the normal build, so it doesn't affect rendering.

### Serialize Path Has Matching Issue

The serialize debug conversion matches on `Real::SetFilterOps` and
`Real::SetGradientStops` without `{ .. }` — this likely causes compile errors
if features are enabled. Non-critical for normal builds.

### Section Handling Collapsed

```rust
// ORIGINAL: items go to different buffers based on section
fn push_item_to_section(&mut self, item, section) {
    poke_into_vec(item, self.buffer_from_section(section));
}

// MODIFIED: section parameter is ignored, all items go to payload.items
pub fn push_item_to_section(&mut self, item, _section) {
    self.payload.items.push(*item);
}
```

This means `RetainedItems` markers and cached items all end up in the same
`items` vec. For Azul's use case (no retained display lists), this is fine.

### Red Zones Removed

All `ensure_red_zone` and `strip_red_zone` calls removed. These were safety
buffers at the end of byte arrays to prevent peek_poke from reading past the
end. Not needed with Vec storage.

## Category 2: GL Context Abstraction (device/gl.rs)

- `dyn gl::Gl` replaced with `GenericGlContext` (from `azul_core::gl`)
- `ProgramBinary::new()` gained a `format: gl::GLenum` parameter
- `ShaderVersion::Gles` renamed to `ShaderVersion::GlEs`
- `source_digest` field made public

These are integration changes for Azul's GL abstraction layer.

## Category 3: MallocSizeOf Removal

`MallocSizeOf` derive and `malloc_size_of` dependency removed from all structs.
This was a Firefox memory reporting mechanism. Purely mechanical removal, no
functional impact.

## Category 4: Formatting / Import Reorganization (~80% of diff)

The vast majority of the diff (estimated **80%+**) consists of:
- `cargo fmt` style reformatting (line breaks, alignment, parentheses)
- Import reorganization (grouped `use` statements)
- `profile_scope!()` macro removals/comments
- Comment rewrapping

Files with ONLY formatting changes (no semantic impact):
- `batch.rs`, `prepare.rs`, `render_target.rs`, `render_task.rs`
- `gpu_cache.rs`, `gpu_types.rs`, `picture.rs`, `renderer/mod.rs`
- `prim_store/gradient/*.rs`, `internal_types.rs`, `filterdata.rs`
- `scene_building.rs` (except ItemRange→slice changes)

## Category 5: scene_building.rs — ItemRange to Slice

All `ItemRange<T>` parameters changed to `&[T]`:
```rust
// ORIGINAL
fn read_gradient_stops(stops: ItemRange<GradientStop>) -> Vec<GradientStopKey>
fn filter_ops_for_compositing(input_filters: ItemRange<FilterOp>) -> Vec<Filter>
fn filter_primitives_for_compositing(input: ItemRange<FilterPrimitive>) -> Vec<...>

// MODIFIED
fn read_gradient_stops(stops: &[GradientStop]) -> Vec<GradientStopKey>
fn filter_ops_for_compositing(input_filters: &[FilterOp]) -> Vec<Filter>
fn filter_primitives_for_compositing(input: &[FilterPrimitive]) -> Vec<...>
```

The `ItemRange::iter()` calls became direct slice iteration (`.iter().map(...)`).
Functionally equivalent.

Accessor changes on `DisplayItemRef`:
```rust
// ORIGINAL
item.clip_chain_items().into_iter()  // ItemRange iterator

// MODIFIED
item.clip_chain_items().iter().copied()  // slice iterator
```

---

## ROOT CAUSE: Why Gradients Don't Render

### The Bug

**The `compositor2.rs` pushes gradient stops, then defines clip items, then pushes
the gradient.** This causes clip items to interleave between `SetGradientStops`
and the `Gradient` display item.

The **display list iterator** (`BuiltDisplayListIter::next()`) accumulates auxiliary
data (stops, filters, etc.) while skipping marker items, but **stops when it hits
any non-marker item**. When a `RectClip` or `RoundedRectClip` item appears between
`SetGradientStops` and `Gradient`, the iterator:

1. Encounters `SetGradientStops` → sets `cur_stops` → continues (marker, skipped)
2. Encounters `RectClip` → this is NOT a marker → **breaks the loop**
3. Returns `RectClip` to the scene builder (with `cur_stops` set — but ignored)
4. On the next call to `next()`, `cur_stops` is **reset to `&[]`**
5. Encounters `Gradient` → returns it with **empty `cur_stops`**
6. Scene builder calls `item.gradient_stops()` → gets **zero stops**

### Why This Affects All Gradients in the Showcase

Every gradient in `effects-showcase.c` has `border-radius`, which causes
`define_border_radius_clip()` to be called between `push_stops()` and
`push_gradient()`:

```c
// ALL linear gradient examples have border-radius:
"width: 120px; height: 80px; border-radius: 8px;"
"background: linear-gradient(to right, #ff0000, #0000ff);"
```

### The Offending Code Pattern (compositor2.rs)

```rust
// 1. Push stops (adds SetGradientStops item + extends payload.stops)
builder.push_stops(&wr_stops);

// 2. Build gradient struct (just data, no display list effect)
let wr_gradient = WrGradient { start_point, end_point, extend_mode };

// 3. Define clip (pushes RectClip + RoundedRectClip + ClipChain items!)
if !wr_border_radius.is_zero() {
    let new_clip_id = define_border_radius_clip(&mut builder, ...);
    //                ^^^^^^^^^^^^^^^^^^^^^^^^^^
    // This pushes items BETWEEN SetGradientStops and Gradient!

    // 4. Push gradient (adds Gradient item)
    builder.push_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
}
```

### Fix

Move `push_stops(&wr_stops)` to immediately before `push_gradient()`:

```rust
let wr_gradient = WrGradient { start_point, end_point, extend_mode };

if !wr_border_radius.is_zero() {
    let new_clip_id = define_border_radius_clip(&mut builder, ...);
    let info = CommonItemProperties { clip_chain_id: new_clip_id, ... };
    builder.push_stops(&wr_stops);  // ← moved here
    builder.push_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
} else {
    let info = CommonItemProperties { clip_chain_id: current_clip!(), ... };
    builder.push_stops(&wr_stops);  // ← moved here
    builder.push_gradient(&info, rect, wr_gradient, tile_size, tile_spacing);
}
```

**This same fix must be applied to all three gradient types** (linear, radial,
conic) and any other pattern where auxiliary data markers are followed by
non-marker items before the consuming display item.

### Note on Original WebRender

The original WR has the **exact same iterator logic** — the interleaving would
also be a bug there. However, in the original Gecko usage, `define_clip_*` is
typically NOT called between stops and gradients (Gecko calls `create_gradient`
which bundles stops+gradient atomically). The `compositor2.rs` code is new and
inadvertently introduced this ordering bug.

## Summary of Actionable Items

| Priority | Issue | File | Fix |
|----------|-------|------|-----|
| **P0** | Gradient stops lost due to clip interleaving | `compositor2.rs` | Move `push_stops` to immediately before `push_gradient/push_radial_gradient/push_conic_gradient` |
| P2 | Deserialize path missing counts | `display_list.rs:327-343` | Add `{ stop_count: stops.len() }` etc. (feature-gated) |
| P2 | Serialize path pattern matching | `display_list.rs:655,681` | Add `{ .. }` to match patterns (feature-gated) |
| P3 | CachedDisplayItem transmute_copy | `display_item_cache.rs` | Replace with proper byte conversion (only affects Gecko-style caching) |
| P3 | Item group caching disabled | `display_list.rs:2091-2120` | Re-implement if retained items needed |
