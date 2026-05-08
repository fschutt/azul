---
slug: internals/rendering/image-pipeline
title: Image Pipeline
language: en
canonical_slug: internals/rendering/image-pipeline
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Decoding, caching, and uploading raster images
prerequisites: [layout, dom-internals]
tracked_files:
  - layout/src/lib.rs
  - layout/src/window.rs
  - layout/src/image.rs
  - dll/src/desktop/gl_texture_cache.rs
  - dll/src/desktop/shader_cache.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:55:41Z
---

# Image Pipeline

## Overview

The image pipeline covers raster image decoding, the WebRender-facing
texture caches, and the on-disk shader binary cache. *WIP — image and
texture handling is split across multiple caches with overlapping names;
naming cleanup is queued behind API stability.*

There are four caches, two layers, and one disk format. This page maps
them so a contributor can find the right one to extend. The four caches
serve distinct roles: layout writes a *solved* per-DOM-node table, the
runtime store holds the *physical* GL textures referenced by WebRender
display lists, the URL-keyed `ImageCache` resolves CSS
`background-image: url(...)` to decoded rasters, and `ShaderDiskCache`
persists WebRender shader binaries between runs.

## The four caches

- **`ImageCache`.** Lives at `LayoutWindow.image_cache`. Maps CSS
  `background-image: url(...)` URLs to `ImageRef` decoded rasters, plus
  the image-mask resolution table. Window-scoped. Defined in
  `core/src/resources.rs`.
- **`GlTextureCache` (layout side).** Lives at
  `LayoutWindow.gl_texture_cache`. Holds per-DOM-node texture metadata:
  `(DomId, NodeId) → (ImageKey, ImageDescriptor, ExternalImageId)`. Layout
  *solves* it, WebRender translation *consumes* it. Window-scoped.
- **`gl_texture_cache` (runtime store).** Thread-local `TEXTURE_CACHE`
  in `dll/src/desktop/gl_texture_cache.rs`. Holds actual `Texture`
  objects keyed by `ExternalImageId`, with `Epoch` for cleanup. Lives on
  the GL thread.
- **`ShaderDiskCache`.** Per-process. Persists WebRender shader binaries
  (`ProgramBinary`) on disk by source digest. Lives in the cache
  directory.

The naming clash between layout's `GlTextureCache` (solved metadata) and
the runtime `gl_texture_cache` module (actual textures) is flagged for
cleanup. They serve distinct roles: layout writes the *solved* table,
the runtime store holds the *physical* textures referenced by WebRender
display lists.

## Stable ExternalImageId

```rust,ignore
#[repr(C)]
pub struct ExternalImageId {
    pub inner: u64,
}
```

WebRender caches display lists across frames. When a display list
references an `ExternalImageId`, that ID must remain valid across frames
and point to the current texture. If IDs were generated fresh each frame,
cached display lists would reference stale IDs.

Two id-derivation strategies, both deterministic:

```rust,ignore
// Per (DomId, NodeId), used for canvas/GL callback textures:
pub(crate) struct TextureSlotKey {
    pub dom_id: DomId,
    pub node_id: NodeId,
}

impl TextureSlotKey {
    pub fn to_external_image_id(&self) -> ExternalImageId {
        let dom = self.dom_id.inner as u64;
        let node = self.node_id.index() as u64;
        let combined = (dom << 32) | (node & 0xFFFFFFFF);
        ExternalImageId { inner: combined }
    }
}

// Per ImageRef hash, used for raster images:
ExternalImageId { inner: image_ref_hash.inner as u64 }
```

The same DOM node (or the same `ImageRef`) thus always produces the same
`ExternalImageId`, so WebRender's cached display lists keep working.

## Texture insertion API

The runtime texture store exposes two insertion functions, both routing
through the same internal cache:

