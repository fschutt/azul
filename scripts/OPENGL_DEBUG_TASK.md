# OpenGL Rendering Debug Task

## Goal

Fix three categories of bugs in the `opengl.c` example application:

1. **Polygon draw call**: SVG polygons are not properly drawn in a single
   draw call despite `GL_RESTART_INDEX` being inserted between meshes.
2. **FXAA anti-aliasing**: The existing `gl_fxaa.rs` FXAA shader is compiled
   but `apply_fxaa()` is a TODO stub — implement the full-screen FXAA pass.
3. **Layout/compositing**: The image box-shadow is clipped, border-radius is
   not applied to the GPU texture, and the entire screen appears offset with
   unwanted padding.

The agent should build, run, and take native screenshots to validate each fix.

---

## Quick-start commands

```bash
# 1. Build the library
cargo build --release -p azul-dll

# 2. Copy the generated header and compile
cp target/codegen/azul.h examples/c/azul.h
cd examples/c
cc -o opengl opengl.c -I. -L../../target/release -lazul \
   -Wl,-rpath,../../target/release

# 3. Run with debug server
cd ../..
AZUL_DEBUG=8765 examples/c/opengl

# 4. In another terminal – take a native screenshot
PORT=8765; API="http://localhost:$PORT"
curl -s -X POST $API/ -d '{"op":"take_native_screenshot"}' \
  | python3 -c "import sys,json,base64; d=json.load(sys.stdin); \
    open('screenshot.png','wb').write(base64.b64decode(d['data']['screenshot']))"
```

See `scripts/DEBUG_API.md` for the full debug API reference.

**IMPORTANT**: Always use `take_native_screenshot` (macOS window capture) as
the source of truth. The software `take_screenshot` may differ.

---

## Architecture overview

### Rendering pipeline for OpenGL callback textures

```
1. layout() returns DOM with Dom::create_image(ImageRef::callback(...))
2. process_image_callback_updates()            wr_translate2.rs:2812
   └─ invokes render_my_texture()              (C callback)
      ├─ Texture::allocate_rgba8()             gl.rs:2515
      ├─ Texture::clear()                      gl.rs:2557
      ├─ TessellatedGPUSvgNode::draw()         svg.rs:894
      │   └─ GlShader::draw()                  gl.rs:3526
      │       ├─ Creates FBO, binds texture
      │       ├─ Enables PRIMITIVE_RESTART_FIXED_INDEX
      │       ├─ For each buffer: bind VAO, set uniforms, glDrawElements
      │       └─ Restores GL state
      └─ Returns ImageRef::gl_texture(texture)
3. Display list generation                     display_list.rs
   ├─ paint_node_background_and_border()       display_list.rs:2431
   │   ├─ BoxShadow (with border_radius)       display_list.rs:2575
   │   ├─ Backgrounds (Rect/Gradient)          display_list.rs:2609
   │   └─ Border                               display_list.rs:2609
   ├─ push_node_clips()                        display_list.rs:2258
   │   └─ ONLY if overflow is clipped!
   └─ paint_node_content()                     display_list.rs:2904
       └─ push_image()                         display_list.rs:1287
4. Compositor (WebRender translation)          compositor2.rs
   ├─ Rect   → define_border_radius_clip ✓    compositor2.rs:279
   ├─ BoxShadow → push_box_shadow ✓           compositor2.rs:1964
   ├─ Border → push_border ✓                  compositor2.rs:353
   └─ Image  → push_image (NO clip!) ✗        compositor2.rs:1274
```

---

## Issue 1: Polygon draw call with GL_RESTART_INDEX

### Current flow

1. `opengl.c` parses GeoJSON, tessellates each polygon via
   `AzSvgMultiPolygon_tessellateFill` / `tessellateStroke`.
2. `AzTessellatedSvgNode_fromNodes()` joins all tessellated nodes via
   `join_tessellated_nodes()` — this inserts `GL_RESTART_INDEX` (`u32::MAX`)
   between each polygon's index buffer.
