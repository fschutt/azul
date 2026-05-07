---
slug: rendering-pipeline
title: Rendering Pipeline
language: en
canonical_slug: rendering-pipeline
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: From `StyledDom` to pixels
prerequisites: []
tracked_files:
  - core/src/gl.rs
  - core/src/gl_fxaa.rs
  - core/src/glconst.rs
  - core/src/gpu.rs
  - dll/src/desktop/compositor2.rs
  - dll/src/desktop/shell2/common/gl_loader.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

> **WIP** — APIs in `compositor2` and `gpu` are still moving. The shape
> documented here matches HEAD; signatures may shift before 1.0.

A frame moves through three boundaries. The layout engine emits an
`azul_layout::solver3::display_list::DisplayList` (a flat `Vec<DisplayListItem>`
in absolute window coordinates). `dll/src/desktop/compositor2.rs::translate_displaylist_to_wr`
walks that list and pushes equivalent items into a WebRender
`DisplayListBuilder`. The [WebRender bridge](webrender-bridge.md) wraps the
resulting `BuiltDisplayList` in a `Transaction` and ships it to the WebRender
backend thread. Hardware (GPU WebRender) and software (CPU `cpurender`) paths
share this exact display list. The split is in the sink, not the source.

## Stage map

A frame moves through these stages in order:

- **Layout → display list.** Produces a `DisplayList` (`Vec<DisplayListItem>`).
  - `layout/src/window.rs::layout_and_generate_display_list`
- **GPU key sync.** Walks the `StyledDom` and emits a `GpuEventChanges` describing transform/opacity deltas.
  - `core/src/gpu.rs::GpuValueCache::synchronize`
- **Display list translation.** Walks the display list and pushes WebRender items. Produces a `WrBuiltDisplayList`, nested pipelines, and resource updates.
  - `dll/src/desktop/compositor2.rs::translate_displaylist_to_wr`
- **Resource translation.** Produces a `Vec<WrResourceUpdate>`.
  - `dll/src/desktop/wr_translate2.rs`
- **Frame submission.** Wraps the result in a `Transaction` and ships it to the WebRender backend thread.
  - `dll/src/desktop/wr_translate2.rs::generate_frame`
- **GL function loading.** Resolves GL symbols per platform into a `GenericGlContext`.
  - `dll/src/desktop/shell2/common/gl_loader.rs::load_gl_context`
- **Library shaders.** Compiles the SVG, multicolor-SVG, and FXAA programs once at context creation.
  - `core/src/gl.rs::GlContextPtr::new`

The `LayoutWindow` owns the layout side; `dll/src/desktop/window.rs` owns the
WebRender side and the GL context. Read [WebRender bridge](webrender-bridge.md)
for the transaction shape and [GL function loading](gl-loading.md) for the
per-platform symbol resolution.

## translate_displaylist_to_wr

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
    (Vec<WrResourceUpdate>, WrBuiltDisplayList, Vec<(PipelineId, WrBuiltDisplayList)>),
    String,
