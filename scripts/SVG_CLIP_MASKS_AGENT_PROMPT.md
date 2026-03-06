# SVG Clip Masks — Implementation Task

## Goal

Implement **texture-based clip masks** for the Azul GUI framework so that:

1. A DOM node can be clipped by an **SVG-shaped mask** (black-and-white R8 texture)
2. This works both with **GPU (WebRender)** and **CPU (tiny_skia)** rendering
3. The `ImageRef::callback()` mechanism works for CPU rendering (not just GPU)
4. A new **chart widget example** (`examples/c/chart.c`) demonstrates the feature

The key use case: render UI elements (gradients, backgrounds) via WebRender, then clip them through an SVG-shaped texture mask to create charts (pie charts, bar charts, line charts with filled areas, etc.).

## Architecture Overview

```
User creates SVG path → Tessellate → Render to R8 texture (GPU or CPU)
                                            ↓
                                     ImageRef (mask)
                                            ↓
                              Dom::with_clip_mask(ImageMask { image, rect, repeat })
                                            ↓
                            Display List: PushImageMaskClip / PopImageMaskClip
                                            ↓
                   ┌────────────────────────┴────────────────────────┐
                   ↓                                                 ↓
         compositor2.rs                                     cpurender.rs
    builder.define_clip_image_mask()              tiny_skia ClipMask integration
    → ClipId → define_clip_chain()                → render mask + apply as alpha
```

## What Already Exists (DO NOT rewrite)

### 1. ImageRef callback API — FULLY WORKING
- `ImageRef::callback(callback, data)` creates a lazy-rendered image
- `RenderImageCallbackInfo::get_gl_context()` returns `Option<GlContextPtr>`
- `RenderImageCallbackInfo::get_bounds()` returns physical pixel bounds
- Callback returns `ImageRef::gl_texture(texture)` (GPU) or `ImageRef::null_image(...)` / `ImageRef::new_rawimage(...)` (CPU)
- See working example: `examples/c/opengl.c`

### 2. SVG tessellation and rendering — FULLY WORKING
- `SvgNode` types: `MultiPolygon`, `Path`, `Circle`, `Rect`, `MultiShape`, etc.
- `tessellate_node_fill()` / `tessellate_node_stroke()` → `TessellatedSvgNode` (CPU vertices)
- `TessellatedGPUSvgNode::new()` → uploads to GPU
- `TessellatedGPUSvgNode::draw()` → renders to `Texture` with transforms and color
- GPU clip mask: `allocate_clipmask_texture()` creates R8 texture, `render_tessellated_node_gpu()` renders into it
- CPU clip mask: `render_node_clipmask_cpu()` renders SVG node to R8 `RawImage` via tiny_skia

### 3. ImageMask type — EXISTS but NOT wired through
- `ImageMask { image: ImageRef, rect: LogicalRect, repeat: bool }` — defined in `core/src/resources.rs:1402`
- `NodeData::set_clip_mask(ImageMask)` — sets mask on a DOM node (`core/src/dom.rs:1947`)
- `Dom::with_clip_mask(ImageMask)` — builder pattern (`core/src/dom.rs:4546`)
- `CallbackChange::ChangeNodeImageMask` — dynamic update via callback (`layout/src/callbacks.rs:231`)
- **BUT**: The display list generator and compositor NEVER read this field

### 4. WebRender clip mask API — EXISTS in WebRender
- `builder.define_clip_image_mask(spatial_id, image_mask, points, fill_rule)` → `ClipId`
- `webrender::api::ImageMask { image: ImageKey, rect: LayoutRect }`
- Clip mask shaders exist in `webrender/swgl/src/`
- **BUT**: `compositor2.rs` never calls `define_clip_image_mask()`

## What Needs to Be Implemented

### Task 1: Display List — Add ImageMask clip items

**File:** `layout/src/solver3/display_list.rs`

Add new `DisplayListItem` variants:
```rust
PushImageMaskClip {
    bounds: WindowLogicalRect,
    mask_image: ImageRef,
    mask_rect: WindowLogicalRect,
},
PopImageMaskClip,
```

In `generate_display_list()` (the main traversal), after pushing regular clips for a node, check if `node_data.get_clip_mask()` returns `Some(ImageMask)` and emit `PushImageMaskClip` before children and `PopImageMaskClip` after.

Relevant code to modify:
- `DisplayListItem` enum (~line 567)
- `DisplayListBuilder` impl — add `push_image_mask_clip()` / `pop_image_mask_clip()`
- The main node traversal — look for where `push_node_clips()` is called (lines ~2085, 2151, 2214) and add image mask handling nearby
- `to_debug_json()` — add debug serialization for the new items
- The bounds extraction functions at the bottom of the file

### Task 2: Compositor — Wire ImageMask to WebRender