3. `AzTessellatedGPUSvgNode_create()` uploads to GPU as a single
   `VertexBuffer` with `IndexBufferFormat::Triangles`.
4. `AzTessellatedGPUSvgNode_draw()` → `GlShader::draw()` which enables
   `gl::PRIMITIVE_RESTART_FIXED_INDEX` and calls `gl::DrawElements(GL_TRIANGLES, ...)`.

### Potential problems

| # | Issue | Location |
|---|-------|----------|
| 1 | `GL_TRIANGLES` + `PRIMITIVE_RESTART`: When restart index falls in a group of 3, only that incomplete triangle is discarded. This works correctly for lyon output (always multiples of 3 indices per mesh). **If the visual glitch is "wrong triangles connecting different polygons"**, it suggests the restart index is not being honored — check if the GL driver supports `PRIMITIVE_RESTART_FIXED_INDEX`. | `gl.rs:3665` |
| 2 | The old `render_tessellated_node_gpu()` (R8 clipmask path) does NOT enable `PRIMITIVE_RESTART_FIXED_INDEX` — but this path is not used by opengl.c. | `svg.rs:1738` |
| 3 | If the result looks correct for individual polygons but wrong when joined, check that vertex offsets are computed correctly in `join_tessellated_nodes`. | `svg.rs:1530-1572` |
| 4 | The shader uses `gl_Position = vec4(vCalcFinal, 1.0, 1.0)` — z=1.0 and w=1.0 means all vertices are at the same depth. If depth testing is enabled but depth-write/clear is misconfigured, fragments from polygon B may fail the depth test against polygon A. | `gl.rs:920-937` |

### Debugging steps

1. Add a `println!` in `join_tessellated_nodes` to print the total vertex
   count, total index count, and number of restart indices inserted.
2. Draw only a small subset of polygons (e.g. 3) with distinct colors to
   see if primitive restart is the issue or if it's a transform/offset issue.
3. Check `glGetError()` after the draw call (via `gl_context.get_error()`).
4. Try changing `IndexBufferFormat::Triangles` to `TriangleStrip` to see if
   the visual output changes — if it does, the issue is in how lyon indices
   are interpreted.

### Key files

| File | Lines | What |
|------|-------|------|
| `layout/src/xml/svg.rs` | 1515-1575 | `join_tessellated_nodes()` — inserts GL_RESTART_INDEX |
| `layout/src/xml/svg.rs` | 1580-1640 | `join_tessellated_colored_nodes()` — same for colored |
| `core/src/svg.rs` | 870-950 | `TessellatedGPUSvgNode` — holds VertexBuffer, draw() method |
| `core/src/gl.rs` | 3051-3110 | `VertexBuffer` struct |
| `core/src/gl.rs` | 3106-3210 | `VertexBuffer::new()` — uploads vertices/indices to GPU |
| `core/src/gl.rs` | 3240-3270 | `IndexBufferFormat` enum (Triangles, TriangleStrip, etc.) |
| `core/src/gl.rs` | 3526-3750 | `GlShader::draw()` — the actual draw call, enables PRIMITIVE_RESTART |
| `core/src/gl.rs` | 48 | `GL_RESTART_INDEX = u32::MAX` |
| `core/src/gl.rs` | 920-957 | SVG vertex/fragment shader source |
| `examples/c/opengl.c` | 270-280 | `from_nodes` + GPU upload |
| `examples/c/opengl.c` | 370-410 | `render_my_texture` — draw calls |

---

## Issue 2: FXAA integration

### Current state

- The FXAA shader is **compiled** at startup in `GlContextPtr::new()`
  (`core/src/gl.rs:1044-1064`) and stored in `GlContextPtrInner.fxaa_shader`.
- `gl_context.get_fxaa_shader()` returns the program ID (`gl.rs:883`).
- `apply_fxaa()` in `layout/src/xml/svg.rs:1733` is a **TODO stub** that
  does nothing.