>
```

The function (`compositor2.rs::translate_displaylist_to_wr`) is a single
sequential walk. There is no intermediate IR; each `DisplayListItem` translates
directly into one or more `builder.push_*` calls. Three stacks track the
WebRender-side context:

- `clip_stack: Vec<WrClipChainId>` is the current clip chain (rounded-rect, image
  mask, scroll-frame viewport).
- `spatial_stack: Vec<SpatialId>` is the current spatial node (root scroll node,
  scroll frame, transform reference frame).
- `offset_stack: Vec<(f32, f32)>` is the coordinate origin offset. The display list
  uses absolute window coordinates; WebRender wants frame-relative coordinates
  for stacking contexts. Push when entering a `PushStackingContext`, subtract
  on every coordinate translation, pop on `PopStackingContext`.

The call sites are in `dll/src/desktop/wr_translate2.rs`. Each builds a
transaction containing one or more display lists.

### resolve_rect — the coordinate gate

Every coordinate that crosses into WebRender goes through
`compositor2.rs::resolve_rect`:

```rust,ignore
fn resolve_rect(
    bounds: &azul_layout::solver3::display_list::WindowLogicalRect,
    dpi: f32,
    offset: (f32, f32),
) -> LayoutRect {
    let raw = scale_bounds_to_layout_rect(bounds.inner(), dpi);
    LayoutRect::from_origin_and_size(
        LayoutPoint::new(raw.min.x - offset.0, raw.min.y - offset.1),
        LayoutSize::new(raw.width(), raw.height()),
    )
}
```

The two adjustments (DPI scale and stacking-context offset subtraction) are
fused so callers cannot forget one. `resolve_point` is the equivalent for
single points (used for `push_simple_stacking_context`). See
`scripts/SCROLL_COORDINATE_ARCHITECTURE.md` for the rationale.

### Item dispatch

The matcher in `compositor2.rs` routes each `DisplayListItem`. Common patterns:

- **`Rect`, `SelectionRect`, `CursorRect`.** `resolve_rect` then
  `CommonItemProperties` then `builder.push_rect`. Optional rounded-corner
  `define_border_radius_clip`.
  - `dll/src/desktop/compositor2.rs::define_border_radius_clip`
- **`Border`.** Resolve rect, call `get_webrender_border` for per-side widths,
  call `builder.push_border`.
  - `dll/src/desktop/wr_translate2.rs::get_webrender_border`
- **`ScrollBarStyled`.** Push an optional opacity stacking context bound to
  `opacity_key`, define a rounded clip if the container has border-radius,
  render the track, optional buttons, then the thumb. The thumb may be wrapped
  in its own reference frame for GPU-driven `transform`.
- **`PushClip` / `PopClip`.** `define_clip_rect` (or
  `define_clip_rounded_rect`), then `define_clip_chain(parent)`, then push to
  `clip_stack`.
- **`PushScrollFrame` / `PopScrollFrame`.** See
  [scroll-frame clipping](#scroll-frame-clipping).
- **`Text`.** Look up `FontInstanceKey`, scale glyph positions by DPI, subtract
  scroll offset, call `builder.push_text`.
- **`Image`.** Resolve `ImageRefHash` to `WrImageKey`, apply an optional
  rounded clip, call `builder.push_image`.
- **`LinearGradient` / `RadialGradient` / `ConicGradient`.** Compute
  start/end points from the CSS direction, convert stops, call `push_stops`
  immediately followed by `push_gradient` / `push_radial_gradient` /
  `push_conic_gradient`. No clip items can sit between the two; WebRender
  requires them adjacent.
- **`BoxShadow`.** Convert blur, offset, and spread, then call
  `push_box_shadow` with the appropriate `BoxShadowClipMode`.
- **`PushStackingContext` / `PopStackingContext`.** `push_simple_stacking_context`
  at the resolved origin. The offset stack tracks the CSS origin for nested
  children.
- **`PushReferenceFrame` / `PopReferenceFrame`.** GPU transform animation.
  Translation components are scaled by DPI; the spatial node binds to
  `transform_key`. Pushes spatial only, no offset.
- **`VirtualView`.** Recursively call `translate_displaylist_to_wr` for the
  child DOM, accumulate its built list under `nested_pipelines`, then
  `builder.push_iframe` to splice it in.
- **`PushImageMaskClip` / `PopImageMaskClip`.** `define_clip_image_mask` for
  SVG mask-style clipping.
- **`HitTestArea`.** `builder.push_hit_test` with the supplied `ItemTag`.
  Under `debug_assertions` the area also renders as a 30%-opaque red rect.

`TextLayout` items are no-ops here. An earlier pass consumed them and emitted
the resolved `Text` items.

### Scroll-frame clipping

A `ScrollFrame` in WebRender is *only* a transformation node. It does not clip.
The viewport clip must be defined separately, in **parent space**, so it stays
stationary while content scrolls. The `PushScrollFrame` arm in
`dll/src/desktop/compositor2.rs` performs four steps:

```rust,ignore
// 1. Define the spatial node (transformation only)
let scroll_spatial_id = builder.define_scroll_frame(
    parent_space,
    external_scroll_id,
    content_rect,        // total scrollable size, origin = frame_rect.origin
    adjusted_frame_rect, // visible viewport, in parent space
    LayoutVector2D::zero(),
    0,
    HasScrollLinkedEffect::No,
    SpatialTreeItemKey::new(*scroll_id, 0),
);
spatial_stack.push(scroll_spatial_id);

