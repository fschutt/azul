---
slug: webrender-bridge
title: WebRender Bridge
language: en
canonical_slug: webrender-bridge
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: []
tracked_files:
  - core/src/gpu.rs
  - dll/src/desktop/compositor2.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T12:00:00Z
---

> **WIP** — `compositor2::translate_displaylist_to_wr` is ~2170 LOC of one
> match. Per-arm extraction is on the cleanup list; signatures here are
> stable but call sites may be reorganised.

`dll/src/desktop/compositor2.rs` is the bridge between
`azul_layout::solver3::display_list::DisplayList` (Azul's primitives, in
logical CSS pixels, in absolute window coordinates) and WebRender's
`BuiltDisplayList` (physical pixels, frame-local coordinates, with explicit
clip chains and spatial nodes). One function does the translation;
`core/src/gpu.rs` keeps WebRender's animatable property keys in sync so
`transform`/`opacity` updates skip the bridge entirely.

## Entry point

```rust,ignore
pub fn translate_displaylist_to_wr(
    display_list: &DisplayList,
    pipeline_id: PipelineId,
    viewport_size: DeviceIntSize,
    renderer_resources: &azul_core::resources::RendererResources,
    dpi: DpiScaleFactor,
    wr_resources: Vec<WrResourceUpdate>,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    document_id: u32,
) -> Result<
    (
        Vec<WrResourceUpdate>,
        WrBuiltDisplayList,
        Vec<(PipelineId, WrBuiltDisplayList)>,
    ),
    String,
>;
```

`compositor2.rs:165`. Returns the (possibly augmented) resource update list,
the built WebRender display list, and a flat vector of nested pipelines
produced by recursive `VirtualView` translations. The caller (`generate_frame`
in `wr_translate2.rs`) packages these into a WebRender `Transaction` and
ships it to the render backend thread.

## The three stacks

State carried through the loop:

```rust,ignore
let mut clip_stack: Vec<WrClipChainId> = vec![root_clip_chain_id];
let mut spatial_stack: Vec<SpatialId>   = vec![spatial_id];
let     offset_stack: Vec<(f32, f32)>   = vec![(0.0, 0.0)];
```

`compositor2.rs:230-239`. The current top of each stack is read through three
macros, which return the root entry on underflow rather than panicking:

```rust,ignore
macro_rules! current_spatial { () => { spatial_stack.last().copied().unwrap_or(spatial_id) } }
macro_rules! current_clip    { () => { clip_stack.last().copied().unwrap_or(root_clip_chain_id) } }
macro_rules! current_offset  { () => { offset_stack.last().copied().unwrap_or((0.0, 0.0)) } }
```

`PopScrollFrame` underflow returns `Err("Scroll frame stack underflow")`
(`compositor2.rs:1051`); other `Pop*` items skip silently when only the root
remains.

## Coordinate translation

Every `Push*` and primitive item runs the same two transforms on its bounds:

1. **Logical → physical**: multiply by `dpi.inner.get()`.
2. **Absolute → frame-local**: subtract `current_offset!()`.

Two helpers bundle both steps so callers cannot do step 1 and forget step 2:

```rust,ignore
fn resolve_rect(
    bounds: &WindowLogicalRect, dpi: f32, offset: (f32, f32),
) -> LayoutRect;

fn resolve_point(
    bounds: &WindowLogicalRect, dpi: f32, offset: (f32, f32),
) -> LayoutPoint;
```

`compositor2.rs:81-107`. The single-axis helper `scale_px(val, dpi) -> f32`
exists for components (border widths, blur radii) that only need DPI scaling.

`LayoutTransform`'s translation row is also pre-multiplied by DPI
(`compositor2.rs:1336-1345`) so transformed reference frames operate in the
same physical-pixel space as everything else.

## Why scroll frames don't push an offset

The `offset_stack` exists for the rare context that genuinely shifts the
coordinate origin. Scroll frames and reference frames **do not push to it**.
The reasoning is at `compositor2.rs:950-964`:

> WebRender scroll frames do NOT create a new coordinate origin. They are NOT
> reference frames — they share the same coordinate space as their parent.
> The scroll frame only applies a translation (scroll offset) to child
> content, and clips it to the viewport (frame_rect).
>
> Our display list uses ABSOLUTE WINDOW coordinates for all primitives.
> Inside a scroll frame, those absolute coordinates ARE the correct
> scroll-frame-local coordinates.

Subtracting `frame_rect.origin` would push row 0 from window-y=53 to y=0,
above the viewport clip at y=53. The clip and the content must share the
same coordinate space; pushing an offset would silently desync them.