```rust,ignore
// (DomId, NodeId) → ExternalImageId via TextureSlotKey
pub fn insert_texture_for_node(
    document_id: DocumentId,
    dom_id: DomId,
    node_id: NodeId,
    epoch: Epoch,
    texture: Texture,
) -> ExternalImageId;

// Caller-supplied ExternalImageId (already derived from an ImageRefHash)
pub fn insert_texture_by_id(
    document_id: DocumentId,
    external_image_id: ExternalImageId,
    epoch: Epoch,
    texture: Texture,
);
```

`insert_texture_for_node` calls `insert_texture_by_id` internally —
single keyspace, two convenience entry points. The cache layout is
`DocumentId → ExternalImageId → TextureEntry { texture, epoch }`.
Per-document because WebRender keeps one document per window.

## Epoch-based eviction

`Epoch` is a per-document u32 frame counter incremented each render.
`remove_old_epochs(document_id, current_epoch)` walks the cache and
drops entries whose epoch is older than `current_epoch - 1`. The "− 1"
is for double-buffering: a frame that's actively rendering (or queued
for compositor) may still be referencing textures from the previous
epoch.

```rust,ignore
let current = current_epoch.into_u32();
let min_epoch_to_keep = if current >= 2 {
    Epoch::from(current - 1)
} else {
    Epoch::new()
};
```

The shell calls `remove_old_epochs` after each frame. Textures unused
for 2+ frames are dropped (and their underlying GL texture freed).

## Thread-local enforcement

The runtime texture store uses `thread_local!`:

```rust,ignore
thread_local! {
    static TEXTURE_CACHE: RefCell<Option<OrderedMap<DocumentId, GlTextureStorage>>> =
        RefCell::new(None);
}
```

Texture creation requires an OpenGL context, which is single-threaded by
API contract. Putting the cache in `thread_local!` enforces this at the
type system level — a function that touches `TEXTURE_CACHE` cannot be
called from a non-GL thread without panic.

## Raster image decode

