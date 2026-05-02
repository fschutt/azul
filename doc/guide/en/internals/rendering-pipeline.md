---
slug: rendering-pipeline
title: Rendering Pipeline
language: en
canonical_slug: rendering-pipeline
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: []
tracked_files:
  - core/src/gl.rs
  - core/src/gl_fxaa.rs
  - core/src/glconst.rs
  - core/src/gpu.rs
  - dll/src/desktop/compositor2.rs
  - dll/src/desktop/shell2/common/gl_loader.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T12:00:00Z
---

> **WIP** — APIs in `compositor2` and `gpu` are still moving. The shape
> documented here matches HEAD; signatures may shift before 1.0.

The frame pipeline: `LayoutWindow::layout_and_generate_display_list()` produces an
`azul_layout::solver3::display_list::DisplayList`; `compositor2::translate_displaylist_to_wr`
converts it into a WebRender `BuiltDisplayList`; the [WebRender bridge](webrender-bridge.md)
submits the resulting `Transaction` to the WebRender backend thread. Hardware (GPU
WebRender) and software (CPU `cpurender`) paths share the same display list — the split
is purely in the sink.

## Where each stage lives

| Stage | Code | Output |
|---|---|---|
| Layout → display list | `layout/src/window.rs:484` (`layout_and_generate_display_list`) | `DisplayList` (Vec of `DisplayListItem`) |
| GPU key sync | `core/src/gpu.rs` (`GpuValueCache::synchronize`) | `GpuEventChanges` |
| Display list translation | `dll/src/desktop/compositor2.rs:165` (`translate_displaylist_to_wr`) | `WrBuiltDisplayList` + nested pipelines |
| Resource translation | `dll/src/desktop/wr_translate2.rs` | `Vec<WrResourceUpdate>` |
| Frame submission | `dll/src/desktop/wr_translate2.rs` (`generate_frame`) | WebRender `Transaction` |
| GL function loading | `core/src/gl.rs` + `dll/src/desktop/shell2/common/gl_loader.rs` | `Rc<GenericGlContext>` |
| Built-in shaders | `core/src/gl.rs:1027` (`GlContextPtr::new`) | SVG / multicolor / FXAA programs |

## Per-frame call order

1. `LayoutWindow` runs cascade + layout, producing `DomLayoutResult` per `DomId`.
2. `GpuValueCache::synchronize(&styled_dom)` walks every node, diffs CSS `transform` and
   `opacity` against the previous frame, and emits `GpuTransformKeyEvent` /
   `GpuOpacityKeyEvent` deltas (`core/src/gpu.rs:103`).
3. `wr_translate2` converts new resources (fonts, images) into `WrResourceUpdate`s.
4. `compositor2::translate_displaylist_to_wr` walks the `DisplayList`, building the
   WebRender display list and spawning recursive translations for `VirtualView` items.
5. WebRender's `Transaction` is sent to the render backend thread; on completion,
   the OS-driven repaint (`WM_PAINT`, `drawRect:`, `xdg_surface.configure`, …) blits
   the framebuffer.
6. GPU-only updates (CSS `transform` / `opacity` animations) skip steps 1 and 4.
   `GpuValueCache::synchronize` produces a `GpuEventChanges` that
   `append_dynamic_properties` ships to WebRender without a fresh display list.

## Display list items

`DisplayListItem` is the `enum` defined in `layout/src/solver3/display_list.rs:176`.
The `compositor2` match arms cover the full set:

| Variant | WebRender API call |
|---|---|
| `Rect`, `SelectionRect`, `CursorRect` | `builder.push_rect` (with optional rounded clip) |
| `Border` | `builder.push_border` |
| `ScrollBar`, `ScrollBarStyled` | `push_rect` + `push_hit_test` (+ optional opacity stacking context) |
| `PushClip` / `PopClip` | `define_clip_rect` / `define_clip_rounded_rect` + `define_clip_chain` |
| `PushScrollFrame` / `PopScrollFrame` | `define_scroll_frame` |
| `HitTestArea` | `push_hit_test` |
| `Underline` | `push_text_decoration_rect` (helper at `compositor2.rs:128`) |
| `Text` | `push_text` (`compositor2.rs:2334`) |
| `Image` | `push_image` |
| `PushStackingContext` / `PopStackingContext` | `push_simple_stacking_context` / `pop_stacking_context` |
| `PushReferenceFrame` / `PopReferenceFrame` | `push_reference_frame` (with `PropertyBinding`) |
| `VirtualView` | recursive `translate_displaylist_to_wr` + `push_iframe` |
| `LinearGradient`, `RadialGradient`, `ConicGradient` | `create_*_gradient` + `push_gradient` |
| `BoxShadow` | `push_box_shadow` |
| `PushFilter` / `PopFilter` | `push_simple_stacking_context_with_filters` (filters mapped via `translate_style_filters_to_wr`) |
| `PushBackdropFilter` / `PopBackdropFilter` | stacking context with `backdrop_filters` |
| `PushOpacity` / `PopOpacity` | stacking context with `WrFilterOp::Opacity` |
| `PushTextShadow` / `PopTextShadow` | `push_shadow` / `pop_all_shadows` |
| `PushImageMaskClip` / `PopImageMaskClip` | `define_clip_image_mask` + clip chain |

