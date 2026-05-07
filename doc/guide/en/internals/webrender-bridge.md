---
slug: webrender-bridge
title: WebRender Bridge
language: en
canonical_slug: webrender-bridge
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: How azul talks to WebRender
prerequisites: []
tracked_files:
  - core/src/gpu.rs
  - dll/src/desktop/compositor2.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

> **WIP** — `wr_translate2.rs` is mid-rework; the `compositor2` ↔ WebRender
> seam is stable, but the resource-update collection helpers may consolidate.

The bridge is the boundary between azul's native types and WebRender's API
types. Two files own it:

- `dll/src/desktop/compositor2.rs` translates `DisplayListItem` → WebRender
  display-list `push_*` calls (covered in [Rendering Pipeline](rendering-pipeline.md)).
- `dll/src/desktop/wr_translate2.rs` translates everything else: pipeline
  IDs, image keys, font keys, border radii, colors, clip-chain IDs, and the
  per-frame `Transaction` assembly.

`dll/src/desktop/wr_translate2.rs::generate_frame` is the single entry point
that the platform shells call once per frame.

## Per-frame transaction shape

```rust,ignore
pub fn generate_frame(
    txn: &mut WrTransaction,
    layout_window: &mut LayoutWindow,
    render_api: &mut WrRenderApi,
    display_list_was_rebuilt: bool,
    gl_context: &azul_core::gl::OptionGlContextPtr,
) {
    // ... bail if minimized
    if display_list_was_rebuilt {
        // 1. Resource updates (fonts + images)
        txn.update_resources(font_resources);
        txn.update_resources(image_resources);

        // 2. Display list per DOM (root + nested IFrames/VirtualViews)
        for (dom_id, layout_result) in &layout_window.layout_results {
            let (_, dl, nested) = compositor2::translate_displaylist_to_wr(...)?;
            txn.set_display_list(epoch, (pipeline_id, dl));
            for (nested_pid, nested_dl) in nested {
                txn.set_display_list(epoch, (nested_pid, nested_dl));
            }
        }
        layout_window.epoch.increment();
    } else {
        txn.skip_scene_builder();
    }

    // 3. Root pipeline + viewport
    txn.set_root_pipeline(root_pipeline_id);
    txn.set_document_view(view_rect, DevicePixelScale::new(hidpi));

    // 4. Image callbacks, virtual-view re-renders
    process_image_callback_updates(layout_window, gl_context, txn);
    process_virtual_view_updates(layout_window, txn);

    // 5. Scroll positions
    scroll_all_nodes(layout_window, txn);

    // 6. GPU dynamic properties (transforms, opacities)
    synchronize_gpu_values(layout_window, txn);

    // 7. Submit
    txn.generate_frame(0, WrRenderReasons::empty());
}
```

The order matters. Resources go in *before* `set_display_list` because the
display list references `FontInstanceKey`s and `ImageKey`s that must already
exist on the backend thread by the time the scene is built. The root pipeline
goes in *after* the display list for the same reason. WebRender's upstream
expects the dependency graph to be populated before pipeline activation.

`display_list_was_rebuilt` is the flag from the layout layer. `false` means
"only properties changed" and the function calls `txn.skip_scene_builder()`.
WebRender reuses the previous scene and only applies the dynamic properties
from `synchronize_gpu_values` and the scroll offsets from `scroll_all_nodes`.
This is the path that makes scrollbar fade and CSS animations cheap.

## Pipelines, DOMs, and IFrames

One `WrPipelineId` per `DomId`. `wr_translate_pipeline_id` packs the ID:

```rust,ignore
PipelineId(dom_id.inner as u32, layout_window.document_id.id)
```

The root DOM gets `PipelineId(0, document_id)`. IFrames and `VirtualView`s get
the child DOM's ID. `translate_displaylist_to_wr` returns a flat
`Vec<(PipelineId, WrBuiltDisplayList)>` of nested pipelines. The caller adds
each one to the transaction with `set_display_list`. Pipeline IDs are pure
identifiers. WebRender stitches the trees together via `push_iframe` items
inside the parent display list.

- `dll/src/desktop/compositor2.rs::push_iframe`

## Resource translation