**File:** `dll/src/desktop/compositor2.rs`

In the main display list iteration loop (starting ~line 140), add handlers for the new `PushImageMaskClip` / `PopImageMaskClip` items:

```rust
DisplayListItem::PushImageMaskClip { bounds, mask_image, mask_rect } => {
    // 1. Resolve mask_image → ImageKey (same as Image handling at line 1256)
    // 2. Create webrender::api::ImageMask { image: wr_image_key, rect: wr_rect }
    // 3. Call builder.define_clip_image_mask(current_spatial, wr_mask, &[], FillRule::Nonzero)
    // 4. Create clip chain: builder.define_clip_chain(parent_clip, vec![clip_id])
    // 5. Push to clip_stack
}

DisplayListItem::PopImageMaskClip => {
    // Pop from clip_stack (same as PopClip)
}
```

The mask image must be registered as a WebRender image (not an external GL texture) — it should be an R8 format texture. Check how regular images are registered in `wr_translate2.rs:1044` (`build_add_image_resource_updates()`).

### Task 3: CPU Renderer — Implement image blitting and clip masks

**File:** `layout/src/cpurender.rs`

1. **Fix `render_image()`** (line 925): Currently a grey placeholder. Implement actual image blitting:
   - Extract pixel data from `ImageRef::get_data()` → match `DecodedImage::Raw((descriptor, data))`
   - Convert to `tiny_skia::Pixmap` or use `pixmap.draw_pixmap()` with proper scaling
   - Handle `DecodedImage::NullImage` as empty/transparent

2. **Add `PushImageMaskClip` / `PopImageMaskClip` handling** in the main render loop:
   - When encountering `PushImageMaskClip`: extract mask image data, create `tiny_skia::Mask`, push to a mask stack
   - Apply mask to all subsequent drawing operations until `PopImageMaskClip`
   - When encountering `PopImageMaskClip`: pop from mask stack

3. **Resolve image callbacks in CPU mode**: Currently `process_image_callback_updates()` in `wr_translate2.rs` only runs in the WebRender path. For CPU screenshots (via debug server `take_screenshot`), callbacks need to be invoked with `gl_context = None`, and the callback should return a CPU-rendered `ImageRef::new_rawimage(...)`.

### Task 4: Image callback CPU fallback

**File:** `dll/src/desktop/wr_translate2.rs` (or wherever CPU screenshot is triggered)

Ensure that when `get_gl_context()` returns `None` inside a `RenderImageCallback`, the callback can still produce a valid image via `ImageRef::new_rawimage()`. The CPU renderer should call `process_image_callback_updates()` or an equivalent before rendering.

Check `debug_server.rs` line ~6063 for `take_screenshot` — it calls `cpurender::render()` but may not resolve callbacks first.

### Task 5: Chart widget example

**File:** `examples/c/chart.c` (NEW)

Create a C example that demonstrates:

1. **A simple bar chart** using clip masks:
   - Create a div with a `background: linear-gradient(...)` (the fill color)
   - Generate SVG rectangles for each bar → tessellate → render to R8 clip mask
   - Attach the clip mask to the gradient div via `AzDom_withClipMask()`

2. **An animated pie chart** (stretch goal):
   - Use `ImageRef::callback()` to render the clip mask dynamically
   - In the callback, generate SVG arcs based on data percentages
   - Return `ImageRef::gl_texture()` (GPU) or `ImageRef::new_rawimage()` (CPU fallback)

3. **CPU/GPU toggle**: Show that the same chart renders correctly both with and without GL context

The example should work with:
```bash
cp target/codegen/azul.h examples/c/azul.h
cd examples/c
cc -o chart chart.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release
DYLD_LIBRARY_PATH=../../target/release ./chart
```

## Key Source Files Reference

| File | Lines | Description |
|------|-------|-------------|
| `core/src/resources.rs` | 2873 | `ImageRef`, `DecodedImage`, `ImageMask`, `RawImage` types |
| `core/src/dom.rs` | 4612 | `NodeData::set_clip_mask()`, `Dom::with_clip_mask()` |
| `core/src/svg.rs` | 1436 | `SvgNode`, `TessellatedSvgNode`, `TessellatedGPUSvgNode::draw()` |
| `core/src/callbacks.rs` | — | `CoreImageCallback`, `CoreRenderImageCallback` |
| `layout/src/callbacks.rs` | 4199 | `RenderImageCallbackInfo`, `CallbackChange::ChangeNodeImageMask` |
| `layout/src/xml/svg.rs` | 2524 | SVG tessellation, `render_node_clipmask_cpu()`, `allocate_clipmask_texture()`, `render_tessellated_node_gpu()` |
| `layout/src/solver3/display_list.rs` | 5170 | Display list generation — **ADD** `PushImageMaskClip`/`PopImageMaskClip` |
| `layout/src/cpurender.rs` | 1312 | CPU software renderer — **FIX** image rendering, **ADD** mask support |
| `dll/src/desktop/compositor2.rs` | 2359 | WebRender compositor — **ADD** `define_clip_image_mask()` call |
| `dll/src/desktop/wr_translate2.rs` | 3162 | WebRender translation, `process_image_callback_updates()` |
| `dll/src/desktop/shell2/common/event.rs` | 4289 | `ChangeNodeImageMask` handling (line 1280) |
| `dll/src/desktop/shell2/common/debug_server.rs` | 10006 | Debug server, `take_screenshot` |
| `examples/c/opengl.c` | 513 | Working GPU image callback example (reference) |
| `api.json` | — | C API definitions — may need updates for new functions |