## The three coordinate stacks

`translate_displaylist_to_wr` carries three parallel stacks
(`compositor2.rs:230-239`):

```rust,ignore
let mut clip_stack: Vec<WrClipChainId> = vec![root_clip_chain_id];
let mut spatial_stack: Vec<SpatialId>   = vec![spatial_id];
let     offset_stack: Vec<(f32, f32)>   = vec![(0.0, 0.0)];
```

- **`clip_stack`** is pushed by `PushClip`, `PushScrollFrame`, `PushImageMaskClip`;
  popped by their `Pop*` counterparts. The current top is the `clip_chain_id` for
  every primitive.
- **`spatial_stack`** is pushed by `PushScrollFrame` and `PushReferenceFrame`. Every
  primitive's `spatial_id` is the current top.
- **`offset_stack`** subtracts a coarse origin from absolute window coordinates. It
  exists for non-scroll-frame, non-reference-frame contexts that *do* shift the
  origin. **Scroll frames and reference frames deliberately do not push an offset**
  — see the comment at `compositor2.rs:950`. WebRender scroll frames share the
  parent's coordinate space; subtracting `frame_rect.origin` would push content
  above the viewport clip.

`PopScrollFrame` returning a stack underflow is treated as a hard error
(`compositor2.rs:1051`). `PopClip` and `PopImageMaskClip` skip silently when only
the root entry remains, since underflow there merely produces a degraded paint
rather than a corrupt scene.

## Coordinate model

Display list items hold logical CSS pixels in `WindowLogicalRect` (absolute window
coordinates). `compositor2` converts every coordinate in two steps:

1. Multiply by `dpi.inner.get()` (DPI scale factor).
2. Subtract the current `offset_stack` top.

The `resolve_rect` and `resolve_point` helpers (`compositor2.rs:81-107`) bundle
both steps so callers cannot forget one. Any `LayoutTransform` translation
component is also pre-multiplied by DPI (`compositor2.rs:1336-1345`) — transforms
share the post-DPI coordinate space.

A scroll frame defines its `frame_rect` (viewport) and `content_rect` (scrollable
extent) both in **parent space**. Children inside a scroll frame keep their
absolute window coordinates because the scroll frame is not a reference frame.
The viewport clip is defined in parent space so it stays stationary while content
scrolls (`compositor2.rs:967-969`).

## GPU-resident state

`core/src/gpu.rs` caches WebRender keys for animatable CSS properties so that GPU
updates can bypass display-list rebuilds.

```rust,ignore
pub struct GpuValueCache {
    pub css_transform_keys: HashMap<NodeId, TransformKey>,
    pub css_current_transform_values: HashMap<NodeId, ComputedTransform3D>,
    pub opacity_keys: HashMap<NodeId, OpacityKey>,
    pub current_opacity_values: HashMap<NodeId, f32>,
    pub transform_keys: HashMap<NodeId, TransformKey>,            // scrollbar v thumb
    pub h_transform_keys: HashMap<NodeId, TransformKey>,          // scrollbar h thumb
    pub scrollbar_v_opacity_keys: HashMap<(DomId, NodeId), OpacityKey>,
    pub scrollbar_h_opacity_keys: HashMap<(DomId, NodeId), OpacityKey>,
    // ...current_values and current_opacity_values mirrors
}
```

`synchronize(&mut self, &StyledDom) -> GpuEventChanges` walks every node and emits
one of three event variants per changed property:

```rust,ignore
pub enum GpuTransformKeyEvent {
    Added(NodeId, TransformKey, ComputedTransform3D),
    Changed(NodeId, TransformKey, ComputedTransform3D, ComputedTransform3D),
    Removed(NodeId, TransformKey),
}
```

The hot path is short-circuited by the compact property cache
(`gpu.rs:106` for transforms, `gpu.rs:171` for opacity): if the node has neither a
prior cached value nor a non-default property, the loop returns `None` immediately
without walking the cascade. Only nodes with a non-default `transform` or `opacity`
pay the slow read.