`collect_font_resource_updates` and `collect_image_resource_updates` read from
`layout_window.renderer_resources` and emit `azul_core::resources::ResourceUpdate`
values. Each is then routed through `translate_resource_update` to a
`webrender::ResourceUpdate`. Azul's
`ResourceUpdate::AddFont(AddFont { font, key })` becomes
`ResourceUpdate::AddFont(WrFontKey, WrFontTemplate, ...)`. Azul's
`ResourceUpdate::AddFontInstance(AddFontInstance { font_key, glyph_size, key })`
becomes `ResourceUpdate::AddFontInstance(WrFontInstanceKey, WrFontKey, ...)`.
Azul's `ResourceUpdate::AddImage(AddImage { key, descriptor, data })` becomes
`ResourceUpdate::AddImage(WrImageKey, WrImageDescriptor, WrImageData)`.
Removals translate symmetrically through the `Delete*` variants.

While translating, `generate_frame` also mirrors the registrations into the
azul-side maps:

- `font_hash_map: HashMap<u64, FontKey>`. `compositor2`'s `push_text` looks
  up `FontKey` by `font_hash`.
- `currently_registered_fonts: HashMap<FontKey, (FontRef, BTreeMap<(Au, DpiScaleFactor), FontInstanceKey>)>`.
  The same path resolves the `FontInstanceKey` by `(size, dpi)`.
- `image_key_map: HashMap<ImageKey, ImageRefHash>`. Reverse lookup for
  hit-testing.

If a `Text` item references a `font_hash` that hasn't made it into the
transaction's resource updates, `push_text` logs a warning and silently drops
the glyphs. The same goes for missing image keys. Both are usually a sign that
the layout pass and the resource collection are out of sync. A glyph was
shaped against a font that wasn't registered, or an image was loaded but not
inserted into `renderer_resources`.

## GPU dynamic properties

`synchronize_gpu_values` is the bridge between `core/src/gpu.rs` and
WebRender's `Transaction::append_dynamic_properties`. It walks
`layout_window.gpu_value_cache` (a `GpuValueCache`) and emits one
`PropertyValue<T>` per active key:

- `dll/src/desktop/wr_translate2.rs::synchronize_gpu_values`
- `core/src/gpu.rs::GpuValueCache`

```rust,ignore
let scrollbar_v_props: Vec<PropertyValue<f32>> = ...;       // scrollbar opacity
let scrollbar_h_props: Vec<PropertyValue<f32>> = ...;
let v_transform_props: Vec<PropertyValue<LayoutTransform>> = ...; // scrollbar thumb transform
let h_transform_props: Vec<PropertyValue<LayoutTransform>> = ...;
let css_transform_props: Vec<PropertyValue<LayoutTransform>> = ...;  // CSS transforms
let css_opacity_props: Vec<PropertyValue<f32>> = ...;                 // CSS opacities

txn.append_dynamic_properties(DynamicProperties {
    transforms: ...,
    floats: ...,
    colors: ...,
});
```

The `PropertyBindingKey` ID is the `OpacityKey`/`TransformKey` value — the
same number that was embedded in the display list when the
`PushReferenceFrame` or stacking-context-with-opacity-filter was emitted.
WebRender resolves the binding at frame-build time by replacing the bound
property with the supplied `PropertyValue`. This is what lets a scrollbar
fade-out animation update without rebuilding the display list — the layout
pass produces a new opacity number, `synchronize_gpu_values` packs it into the
transaction, WebRender re-rasterises the affected primitives.

`txn.skip_scene_builder()` is compatible with `append_dynamic_properties`.
The dynamic properties path is *separate* from scene building, so the cheap
"property update only" path stays cheap.

The `DynamicProperties` translation has one DPI-related quirk. Translation
components of bound transforms (`m[3][0]`, `m[3][1]`, `m[3][2]`) must be
scaled by the HiDPI factor, because the display list's coordinates are
already in physical pixels. `dll/src/desktop/compositor2.rs` does this on
display-list emission, and `synchronize_gpu_values` does the equivalent on
every dynamic update.

## Coordinate conventions

Three coordinate spaces meet here. The bridge converts between them per
component, not in one place:

- **Window logical (CSS).** `DisplayList` items use logical pixels (CSS px)
  in absolute window coordinates.
- **Window physical.** The WebRender display list uses physical pixels
  (logical times DPI) in absolute window coordinates.
- **Stacking-context-relative physical.** Inside a stacking context that
  pushed an origin, items use physical pixels minus the stacking context
  origin.