The same reasoning applies to `PushReferenceFrame`: the reference frame is
pushed at `LayoutPoint::zero()` and the dynamic `LayoutTransform` carries all
the movement; child items keep their absolute (DPI-scaled) coordinates
(`compositor2.rs:1357-1376`).

## Item → WebRender call mapping

| Display list item | WebRender call |
|---|---|
| `Rect`, `SelectionRect`, `CursorRect` | `push_rect` (with optional `define_clip_rounded_rect` for non-zero `border_radius`) |
| `Border` | `push_border` via `wr_translate2::get_webrender_border` |
| `ScrollBar` | `push_rect` + `push_hit_test`, optional opacity stacking context |
| `ScrollBarStyled` | composite of track + thumb + arrows + hit-test |
| `PushClip` | `define_clip_rect` or `define_clip_rounded_rect`, then `define_clip_chain` |
| `PushScrollFrame` | `define_scroll_frame` + `define_clip_rect` (in **parent space**) + `define_clip_chain` + scroll-container `push_hit_test` |
| `HitTestArea` | `push_hit_test` (+ debug red overlay under `cfg(debug_assertions)`) |
| `Underline` | `push_text_decoration_rect` helper |
| `Text` | `push_text` helper (font lookup + glyph translation) |
| `Image` | `push_image` |
| `PushStackingContext` | `push_simple_stacking_context` at scaled origin |
| `PushReferenceFrame` | `push_reference_frame` with `PropertyBinding::Binding` |
| `VirtualView` | recursive `translate_displaylist_to_wr` + `push_iframe` |
| `LinearGradient` / `RadialGradient` / `ConicGradient` | `create_*_gradient` + `push_gradient` |
| `BoxShadow` | `push_box_shadow` |
| `PushFilter` | `push_simple_stacking_context_with_filters` |
| `PushBackdropFilter` | stacking context with `backdrop_filters` |
| `PushOpacity` | stacking context with `WrFilterOp::Opacity(PropertyBinding::Value)` |
| `PushTextShadow` / `PopTextShadow` | `push_shadow` / `pop_all_shadows` |
| `PushImageMaskClip` | `define_clip_image_mask` + `define_clip_chain` |

`Pop*` items pop the corresponding stacks and call the inverse WebRender
function (`pop_stacking_context`, `pop_reference_frame`, …).

## The text path

`push_text` (`compositor2.rs:2334-2422`) is the only primitive that walks
auxiliary state. It performs three lookups against `RendererResources`:

```rust,ignore
// 1. font_hash (computed in layout) -> FontKey
let font_key = renderer_resources.font_hash_map.get(&font_hash)?;

// 2. (FontKey, Au-size, dpi) -> FontInstanceKey
let (_, instances) = renderer_resources.currently_registered_fonts.get(font_key)?;
let font_instance_key = *instances.get(&(font_size, dpi))?;

// 3. translate to WebRender FontInstanceKey
let wr_font_instance_key =
    crate::desktop::wr_translate2::wr_translate_font_instance_key(font_instance_key);
```

Glyph positions arrive already in absolute window coordinates (the layout's
`paint_inline_content` adds the container origin); `push_text` only DPI-scales
and subtracts `scroll_offset`. A missing key short-circuits the function with
a debug log; the text just doesn't render.

Underlines are emitted separately as `Underline` items so they participate in
the same clip/spatial context as the text without additional state.

## Border-radius clipping

`define_border_radius_clip` (`compositor2.rs:2234-2263`) is invoked from any
item that takes a `BorderRadius`:

```rust,ignore
fn define_border_radius_clip(
    builder: &mut WrDisplayListBuilder,
    layout_rect: LogicalRect,
    wr_border_radius: WrBorderRadius,
    rect_spatial_id: SpatialId,
    parent_clip_chain_id: WrClipChainId,
) -> WrClipChainId
```

For zero radii it falls back to `define_clip_rect`. For non-zero radii it
calls `define_clip_rounded_rect` with `WrComplexClipRegion`. Either way it
parents the new clip into the current chain via `define_clip_chain`. Items
that need rounded clipping push a *new* clip-chain id onto `clip_stack` and
do **not** require a matching `PopClip` — the chain is implicit and dropped
when the next `PopClip` would discard it.

## Scroll frames

