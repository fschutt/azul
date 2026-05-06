---
slug: canvas-gl
title: GL Canvas
language: en
canonical_slug: canvas-gl
audience: external
maturity: wip
guide_order: 72
topic_only: false
short_desc: Embedding an OpenGL canvas inside a Dom node
prerequisites: [images]
tracked_files:
  - core/src/svg.rs
  - core/src/svg_path_parser.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# GL Canvas
> WIP. The GL callback ABI is stable. What you can call on the GL context is the OpenGL 3.2-core subset exposed by `GlContextPtr`.

A render-image callback gives you a fresh GL context and the laid-out bounding box of a DOM node, and asks you to return an `ImageRef` for that node. Use it whenever the pixel content depends on size or animation: an OpenGL scene, a tessellated SVG that needs custom transforms, a chart sized to fit its container.

## The callback

```rust,ignore
pub type RenderImageCallbackType =
    extern "C" fn(RefAny, RenderImageCallbackInfo) -> ImageRef;
```rust

Wrap a function pointer in `RenderImageCallback::create(...)` and convert it to the FFI-compatible form with `.to_core()`:

```rust,no_run
# use azul::prelude::*;
# use azul::callbacks::RenderImageCallbackInfo;
# use azul::dom::RenderImageCallback;
# use azul::image::ImageRef;
extern "C" fn render(_data: RefAny, _info: RenderImageCallbackInfo) -> ImageRef {
    // ... build texture ...
    panic!("returned a Texture-backed ImageRef")
}

# fn build(state: RefAny) -> Dom {
Dom::create_image(ImageRef::callback(
    RenderImageCallback::create(render).to_core(),
    state,
))
# }
```

`RenderImageCallbackInfo` exposes:

- `get_bounds()`. Returns `HidpiAdjustedBounds` with logical and physical sizes.
- `get_gl_context()`. Returns `OptionGlContextPtr`. `None` on backends without GL.
- `get_callback_node_id()`. The node ID this callback is attached to.
- `get_ctx()`. The FFI context, used by Python and C bindings.

## Allocating a texture

`Texture::allocate_rgba8` allocates an RGBA8 texture sized to a `PhysicalSizeU32` and clears it to a background color:

```rust,no_run
# use azul::prelude::*;
# use azul::callbacks::RenderImageCallbackInfo;
# use azul::gl::Texture;
# fn body(info: &mut RenderImageCallbackInfo) -> Option<Texture> {
let gl = info.get_gl_context().into_option()?;
let size = info.get_bounds().get_physical_size();
let mut texture = Texture::allocate_rgba8(gl, size, ColorU::from_str("#ffffffff"));
texture.clear();
# Some(texture)
# }
```

Sizing matches the post-layout physical pixel box, so the texture and the on-screen draw area are 1:1. The renderer doesn't rescale.

## Drawing tessellated SVG into a texture

`Texture::draw_tesselated_svg_gpu_node` takes a GPU mesh, a target size, a fill color, and an optional list of transforms. Transforms are `StyleTransform`s — the same ones the CSS layer uses — so percentage translations resolve against the texture size:

```rust,no_run
# use azul::prelude::*;
# use azul::css::{AngleValue, PixelValue, StyleTransform, StyleTransformTranslate2D};
# use azul::gl::{PhysicalSizeU32, Texture};
# use azul::svg::TessellatedGPUSvgNode;
# fn run(texture: &mut Texture, mesh: TessellatedGPUSvgNode,
#        size: PhysicalSizeU32, deg: f32) {
texture.draw_tesselated_svg_gpu_node(
    mesh,
    size,
    ColorU::from_str("#cc00cc"),
    vec![
        StyleTransform::Translate(StyleTransformTranslate2D {
            x: PixelValue::percent(50.0),
            y: PixelValue::percent(50.0),
        }),
        StyleTransform::Rotate(AngleValue::deg(deg)),
    ],
);
# }
```rust

To anti-alias the result, call `texture.apply_fxaa()` before returning. It's a single-pass post-process suitable for vector content.

## Returning the texture

A texture becomes an `ImageRef` via `ImageRef::gl_texture(texture)`. Refs are reference-counted, so returning a clone after every callback is fine:

```rust,no_run
# use azul::prelude::*;
# use azul::image::ImageRef;
# use azul::gl::Texture;
# fn done(t: Texture) -> ImageRef {
ImageRef::gl_texture(t)
# }
```rust

If something fails (no GL context, missing data, GPU error), return a `null_image` of the requested size. The renderer treats it as transparent and reserves space in the layout.

## When the callback runs

A render-image callback runs:

1. Once on first display of the DOM node, after layout has assigned the node a size.
2. On every frame the DOM is re-built, if the node is still in the tree. To drive animation without rebuilding, return `Update::RefreshDom` from a [timer](timers.md) callback so the framework reissues the render.
3. Never if the node never enters the tree, or if the render-image feature is disabled (e.g. headless mode without a GL context).

The renderer doesn't memoize results. The callback is responsible for caching its own state in the `RefAny` it received.

## A complete loop

The end-to-end pattern: tessellate SVG geometry once, upload to GPU buffers in a startup callback, redraw on every animation tick. The shape of the code is:

```rust,no_run
# use azul::prelude::*;
# use azul::callbacks::{RenderImageCallbackInfo};
# use azul::dom::RenderImageCallback;
# use azul::gl::{PhysicalSizeU32, Texture};
# use azul::image::{ImageRef, RawImageFormat};
# use azul::svg::*;
# use azul::vec::U8VecRef;
struct AppState {
    rotation_deg: f32,
    fill_buffer: Option<TessellatedGPUSvgNode>,
}

extern "C" fn render(mut data: RefAny, mut info: RenderImageCallbackInfo) -> ImageRef {
    let size = info.get_bounds().get_physical_size();
    let invalid = ImageRef::null_image(
        size.width as usize, size.height as usize,
        RawImageFormat::RGBA8, U8VecRef::from(&[][..]),
    );
    let result = (|| {
        let mut state = data.downcast_mut::<AppState>()?;
        let gl = info.get_gl_context().into_option()?;
        let buffer = state.fill_buffer.as_ref()?.clone();
        let mut texture = Texture::allocate_rgba8(gl, size, ColorU::from_str("#ffffff"));
        texture.clear();
        texture.draw_tesselated_svg_gpu_node(
            buffer, size, ColorU::from_str("#0080ff"),
            vec![],
        );
        texture.apply_fxaa();
        Some(ImageRef::gl_texture(texture))
    })();
    result.unwrap_or(invalid)
}
```

The full working example lives in `examples/rust/src/opengl.rs`. It parses GeoJSON, tessellates the polygons once, and rotates the result on a timer.

## Lifetime and cleanup

`Texture` is reference-counted and frees its GL texture when the last clone drops. Returning a fresh `ImageRef::gl_texture(...)` from each callback invocation is the normal pattern: the previous frame's texture is dropped automatically by the framework once the new one replaces it.

`GlContextPtr` is also reference-counted. Cloning it in your `RefAny` state is safe and avoids re-fetching it via `info.get_gl_context()` on hot paths.

## Coming Up Next

- [Animations](../animations.md) — CSS transitions and @keyframes
- [SVG](svg.md) — Parsing and rendering SVG documents
- [Images](../images.md) — Loading raster images and CSS backgrounds
