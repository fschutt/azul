---
slug: images
title: Images and Drawing
language: en
canonical_slug: images
audience: external
maturity: wip
guide_order: 70
topic_only: false
prerequisites: [dom]
tracked_files:
  - core/src/svg.rs
  - core/src/svg_path_parser.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:31:47Z
---

# Images and Drawing

> **WIP** — APIs around `ImageRef`, SVG, and GPU textures are stable enough to
> use, but some helpers (e.g. PNG/JPEG decoding) live behind feature flags and
> may move between crates.

Azul has three sources of pixel content in a DOM tree:

| Source | Backed by | Use case |
|---|---|---|
| Raster image | CPU pixel buffer (`RawImageFormat`) | PNG, JPEG, BMP, raw bytes |
| Vector / SVG | `SvgMultiPolygon` + tessellation | Icons, charts, diagrams |
| GPU texture | `gl::Texture` produced by callback | OpenGL scenes, custom shaders |

All three end up wrapped in a single `ImageRef` and inserted into the tree
via `Dom::create_image(image_ref)`. The framework hashes the handle for
caching and uploads the backing data to the renderer the first time it is
shown.

## `ImageRef`

`ImageRef` (`core/src/resources.rs:790`) is a reference-counted pointer to a
`DecodedImage`:

```rust,ignore
pub enum DecodedImage {
    NullImage { width: usize, height: usize, format: RawImageFormat, tag: Vec<u8> },
    Gl(Texture),
    Raw((ImageDescriptor, ImageData)),
    Callback(CoreImageCallback),
}
```

`ImageRef` is `Send + Sync`. Cloning bumps a refcount; the underlying buffer
is freed when the last clone drops. Construct one via:

| Constructor | Returns |
|---|---|
| `ImageRef::null_image(w, h, format, tag)` | a placeholder of known size |
| `ImageRef::new_rawimage(raw)` | wraps a CPU pixel buffer |
| `ImageRef::new_gltexture(texture)` | wraps an existing GL texture |
| `ImageRef::callback(cb, data)` | defers rendering until layout knows the size |

## Raster images

Raw pixel data flows through `RawImage`. The pixel layout is described by
`RawImageFormat` (`core/src/resources.rs:693`): `R8`, `RG8`, `RGB8`, `RGBA8`,
`R16`, `RG16`, `RGB16`, `RGBA16`, `BGR8`, `BGRA8`, `RGBF32`, `RGBAF32`. Pick
the one that matches the source bytes — the renderer converts to its
internal format on upload.

```rust,no_run
# use azul::prelude::*;
# use azul::image::{ImageRef, RawImage, RawImageFormat, RawImageData};
# fn load_pixels() -> Vec<u8> { vec![] }
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

Decoded image dimensions are reported by `ImageRef::get_size()` and the
size is fixed once the buffer is created. To resize at render time, wrap
the image in a styled element and let CSS scale the box:

```rust,no_run
# use azul::prelude::*;
# fn icon() -> ImageRef { panic!() }
Dom::create_image(icon())
    .with_inline_css("width: 32px; height: 32px;");
```

A `null_image` keeps a slot in the cache without uploading data — useful as
a fallback when an asynchronous loader has not finished yet.

## SVG

Vector graphics go through tessellation: every closed path is converted to
a triangle list at a tolerance you control, then either rendered on the CPU
or uploaded to the GPU as a `TessellatedGPUSvgNode`. Path strings (`d`
attributes) come from `parse_svg_path_d`:

```rust,no_run
# use azul::prelude::*;
# use azul::svg::{parse_svg_path_d, SvgFillStyle, SvgMultiPolygon, SvgPath, SvgPathVec};
let multipolygon: SvgMultiPolygon = SvgMultiPolygon {
    rings: SvgPathVec::from_vec(vec![
        parse_svg_path_d("M 10 10 L 100 10 L 100 100 L 10 100 Z")
            .expect("malformed SVG path")
            .rings
            .into_iter()
            .next()
            .unwrap()
    ]),
};

// Tessellate once, draw many times
let tessellated = multipolygon.tessellate_fill(SvgFillStyle::default());
```

See [SVG](images/svg.md) for the full geometry model, stroking options, and
how to combine multiple polygons into one draw call.

## GPU textures and custom drawing

`ImageRef::callback(...)` defers image production until the layout pass
knows the box dimensions. The callback receives a `RenderImageCallbackInfo`
with the available GL context and the laid-out bounds, and returns an
`ImageRef` (typically wrapping a fresh `Texture`):

```rust,no_run
# use azul::prelude::*;
# use azul::callbacks::RenderImageCallbackInfo;
# use azul::dom::RenderImageCallback;
# use azul::image::{ImageRef, RawImageFormat};
# use azul::vec::U8VecRef;
extern "C" fn render(_data: RefAny, mut info: RenderImageCallbackInfo) -> ImageRef {
    let size = info.get_bounds().get_physical_size();
    // ...allocate a texture, draw into it, return ImageRef::gl_texture(tex)
    ImageRef::null_image(size.width as usize, size.height as usize,
                        RawImageFormat::RGBA8, U8VecRef::from(&[][..]))
}

# fn build_dom(state: RefAny) -> Dom {
Dom::create_image(ImageRef::callback(
    RenderImageCallback::create(render).to_core(),
    state,
))
# }
```

See [Canvas and GL Textures](images/canvas-gl.md) for the full texture
allocation, drawing, and FXAA flow used by the `opengl` example.

## Sizing and aspect ratio

The renderer treats an `ImageRef` like an `<img>` tag: it expands to fill
its CSS box. To preserve aspect ratio:

| CSS combination | Result |
|---|---|
| `width: 100px; height: 100px` | stretched to a square |
| `width: 100px` (height auto) | scaled to keep the source aspect ratio |
| `flex-grow: 1` (square parent) | fills the parent |

Image content is stretched without filtering hints; for icon-quality output
you usually want the source resolution to match the on-screen pixel size.

## Updating an image

`ImageRef` is intern-keyed: passing a clone in two consecutive `Dom`s
re-uses the same upload. To swap pixels you build a new `ImageRef`. Drop
the old clone if you want the GPU memory freed.

For animation, use a render-image callback (`ImageRef::callback`) and a
[timer](timers.md) to flag the DOM for repaint each frame. The callback
re-runs and returns a fresh texture; the previous one is freed
automatically when the new clone replaces it.