- The FXAA shader expects these uniforms:
  - `uTexture` — sampler2D (the rendered texture)
  - `uTexelSize` — vec2 (1.0/width, 1.0/height)
  - `uEdgeThreshold` — float (default 0.125)
  - `uEdgeThresholdMin` — float (default 0.0312)
- The vertex shader does `vTexCoord = vAttrXY * 0.5 + 0.5` and
  `gl_Position = vec4(vAttrXY, 0.0, 1.0)` — expects a fullscreen quad
  with vertices in [-1, 1] range.

### Implementation plan

The FXAA pass should:

1. Given `texture: &mut Texture` (already rendered SVG content):
   a. Create a second texture (same size) to hold the FXAA output.
   b. Create a fullscreen quad VBO (4 vertices: [-1,-1], [1,-1], [1,1], [-1,1]).
   c. Create an FBO, bind the FXAA output texture.
   d. Bind the input texture to `GL_TEXTURE0` and set `uTexture = 0`.
   e. Set `uTexelSize`, `uEdgeThreshold`, `uEdgeThresholdMin` uniforms.
   f. Use `fxaa_shader` program.
   g. Draw the fullscreen quad.
   h. Copy the FXAA output back to the original texture (or swap).
   i. Clean up (delete FBO, temp texture, VBO).

2. Alternatively, render FXAA in-place using ping-pong:
   - Read from the existing texture, write to a temp texture.
   - Swap texture IDs so the caller gets the post-FXAA result.

3. Expose as a C API: `AzTexture_applyFxaa(&texture)` or add an
   `FxaaConfig` parameter.

### Fullscreen quad vertices

```
const FULLSCREEN_QUAD: [f32; 8] = [
    -1.0, -1.0,  // bottom-left
     1.0, -1.0,  // bottom-right
     1.0,  1.0,  // top-right
    -1.0,  1.0,  // top-left
];
const FULLSCREEN_QUAD_INDICES: [u32; 6] = [0, 1, 2, 0, 2, 3];
```

### Key files

| File | Lines | What |
|------|-------|------|
| `core/src/gl_fxaa.rs` | 1-177 | FXAA config, vertex shader, fragment shader |
| `core/src/gl.rs` | 883-884 | `get_fxaa_shader()` |
| `core/src/gl.rs` | 896 | `fxaa_shader: GLuint` in GlContextPtrInner |
| `core/src/gl.rs` | 1044-1064 | FXAA shader compilation at startup |
| `layout/src/xml/svg.rs` | 1733-1735 | `apply_fxaa()` — TODO stub |
| `layout/src/xml/svg.rs` | 1738-1900 | `render_tessellated_node_gpu()` — existing R8 render path (references fxaa_shader but never uses it) |

### C API addition

In `api.json`, add a method on `Texture`:
```json
"apply_fxaa": {
    "doc": ["Applies FXAA anti-aliasing to the texture"],
    "fn_args": [],
    "fn_body": "azul_layout::xml::svg::apply_fxaa(object)"
}
```

Or, if `FxaaConfig` should be exposed:
```json
"apply_fxaa_with_config": {
    "fn_args": [{"config": "FxaaConfig"}],
    "fn_body": "azul_layout::xml::svg::apply_fxaa_with_config(object, config)"
}
```

Then in `opengl.c`, after the two draw calls:
```c
AzTexture_applyFxaa(&texture);
```

---

## Issue 3: Layout and compositing bugs

### 3a. Box-shadow clipping

**Diagnosis**: The box-shadow on the image element is clipped. In the display
list paint order:

```
body (root):
  1. paint_node_background_and_border → gradient, no box-shadow
  2. push_node_clips → body has overflow:visible → NO CLIP pushed
  image (child):
    1. paint_node_background_and_border → box-shadow, background, border
    2. push_node_clips → image has overflow:visible → NO CLIP pushed
    3. paint_node_content → push_image (the GL texture)
```