// 2. Define the viewport clip in PARENT space
let scroll_clip_id = builder.define_clip_rect(parent_space, adjusted_frame_rect);

// 3. Chain it onto the current clip
let scroll_clip_chain = builder.define_clip_chain(parent_clip, [scroll_clip_id]);
clip_stack.push(scroll_clip_chain);

// 4. Push a scroll-container hit-test in parent space (TAG_TYPE_SCROLL_CONTAINER)
builder.push_hit_test(adjusted_frame_rect, scroll_clip_chain, parent_space, ..., scroll_container_tag);
```

Three things will silently break clipping if you get them wrong:

- **Clip in scroll space.** `define_clip_rect(scroll_spatial_id, ...)` makes the
  clip scroll *with* the content — content disappears off-screen. The clip must
  be in `parent_space`.
- **Content origin ≠ frame origin.** If `content_rect.origin` is `LayoutPoint::zero()`
  while `frame_rect.origin` is non-zero, the content is shifted by exactly that
  delta and items render outside the viewport.
- **Forgetting to push the clip chain.** `clip_stack.push(scroll_clip_chain)`
  must run before any inner item is processed; otherwise items inherit the parent
  clip and bleed past the viewport.

`scripts/WEBRENDER_CLIPPING_ANALYSIS.md` walks through the diagnosis of this
class of bug. The compositor2 implementation matches the doc's "correct"
recipe; if you see clip-leak symptoms, the divergence is more likely to be in
the *consumer* of the scroll frame (a `Text` or `Rect` arm that forgets to
apply `current_offset!()`) than in the `PushScrollFrame` arm itself.

`PopScrollFrame` pops both the spatial and clip stacks and returns
`Err("Scroll frame stack underflow")` if either underflows. The caller
(`generate_frame`) treats that as a fatal frame error.

- `dll/src/desktop/compositor2.rs::PopScrollFrame`

### Coordinate offset, stacking context, scroll frame

Three coordinate concepts collide in this file. Read carefully:

- The display list emits **absolute window** coordinates for everything.
  A `Text` item's clip rect at `(11, 53)` means *the window position*, not the
  position inside its parent.
- A **scroll frame** in WebRender shares its parent's coordinate space. It does
  *not* create a new origin. `offset_stack` is **not** pushed on
  `PushScrollFrame` in `dll/src/desktop/compositor2.rs`. The scroll-frame
  transform handles scroll movement; absolute coordinates stay absolute.
- A **stacking context** (`PushStackingContext`) *does* shift the origin —
  WebRender offsets every child by the stacking context's origin. The
  compositor pushes the scaled origin onto `offset_stack` so that children
  subtract it back out (yielding net zero offset, with the visible effect being
  paint order and z-index). The same applies to `PushReferenceFrame`, but for
  stacking-context-style filters (opacity, transform).

If a scroll-container clip suddenly stops working, the first thing to check is
that no intervening `PushStackingContext` was added that pushed onto the offset
stack but failed to pop. Run with `LogCategory::DisplayList` enabled — the arm
prints `clip_stack.len` and `spatial_stack.len` on every push/pop.

## GPU value cache

`core/src/gpu.rs` decouples the layout pass from per-frame GPU updates.
`GpuValueCache::synchronize` walks the `StyledDom` once per frame, comparing
each node's current CSS `transform` and `opacity` against the previously-stored
values, and emits a `GpuEventChanges` describing the deltas.

- `core/src/gpu.rs::GpuValueCache::synchronize`

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

Two compact-cache fast paths short-circuit the cascade walk:

- `cc.has_transform(node_id.index())` is a single bit — if unset and the cache
  has no prior key, skip the node entirely.
- `cc.get_opacity_raw(node_id.index())` returns a `u8`. The sentinel
  `OPACITY_SENTINEL` means "unset", which decodes to `1.0` and lets the loop
  bail out without invoking `css_property_cache.get_opacity`.

The result feeds two consumers:

- `Added` events generate fresh `TransformKey` / `OpacityKey` values via
  `unique()`, which wrap WebRender `PropertyBindingKey`s.
- `Changed` events go to `Transaction::append_dynamic_properties`, which
  WebRender applies *without* rebuilding the display list. This is the path that
  makes scrollbar fade-out and CSS `transform` animation cheap — a scroll
  doesn't trigger a re-layout, just a property update.

`GpuScrollbarOpacityEvent` is the parallel for scrollbar fade timers; it is
maintained by `layout/src/managers/scroll_state.rs` rather than `synchronize`.

The cache is owned per-window and survives across frames; only the `_changes`
struct is consumed each frame.

## Library shaders

`GlContextPtr::new` compiles three shader programs at context creation and
stores them in `GlContextPtrInner`:

- **`svg_shader`.** Solid-color SVG path fills.
  - `core/src/gl.rs::SVG_VERTEX_SHADER`
  - `core/src/gl.rs::SVG_FRAGMENT_SHADER`
- **`svg_multicolor_shader`.** Per-vertex coloured SVG.
  - `core/src/gl.rs`
- **`fxaa_shader`.** Post-process anti-aliasing.
  - `core/src/gl_fxaa.rs::FXAA_VERTEX_SHADER`
  - `core/src/gl_fxaa.rs::FXAA_FRAGMENT_SHADER`

Compilation is checked through `check_shader_compile` and `check_program_link`.
Both log to stderr under `feature = "std"`; under `no_std` they swallow the
failure. There is no recovery. A failed shader compile leaves the program ID
present but unusable, and subsequent draws produce no output. If you hit a
black SVG, run with `RUST_LOG` and look for "shader compile error".

`GlContextPtrInner::Drop` calls `delete_program` on all three when the context
is destroyed.

- `core/src/gl.rs::GlContextPtr::new`
- `core/src/gl.rs::check_shader_compile`
- `core/src/gl.rs::check_program_link`

### FXAA pass

`FxaaConfig` carries the runtime tunables:

- `core/src/gl_fxaa.rs::FxaaConfig`

```rust,ignore
pub struct FxaaConfig {
    pub enabled: bool,
    pub edge_threshold: f32,      // 0.063 – 0.333, default 0.125
    pub edge_threshold_min: f32,  // 0.0312 – 0.0833, default 0.0312
}
```

Presets (`enabled`, `high_quality`, `balanced`, `performance`) are convenience
constructors. The fragment shader is the standard NVIDIA FXAA 3.11 algorithm:
sample center + N/S/E/W luminance, compute `lumRange`, early-exit below the
threshold, otherwise sample along the detected edge direction and blend.

The actual FXAA render pass lives in `layout/src/xml/svg.rs` (`apply_fxaa` /
`apply_fxaa_with_config`), not in `core/src/gl_fxaa.rs`. The `core` crate owns
the data + GLSL strings; the SVG render pipeline owns the FBO setup, fullscreen
quad, and state save/restore.

## Texture cache

`core/src/gl.rs` declares the active texture map:

```rust,ignore
static mut ACTIVE_GL_TEXTURES:
    Option<OrderedMap<DocumentId, GlTextureStorage>> = None;

