---
slug: image-pipeline
title: Image Pipeline
language: en
canonical_slug: image-pipeline
audience: contributor
maturity: wip
guide_order: null
topic_only: false
prerequisites: [code-organization, dom, css-properties]
tracked_files:
  - dll/src/desktop/gl_texture_cache.rs
  - dll/src/desktop/shader_cache.rs
  - dll/src/desktop/wr_translate2.rs
  - dll/src/desktop/gl_texture_integration.rs
  - core/src/resources.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:32:10Z
---

# Image Pipeline

> **WIP** — three caches feed images into WebRender (`ImageCache`, `RendererResources`, `gl_texture_cache::TEXTURE_CACHE`). The story is currently spread across `core/src/resources.rs` and `dll/src/desktop/`; the duplicated `GlTextureCache` name is a known wart.

The image pipeline turns CSS `background-image: url(…)` declarations and `NodeType::Image` nodes into GPU-resident textures referenced by WebRender. It also caches compiled shader binaries on disk so the WebRender renderer skips its 10–50 ms compile/link step on every launch.

```
url("…") in CSS  ┐
                 ├─►  ImageCache (user-managed AzString → ImageRef)
NodeType::Image  ┘                 │
                                   ▼
                            ImageRefHash (content hash)
                                   │
                                   ▼
                       RendererResources::currently_registered_images
                                   │  ⟵ azul-allocated ImageKey
                                   ▼
                              WebRender RenderApi
                                   │
                       ┌───────────┴────────────┐
                       ▼                        ▼
              CPU-decoded pixels         External GL texture
              (PNG, JPEG, …)             (gl_texture_cache::TEXTURE_CACHE)
                                                │
                                                ▼  ⟵ ExternalImageId
                                         GpuRender callback
```

Shader caching is parallel infrastructure: WebRender programs are compiled lazily on first use; the binaries are saved to disk via `ShaderDiskCache` so the next launch loads them with `glProgramBinary()` instead of recompiling.

## Three caches, three roles

| cache | location | role | lifetime |
|---|---|---|---|
| `ImageCache` | [`core/src/resources.rs:1115`](../../../../core/src/resources.rs) | maps user CSS `url(...)` strings to `ImageRef` | user-managed |
| `RendererResources::currently_registered_images` | [`core/src/resources.rs:1227`](../../../../core/src/resources.rs) | maps `ImageRefHash` → `(ImageKey, ImageDescriptor)` | per-window, auto-managed |
| `TEXTURE_CACHE` (thread-local) | [`dll/src/desktop/gl_texture_cache.rs`](../../../../dll/src/desktop/gl_texture_cache.rs) | stores live GL `Texture` handles for external image refs | per-window, GL-thread-local |

The user only ever touches `ImageCache`. The other two are bookkeeping driven by layout + frame lifecycle.

## ImageCache

User-facing entry point. Holds `OrderedMap<AzString, ImageRef>`:

```rust,ignore
pub struct ImageCache {
    pub image_id_map: OrderedMap<AzString, ImageRef>,
}

impl ImageCache {
    pub fn add_css_image_id(&mut self, css_id: AzString, image: ImageRef);
    pub fn get_css_image_id(&self, css_id: &AzString) -> Option<&ImageRef>;
    pub fn delete_css_image_id(&mut self, css_id: &AzString);
}
```

Calling `cache.add_css_image_id("logo".into(), image_ref)` makes `background-image: url("logo")` resolve to that `ImageRef` during layout. The map is the *only* lifecycle-managed cache; everything downstream is reference-counted.

`ImageRef` is a refcounted handle to either CPU pixel data, a callback-driven `RawImage` (for procedural images), or a GL `Texture`. `ImageRefHash` is a SipHash of the content — equality of hashes ⟹ equality of pixels, modulo collision risk.

## RendererResources

The per-window resource registry that owns WebRender keys. Backs the `RendererResourcesTrait` ([`core/src/resources.rs:1159`](../../../../core/src/resources.rs)) used by display-list generation:

```rust,ignore
pub trait RendererResourcesTrait: Debug {
    fn get_font_family(&self, h: &StyleFontFamiliesHash) -> Option<&StyleFontFamilyHash>;
    fn get_font_key(&self, h: &StyleFontFamilyHash) -> Option<&FontKey>;
    fn get_registered_font(&self, k: &FontKey)
        -> Option<&(FontRef, OrderedMap<(Au, DpiScaleFactor), FontInstanceKey>)>;
    fn get_image(&self, h: &ImageRefHash) -> Option<&ResolvedImage>;
    fn update_image(&mut self, h: &ImageRefHash, d: ImageDescriptor);
}
```