## Key Function Locations

### Display List Generation
- `generate_display_list()` — `layout/src/solver3/display_list.rs:1293`
- `push_node_clips()` — `layout/src/solver3/display_list.rs:2258`
- `pop_node_clips()` — `layout/src/solver3/display_list.rs:2349`
- `push_image()` — `layout/src/solver3/display_list.rs:1287`
- `DisplayListItem` enum — `layout/src/solver3/display_list.rs:567`

### Compositor (WebRender)
- Main loop — `dll/src/desktop/compositor2.rs:140`
- `PushClip` handling — `dll/src/desktop/compositor2.rs:757`
- `PopClip` handling — `dll/src/desktop/compositor2.rs:838`
- `Image` handling — `dll/src/desktop/compositor2.rs:1256`
- `define_clip_rect()` — `dll/src/desktop/compositor2.rs:815`

### Image Callback Pipeline
- `process_image_callback_updates()` — `dll/src/desktop/wr_translate2.rs:2810`
- `ExternalImageHandler::lock()` — `dll/src/desktop/wr_translate2.rs:215`
- `build_add_image_resource_updates()` — `dll/src/desktop/wr_translate2.rs:1044`

### SVG Clip Mask Rendering
- `render_node_clipmask_cpu()` — `layout/src/xml/svg.rs:1916` (tiny_skia, R8 output)
- `allocate_clipmask_texture()` — `layout/src/xml/svg.rs:1709` (GPU R8 texture)
- `render_tessellated_node_gpu()` — `layout/src/xml/svg.rs:1738` (GPU render)

### CPU Renderer
- `render()` entry — `layout/src/cpurender.rs:31`
- `render_image()` — `layout/src/cpurender.rs:925` (TODO placeholder)
- Image item handling — `layout/src/cpurender.rs:283`

### WebRender API
- `define_clip_image_mask()` — `webrender/api/src/display_list.rs:1931`
- `ImageMask` — `webrender/api/src/display_item.rs:2107`

## Implementation Order

1. **Display list items** (Task 1) — foundation, no runtime effect yet
2. **Compositor wiring** (Task 2) — makes GPU clip masks render
3. **CPU renderer fixes** (Task 3) — makes CPU path work
4. **CPU callback fallback** (Task 4) — ensures callbacks work without GL
5. **Chart example** (Task 5) — demonstrates everything works end-to-end

## Build & Test

```bash
# Build DLL
cargo build -p azul-dll --features build-dll --release

# Build example
cp target/codegen/azul.h examples/c/azul.h
cd examples/c
cc -o chart chart.c -I. -L../../target/release -lazul -Wl,-rpath,../../target/release

# Run
DYLD_LIBRARY_PATH=../../target/release ./chart

# Run with debug server for inspection
AZUL_DEBUG=8766 DYLD_LIBRARY_PATH=../../target/release ./chart

# Take screenshots via debug server
curl -s -X POST http://localhost:8766/ -d '{"op": "take_native_screenshot"}' > native.json
curl -s -X POST http://localhost:8766/ -d '{"op": "take_screenshot"}' > cpu.json
```

## Codegen

After modifying Rust code, if the C API needs changes (new functions exposed), update `api.json` and run:
```bash
cargo run -p azul-doc -- codegen all
```

## Notes

- The `ImageMask` image MUST be in `R8` (single-channel) format for WebRender. The `render_node_clipmask_cpu()` function already outputs R8. The `allocate_clipmask_texture()` function allocates R8 textures.
- WebRender's `define_clip_image_mask()` uses the alpha channel of the image as a mask — white (255) = visible, black (0) = clipped. With R8 format, each pixel is a single byte representing the mask value.
- The `repeat` field in `ImageMask` controls whether the mask tiles. For chart use cases, `repeat: false` is typical.
- The clip mask should work with ALL display list items inside it: rects, gradients, text, images, etc.
- Do NOT modify the existing `PushClip`/`PopClip` handling — image mask clips are a separate mechanism that works alongside rectangular/rounded clips.