`compositor2`'s `resolve_rect` and `resolve_point` fuse the DPI multiply and
the offset subtract. The *scroll* frame does **not** add to the offset stack,
because WebRender scroll frames share their parent's coordinate space. Only
stacking contexts and reference frames push offsets. This is documented in
detail at
[Rendering Pipeline § Coordinate offset](rendering-pipeline.md#coordinate-offset-stacking-context-scroll-frame).

- `dll/src/desktop/compositor2.rs::resolve_rect`
- `dll/src/desktop/compositor2.rs::resolve_point`

Scroll positions: `scroll_all_nodes` reads the `ScrollManager`'s state
(logical CSS pixels), multiplies by `hidpi_factor`, and submits via
`txn.set_scroll_offsets`. The scroll offset and the display list both end up
in physical pixels, so the addition in WebRender's spatial tree resolves
correctly.

- `dll/src/desktop/wr_translate2.rs::scroll_all_nodes`

## Hit-test bridge

The forward path is straightforward: `compositor2` emits a `HitTestArea` item
that calls `builder.push_hit_test(rect, clip_chain, spatial_id, flags, tag)`
with an `ItemTag = (u64, u16)`. The `u16` namespace marker distinguishes:

- `0x0100`. DOM node hit.
- `0x0200`. Scrollbar component hit, decoded by
  `wr_translate_scrollbar_hit_id`.
- `0x0500`. `TAG_TYPE_SCROLL_CONTAINER`, the scroll container itself, used
  for wheel/trackpad scroll target lookup.
  - `dll/src/desktop/compositor2.rs::TAG_TYPE_SCROLL_CONTAINER`

The reverse path (cursor to tag) is owned by the platform shell. It queries
WebRender's hit tester each frame and routes results through the event
dispatch system.

## Scene-builder skip

Two flags govern WebRender's scene-build cycle:

- **Display list rebuilt** (`display_list_was_rebuilt = true`). Full scene
  build. Resources, display lists, and root pipeline are all submitted.
- **Property-only update** (`display_list_was_rebuilt = false`).
  `txn.skip_scene_builder()`. WebRender reuses the existing scene; only
  `set_scroll_offsets`, `append_dynamic_properties`, and `set_document_view`
  apply.

Layout decides which path to take based on the `Update` value returned from
callbacks. `Update::RefreshDom` means full rebuild. `Update::DoNothing` with
only GPU or scroll changes means property-only.

## Software path (cpurender)

The bridge does *not* know about `cpurender`. The split happens one layer
above, in `dll/src/desktop/window.rs`, where the renderer is constructed.
With the GPU path, `WrRenderApi` is a real WebRender API. With the software
path, it's a `cpurender` API that exposes the same `Transaction` interface but
rasterises into a `Vec<u8>` framebuffer instead of dispatching GL. Everything
in `wr_translate2.rs` runs identically on both paths. The only difference is
who is on the receiving end of `txn.generate_frame`.

That's the point of the abstraction. If you add a new bridge function, make
sure it works against the `webrender::api::Transaction` interface and not
against any GL-specific assumption. Otherwise the headless reftest harness
breaks.

## Common bridge bugs

- **Resource not registered before display list.** `txn.update_resources(...)`
  must run before `txn.set_display_list(...)` *in the same transaction*. If
  the call sites are reordered, the scene builder fails to resolve the key
  and the entire DOM renders blank.
- **Epoch not incremented after a rebuild.** WebRender uses `Epoch` to
  decide whether to drop old textures. If two consecutive frames share an
  epoch, GL textures from the old frame leak. `layout_window.epoch.increment()`
  must run on every rebuild path. Check the rebuild branch in
  `dll/src/desktop/wr_translate2.rs::generate_frame`.
- **Forgetting `set_root_pipeline`.** Required even if the display list was
  reused — WebRender needs to know which pipeline is the root for hit-testing
  and viewport calculations.
- **Dynamic property without a corresponding binding in the display list.**
  WebRender silently ignores updates whose key doesn't match a
  `PropertyBinding::Binding(key, ...)` in the live display list. Symptom:
  scrollbar opacity changes but nothing visible. Diagnosis: check that
  `compositor2` actually pushed a stacking context with the binding (search
  for the `OpacityKey::id` in the compositor logs).

## Coming Up Next

- [Rendering Pipeline](rendering-pipeline.md) — From `StyledDom` to pixels
- [Image Pipeline](image-pipeline.md) — Decoding, caching, and uploading raster images
- [GL Function Loading](gl-loading.md) — Loading GL function pointers across platforms