`get_image(hash)` returns `Some(ResolvedImage { key, descriptor })` if the image is already registered with WebRender. New images go through `add_image` (called from frame setup), which decodes pixel data, allocates a WebRender `ImageKey`, and inserts both into `currently_registered_images`.

GC is automatic: the wrapper `start_frame_gc` / `end_frame_gc` pair around each frame drops keys that no `ImageRef` references anymore. This is what lets users `delete_css_image_id` without manually releasing GPU memory.

## GL texture cache (external images)

[`dll/src/desktop/gl_texture_cache.rs`](../../../../dll/src/desktop/gl_texture_cache.rs) handles textures that azul allocates and hands to WebRender via the **external image API**. Use cases: GPU canvases drawn by user callbacks, video frames, OpenGL widgets.

The cache is thread-local (the GL context is per-thread) and indexed by `DocumentId → ExternalImageId → TextureEntry`:

```rust,ignore
type GlTextureStorage = OrderedMap<ExternalImageId, TextureEntry>;

thread_local! {
    static TEXTURE_CACHE: RefCell<Option<OrderedMap<DocumentId, GlTextureStorage>>>
        = RefCell::new(None);
}

struct TextureEntry {
    texture: Texture,
    epoch: Epoch,
}
```

`ExternalImageId` is the single key WebRender uses for external images. Callers compute it deterministically from whatever stable identity they have:

- DOM-bound textures (per `(DomId, NodeId)`): use `TextureSlotKey::to_external_image_id`, which packs `dom_id << 32 | node_id` into the 64-bit id space
- Hash-bound textures (per `ImageRef`): use `ExternalImageId { inner: hash.inner as u64 }`

```rust,ignore
pub fn insert_texture_for_node(
    document_id: DocumentId,
    dom_id: DomId,
    node_id: NodeId,
    epoch: Epoch,
    texture: Texture,
) -> ExternalImageId
```

The same DOM node always produces the same `ExternalImageId`, so WebRender's cached display lists keep working across frames. If the cache assigned fresh IDs each frame, every cached display list would hold dangling references after the first re-render.

## Why epochs

Textures live for *at least* the current and previous frame:

```rust,ignore
pub fn remove_old_epochs(document_id: &DocumentId, current_epoch: Epoch) {
    // … find entries where entry.epoch < (current_epoch − 1) …
    // … remove them …
}
```

WebRender pipelines a frame ahead, so the GL backend may still be reading texture N while the next frame starts. Holding two frames' worth of textures (current + previous) gives the renderer a safety margin without unbounded growth. The retention rule is implemented in `remove_old_epochs`: anything strictly older than `current_epoch − 1` is freed.

## The naming wart

There are two `GlTextureCache`s and they serve different purposes:

| name | location | role |
|---|---|---|
| module `gl_texture_cache` | `dll/src/desktop/gl_texture_cache.rs` | runtime: live `Texture` handles + GL bindings |
| struct `GlTextureCache` | `core/src/resources.rs:1403` | layout-time: `(ImageKey, ImageDescriptor, ExternalImageId)` triples per `(DomId, NodeId)` |

The core struct is read by display-list generation when emitting `PushImage` items (it tells the layout pass which ImageKey to push without going through the full `RendererResources` lookup). The desktop module is what actually owns the GPU memory.

The duplicate name is documented in the autoreview report as a medium finding ([`doc/target/autoreview/reports/dll__src__desktop__gl_texture_cache.md`](../../../../doc/target/autoreview/reports/dll__src__desktop__gl_texture_cache.md)). A future rename to `gl_texture_store` (runtime) and `GlTextureSolvedCache` (layout) is on the cleanup list. Until then: think of one as *handles* and the other as *bindings*.

## ShaderDiskCache

[`dll/src/desktop/shader_cache.rs`](../../../../dll/src/desktop/shader_cache.rs) implements WebRender's [`ProgramCacheObserver`](../../../../webrender/core/src/device/gl.rs) trait against an on-disk binary cache. After WebRender lazily compiles + links a shader on first use, the binary is extracted via `glGetProgramBinary()` and written to disk. On the next launch, the binary is loaded back via `glProgramBinary()` — usually 100× faster than recompiling.