The box-shadow should NOT be clipped by the body because the body has
`overflow: visible` (no PushClip). If it IS clipped, the issue is likely in
the compositor where the `clip_rect` on the `BoxShadow` item is set to the
element's own bounds, which would clip the shadow spread.

**Key insight**: In `compositor2.rs:2018-2023`, the `CommonItemProperties`
for box-shadow uses `clip_rect: rect` where `rect` is the element's bounds.
But outset box-shadow extends BEYOND the element bounds. The `clip_rect`
should be larger (expanded by offset + blur + spread) or set to a parent
clip chain.

**Note**: WebRender handles this internally — `push_box_shadow` takes the
box rect and computes the shadow bounds. The `clip_rect` in
`CommonItemProperties` is supposed to be the *clipping* rectangle, not the
shadow bounds. If `clip_rect` equals the element box, shadows that extend
beyond are clipped.

**Fix approach**: For `BoxShadow` items, compute an expanded `clip_rect` that
accounts for shadow offset + blur + spread:

```rust
let expand = blur_radius + spread_radius.abs() + offset.x.abs().max(offset.y.abs());
let shadow_clip = LayoutRect::from_origin_and_size(
    LayoutPoint::new(rect.min.x - expand, rect.min.y - expand),
    LayoutSize::new(rect.width() + 2.0 * expand, rect.height() + 2.0 * expand),
);
let info = CommonItemProperties {
    clip_rect: shadow_clip,  // not rect!
    clip_chain_id: current_clip!(),
    spatial_id: current_spatial!(),
    flags: Default::default(),
};
```

### 3b. Border-radius not applied to GPU texture

**Root cause**: `DisplayListItem::Image` in `compositor2.rs:1274-1316` does
NOT create a border-radius clip. Compare with `DisplayListItem::Rect` at
line 279 which calls `define_border_radius_clip()`.

**Fix approach A — Compositor-side**: Add border-radius handling to the
`Image` arm, mirroring what `Rect` does:

```rust
DisplayListItem::Image { bounds, image } => {
    let image_ref_hash = image.get_hash();
    if let Some(resolved_image) = renderer_resources.get_image(&image_ref_hash) {
        let wr_image_key = translate_image_key(resolved_image.key);
        let rect = resolve_rect(bounds, dpi_scale, current_offset!());

        // TODO: need border_radius from display list item
        let info = CommonItemProperties { ... };
        builder.push_image(&info, rect, ...);
    }
}
```

Problem: the `Image` display list item doesn't carry `border_radius`.