pub type GlTextureStorage =
    OrderedMap<Epoch, OrderedMap<ExternalImageId, Texture>>;
```

The keying (`DocumentId → Epoch → ExternalImageId → Texture`) exists because
WebRender may still be rendering against an old frame's textures while the
application is generating the next frame. Textures are kept alive until the
backend thread acknowledges that the epoch is no longer in use, then
`gl_textures_remove_epochs_from_pipeline` drops everything strictly older than
the supplied epoch.

- `core/src/gl.rs::gl_textures_remove_epochs_from_pipeline`

The map is **not** thread-safe. The doc comment in `core/src/gl.rs` argues
that `Texture` itself is not `Send`/`Sync`, so accidental concurrent access is
unlikely, but the warning still applies: do not call any of
`insert_into_active_gl_textures`, `gl_textures_remove_epochs_from_pipeline`,
`gl_textures_remove_active_pipeline`, `gl_textures_clear_opengl_cache`, or
`get_opengl_texture` from anything other than the main thread. Rust 2024 will
also forbid `static mut` references. Replacement with a `Mutex<...>` or
`thread_local!` is on the cleanup list.

`Texture` carries a refcount, the `GLuint` ID, and a clone of the
`GlContextPtr`. Its `Drop` decrements the refcount and, on last drop, calls
`delete_textures` on the held context. `Texture::create`,
`Texture::allocate_rgba8`, and `Texture::clear` cover the construction surface.
`GlShader::draw` is the single render entry point that reads from the cache.

- `core/src/gl.rs::Texture`
- `core/src/gl.rs::GlShader::draw`

## Hardware vs software path

`RendererType` (in `core/src/window.rs`) selects between:

- **Hardware (GPU)** — WebRender renders into a real OpenGL context. The
  context is created by the platform shell ([GL function loading](gl-loading.md)),
  the WebRender renderer is built on top of `GenericGlContext`, and frames go
  through the standard transaction pipeline.
- **Software (CPU)** — `cpurender` rasterises into a `Vec<u8>` framebuffer.
  The display list is the same; the sink is different. Used for headless
  rendering, the reftest harness, and machines without GL.

The split happens at `dll/src/desktop/window.rs` when the renderer is
constructed; everything upstream of that (layout, display list, GPU value
cache, [WebRender bridge](webrender-bridge.md)) is identical.

## Debugging clip and spatial bugs

The most common WebRender visual bug is "items render in the wrong place".
The compositor logs every push/pop to `LogCategory::DisplayList`. Useful
patterns:

- `[CLIP DEBUG] Rect: adjusted=..., clip_chain=..., spatial=...` — every Rect
  prints its resolved bounds and stack heads. Cross-reference against the
  intended clip chain.
- `[compositor2] PushScrollFrame START` ... `DONE` brackets the scroll frame.
  The `clip_stack.len` and `spatial_stack.len` deltas should both be `+1`. If
  one of them stays the same, an arm above failed to push.
- `[compositor2] PopClip: SKIPPED (clip_stack.len=1, would underflow)` —
  unbalanced `PushClip` / `PopClip`. Means the display list generator is
  buggy, not the compositor.

WebRender's own debug flags are also useful: enable `DebugFlags::PRIMITIVE_DBG
| DebugFlags::CLIP_DBG | DebugFlags::SPATIAL_DBG` in the renderer config to
get on-screen overlays.

If clipping is wrong, the diagnosis is almost always: clip defined in the
wrong space, or clip chain not chained to its parent. See
`scripts/WEBRENDER_CLIPPING_ANALYSIS.md` for the failure modes.

## Coming Up Next

- [WebRender Bridge](webrender-bridge.md) — How azul talks to WebRender
- [Image Pipeline](image-pipeline.md) — Decoding, caching, and uploading raster images
- [GL Function Loading](gl-loading.md) — Loading GL function pointers across platforms