Cache layout:

```
Linux:    $XDG_CACHE_HOME/azul/shaders/<renderer_hash>/
                or  $HOME/.cache/azul/shaders/<renderer_hash>/
macOS:    $HOME/Library/Caches/azul/shaders/<renderer_hash>/
Windows:  %LOCALAPPDATA%\azul\shaders\<renderer_hash>\
```

Each cached shader is two files:

- `<digest_hex>.bin` — raw program binary bytes
- `<digest_hex>.meta` — 12 bytes: `format` (u32 LE) + `digest` (u64 LE)

`<renderer_hash>` is `DefaultHasher::new(); gl_renderer.hash(); gl_version.hash(); finish()`. When the GPU driver changes (renderer string or version differs), the new launch hashes to a different subdirectory and the cache is effectively invalidated without an explicit purge step.

## ProgramCacheObserver impl

```rust,ignore
impl ProgramCacheObserver for ShaderDiskCache {
    fn save_shaders_to_disk(&self, entries: Vec<Arc<ProgramBinary>>) { /* write *.bin + *.meta */ }
    fn set_startup_shaders(&self, _entries: Vec<Arc<ProgramBinary>>) { /* no-op */ }
    fn try_load_shader_from_disk(&self, digest: &ProgramSourceDigest, cache: &Rc<ProgramCache>);
    fn notify_program_binary_failed(&self, binary: &Arc<ProgramBinary>) { /* delete *.bin + *.meta */ }
}
```

`set_startup_shaders` is a no-op because azul preloads *all* cached binaries at startup via `load_all_from_disk`, not a separate startup-shader list. `notify_program_binary_failed` deletes the corrupt entry so the next launch skips it and falls through to a fresh compile.

## create_program_cache

[`wr_translate2::create_program_cache`](../../../../dll/src/desktop/wr_translate2.rs) at `wr_translate2.rs:132` ties it together:

```rust,ignore
pub fn create_program_cache(
    gl: &Rc<GenericGlContext>,
) -> Option<Rc<webrender::ProgramCache>> {
    let renderer = gl.get_string(RENDERER);
    let version = gl.get_string(VERSION);
    if renderer.is_empty() || version.is_empty() { return None; }

    let observer = ShaderDiskCache::new(&renderer, &version)?;
    let loader   = ShaderDiskCache::new(&renderer, &version)?;
    let cache    = webrender::ProgramCache::new(Some(Box::new(observer)));
    let count    = loader.load_all_from_disk(&cache);
    // … log the count …
    Some(cache)
}
```

Two `ShaderDiskCache` instances point at the same directory: one is moved into the `ProgramCache` as the observer, one is kept on the side as a loader because `ProgramCache::new` takes ownership of its observer. The loader does the bulk preload; the observer handles save/load callbacks for the rest of the session.

`Option<Rc<ProgramCache>>` returns `None` when GL strings are unavailable (e.g. CPU fallback path) or when the cache directory cannot be created (read-only filesystem, missing `$HOME`, etc.). WebRender works fine without it — the cache is purely a startup latency optimization.

## What images look like in the display list

When `display_list::generate_display_list` walks an image node, it emits a `DisplayListItem::PushImage`:

```rust,ignore
DisplayListItem::PushImage {
    rect: LogicalRect,
    image_ref: ImageRef,
    descriptor: ImageDescriptor,
    image_rendering: StyleImageRendering,  // pixelated | crisp-edges | auto
}
```

The compositor (`wr_translate2.rs` for WebRender, `cpurender.rs` for CPU) resolves the `ImageRef` against `RendererResources` to a concrete `ImageKey` and pushes a WebRender `image_display_item`. For external images (GL textures), the `ImageDescriptor::ExternalImage` variant points to a stable `ExternalImageId` that WebRender resolves through the registered `ExternalImageHandler`, which goes back to `gl_texture_cache::get_texture`.

## See also

- [Text Pipeline](text-pipeline.md) — parallel infrastructure for fonts and glyphs (similar ref/key/cache layering)
- [Layout Solver (Flex/Grid)](layout-solver.md) — where `image_cache` is consulted during background-image resolution
- [Rendering Pipeline](rendering.md) — how the display list ends up on screen via WebRender or CPU compositor