Behind `feature = "image_decoding"`. `layout/src/image.rs` wraps the
[`image`](https://crates.io/crates/image) crate behind FFI-friendly
types:

```rust,ignore
pub fn decode_raw_image_from_any_bytes(image_bytes: &[u8]) -> ResultRawImageDecodeImageError;
```

Format detection is `image::guess_format`. Supported pixel formats map
to `RawImageFormat` as follows: `ImageLuma8` to `R8`, `ImageLumaA8` to
`RG8`, `ImageRgb8` to `RGB8`, `ImageRgba8` to `RGBA8`, `ImageLuma16` to
`R16`, `ImageLumaA16` to `RG16`, `ImageRgb16` to `RGB16`, `ImageRgba16`
to `RGBA16`, `ImageRgb32F` to `RGBF32`, and `ImageRgba32F` to `RGBAF32`.

`RawImage` carries pixel data as `RawImageData` (`U8` / `U16` / `F32`)
plus dimensions, format, and `premultiplied_alpha: bool`. The decoder
always returns `premultiplied_alpha = false` — premultiplication happens
later (in WebRender translation) if the descriptor flags request it.

`DecodeImageError`:

```rust,ignore
#[repr(C)]
pub enum DecodeImageError {
    InsufficientMemory,
    DimensionError,
    UnsupportedImageFormat,
    Unknown,
}
```

`InsufficientMemory` and `DimensionError` come from
`image::error::LimitErrorKind`. Image-format errors collapse into
`Unknown` because the underlying error variants don't have stable C ABI
shape.

## Encoding

`encode_png`, `encode_jpeg(image, quality)`, `encode_bmp`, `encode_tga`,
`encode_tiff`, `encode_gif`, `encode_pnm`. Each is gated behind a
per-format feature flag (`png`, `jpeg`, `bmp`, …). When the flag is off,
the function returns `EncoderNotAvailable` so callers don't crash on a
missing codec — just degrade.

`translate_rawimage_colortype` handles `BGR8`/`BGRA8` → `Rgb8`/`Rgba8`
mapping. The TODO marker in the source flags an inconsistency: BGR/RGB
conversion isn't actually applied, just relabelled. Loaders that produce
`BGRA8` and round-trip through `encode_*` will get colour-channel-swapped
output.

## ImageRef and reference counting

```rust,ignore
#[repr(C)]
pub struct ImageRef {
    pub data: *const DecodedImage,
    pub copies: *const AtomicUsize,
    pub run_destructor: bool,
}
```

C-ABI-compatible reference counting. `data` points to a heap
`DecodedImage` (the variant of which is hidden from C), `copies` points
to a heap `AtomicUsize` reference counter. `Clone` bumps `copies`;
`Drop` decrements and frees on zero.

`ImageRef::into_inner()` extracts `DecodedImage` if `*copies == 1` (no
other holders); `ImageRef::deep_copy()` clones the underlying image.
Deep copy of `DecodedImage::Gl(tex)` returns `NullImage` because GL
textures cannot be cloned without the GL context — that's a known
limitation in the OpenGL trait surface.

`DecodedImage` covers raster (`Raw`), GL texture (`Gl`), null image
(`NullImage`), and callback-driven images (`Callback`). The callback
variant lets the layout postpone resolution until rendering — the
callback runs once we have a GL context and produces the actual
`Texture`.

## ImageRefHash for content-addressed deduplication

`ImageRefHash { inner: usize }` is a stable hash of the `ImageRef`'s
content. Two `ImageRef`s pointing at byte-identical decoded images
compare equal; two pointing at different bytes don't. Used as the
`ExternalImageId` derivation key for raster images (so two
`<img src="x.png">` tags pointing at the same file get the same
texture).

`image_ref_get_hash(image_ref)` is the canonical hasher.

## RendererResources

`RendererResources` holds parsed font and image resources per renderer
(per window). Layout reads from this when measuring image intrinsic
sizes (`InlineImage::intrinsic_size`). The image's natural width and
height come from the decoded `RawImage`'s dimensions.

The split is: `ImageCache` (in `LayoutWindow`) is the *DOM-side* lookup
keyed by URL, `RendererResources` (also in `LayoutWindow`) is the
*renderer-side* lookup keyed by `ImageKey` / `FontKey`.

## Shader binary disk cache

WebRender lazily compiles + links each shader on first use; the cost is
~10–50 ms per shader. `ShaderDiskCache` extracts the linked binary via
`glGetProgramBinary` and persists it. On the next run, `glProgramBinary`
skips compile + link.

Disk layout:

```text
~/Library/Caches/azul/shaders/<renderer_hash>/        (macOS)
~/.cache/azul/shaders/<renderer_hash>/                 (Linux, $XDG_CACHE_HOME aware)
%LOCALAPPDATA%\azul\shaders\<renderer_hash>\           (Windows)

<renderer_hash>/<digest_hex>.bin    raw program binary
<renderer_hash>/<digest_hex>.meta   12 bytes: format (u32 LE) + digest (u64 LE)
```

The `<renderer_hash>` subdirectory is `hash(gl_renderer_string +
gl_version)`. When the user upgrades their GPU driver, `<renderer_hash>`
changes and old binaries are no longer found — automatic invalidation,
no version-gating logic needed.

`ShaderDiskCache` implements WebRender's `ProgramCacheObserver`:

```rust,ignore
impl ProgramCacheObserver for ShaderDiskCache {
    fn save_shaders_to_disk(&self, entries: Vec<Arc<ProgramBinary>>);
    fn set_startup_shaders(&self, _entries: Vec<Arc<ProgramBinary>>);  // no-op
    fn try_load_shader_from_disk(
        &self,
        digest: &ProgramSourceDigest,
        program_cache: &Rc<ProgramCache>,
    );
    fn notify_program_binary_failed(&self, program_binary: &Arc<ProgramBinary>);
}
```

`set_startup_shaders` is a no-op — `load_all_from_disk` is called
explicitly at startup and loads every cached binary, so a separate
"startup set" is redundant. `notify_program_binary_failed` removes both
`.bin` and `.meta` from disk: a cached binary that fails to re-link
(driver bug, GPU change WebRender didn't catch) is treated as poison
and not retried.

## Pipeline: from CSS image to GPU texture

```text
CSS background-image: url("logo.png")
   │
   ▼  parser stores StyleBackgroundContent::Image(CssImageId)
   │
   ▼  build_compact_cache(): extract CssImageId → records in tier2 props
   │
   ▼  layout_dom_recursive (window.rs)
   │   resolves CssImageId via ImageCache, gets ImageRef
   │
   ▼  solver3 sizing.rs measures intrinsic size from ImageRef
   │
   ▼  display_list.rs emits ImageCommand { image_ref, descriptor, bounds }
   │
   ▼  wr_translate2.rs translates to WebRender display list
   │   computes ExternalImageId from ImageRefHash
   │   inserts texture into runtime store if needed
   │   emits WR ImageDisplayItem with the ExternalImageId
   │
   ▼  WebRender composites; on image lookup, gl_texture_integration.rs
   │   serves the GL Texture from the runtime store
   │
   ▼  glDrawElements with the texture bound
```

For GL callback / canvas content, the same pipeline runs but the
`ImageRef`'s `DecodedImage::Callback` variant is invoked at translation
time. The callback produces a `Texture` that's inserted into the runtime
store keyed by `(DomId, NodeId).to_external_image_id()`.

## CSS image masks and effects

`ImageDescriptor` carries `format`, `size`, `flags`, and an
`OptionImageMask`. When a node has `image-mask: url(...)`, the layout
side resolves the mask to an `ImageRef` and includes it in the
`ImageDescriptor`. WebRender uses the mask as an alpha brush during
composition.

## Adding a new image format

For raster formats handled by the `image` crate:

1. Add a feature flag in `layout/Cargo.toml` (e.g.
   `webp = ["image/webp"]`).
2. The `image` crate auto-supports the new format through
   `image::guess_format`; `decode_raw_image_from_any_bytes` requires no
   change.
3. For encoding: add an `encode_<fmt>` line via the `encode_func!`
   macro in `image.rs`.
4. If the format has a unique `DynamicImage::Image*` variant, extend
   the match in `decode_raw_image_from_any_bytes`.

For non-raster formats (SVG, PDF page images, video frames):

1. Add a `DecodedImage` variant in `core/src/resources.rs`
   (`SvgImage(parsed_svg)`, `Pdf(...)`).
2. Extend `deep_copy` to handle the new variant.
3. Extend the renderer-side translator (`wr_translate2.rs`) to convert
   the variant to a WebRender display item.
4. Update `RawImage` translation only if the new format can be
   rasterized to RGBA — otherwise it stays in its native form for as
   long as possible.

## Known gotchas

- **Naming collision.** `core::resources::GlTextureCache` (solved
  metadata, layout-owned) vs the `gl_texture_cache` module in
  `dll/src/desktop/` (runtime texture store, thread-local). They are
  not the same thing despite sharing a name.
- **GL textures cannot be `Clone`d.** `ImageRef::deep_copy` returns
  `NullImage` for `DecodedImage::Gl`. Code that needs an owned copy of
  a GL texture must blit it explicitly through GL.
- **`BGR8`/`BGRA8` encode mislabel.** `translate_rawimage_colortype`
  maps both to `Rgb8`/`Rgba8` without channel swap. A `BGRA8` image
  round-tripped through `encode_png` will have R and B swapped in the
  output. Decoder side is fine — only encoder.
- **`RawImage::premultiplied_alpha` is always `false` from the decoder.**
  Premultiplication happens at WebRender translation time based on
  `ImageDescriptorFlags`. Decoders don't premultiply.
- **Thread-local `TEXTURE_CACHE` panics if accessed off-thread.** Any
  test or callback that touches the runtime texture store must run on
  the GL thread or use a stub. CPU-only headless tests skip this code
  path entirely.

## Coming Up Next

- [Rendering](../rendering.md) — display list to pixels
- [WebRender Bridge](webrender-bridge.md) — transactions, pipelines, IFrames
- [GL Loading](gl-loading.md) — per-platform GL symbol resolution
- [Text Pipeline](text-pipeline.md) — font discovery, parsing, fallback chains
