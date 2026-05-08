---
slug: images
title: Images
language: en
canonical_slug: images
audience: external
maturity: wip
guide_order: 70
topic_only: false
short_desc: Loading raster images and CSS backgrounds
prerequisites: [dom]
tracked_files:
  - core/src/svg.rs
  - core/src/svg_path_parser.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Images
> WIP. APIs around `ImageRef`, SVG, and GPU textures are stable enough to use, but some helpers (e.g. PNG/JPEG decoding) live behind feature flags and may move between crates.

Azul has three sources of pixel content in a DOM tree:

- Raster image. Backed by a CPU pixel buffer (`RawImageFormat`). PNG, JPEG, BMP, raw bytes.
- Vector / SVG. Backed by `SvgMultiPolygon` plus tessellation. Icons, charts, diagrams.
- GPU texture. Backed by a `Texture` produced by callback. OpenGL scenes, custom shaders.

All three end up wrapped in a single `ImageRef` and inserted into the tree via `Dom::create_image(image_ref)`. The framework hashes the handle for caching and uploads the backing data to the renderer the first time it's shown.

## ImageRef

`ImageRef` is a reference-counted handle to decoded image data. It's `Send + Sync`. Cloning bumps a refcount; the underlying buffer is freed when the last clone drops. Construct one via:

- `ImageRef::null_image(w, h, format, tag)`. A placeholder of known size.
- `ImageRef::new_rawimage(raw)`. Wraps a CPU pixel buffer.
- `ImageRef::gl_texture(texture)` (also `new_gltexture`). Wraps an existing GL texture.
- `ImageRef::callback(cb, data)`. Defers rendering until layout knows the size.

Inspect with `is_null_image`, `is_raw_image`, `is_gl_texture`, `is_callback`, `is_invalid`, `get_size`, `get_hash`.

## Raster images

Raw pixel data flows through `RawImage`. The pixel layout is `RawImageFormat`: `R8`, `RG8`, `RGB8`, `RGBA8`, `R16`, `RG16`, `RGB16`, `RGBA16`, `BGR8`, `BGRA8`, `RGBF32`, `RGBAF32`. Pick the one that matches the source bytes; the renderer converts to its internal format on upload.

```rust,no_run
use azul::prelude::*;

fn load_pixels() -> Vec<u8> { vec![] }

let bytes: Vec<u8> = load_pixels();
let raw = RawImage {
    pixels: RawImageData::U8(bytes.into()),
    width: 256,
    height: 256,
    premultiplied_alpha: true,
    data_format: RawImageFormat::RGBA8,
    tag: Vec::new().into(),
};
let image_ref = ImageRef::new_rawimage(raw).expect("invalid pixel data");
let dom = Dom::create_image(image_ref);
```

`RawImageData` is one of `U8`, `U16`, `F32`. Decoded image dimensions are reported by `ImageRef::get_size()`. The size is fixed once the buffer is created. To resize at render time, wrap the image in a styled element and let CSS scale the box (set `width` / `height` via `Dom::with_css` or `Dom::with_css_property`).

A `null_image` keeps a slot in the cache without uploading data. It's useful as a fallback when an asynchronous loader hasn't finished yet.

`RawImage` also exposes encoders (`encode_png`, `encode_jpeg`, `encode_bmp`, `encode_gif`, `encode_pnm`, `encode_tga`, `encode_tiff`) and the universal decoder `RawImage::decode_image_bytes_any`.

## SVG

Vector graphics go through tessellation: every closed path is converted to a triangle list, then rendered on the CPU or uploaded to the GPU. Build paths in memory with `SvgPath::create` and combine them via `SvgMultiPolygon::create`. Then call `SvgMultiPolygon::tessellate_fill` or `tessellate_stroke` to produce a `TessellatedSvgNode`.

See [SVG](images/svg.md) for the full geometry model, stroking options, and how to combine multiple polygons into one draw call.

## GPU textures and custom drawing

`ImageRef::callback(...)` defers image production until the layout pass knows the box dimensions. The callback receives a `RenderImageCallbackInfo` with the available GL context and the laid-out bounds, and returns an `ImageRef` (typically wrapping a fresh `Texture`):

```rust,no_run
use azul::prelude::*;

extern "C" 
fn render(_data: RefAny, mut info: RenderImageCallbackInfo) -> ImageRef {
    let size = info.get_bounds().get_physical_size();
    // allocate a texture, draw into it, return ImageRef::gl_texture(tex)
    ImageRef::null_image(size.width as usize, size.height as usize,
                        RawImageFormat::RGBA8, U8VecRef::from(&[][..]))
}

fn build_dom(state: RefAny) -> Dom {
    Dom::create_image(ImageRef::callback(
        RenderImageCallback::create(render).to_core(),
        state,
    ))
}
```

See [Canvas and GL Textures](images/canvas-gl.md) for the full texture allocation, drawing, and FXAA flow used by the `opengl` example.

## Sizing and aspect ratio

The renderer treats an `ImageRef` like an `<img>` tag: it expands to fill its CSS box. To preserve aspect ratio:

- `width: 100px; height: 100px`. Stretched to a square.
- `width: 100px` (height auto). Scaled to keep the source aspect ratio.
- `flex-grow: 1` (square parent). Fills the parent.

Image content is stretched without filtering hints. For icon-quality output you usually want the source resolution to match the on-screen pixel size.

## Updating an image

`ImageRef` is intern-keyed: passing a clone in two consecutive `Dom`s reuses the same upload. To swap pixels you build a new `ImageRef`. Drop the old clone if you want the GPU memory freed.

For animation, use a render-image callback (`ImageRef::callback`) and a [timer](timers.md) to flag the DOM for repaint each frame. The callback re-runs and returns a fresh texture; the previous one is freed automatically when the new clone replaces it.

## Image masks

`ImageMask` clips drawn content to an image-defined alpha mask. It carries `image: ImageRef`, `rect: LogicalRect`, and `repeat: bool`. Apply one to a `Dom` with `Dom::with_clip_mask`.

## Coming Up Next

- [SVG](images/svg.md) — Parsing and rendering SVG documents
- [GL Canvas](images/canvas-gl.md) — Embedding an OpenGL canvas inside a Dom node
- [Styling with CSS](styling.md) — Stylesheets, selectors, and the cascade