CPU feature detection (SSE / AVX) for matrix math runs once on first call
(`gpu.rs:113-126`); the flags are stored in `core::transform::USE_SSE` / `USE_AVX`.

## Dynamic property updates

A reference frame pushed for a CSS `transform` uses
`PropertyBinding::Binding(PropertyBindingKey::new(transform_key.id as u64),
initial)`:

```rust,ignore
let binding = PropertyBinding::Binding(
    webrender::api::PropertyBindingKey::new(transform_key.id as u64),
    wr_transform,
);
let new_spatial_id = builder.push_reference_frame(
    LayoutPoint::zero(), parent_spatial_id, TransformStyle::Flat,
    binding,
    ReferenceFrameKind::Transform { /* ... */ },
    SpatialTreeItemKey::new(transform_key.id as u64, 0),
);
```

`compositor2.rs:1320-1378`. Subsequent frames update the transform via
`Transaction::append_dynamic_properties` keyed on the same `transform_key.id` —
no new display list, no relayout.

`PushOpacity` and `ScrollBar` opacity wrap their children in
`push_simple_stacking_context_with_filters` whose `WrFilterOp::Opacity` carries
the same kind of `PropertyBinding` (`compositor2.rs:434-451`, `2052-2073`).

## Built-in shaders

`GlContextPtr::new` (`core/src/gl.rs:1027`) compiles three programs at construction
and stores their GL program IDs on `GlContextPtrInner`:

| Program | Source | Use |
|---|---|---|
| `svg_shader` | `SVG_VERTEX_SHADER` / `SVG_FRAGMENT_SHADER` (`gl.rs:923-956`) | Solid-color SVG path fill |
| `svg_multicolor_shader` | `SVG_MULTICOLOR_*_SHADER` (`gl.rs:958-1018`) | Per-vertex coloured SVG (gradient meshes) |
| `fxaa_shader` | `core/src/gl_fxaa.rs:FXAA_VERTEX_SHADER` / `FXAA_FRAGMENT_SHADER` | Post-process anti-aliasing |

`GlContextPtrInner::Drop` (`gl.rs:902-908`) deletes all three programs when the
context is destroyed. The FXAA pass is wired up in `layout/src/xml/svg.rs` —
see the [WebRender bridge](webrender-bridge.md) page for the full SVG render path.

## Texture lifecycle

WebRender owns its own GPU textures for primitives in the display list. Custom
GL textures (e.g. SVG render targets, user-drawn `Texture` callbacks) are kept
alive by a process-global cache in `core/src/gl.rs:733`:

```rust,ignore
static mut ACTIVE_GL_TEXTURES: Option<OrderedMap<DocumentId, GlTextureStorage>> = None;
```

The map is keyed `DocumentId → Epoch → ExternalImageId → Texture`. WebRender
holds an `ExternalImageId` referring to the texture; the `Drop` impl on `Texture`
runs only when both WebRender has released the image and the cache entry has
been evicted.

Cache management functions:

| Function | When called |
|---|---|
| `insert_into_active_gl_textures` (`gl.rs:739`) | Display list referenced a new external texture |
| `gl_textures_remove_epochs_from_pipeline` (`gl.rs:765`) | After WebRender publishes a frame; older epochs are safe to drop |
| `gl_textures_remove_active_pipeline` (`gl.rs:808`) | Document destroyed |
| `gl_textures_clear_opengl_cache` (`gl.rs:819`) | Before the GL context itself is torn down |
| `get_opengl_texture` (`gl.rs:835`) | WebRender external-image callback resolves an ID back to a `(GLuint, (w, h))` pair |

The cache is **not thread-safe**; the `Texture` itself is `!Send`, so all access
happens from the renderer thread that owns the GL context.

## Headless / CPU path

The same `DisplayList` feeds `cpurender` for headless rendering and reftests.
The `dll/src/desktop/shell2/headless` backend installs a CPU `WebRender` instance
and a software `GenericGlContext` so that `compositor2` runs unchanged. The PNG
output of `azul-render` fenced examples in this guide is produced by that path.

## Reading order for new contributors

1. `layout/src/solver3/display_list.rs` — the `DisplayListItem` enum.
2. `dll/src/desktop/compositor2.rs:165` (`translate_displaylist_to_wr`) — the main
   match.
3. [`webrender-bridge.md`](webrender-bridge.md) — coordinate, clip, and resource
   translation in detail.
4. [`gl-loading.md`](gl-loading.md) — how the `GenericGlContext` arrives in the
   first place.
5. `core/src/gpu.rs` — how animatable properties bypass relayout.