**Fix approach B — Display-list-side**: In `push_node_clips`
(`display_list.rs:2258`), also push a clip when an image node has a non-zero
`border-radius`, even if `overflow` is not set to clipped. This is the
correct CSS behavior: `overflow: visible` + `border-radius` should still
clip children to the rounded rect (CSS3 Backgrounds §5.3: "A box's
backgrounds, but NOT its content, are clipped to the curves").

Actually, in CSS, `border-radius` clips the background but NOT content
unless `overflow` is not `visible`. So to clip the GL texture to the rounded
corners, the CSS should have `overflow: hidden` on the image node:

```c
AzDom_setInlineStyle(&image, az_str(
    "flex-grow: 1;"
    "width: 100%;"
    "border: 5px solid red;"
    "border-radius: 50px;"
    "box-sizing: border-box;"
    "box-shadow: 0px 0px 10px black;"
    "overflow: hidden;"  // ADD THIS
));
```

Or alternatively, add a `PushClip` with border-radius specifically for image
nodes in the display list generator.

**Hybrid approach**: The cleanest fix is to:
1. Add `border_radius` to `DisplayListItem::Image`.
2. In `compositor2.rs`, apply `define_border_radius_clip` before
   `push_image` (like `Rect` does).
3. This way CSS `border-radius` on images works without requiring
   `overflow: hidden`.

### 3c. Screen offset / padding

**Diagnosis**: The body has `padding: 10px` in its inline style. This is
intentional per the Rust/C example code. If the user perceives it as an
unwanted offset, it may be because:

1. The gradient background on the body fills only the body's content box
   (within padding). The canvas background propagation
   (`display_list.rs:1337-1370`) only copies a **solid color**, not a
   gradient. Since the body has `background: linear-gradient(blue, black)`,
   the canvas background would be transparent (alpha=0) and the gradient
   would only fill the body's painted area.

2. The body's border-box with `box-sizing: border-box` should make
   `width: 100%; height: 100%` fill the viewport. The 10px padding is
   inside the border-box, so the gradient should cover the full viewport.
   But if the gradient's painted area is inset by padding, there would be a
   10px transparent border around the edge.

**Investigation**: Check whether the body's `paint_rect` covers the full
viewport or is inset. The paint rect for a border-box element with
`width: 100%; height: 100%; padding: 10px` should be the full viewport.

**Possible fix**: If the offset is unwanted, simply remove `padding: 10px`
from the body style in `opengl.c`. The Rust example also has this padding —
it may be a deliberate design choice.

### Key files for all layout/compositing issues

| File | Lines | What |
|------|-------|------|
| `dll/src/desktop/compositor2.rs` | 1274-1316 | `DisplayListItem::Image` handling — **no border-radius clip** |
| `dll/src/desktop/compositor2.rs` | 250-310 | `DisplayListItem::Rect` handling — **has border-radius clip** |
| `dll/src/desktop/compositor2.rs` | 1964-2030 | `DisplayListItem::BoxShadow` handling |
| `dll/src/desktop/compositor2.rs` | 2175-2210 | `define_border_radius_clip()` |
| `dll/src/desktop/compositor2.rs` | 793-870 | `DisplayListItem::PushClip` with rounded corners |
| `layout/src/solver3/display_list.rs` | 2258-2360 | `push_node_clips()` — only clips when overflow is clipped |
| `layout/src/solver3/display_list.rs` | 2431-2620 | `paint_node_background_and_border()` |
| `layout/src/solver3/display_list.rs` | 2575-2600 | BoxShadow emission |
| `layout/src/solver3/display_list.rs` | 2904-2912 | `paint_node_content()` → `push_image()` |
| `layout/src/solver3/display_list.rs` | 1287-1289 | `push_image()` method |
| `layout/src/solver3/display_list.rs` | 578-581 | `DisplayListItem::Image` enum variant |
| `layout/src/solver3/display_list.rs` | 697-701 | `DisplayListItem::BoxShadow` enum variant |
| `layout/src/solver3/display_list.rs` | 1337-1370 | Canvas background propagation |
| `dll/src/desktop/wr_translate2.rs` | 2812-2960 | `process_image_callback_updates()` — GL callback invocation |
| `examples/c/opengl.c` | 1-514 | The C test program |
| `examples/rust/src/opengl.rs` | 1-286 | The Rust reference example |

---

## Debugging plan

### Phase 1 — Reproduce all three bugs

1. Build, compile `opengl.c`, launch with `AZUL_DEBUG=8765`.
2. Take a baseline native screenshot.
3. Document observed issues:
   - Are polygons rendered incorrectly (gaps, missing triangles, wrong connections)?
   - Is the SVG rendering aliased (jagged edges)?
   - Is the box-shadow visually cut off?
   - Does the image have square corners instead of rounded?
   - Is there unexplained padding/offset?

### Phase 2 — Fix border-radius on Image (Issue 3b)

This is the most impactful fix. Two approaches:

**Approach A (recommended)**: Add `border_radius` field to
`DisplayListItem::Image` and handle it in compositor2.rs:

1. In `layout/src/solver3/display_list.rs`, change `DisplayListItem::Image`:
   ```rust
   Image {
       bounds: WindowLogicalRect,
       image: ImageRef,
       border_radius: BorderRadius,  // ADD
   },
   ```

2. In `push_image()` and `paint_node_content()`, pass border_radius:
   ```rust
   if let NodeType::Image(image_ref) = node_data.get_node_type() {
       builder.push_image(paint_rect, image_ref.clone(), border_radius);
   }
   ```

3. In `compositor2.rs`, handle `Image` like `Rect`:
   ```rust
   DisplayListItem::Image { bounds, image, border_radius } => {
       // ... existing code ...
       let info = if !border_radius.is_zero() {
           let new_clip_id = define_border_radius_clip(...);
           CommonItemProperties { clip_chain_id: new_clip_id, ... }
       } else {
           CommonItemProperties { clip_chain_id: current_clip!(), ... }
       };
       builder.push_image(&info, rect, ...);
   }
   ```

**Approach B (simpler)**: Add `overflow: hidden` to the image CSS in
`opengl.c`. This triggers `push_node_clips` to emit a `PushClip` with
border-radius before the image content is drawn.

### Phase 3 — Fix box-shadow clipping (Issue 3a)

In `compositor2.rs`, for the `BoxShadow` arm (line ~1964), expand `clip_rect`
to accommodate shadow spread:

```rust
// Compute shadow extent
let extent = blur_radius + spread_radius.abs()
    + offset.x.abs().max(offset.y.abs());
let expanded_rect = LayoutRect::from_origin_and_size(
    LayoutPoint::new(rect.min.x - extent, rect.min.y - extent),
    LayoutSize::new(
        rect.width() + 2.0 * extent,
        rect.height() + 2.0 * extent,
    ),
);
let info = CommonItemProperties {
    clip_rect: expanded_rect,
    clip_chain_id: current_clip!(),
    spatial_id: current_spatial!(),
    flags: Default::default(),
};
builder.push_box_shadow(&info, rect, offset, ...);
```

Rebuild, take screenshot, confirm shadow is no longer clipped.

### Phase 4 — Fix polygon draw call (Issue 1)

1. Add diagnostic prints:
   - In `join_tessellated_nodes` — print vertex count, index count,
     restart index count.
   - After `GlShader::draw` — check `gl_context.get_error()`.

2. Reduce to a minimal test: parse only 2-3 polygons and assign distinct
   colors to verify they render independently.

3. If primitive restart works correctly but polygons appear wrong, check
   the vertex transform in the shader — the `vBboxSize` uniform must match
   the texture dimensions.

4. If the issue is that all polygons look like a single merged shape,
   verify they have distinct boundaries in the testdata.json.

### Phase 5 — Implement FXAA (Issue 2)

Implement `apply_fxaa()` in `layout/src/xml/svg.rs:1733`:

```rust
pub fn apply_fxaa(texture: &mut Texture) -> Option<()> {
    apply_fxaa_with_config(texture, FxaaConfig::enabled())
}

pub fn apply_fxaa_with_config(texture: &mut Texture, config: FxaaConfig) -> Option<()> {
    if !config.enabled || texture.size.width == 0 || texture.size.height == 0 {
        return Some(());
    }

    use gl_context_loader::gl;
    use azul_core::gl::{GLuint, GlVoidPtrConst};

    let gl_context = &texture.gl_context;
    let fxaa_shader = gl_context.get_fxaa_shader();
    let texture_size = texture.size;

    // Save GL state ... (same pattern as GlShader::draw)

    // 1. Create temp output texture
    let temp_textures = gl_context.gen_textures(1);
    let temp_tex_id = *temp_textures.get(0)?;
    gl_context.bind_texture(gl::TEXTURE_2D, temp_tex_id);
    gl_context.tex_image_2d(gl::TEXTURE_2D, 0, gl::RGBA as i32,
        texture_size.width as i32, texture_size.height as i32,
        0, gl::RGBA, gl::UNSIGNED_BYTE, None.into());
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);

    // 2. Create FBO targeting temp texture
    let fbo = gl_context.gen_framebuffers(1);
    let fbo_id = *fbo.get(0)?;
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, fbo_id);
    gl_context.framebuffer_texture_2d(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0,
        gl::TEXTURE_2D, temp_tex_id, 0);

    // 3. Create fullscreen quad VAO/VBO
    let quad_verts: [f32; 8] = [-1.0,-1.0, 1.0,-1.0, 1.0,1.0, -1.0,1.0];
    let quad_indices: [u32; 6] = [0, 1, 2, 0, 2, 3];
    // ... upload to VBO/IBO ...

    // 4. Render FXAA pass
    gl_context.use_program(fxaa_shader);
    gl_context.active_texture(gl::TEXTURE0);
    gl_context.bind_texture(gl::TEXTURE_2D, texture.texture_id);
    // Set uniforms
    let u_texture = gl_context.get_uniform_location(fxaa_shader, "uTexture".into());
    gl_context.uniform_1i(u_texture, 0);
    let u_texel = gl_context.get_uniform_location(fxaa_shader, "uTexelSize".into());
    gl_context.uniform_2f(u_texel, 1.0/w, 1.0/h);
    let u_threshold = gl_context.get_uniform_location(fxaa_shader, "uEdgeThreshold".into());
    gl_context.uniform_1f(u_threshold, config.edge_threshold);
    let u_threshold_min = gl_context.get_uniform_location(fxaa_shader, "uEdgeThresholdMin".into());
    gl_context.uniform_1f(u_threshold_min, config.edge_threshold_min);

    gl_context.viewport(0, 0, w as i32, h as i32);
    gl_context.draw_elements(gl::TRIANGLES, 6, gl::UNSIGNED_INT, 0);

    // 5. Swap: copy temp → original (blit or swap texture IDs)
    // ...

    // 6. Cleanup
    gl_context.delete_framebuffers((&[fbo_id])[..].into());
    gl_context.delete_textures((&[temp_tex_id])[..].into());
    // delete quad VBO/IBO/VAO

    // Restore GL state ...
    Some(())
}
```

Then call from `opengl.c`:
```c
// After draw calls:
AzTexture_applyFxaa(&texture);
```

And expose in `api.json` as `AzTexture_applyFxaa`.

### Phase 6 — Final validation

1. Rebuild everything, recompile `opengl.c`.
2. Take native screenshots before and after each fix.
3. Verify:
   - Polygons render correctly as individual shapes.
   - FXAA smooths edges (compare before/after screenshots at zoom).
   - Box-shadow renders fully without clipping.
   - Image has rounded corners matching border-radius.
   - The layout looks correct (padding is consistent, no weird offsets).
4. `cargo build --release -p azul-dll` with no new warnings.

---

## Debug API commands (quick reference)

```bash
PORT=8765; API="http://localhost:$PORT"

# Native screenshot (ground truth)
curl -s -X POST $API/ -d '{"op":"take_native_screenshot"}' \
  | python3 -c "import sys,json,base64; d=json.load(sys.stdin); \
    open('out.png','wb').write(base64.b64decode(d['data']['screenshot']))"

# Display list inspection
curl -s -X POST $API/ -d '{"op":"get_display_list"}' | python3 -m json.tool

# DOM tree
curl -s -X POST $API/ -d '{"op":"get_dom_tree"}' | python3 -m json.tool

# Layout info for image node
curl -s -X POST $API/ -d '{"op":"get_node_layout","node_id":1}' | python3 -m json.tool

# Force redraw
curl -s -X POST $API/ -d '{"op":"redraw"}'

# Window state
curl -s -X POST $API/ -d '{"op":"get_state"}' | python3 -m json.tool
```

---

## Acceptance criteria

1. Polygons render correctly with no visual artifacts from joined index buffers.
2. FXAA is applied: edges appear smooth in native screenshots.
3. Box-shadow extends beyond the element bounds without clipping.
4. The GPU texture respects `border-radius` (rounded corners visible).
5. Layout offset is explained and either fixed or documented as intentional.
6. `cargo build --release -p azul-dll` succeeds with no new warnings.
7. Both `opengl.c` and the Rust `opengl.rs` example work correctly.