```rust,ignore
let scroll_spatial_id = builder.define_scroll_frame(
    parent_space,                                  // outer spatial node
    ExternalScrollId(*scroll_id, pipeline_id),     // stable user-facing id
    content_rect,                                  // total scrollable extent
    adjusted_frame_rect,                           // visible viewport (parent-space)
    LayoutVector2D::zero(),                        // initial offset
    0,                                             // APZ scroll generation
    HasScrollLinkedEffect::No,
    SpatialTreeItemKey::new(*scroll_id, 0),
);
```

`compositor2.rs:930-939`. The viewport clip is defined in **parent space**
(`compositor2.rs:967-969`) so it stays put while content scrolls. A
hit-test rect tagged `(scroll_id, TAG_TYPE_SCROLL_CONTAINER)` is pushed on
the same parent-space clip chain so the scroll manager can resolve wheel
events to a scrollable region.

The scroll offset itself is updated out-of-band by
`Transaction::set_scroll_offsets` keyed on `ExternalScrollId`. The display
list never needs to be rebuilt for scrolling.

## Reference frames and dynamic transforms

`PushReferenceFrame` carries a `transform_key: TransformKey` and an
`initial_transform: ComputedTransform3D`:

```rust,ignore
let binding = PropertyBinding::Binding(
    webrender::api::PropertyBindingKey::new(transform_key.id as u64),
    wr_transform,
);

let new_spatial_id = builder.push_reference_frame(
    LayoutPoint::zero(),
    parent_spatial_id,
    TransformStyle::Flat,
    binding,
    ReferenceFrameKind::Transform { /* ... */ },
    SpatialTreeItemKey::new(transform_key.id as u64, 0),
);
```

`compositor2.rs:1320-1378`. The `PropertyBindingKey` is **the same `u64`** as
`transform_key.id`, which is what `Transaction::append_dynamic_properties`
uses to look up the binding when an animation frame updates the matrix. This
is the channel by which CSS transforms animate without going through layout
or `compositor2` again.

`PushOpacity` and the per-scrollbar opacity wrapper use the same mechanism
with `WrFilterOp::Opacity(PropertyBinding::Binding(…))`
(`compositor2.rs:434-451`, `2052-2073`).

## GPU value cache (`core/src/gpu.rs`)

`GpuValueCache` is the bookkeeper that pairs `NodeId`s with the
`TransformKey` / `OpacityKey` that `compositor2` emits into reference frames
and stacking contexts:

```rust,ignore
pub struct GpuValueCache {
    pub css_transform_keys: HashMap<NodeId, TransformKey>,
    pub css_current_transform_values: HashMap<NodeId, ComputedTransform3D>,
    pub opacity_keys: HashMap<NodeId, OpacityKey>,
    pub current_opacity_values: HashMap<NodeId, f32>,
    // scrollbar mirrors keyed by NodeId / (DomId, NodeId)
}
```

`gpu.rs:39-72`. Each frame, `synchronize(&mut self, &StyledDom) ->
GpuEventChanges` walks every node and emits one of three event variants per
diffed property:

```rust,ignore
pub enum GpuTransformKeyEvent {
    Added(NodeId, TransformKey, ComputedTransform3D),
    Changed(NodeId, TransformKey, ComputedTransform3D, ComputedTransform3D),
    Removed(NodeId, TransformKey),
}

pub enum GpuOpacityKeyEvent {
    Added(NodeId, OpacityKey, f32),
    Changed(NodeId, OpacityKey, f32, f32),
    Removed(NodeId, OpacityKey),
}
```

`gpu.rs:75-87`, `gpu.rs:280-289`. The events are aggregated into
`GpuEventChanges`, whose three vectors (`transform_key_changes`,
`opacity_key_changes`, `scrollbar_opacity_changes`) feed
`Transaction::append_dynamic_properties`.

The fast path is the **compact property cache** check at the top of each
synchronize loop:

```rust,ignore
if let Some(ref cc) = css_property_cache.compact_cache {
    if !cc.has_transform(node_id.index())
        && self.css_current_transform_values.get(&node_id).is_none()
    {
        return None; // never set, never had a value — no event
    }
}
```

`gpu.rs:103-114`. Only nodes that already have, or have just acquired, a
non-default value pay the cascade-walk cost. For opacity, the same check
uses an `OPACITY_SENTINEL` byte (`gpu.rs:170-188`).

## CPU feature detection

The `synchronize` loop runs CPU-feature detection once on first call:

```rust,ignore
#[cfg(target_arch = "x86_64")]
unsafe {
    if !INITIALIZED.load(AtomicOrdering::SeqCst) {
        let cpuid = __cpuid(1);
        USE_SSE.store((cpuid.edx & (1_u32 << 25)) != 0, AtomicOrdering::SeqCst);
        USE_AVX.store((cpuid.ecx & (1_u32 << 28)) != 0, AtomicOrdering::SeqCst);
        INITIALIZED.store(true, AtomicOrdering::SeqCst);
    }
}
```

`gpu.rs:113-126`. The flags live on `core::transform` and gate the SIMD path
inside `ComputedTransform3D::from_style_transform_vec`. Non-x86_64 targets
skip the `cfg` block entirely and run the scalar path.

## Resource translation

`renderer_resources` arrives already populated by
`wr_translate2::translate_resource_updates` (called by `generate_frame`
before `compositor2`). The vector is passed through `translate_displaylist_to_wr`
unchanged and re-emerges in the output tuple. `compositor2` does not synthesise
new resources; image and font lookups inside the loop only consume what's
already there.

The exception is **VirtualView recursion**: when an iframe item pushes a
nested pipeline, the recursive `translate_displaylist_to_wr` call gets an
empty `Vec::new()` for `wr_resources`
(`compositor2.rs:1428`) because resources are global to the document and
already registered on the parent transaction.

## Virtual view recursion

```rust,ignore
DisplayListItem::VirtualView { child_dom_id, bounds, clip_rect } => {
    let child_pipeline_id = wr_translate_pipeline_id(AzulPipelineId(
        child_dom_id.inner as u32, document_id,
    ));
    let child_layout = layout_results.get(child_dom_id)?;

    let (_, child_dl, mut child_nested) = translate_displaylist_to_wr(
        &child_layout.display_list, child_pipeline_id, viewport_size,
        renderer_resources, dpi, Vec::new(), layout_results, document_id,
    )?;

    nested_pipelines.push((child_pipeline_id, child_dl));
    nested_pipelines.append(&mut child_nested);

    builder.push_iframe(wr_bounds, wr_clip_rect, &space_and_clip,
                        child_pipeline_id, false);
}
```

`compositor2.rs:1386-1486`. Each virtualised view becomes its own WebRender
pipeline; `nested_pipelines` is flattened depth-first so the caller can
register every pipeline on the transaction before submitting.

`VirtualViewPlaceholder` items reaching this match are an error — they
should have been resolved to `VirtualView` items by the IFrame manager
during display-list generation. They log a warning and produce no output
(`compositor2.rs:1488-1496`).

## Filter translation

CSS filters map to WebRender `FilterOp`s through `translate_style_filters_to_wr`
(`compositor2.rs:2266-2330`):

| `StyleFilter` | `WrFilterOp` |
|---|---|
| `Blur(w, h)` | `Blur(w_px, h_px)` |
| `Opacity(v)` | `Opacity(PropertyBinding::Value(v), v)` |
| `Brightness`, `Contrast`, `Grayscale`, `Invert`, `Saturate`, `Sepia` | one-arg variants |
| `HueRotate(angle)` | `HueRotate(degrees)` |
| `ColorMatrix(m)` | `ColorMatrix([f32; 20])` |
| `DropShadow{ offset, color, blur }` | `DropShadow(WrShadow)` |
| `Flood(color)` | `Flood(ColorF)` |
| `Blend`, `ComponentTransfer`, `Offset`, `Composite` | dropped (SVG-specific, no WR equivalent) |

Length values inside filters are run through `to_pixels_internal` with a root
font size of `DEFAULT_ROOT_FONT_SIZE_PX = 16.0`.

## Debug overlays

Compiled-in debug overlays are guarded by `#[cfg(debug_assertions)]`:

- `HitTestArea` items render a 30%-opacity red rectangle so hit-test regions
  are visible in debug builds (`compositor2.rs:1059-1068`).

A more granular runtime flag is on the cleanup list — debug-build users have
reported the red overlay as a regression. For now, switch to a release build
to suppress it.

## Reading order

1. `layout/src/solver3/display_list.rs` — `DisplayListItem` enum.
2. `compositor2.rs:165` — `translate_displaylist_to_wr` entry point.
3. `compositor2.rs:230-265` — the three stacks and their accessor macros.
4. `compositor2.rs:872-1028` — `PushScrollFrame` (the most subtle item).
5. `compositor2.rs:1320-1378` — `PushReferenceFrame` + dynamic transforms.
6. `core/src/gpu.rs:75-291` — `GpuValueCache::synchronize`.
7. [`rendering-pipeline.md`](rendering-pipeline.md) — the wider context.
8. [`gl-loading.md`](gl-loading.md) — how the GL context that backs
   WebRender is bootstrapped.
