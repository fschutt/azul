//! WebRender type translation functions for shell2
//!
//! This module provides translations between azul-core types and WebRender types,
//! plus hit-testing integration.

use alloc::{collections::BTreeMap, sync::Arc};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Condvar, Mutex},
};

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect},
    hit_test::{DocumentId, PipelineId, TAG_TYPE_SCROLLBAR, TAG_TYPE_SCROLL_CONTAINER},
    resources::{
        AddImage, ImageData as AzImageData, ImageDirtyRect, ImageKey, ImageRef, SyntheticItalics,
        UpdateImage,
    },
    window::{CursorPosition, DebugState},
};
use azul_layout::{
    headless::compute_scroll_child_rect,
    hit_test::FullHitTest,
    managers::gpu_state::{FrameDigest, SubmittedDigest},
    text3::cache::ParsedFontTrait, // For get_hash() method
    window::DomLayoutResult,
};
use webrender::{
    api::{
        units::{
            DeviceIntPoint, DeviceIntRect, DeviceIntSize, DevicePixelScale,
            WorldPoint as WrWorldPoint,
        },
        ApiHitTester as WrApiHitTester, DebugFlags as WrDebugFlags, DirtyRect,
        DocumentId as WrDocumentId, FontInstanceKey as WrFontInstanceKey,
        FontInstanceOptions as WrFontInstanceOptions,
        FontInstancePlatformOptions as WrFontInstancePlatformOptions, FontKey as WrFontKey,
        FontVariation as WrFontVariation, HitTesterRequest as WrHitTesterRequest,
        ImageData as WrImageData, ImageDescriptor as WrImageDescriptor,
        ImageDescriptorFlags as WrImageDescriptorFlags, ImageKey as WrImageKey,
        PipelineId as WrPipelineId, RenderNotifier as WrRenderNotifier,
        RenderReasons as WrRenderReasons, SyntheticItalics as WrSyntheticItalics,
    },
    render_api::{
        AddFontInstance as WrAddFontInstance, AddImage as WrAddImage, UpdateImage as WrUpdateImage,
    },
    WebRenderOptions as WrRendererOptions,
};
// Re-exports for convenience
pub use webrender::{
    render_api::{RenderApi as WrRenderApi, Transaction as WrTransaction},
    Renderer as WrRenderer,
};

use crate::{desktop::shell2::common::debug_server::LogCategory, log_debug, log_info};

/// Asynchronous hit tester that can be in "requested" or "resolved" state
pub enum AsyncHitTester {
    Requested(WrHitTesterRequest),
    Resolved(Arc<dyn WrApiHitTester>),
}

impl AsyncHitTester {
    pub fn resolve(&mut self) -> Arc<dyn WrApiHitTester> {
        match self {
            AsyncHitTester::Resolved(r) => r.clone(),
            AsyncHitTester::Requested(_) => {
                // Safety: we read out the old value and immediately write back the
                // resolved replacement. No intermediate drop can observe invalid state
                // because we hold &mut self (exclusive access) and the write happens
                // before returning.
                let taken = unsafe { core::ptr::read(self) };
                let arc = match taken {
                    AsyncHitTester::Requested(r) => r.resolve(),
                    AsyncHitTester::Resolved(_) => unreachable!(),
                };
                let ret = arc.clone();
                unsafe {
                    core::ptr::write(self, AsyncHitTester::Resolved(arc));
                }
                ret
            }
        }
    }
}

/// Notifier for WebRender to signal when a new frame is ready
#[derive(Clone)]
pub struct Notifier {
    pub new_frame_ready: Arc<(Mutex<bool>, Condvar)>,
    /// Platform wake hook, invoked from WebRender's backend thread whenever a
    /// frame finishes building. Without it the condvar signalled NOBODY on
    /// backends whose loop blocks in the OS wait (WaitMessage / poll):
    /// the final frame of an interaction stayed unpresented until the next
    /// input event. X11/Wayland write an eventfd in their poll set; Win32
    /// posts a message; macOS's display cycle consumes the flag.
    pub wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl WrRenderNotifier for Notifier {
    fn clone(&self) -> Box<dyn WrRenderNotifier> {
        Box::new(Notifier {
            new_frame_ready: self.new_frame_ready.clone(),
            wake: self.wake.clone(),
        })
    }

    fn wake_up(&self, _composite_needed: bool) {
        // Signal that something happened (non-frame-generating update)
        let (lock, cvar) = &*self.new_frame_ready;
        let mut new_frame_ready = lock.lock().unwrap();
        *new_frame_ready = true;
        cvar.notify_one();
    }

    fn new_frame_ready(
        &self,
        _doc_id: WrDocumentId,
        _scrolled: bool,
        _composite_needed: bool,
        _frame_publish_id: webrender::api::FramePublishId,
    ) {
        // Signal that a new frame is ready to be rendered
        log_debug!(
            LogCategory::Rendering,
            "[Notifier] new_frame_ready called - signaling main thread {_doc_id:?} _scrolled: \
             {_scrolled:?} _composite_needed: {_composite_needed:?} _frame_publish_id: \
             {_frame_publish_id:?}"
        );
        let (lock, cvar) = &*self.new_frame_ready;
        let mut new_frame_ready = lock.lock().unwrap();
        *new_frame_ready = true;
        cvar.notify_one();
        drop(new_frame_ready);
        if let Some(wake) = &self.wake {
            wake();
        }
    }
}

/// Create a ProgramCache backed by an on-disk shader cache.
///
/// Queries the GL context for renderer + version strings, creates a
/// `ShaderDiskCache` keyed on that info, loads any previously-cached
/// binaries, and returns the `Rc<ProgramCache>` ready to be passed
/// into `WebRenderOptions.cached_programs`.
///
/// Returns `None` if the cache directory cannot be created or GL
/// strings are unavailable (e.g. CPU fallback).
pub fn create_program_cache(
    gl: &Rc<azul_core::gl::GenericGlContext>,
) -> Option<Rc<webrender::ProgramCache>> {
    use crate::desktop::shader_cache::ShaderDiskCache;

    let renderer_name = gl.get_string(azul_core::gl::RENDERER);
    let version_string = gl.get_string(azul_core::gl::VERSION);

    if renderer_name.is_empty() || version_string.is_empty() {
        return None;
    }

    let disk_cache = ShaderDiskCache::new(&renderer_name, &version_string)?;

    // Create a second ShaderDiskCache pointing at the same directory
    // so we can pre-load cached binaries before the observer is moved
    // into the ProgramCache (which takes ownership).
    let loader = ShaderDiskCache::new(&renderer_name, &version_string)?;

    // Create ProgramCache with the disk observer
    let program_cache = webrender::ProgramCache::new(Some(Box::new(disk_cache)));

    // Pre-load any existing cached binaries from disk
    let count = loader.load_all_from_disk(&program_cache);
    if count > 0 {
        log_info!(
            LogCategory::Rendering,
            "[Shader Cache] Loaded {} cached shader binaries from disk",
            count
        );
    }

    Some(program_cache)
}

/// Shared cell that receives WebRender's buffer-age-widened TOTAL damage
/// region for the frame being rendered (current frame's dirty rect unioned
/// with the dirty rects of the previous `buffer_age - 1` frames, from WR's
/// internal `BufferDamageTracker`). WR writes it via the
/// [`webrender::PartialPresentCompositor`] hook immediately before
/// compositing to the main framebuffer; the platform presenter `take()`s it
/// after `renderer.render(size, buffer_age)` and passes it to
/// `eglSwapBuffersWithDamage[KHR|EXT]`.
///
/// Rects are physical device px, top-left origin (the EGL presenter must
/// y-flip them to bottom-left origin). `take()` returning `None` means WR
/// did not report a partial region this frame — present in FULL (per
/// DAMAGE_REGION_PLAN §3: "PresentDamage must never silently become ∅").
#[derive(Clone, Default)]
pub struct PartialPresentDamage {
    inner: Arc<Mutex<Option<Vec<DeviceIntRect>>>>,
}

impl PartialPresentDamage {
    /// Take the total damage region WR reported for the last rendered frame.
    pub fn take(&self) -> Option<Vec<DeviceIntRect>> {
        self.inner.lock().ok()?.take()
    }
}

impl webrender::PartialPresentCompositor for PartialPresentDamage {
    fn set_buffer_damage_region(&mut self, rects: &[DeviceIntRect]) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(rects.to_vec());
        }
    }
}

/// Clamp WebRender device rects (physical px, TOP-LEFT origin, i32) to a
/// `fb_w`×`fb_h` framebuffer and convert to `(x, y, w, h)` u32 present
/// rects for swap-with-damage. Degenerate/off-buffer rects are dropped — an
/// empty result must be treated as FULL by the presenter (present damage
/// never silently shrinks to ∅, DAMAGE_REGION_PLAN §3).
pub fn device_rects_to_present_rects(
    rects: &[DeviceIntRect],
    fb_w: u32,
    fb_h: u32,
) -> Vec<(u32, u32, u32, u32)> {
    rects
        .iter()
        .filter_map(|r| {
            let x0 = r.min.x.clamp(0, fb_w as i32);
            let y0 = r.min.y.clamp(0, fb_h as i32);
            let x1 = r.max.x.clamp(0, fb_w as i32);
            let y1 = r.max.y.clamp(0, fb_h as i32);
            if x1 > x0 && y1 > y0 {
                Some((x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
            } else {
                None
            }
        })
        .collect()
}

/// Default WebRender renderer options
///
/// `partial_present`: pass `Some(cell)` on backends that query the back
/// buffer's age (EGL_EXT_buffer_age) and feed it into
/// `renderer.render(size, buffer_age)`. WR then accumulates dirty regions
/// over the buffer's age (`draw_previous_partial_present_regions`), scissors
/// its composite to that total region, and reports it through the cell for
/// swap-with-damage. Backends without buffer-age support pass `None` and get
/// today's behavior unchanged.
/// WebRender's `TextureCacheConfig::DEFAULT` sizes its shared atlases for a
/// desktop-browser workload: 2048² color-linear + 2048² glyph + 2048²
/// alpha-glyph slices, so the first glyph/image of each format reserves a
/// 16 MB (RGBA) or 4 MB (alpha) atlas slice — ~37 MB of GPU/driver memory for
/// even a hello-world window (audit §3.3b). Azul passes no config, so it
/// inherits that. This picks smaller slice dimensions (~10 MB ceiling, and
/// lazily grown — WR allocates additional shared textures on demand, so
/// steady-state is far less). `AZ_WR_ATLAS_SIZE` overrides the large-atlas
/// dimension: 2048 restores WR's default (for heavy scroll/zoom atlas churn),
/// 512 gives a mobile profile. Clamped to [512, 4096] (WR requires ≥512).
///
/// The atlas size is also what WebRender derives its EVICTION threshold from:
/// a third of one atlas (1.4 MB of images at 1024 px), two thirds for glyphs
/// (0.7 MB). Above it, whatever the last frames did not draw is evicted, the
/// harder the fuller - at 7 thresholds after two frames. A window that shows
/// more than that (a map on a 2x display: 44 tiles of 1 MB, 1,567 halo
/// pictures of 28 KB, 2 to 4 MB of glyphs) lost, with every frame that redrew
/// only a part of it (a tile, a strip of a pan, a hover), the images and
/// glyphs of the rest, and the next frame that drew all of it - the first step
/// of a zoom - uploaded some 90 MB and rasterised every glyph again (audit
/// A4-30). azul deletes the images its lists no longer name itself, so what is
/// in the cache is what the window shows: it is kept up to a budget,
/// `AZ_WR_IMAGE_BUDGET_MB` (256) and `AZ_WR_GLYPH_BUDGET_MB` (16); 0 gives
/// WebRender's rule. A budget is a bound on what is KEPT, nothing is allocated
/// for it.
fn azul_texture_cache_config() -> webrender::TextureCacheConfig {
    let big = std::env::var("AZ_WR_ATLAS_SIZE")
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .map(|v| v.clamp(512, 4096))
        .unwrap_or(1024);
    let megabytes = |name: &str, default: usize| -> usize {
        std::env::var(name)
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(default)
            .min(1024)
            * 1024
            * 1024
    };
    webrender::TextureCacheConfig {
        color8_linear_texture_size: big,
        color8_nearest_texture_size: 512,
        color8_glyph_texture_size: big,
        alpha8_texture_size: 512,
        alpha8_glyph_texture_size: big,
        alpha16_texture_size: 512,
        image_budget_bytes: megabytes("AZ_WR_IMAGE_BUDGET_MB", 256),
        glyph_budget_bytes: megabytes("AZ_WR_GLYPH_BUDGET_MB", 16),
    }
}

pub fn default_renderer_options(
    options: &azul_layout::window_state::WindowCreateOptions,
    cached_programs: Option<Rc<webrender::ProgramCache>>,
    partial_present: Option<PartialPresentDamage>,
) -> WrRendererOptions {
    use azul_core::window::WindowBackgroundMaterial;
    use webrender::ShaderPrecacheFlags;

    // THE clear colour (`common::window_clear_color`), as far as it is known
    // before the window exists: a material clears to transparent so the
    // effect shows through; an opaque window to its `background_color`, which
    // every GPU shell has seeded for the mode it will show
    // (`resolve_initial_background_color`) by now. This is only the first
    // frame's: `CommonWindowState::sync_renderer_clear_color` keeps the
    // renderer on the same function from then on, so a mode change moves it.
    let bg = crate::desktop::shell2::common::window_clear_color(
        options.window_state.background_color,
        options.window_state.mode,
        None,
        false,
        !matches!(
            options.window_state.flags.background_material,
            WindowBackgroundMaterial::Opaque
        ),
    );

    WrRendererOptions {
        resource_override_path: None,
        use_optimized_shaders: true,
        enable_aa: true,
        enable_subpixel_aa: true,
        clear_color: wr_translate_color_f(CssColorF::from(bg)),
        enable_multithreading: false,
        // AZ_OVERLAY is merged in, so the webrender verbs (overdraw, profiler,
        // primitives, ...) can be switched on from the environment exactly like
        // the hit-test overlay — without a rebuild, and in release builds too.
        // The window's own DebugState still wins where it asked for something.
        debug_flags: wr_translate_debug_flags(&merge_overlay_env(options.window_state.debug_state)),
        // Shader precaching: use EMPTY to avoid blocking startup with full compilation.
        // Shaders will be compiled on-demand when first needed by the renderer.
        // When a disk cache is present, shaders are loaded via glProgramBinary
        // which is near-instant (<1ms per shader).
        precache_flags: ShaderPrecacheFlags::EMPTY,
        cached_programs,
        // Partial present composites only the dirty rect into the back buffer,
        // so it needs to know what that buffer still holds. With a
        // `partial_present` cell (EGL backends with buffer-age),
        // `draw_previous_partial_present_regions: true` makes WR union the
        // current dirty rect with the tracked damage of the previous
        // `buffer_age - 1` frames. Unknown age (0) or age beyond WR's 4-frame
        // history degrades to Full.
        //
        // Without one (WGL, CGL) the back buffer is undefined after a swap, and
        // drawing only the dirty rect presents stale or black pixels everywhere
        // else: the GPU backends flickered. Those compose every frame in full.
        compositor_config: webrender::CompositorConfig::Draw {
            max_partial_present_rects: if partial_present.is_some() { 1 } else { 0 },
            draw_previous_partial_present_regions: partial_present.is_some(),
            partial_present: partial_present
                .map(|p| Box::new(p) as Box<dyn webrender::PartialPresentCompositor>),
        },
        // Smaller shared-atlas slices than WR's browser-tuned default (−~27 MB
        // GPU memory on a hello-world window). See azul_texture_cache_config.
        texture_cache_config: azul_texture_cache_config(),
        ..WrRendererOptions::default()
    }
}

/// Compositor for external image handling (textures, etc.)
///
/// This allows WebRender to use externally-managed OpenGL textures (e.g., from ImageCallbacks)
/// by looking them up in the global texture cache.
#[derive(Debug, Default, Copy, Clone)]
pub struct Compositor {}

impl webrender::api::ExternalImageHandler for Compositor {
    fn lock(
        &mut self,
        key: webrender::api::ExternalImageId,
        _channel_index: u8,
    ) -> webrender::api::ExternalImage<'_> {
        use azul_core::resources::ExternalImageId;
        use webrender::api::{
            units::{DevicePoint as WrDevicePoint, TexelRect as WrTexelRect},
            ExternalImage as WrExternalImage, ExternalImageSource as WrExternalImageSource,
        };

        // Convert WebRender's external image ID to our type
        let external_image_id = ExternalImageId { inner: key.0 };

        // Look up the texture in the global cache
        match crate::desktop::gl_texture_cache::get_texture(&external_image_id) {
            Some((texture_id, (width, height))) => {
                // Return the native OpenGL texture
                WrExternalImage {
                    uv: WrTexelRect {
                        uv0: WrDevicePoint::zero(),
                        uv1: WrDevicePoint::new(width, height),
                    },
                    source: WrExternalImageSource::NativeTexture(texture_id),
                }
            }
            None => {
                // Texture not found, return invalid
                WrExternalImage {
                    uv: WrTexelRect {
                        uv0: WrDevicePoint::zero(),
                        uv1: WrDevicePoint::zero(),
                    },
                    source: WrExternalImageSource::Invalid,
                }
            }
        }
    }

    fn unlock(&mut self, _key: webrender::api::ExternalImageId, _channel_index: u8) {
        // Single-threaded renderer, nothing to unlock
        // Textures are managed by the global cache with refcounting
    }
}

/// OR the `AZ_OVERLAY` environment verbs into a window's `DebugState`.
///
/// Read once per process — see `compositor2::show_hit_test_overlay` for the same
/// reasoning. Union rather than override: a program that deliberately enabled a
/// flag keeps it, and the environment can only add.
fn merge_overlay_env(mut state: DebugState) -> DebugState {
    static ENV: std::sync::OnceLock<DebugState> = std::sync::OnceLock::new();
    let env = ENV.get_or_init(DebugState::from_az_overlay_env);

    macro_rules! or_in {
        ($($f:ident),* $(,)?) => { $( state.$f = state.$f || env.$f; )* };
    }
    or_in!(
        profiler_dbg,
        render_target_dbg,
        texture_cache_dbg,
        gpu_time_queries,
        gpu_sample_queries,
        disable_batching,
        epochs,
        echo_driver_messages,
        show_overdraw,
        gpu_cache_dbg,
        texture_cache_dbg_clear_evicted,
        picture_caching_dbg,
        primitive_dbg,
        zoom_dbg,
        small_screen,
        disable_opaque_pass,
        disable_alpha_pass,
        disable_clip_masks,
        disable_text_prims,
        disable_gradient_prims,
        obscure_images,
        glyph_flashing,
        smart_profiler,
        invalidation_dbg,
        tile_cache_logging_dbg,
        profiler_capture,
        force_picture_invalidation,
    );
    state
}

pub fn wr_translate_debug_flags(new_flags: &DebugState) -> WrDebugFlags {
    let mut debug_flags = WrDebugFlags::empty();

    debug_flags.set(WrDebugFlags::PROFILER_DBG, new_flags.profiler_dbg);
    debug_flags.set(WrDebugFlags::RENDER_TARGET_DBG, new_flags.render_target_dbg);
    debug_flags.set(WrDebugFlags::TEXTURE_CACHE_DBG, new_flags.texture_cache_dbg);
    debug_flags.set(WrDebugFlags::GPU_TIME_QUERIES, new_flags.gpu_time_queries);
    debug_flags.set(
        WrDebugFlags::GPU_SAMPLE_QUERIES,
        new_flags.gpu_sample_queries,
    );
    debug_flags.set(WrDebugFlags::DISABLE_BATCHING, new_flags.disable_batching);
    debug_flags.set(WrDebugFlags::EPOCHS, new_flags.epochs);
    debug_flags.set(
        WrDebugFlags::ECHO_DRIVER_MESSAGES,
        new_flags.echo_driver_messages,
    );
    debug_flags.set(WrDebugFlags::SHOW_OVERDRAW, new_flags.show_overdraw);
    debug_flags.set(WrDebugFlags::GPU_CACHE_DBG, new_flags.gpu_cache_dbg);
    debug_flags.set(
        WrDebugFlags::TEXTURE_CACHE_DBG_CLEAR_EVICTED,
        new_flags.texture_cache_dbg_clear_evicted,
    );
    debug_flags.set(
        WrDebugFlags::PICTURE_CACHING_DBG,
        new_flags.picture_caching_dbg,
    );
    debug_flags.set(WrDebugFlags::PRIMITIVE_DBG, new_flags.primitive_dbg);
    debug_flags.set(WrDebugFlags::ZOOM_DBG, new_flags.zoom_dbg);
    debug_flags.set(WrDebugFlags::SMALL_SCREEN, new_flags.small_screen);
    debug_flags.set(
        WrDebugFlags::DISABLE_OPAQUE_PASS,
        new_flags.disable_opaque_pass,
    );
    debug_flags.set(
        WrDebugFlags::DISABLE_ALPHA_PASS,
        new_flags.disable_alpha_pass,
    );
    debug_flags.set(
        WrDebugFlags::DISABLE_CLIP_MASKS,
        new_flags.disable_clip_masks,
    );
    debug_flags.set(
        WrDebugFlags::DISABLE_TEXT_PRIMS,
        new_flags.disable_text_prims,
    );
    debug_flags.set(
        WrDebugFlags::DISABLE_GRADIENT_PRIMS,
        new_flags.disable_gradient_prims,
    );
    debug_flags.set(WrDebugFlags::OBSCURE_IMAGES, new_flags.obscure_images);
    debug_flags.set(WrDebugFlags::GLYPH_FLASHING, new_flags.glyph_flashing);
    debug_flags.set(WrDebugFlags::SMART_PROFILER, new_flags.smart_profiler);
    debug_flags.set(WrDebugFlags::INVALIDATION_DBG, new_flags.invalidation_dbg);
    // Note: TILE_CACHE flag doesn't exist in this WebRender version
    // debug_flags.set(WrDebugFlags::TILE_CACHE, new_flags.tile_cache_logging_dbg);
    debug_flags.set(WrDebugFlags::PROFILER_CAPTURE, new_flags.profiler_capture);
    debug_flags.set(
        WrDebugFlags::FORCE_PICTURE_INVALIDATION,
        new_flags.force_picture_invalidation,
    );

    debug_flags
}

/// Translate DocumentId from azul-core to WebRender
pub fn wr_translate_document_id(document_id: DocumentId) -> WrDocumentId {
    WrDocumentId {
        namespace_id: webrender::api::IdNamespace(document_id.namespace_id.0),
        id: document_id.id,
    }
}

/// Translate DocumentId from WebRender to azul-core
pub fn translate_document_id_wr(document_id: WrDocumentId) -> DocumentId {
    DocumentId {
        namespace_id: azul_core::resources::IdNamespace(document_id.namespace_id.0),
        id: document_id.id,
    }
}

/// Translate IdNamespace from WebRender to azul-core
pub fn translate_id_namespace_wr(
    id_namespace: webrender::api::IdNamespace,
) -> azul_core::resources::IdNamespace {
    azul_core::resources::IdNamespace(id_namespace.0)
}

/// Translate PipelineId from azul-core to WebRender
/// Whether the renderer's current frame holds the document `document_id`:
/// WebRender builds a frame after the pass that sent its display list and
/// publishes the document's epoch with it, so until a later
/// `Renderer::update` took that frame in, a render shows only the clear
/// colour.
pub fn renderer_shows_document(
    renderer: &webrender::Renderer,
    document_id: DocumentId,
) -> bool {
    renderer
        .current_epoch(
            wr_translate_document_id(document_id),
            wr_translate_pipeline_id(PipelineId(0, document_id.id)),
        )
        .is_some()
}

pub fn wr_translate_pipeline_id(pipeline_id: PipelineId) -> WrPipelineId {
    WrPipelineId(pipeline_id.0, pipeline_id.1)
}

/// Translate ExternalScrollId from azul-core to WebRender
pub fn wr_translate_external_scroll_id(
    scroll_id: azul_core::hit_test::ExternalScrollId,
) -> webrender::api::ExternalScrollId {
    webrender::api::ExternalScrollId(scroll_id.0, wr_translate_pipeline_id(scroll_id.1))
}

/// Translate LogicalPosition from azul-core to WebRender LayoutPoint
fn wr_translate_logical_position(
    pos: azul_core::geom::LogicalPosition,
) -> webrender::api::units::LayoutPoint {
    webrender::api::units::LayoutPoint::new(pos.x, pos.y)
}

/// Translate physical position to WebRender WorldPoint
/// Used for hit-testing with physical coordinates
pub fn translate_world_point(
    pos: azul_core::geom::PhysicalPosition<f32>,
) -> webrender::api::units::WorldPoint {
    webrender::api::units::WorldPoint::new(pos.x, pos.y)
}

/// Translate ScrollbarHitId to WebRender ItemTag
///
/// Encoding scheme using the type-safe hit_test_tag system:
/// - tag.0 (u64): DomId (upper 32 bits) | NodeId (lower 32 bits)
/// - tag.1 (u16): TAG_TYPE_SCROLLBAR (0x0200) | component type (lower byte)
///   - 0x0200 = Vertical Track
///   - 0x0201 = Vertical Thumb
///   - 0x0202 = Horizontal Track
///   - 0x0203 = Horizontal Thumb
pub fn wr_translate_scrollbar_hit_id(
    hit_id: azul_core::hit_test::ScrollbarHitId,
) -> (webrender::api::ItemTag, webrender::api::units::LayoutPoint) {
    use azul_core::hit_test::ScrollbarHitId;

    let (dom_id, node_id, component_type) = match hit_id {
        ScrollbarHitId::VerticalTrack(dom_id, node_id) => (dom_id, node_id, 0u16),
        ScrollbarHitId::VerticalThumb(dom_id, node_id) => (dom_id, node_id, 1u16),
        ScrollbarHitId::HorizontalTrack(dom_id, node_id) => (dom_id, node_id, 2u16),
        ScrollbarHitId::HorizontalThumb(dom_id, node_id) => (dom_id, node_id, 3u16),
    };

    // tag.0 = DomId (upper 32 bits) | NodeId (lower 32 bits)
    let tag_value = ((dom_id.inner as u64) << 32) | (node_id.index() as u64);
    // tag.1 = TAG_TYPE_SCROLLBAR | component type
    let tag_type = TAG_TYPE_SCROLLBAR | component_type;

    // Return tag as (u64, u16) tuple
    (
        (tag_value, tag_type),
        webrender::api::units::LayoutPoint::zero(),
    )
}

/// Perform WebRender-based hit testing
///
/// This is the main hit-testing function that uses WebRender's hit tester to determine
/// which DOM nodes are under the cursor. It handles nested virtualized views and builds a complete
/// hit test result with all hovered nodes.
/// Convert CPU hit test results to `FullHitTest` format.
///
/// Thin delegation to [`azul_layout::headless::convert_cpu_hit_test_to_full`],
/// which is where the implementation lives so that the desktop shells and the
/// headless E2E runner resolve "which node did the pointer land on" through the
/// SAME code. Keeping a second copy here is how the two hosts would drift.
pub fn convert_cpu_hit_test_to_full(
    tester: &azul_layout::headless::CpuHitTester,
    hits: &[(DomId, NodeId, azul_core::geom::LogicalPosition)],
    old_focus_node: Option<DomNodeId>,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    cursor_position: azul_core::geom::LogicalPosition,
    resolve_scroll: &dyn Fn(DomId, NodeId) -> Option<azul_core::geom::LogicalPosition>,
    resolve_transform: &dyn Fn(DomId, NodeId) -> Option<azul_core::transform::ComputedTransform3D>,
) -> FullHitTest {
    azul_layout::headless::convert_cpu_hit_test_to_full(
        tester,
        hits,
        old_focus_node,
        layout_results,
        cursor_position,
        resolve_scroll,
        resolve_transform,
    )
}
// `fullhittest_new_webrender` lived here: a SECOND hit-test producer, used
// whenever a backend had a WebRender tester. It disagreed with the layout-based
// one (different coordinate space, off by padding + border, and neither added
// the box's own scroll), so which node a click resolved to depended on the
// render backend. Hit testing now goes through `CommonWindowState::
// perform_hit_test` on every backend and render mode, and this had no callers
// left.

// Display list translation and resource management

use std::collections::{HashMap, HashSet};

use azul_core::resources::{
    AddFont, AddFontInstance, Au, DpiScaleFactor, FontKey, ImageCache, ResourceUpdate,
};
use azul_layout::window::LayoutWindow;

// ==== What the GPU path does for a frame, counted (audit A4) ====

use azul_layout::window::GpuFrameCounters;

#[cfg(test)]
thread_local! {
    /// Tests: the bound values `synchronize_gpu_values` walked on this thread for its own
    /// answer (the walk a debug build makes to check a skipped walk is not counted).
    pub(crate) static BOUND_VALUES_WALKED: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

thread_local! {
    /// What the GPU path did on this thread since the last frame was published
    /// ([`publish_gpu_frame`]): the UI thread's part of a frame's
    /// [`GpuFrameCounters`].
    static GPU_FRAME: std::cell::Cell<GpuFrameCounters> =
        std::cell::Cell::new(GpuFrameCounters::default());
    /// A frame was asked of the renderer and its counters are not published yet.
    static GPU_FRAME_OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The renderer's process-wide counters as of the last published frame.
    static WR_COUNTERS_SEEN: std::cell::Cell<Option<webrender::azul_counters::Counters>> =
        const { std::cell::Cell::new(None) };
}

/// Count into the frame this thread is preparing.
pub(crate) fn gpu_count(f: impl FnOnce(&mut GpuFrameCounters)) {
    GPU_FRAME.with(|cell| {
        let mut frame = cell.get();
        f(&mut frame);
        cell.set(frame);
    });
}

/// Microseconds since `started`.
pub(crate) fn elapsed_us(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Which time of [`GpuFrameCounters`] a [`GpuTimer`] adds to.
#[derive(Debug, Clone, Copy)]
pub(crate) enum GpuTime {
    Translate,
    Resources,
    Values,
    Transaction,
}

/// Adds the time it lived to one of the frame's times.
pub(crate) struct GpuTimer(std::time::Instant, GpuTime);

/// Time what follows, until the value is dropped, as `which`.
#[must_use]
pub(crate) fn gpu_time(which: GpuTime) -> GpuTimer {
    GpuTimer(std::time::Instant::now(), which)
}

impl Drop for GpuTimer {
    fn drop(&mut self) {
        let us = elapsed_us(self.0);
        let which = self.1;
        gpu_count(|c| {
            let slot = match which {
                GpuTime::Translate => &mut c.translate_us,
                GpuTime::Resources => &mut c.resources_us,
                GpuTime::Values => &mut c.values_us,
                GpuTime::Transaction => &mut c.transaction_us,
            };
            *slot = slot.saturating_add(us);
        });
    }
}

/// The bytes of an image a resource update hands to the renderer: the whole
/// image, or the dirty part of an update. A texture the renderer only refers
/// to (an external image) has none.
fn resource_image_bytes(
    descriptor: &WrImageDescriptor,
    data: &WrImageData,
    dirty: Option<&DirtyRect<i32, webrender::api::units::DevicePixel>>,
) -> u64 {
    if !matches!(data, WrImageData::Raw(_)) {
        return 0;
    }
    let (w, h) = match dirty {
        Some(DirtyRect::Partial(rect)) => (
            rect.width().clamp(0, descriptor.size.width.max(0)),
            rect.height().clamp(0, descriptor.size.height.max(0)),
        ),
        _ => (descriptor.size.width, descriptor.size.height),
    };
    let px = u64::try_from(w).unwrap_or(0) * u64::try_from(h).unwrap_or(0);
    px * u64::try_from(descriptor.format.bytes_per_pixel()).unwrap_or(0)
}

/// `txn` is about to be sent: it is one transaction of the frame, and its
/// resource updates are counted by kind. Called by whoever sends it.
pub fn count_transaction(txn: &WrTransaction) {
    use webrender::ResourceUpdate as U;

    let (mut adds, mut updates, mut deletes, mut bytes) = (0u64, 0u64, 0u64, 0u64);
    for update in &txn.resource_updates {
        match update {
            U::AddImage(image) => {
                adds += 1;
                bytes += resource_image_bytes(&image.descriptor, &image.data, None);
            }
            U::UpdateImage(image) => {
                updates += 1;
                bytes +=
                    resource_image_bytes(&image.descriptor, &image.data, Some(&image.dirty_rect));
            }
            U::AddBlobImage(..) | U::AddFont(..) | U::AddFontInstance(..) => adds += 1,
            U::UpdateBlobImage(..) | U::SetBlobImageVisibleArea(..) => updates += 1,
            U::DeleteImage(..)
            | U::DeleteBlobImage(..)
            | U::DeleteFont(..)
            | U::DeleteFontInstance(..) => deletes += 1,
        }
    }
    gpu_count(|c| {
        c.transactions += 1;
        c.resource_adds += adds;
        c.resource_updates += updates;
        c.resource_deletes += deletes;
        c.resource_bytes += bytes;
    });
}

/// A transaction asked the renderer for a frame: its counters are owed.
fn gpu_frame_requested() {
    GPU_FRAME_OPEN.with(|open| open.set(true));
}

/// The frame was drawn: what this thread counted for it, with what the
/// renderer's threads did since the last published frame, becomes the
/// window's last GPU frame (`FrameClock::note_gpu`: the frame report's
/// `gpu_last`, `FrameTimings::gpu_*`). `blocked_us`: how long the UI thread
/// waited for the renderer's threads; `render_us`: how long the draw took.
///
/// Called by a shell right after `renderer.render`. A shell that does not call
/// it has its frames published late and without the two times
/// ([`publish_stale_gpu_frame`]).
pub fn publish_gpu_frame(
    layout_window: &LayoutWindow,
    blocked_us: u64,
    render_us: u64,
    draw_calls: u64,
) {
    GPU_FRAME_OPEN.with(|open| open.set(false));
    let mut frame = GPU_FRAME.with(|cell| cell.replace(GpuFrameCounters::default()));
    let now = webrender::azul_counters::snapshot();
    let before = WR_COUNTERS_SEEN
        .with(|seen| seen.replace(Some(now)))
        .unwrap_or_default();
    let wr = now.since(&before);
    frame.scene_builds = wr.scene_builds;
    frame.frame_builds = wr.frame_builds;
    frame.glyphs_rasterized = wr.glyphs_rasterized;
    frame.texture_uploads = wr.texture_uploads;
    frame.texture_upload_px = wr.texture_upload_px;
    frame.scene_build_us = wr.scene_build_ns / 1_000;
    frame.frame_build_us = wr.frame_build_ns / 1_000;
    frame.tiles_drawn = wr.tiles_drawn;
    frame.blocked_us = blocked_us;
    frame.render_us = render_us;
    frame.draw_calls = draw_calls;
    layout_window.frame_clock.note_gpu(&frame);
}

/// The transaction a pass just sent is not drawn by that pass: its frame is
/// drawn when the renderer has built it, by a later pass (audit A4-31,
/// `shell2::common::frame_schedule`). What this thread counted for it stays
/// counted and is published with the frame that shows it
/// ([`publish_gpu_frame`]); until then the builders of the passes in between
/// do not publish it as a frame of its own ([`publish_stale_gpu_frame`]).
pub fn gpu_frame_is_drawn_later() {
    GPU_FRAME_OPEN.with(|open| open.set(false));
}

/// Publish the counters of a frame whose shell did not ([`publish_gpu_frame`])
/// before the next transaction is built, so they are not added to the next
/// frame's.
fn publish_stale_gpu_frame(layout_window: &LayoutWindow) {
    if GPU_FRAME_OPEN.with(std::cell::Cell::get) {
        publish_gpu_frame(layout_window, 0, 0, 0);
    }
}

/// Generate FontKey deterministically from font hash
/// This ensures the same font always gets the same key across frames
fn font_key_from_hash(font_hash: u64) -> FontKey {
    // Derive a non-zero namespace (WebRender requirement) from the upper
    // 32 bits of the hash; keep the full hash as the `u64` key.
    let namespace = (font_hash >> 32) as u32;
    let namespace = if namespace == 0 { 1 } else { namespace };

    FontKey {
        namespace: azul_core::resources::IdNamespace(namespace),
        key: font_hash,
    }
}

/// Text halos a frame draws however long they take (see
/// [`collect_image_resource_updates`]): a frame always makes progress.
const HALOS_DRAWN_ANYWAY: usize = 2;
/// How long a frame goes on drawing text halos after those (the time it spends
/// drawing them, `cpurender::within_halo_budget`).
const HALO_FRAME_BUDGET: std::time::Duration = std::time::Duration::from_millis(12);
/// The most frames in a row that are made only to draw the text halos the scan
/// before them left (a quarter of a minute at 60 frames a second, and every one
/// draws at least [`HALOS_DRAWN_ANYWAY`]). A backstop: a scan drops no halo it
/// uses (audit A5-01), so each follow-up leaves fewer than the one before and the
/// chain ends by itself; a halo that cannot be kept is not left, and owes no frame
/// but the one whose scan may drop the frame before's pictures (audit A5-19).
const HALO_FOLLOW_UPS_MAX: u32 = 900;

#[cfg(test)]
thread_local! {
    /// A test's own halo budget, for the thread it runs on ([`halo_budget`]).
    static TEST_HALO_BUDGET: std::cell::Cell<Option<(usize, std::time::Duration)>> =
        const { std::cell::Cell::new(None) };
}

/// What a frame may spend on drawing text halos: how many it draws however long
/// they take, and for how long it goes on after those.
fn halo_budget() -> (usize, std::time::Duration) {
    #[cfg(test)]
    if let Some(budget) = TEST_HALO_BUDGET.with(std::cell::Cell::get) {
        return budget;
    }
    (HALOS_DRAWN_ANYWAY, HALO_FRAME_BUDGET)
}

/// For a test that looks at the halos of a frame, not at how a frame spends its
/// time: every frame of this thread draws every halo it finds.
#[cfg(test)]
pub(crate) fn draw_every_halo_in_this_thread() {
    TEST_HALO_BUDGET.with(|b| b.set(Some((usize::MAX, std::time::Duration::from_secs(3600)))));
}

/// For a test of the frames that follow a frame that left halos: every frame of
/// this thread draws `free` new halos and no more, however fast the machine is.
#[cfg(test)]
pub(crate) fn draw_this_many_halos_a_frame_in_this_thread(free: usize) {
    TEST_HALO_BUDGET.with(|b| b.set(Some((free, std::time::Duration::ZERO))));
}

/// Collects all ImageRefs from display lists and creates AddImage ResourceUpdates
/// for images that aren't already registered.
///
/// Unlike fonts, ImageKeys are generated directly from ImageRefHash using
/// image_ref_hash_to_image_key(), so no separate mapping table is needed.
pub fn collect_image_resource_updates(
    layout_window: &LayoutWindow,
    renderer_resources: &azul_core::resources::RendererResources,
) -> (
    Vec<(
        azul_core::resources::ImageRefHash,
        azul_core::resources::AddImageMsg,
    )>,
    // The full set of image hashes referenced by ANY display list this frame —
    // the authoritative "live set" for image GC (a superset of the returned
    // AddImage messages, which only cover *newly*-seen images).
    azul_core::FastBTreeSet<azul_core::resources::ImageRefHash>,
) {
    let (updates, live, _halos) =
        collect_image_resource_updates_and_halos(layout_window, renderer_resources);
    (updates, live)
}

/// What a display list names of fonts and images ([`list_names`]).
#[derive(Debug)]
pub(crate) struct ListNames {
    /// Every (font, size) a text item of the list is set in, once.
    pub(crate) fonts: Vec<(u64, Au)>,
    /// The indices of the items that draw or clip by an image
    /// (`LayoutWindow::image_of_item`), in list order.
    pub(crate) image_items: Vec<usize>,
}

/// The [`ListNames`] kept for one display list ([`LIST_NAMES`]).
struct KeptNames {
    list: std::sync::Weak<azul_layout::solver3::display_list::DisplayList>,
    names: std::rc::Rc<ListNames>,
}

thread_local! {
    /// [`ListNames`] by the list's allocation. The `Weak` is what makes the
    /// address an identity: while it is held the allocation is not given to
    /// another list, and a list that has a `Weak` is never changed in place
    /// (`Arc::get_mut` refuses, `Arc::make_mut` moves it to a new allocation):
    /// the same address with a living list is the same items.
    static LIST_NAMES: RefCell<Vec<KeptNames>> = const { RefCell::new(Vec::new()) };
    /// Tests: how many lists [`list_names`] walked on this thread.
    #[cfg(test)]
    static LIST_NAME_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Tests: how many lists [`list_names`] has walked on this thread.
#[cfg(test)]
pub(crate) fn list_name_walks() -> usize {
    LIST_NAME_WALKS.with(std::cell::Cell::get)
}

/// The fonts and the image items of `list`: one walk of its items, kept while
/// the list lives. The scans of a frame's resources asked every list of the
/// window for its fonts and for its images in every transaction that scans
/// (a full one, and every frame that still draws text halos), also the lists
/// that had not changed - two walks of the 17,495 items of a map view each time
/// (audit A4-10g). Lists that are gone are forgotten here.
pub(crate) fn list_names(
    list: &std::sync::Arc<azul_layout::solver3::display_list::DisplayList>,
) -> std::rc::Rc<ListNames> {
    use azul_layout::solver3::display_list::DisplayListItem;

    let at = std::sync::Arc::as_ptr(list);
    let found = LIST_NAMES.with(|kept| {
        let mut kept = kept.borrow_mut();
        kept.retain(|held| held.list.strong_count() > 0);
        kept.iter()
            .find(|held| core::ptr::eq(held.list.as_ptr(), at))
            .map(|held| std::rc::Rc::clone(&held.names))
    });
    if let Some(found) = found {
        return found;
    }
    #[cfg(test)]
    LIST_NAME_WALKS.with(|walks| walks.set(walks.get() + 1));
    let mut fonts: HashSet<(u64, Au)> = HashSet::new();
    let mut image_items = Vec::new();
    for (index, item) in list.items.iter().enumerate() {
        if let DisplayListItem::Text {
            font_hash,
            font_size_px,
            ..
        } = item
        {
            fonts.insert((font_hash.font_hash, Au::from_px(*font_size_px)));
        } else if LayoutWindow::image_of_item(item).is_some() {
            image_items.push(index);
        }
    }
    let names = std::rc::Rc::new(ListNames {
        fonts: fonts.into_iter().collect(),
        image_items,
    });
    LIST_NAMES.with(|kept| {
        kept.borrow_mut().push(KeptNames {
            list: std::sync::Arc::downgrade(list),
            names: std::rc::Rc::clone(&names),
        });
    });
    names
}

/// What a scan of the display lists found of text halos
/// ([`collect_image_resource_updates_and_halos`]).
#[derive(Debug, Default)]
struct HaloScan {
    /// The halos the scan's budget left for a later frame.
    left: usize,
    /// Every text shadow the lists paint as a kept picture, per DOM in list
    /// order, exactly as a translation of the lists made now paints it
    /// (`compositor2`, which asks the same question of the same pictures).
    painted: Vec<(DomId, Vec<azul_layout::cpurender::TextHalo>)>,
    /// Whether a list paints a null image at all (the place of a picture to
    /// come, [`reserve_image_places`]): no list is walked for places otherwise.
    null_images: bool,
}

/// [`collect_image_resource_updates`] and the text halos of the lists. The
/// pictures of the halos are in the live set and NOT among the images to add:
/// a halo is painted from a slot, whose key outlives the picture it shows
/// ([`register_text_halos`]).
fn collect_image_resource_updates_and_halos(
    layout_window: &LayoutWindow,
    renderer_resources: &azul_core::resources::RendererResources,
) -> (
    Vec<(
        azul_core::resources::ImageRefHash,
        azul_core::resources::AddImageMsg,
    )>,
    azul_core::FastBTreeSet<azul_core::resources::ImageRefHash>,
    HaloScan,
) {
    use azul_core::{resources::build_add_image_resource_updates, FastBTreeSet};

    log_debug!(
        LogCategory::Rendering,
        "[collect_image_resource_updates] Scanning {} DOMs for images in display lists",
        layout_window.layout_results.len()
    );

    // Collect all unique ImageRefs from display lists: the pictures, the image
    // masks and the mask of a stroke (the GPU backend paints a stroke as a
    // colour through that mask - it has no vector rasteriser - so the mask is
    // uploaded like any other image; the CPU backend ignores it and strokes the
    // outline directly). ONE definition of which items those are, the image
    // GC's and the e2e runner's too (`LayoutWindow::image_of_item`); which
    // items of a list have one is kept while the list lives (`list_names`).
    let images_in_display_list: FastBTreeSet<ImageRef> = layout_window
        .layout_results
        .values()
        .flat_map(|layout_result| {
            let list = &layout_result.display_list;
            let names = list_names(list);
            (0..names.image_items.len()).filter_map(move |nth| {
                list.items
                    .get(names.image_items[nth])
                    .and_then(LayoutWindow::image_of_item)
            })
        })
        .cloned()
        .collect();

    // The text shadows the compositor paints as kept pictures: drawn here the
    // first time they are seen, and live while a list holds them. A halo is a
    // CPU raster of a label's glyphs, and a frame that found 25 new ones spent
    // 5.0 s drawing them on the UI thread before anything else could run: a
    // frame draws HALOS_DRAWN_ANYWAY of them however long they take and then
    // only while HALO_FRAME_BUDGET lasts; a shadow whose halo is left for a
    // later frame is painted with a stand-in (a spread shadow: `TextHalo::stand_in`,
    // an image of this list like the halo itself) or by WebRender's own blur
    // until then - the frames the platform loop goes on making
    // (`GpuStateManager::halos_owed`) draw the rest. The scan uses every picture
    // the translation of these lists will find, so each is in the live set below.
    let dpi = layout_window.current_window_state.size.get_hidpi_factor().inner.get();
    let (free, time) = halo_budget();
    let mut painted: Vec<(DomId, Vec<azul_layout::cpurender::TextHalo>)> = Vec::new();
    let ((), halos_left) = azul_layout::cpurender::within_halo_budget(
        free,
        time,
        || {
            for (dom_id, layout_result) in &layout_window.layout_results {
                let halos = azul_layout::cpurender::text_halos_of_list(
                    &layout_result.display_list,
                    dpi,
                    Some((renderer_resources, &layout_window.font_manager)),
                );
                if !halos.is_empty() {
                    painted.push((*dom_id, halos));
                }
            }
        },
    );
    // A scan that drew or dropped a picture, or showed one that waited, changed
    // what the shadows it had looked at before are painted with: the picture
    // that stood in for a shape is another one now, a slot shows another
    // picture. What the lists paint is then asked once more, the way their
    // translation asks (without fonts: nothing is drawn, nothing counted twice),
    // so that what is registered is what a translation finds.
    if azul_layout::cpurender::halo_scan_changed_pictures() {
        painted.clear();
        for (dom_id, layout_result) in &layout_window.layout_results {
            let halos =
                azul_layout::cpurender::text_halos_of_list(&layout_result.display_list, dpi, None);
            if !halos.is_empty() {
                painted.push((*dom_id, halos));
            }
        }
    }
    if halos_left > 0 {
        log_debug!(
            LogCategory::Rendering,
            "[collect_image_resource_updates] {} text halo(s) left for a later frame",
            halos_left
        );
    }

    // The live set as hashes (identity), for the GC diff in the caller: the
    // images of the lists and the pictures the halos are painted with.
    let live_hashes: FastBTreeSet<azul_core::resources::ImageRefHash> = images_in_display_list
        .iter()
        .map(azul_core::resources::image_ref_get_hash)
        .chain(
            painted
                .iter()
                .flat_map(|(_, halos)| halos.iter().map(|halo| halo.image.get_hash())),
        )
        .collect();

    // Build AddImage messages for new images using our gl_texture_integration
    let image_updates = build_add_image_resource_updates(
        renderer_resources,
        layout_window.id_namespace,
        layout_window.epoch,
        &layout_window.document_id,
        &images_in_display_list,
        crate::desktop::gl_texture_integration::insert_into_active_gl_textures,
    );

    // A GL image goes into the texture cache once, when it is first added;
    // the cache frees a texture two epochs after the last list that drew it
    // was sent. So every list that still draws one renews its stamp here:
    // WebRender looks the texture up at each render, and one the cache let go
    // is drawn from texture 0 - black.
    for image in images_in_display_list.iter().filter(|image| image.is_gl_texture()) {
        let _ = crate::desktop::gl_texture_cache::keep_alive(
            &layout_window.document_id,
            &azul_core::resources::image_ref_hash_to_external_image_id(image.get_hash()),
            layout_window.epoch,
        );
    }

    log_debug!(
        LogCategory::Rendering,
        "[collect_image_resource_updates] Generated {} AddImage messages",
        image_updates.len()
    );

    let null_images = images_in_display_list.iter().any(ImageRef::is_null_image);
    (
        image_updates,
        live_hashes,
        HaloScan {
            left: halos_left,
            painted,
            null_images,
        },
    )
}

/// This scans all display lists for Text items, extracts their font_hashes,
/// loads the fonts from the FontManager, and creates AddFont + AddFontInstance ResourceUpdates.
/// Also returns the hash of every font a display list draws: the live set of
/// the font GC ([`collect_stale_font_deletes`]).
///
/// CRITICAL: FontKey is generated deterministically from font hash to ensure
/// consistency between layout (which uses hash) and rendering (which uses key).
pub fn collect_font_resource_updates(
    layout_window: &LayoutWindow,
    renderer_resources: &azul_core::resources::RendererResources,
    dpi_factor: DpiScaleFactor,
) -> (Vec<ResourceUpdate>, azul_core::FastBTreeSet<u64>) {
    use std::collections::BTreeMap;

    use azul_core::resources::{
        AddFontInstance, FontInstanceKey, FontInstanceOptions, FontInstancePlatformOptions,
        FontRenderMode, IdNamespace, FONT_INSTANCE_FLAG_NO_AUTOHINT,
    };
    log_debug!(
        LogCategory::Rendering,
        "[collect_font_resource_updates] Scanning {} DOMs for fonts",
        layout_window.layout_results.len()
    );

    // Map from font_hash to set of font sizes
    let mut font_hash_sizes: BTreeMap<u64, HashSet<Au>> = BTreeMap::new();
    let mut resource_updates = Vec::new();

    // Collect all unique font hash + size combinations from display lists
    // (kept per list while the list lives: `list_names`).
    for layout_result in layout_window.layout_results.values() {
        for &(font_hash, font_size_au) in &list_names(&layout_result.display_list).fonts {
            font_hash_sizes.entry(font_hash).or_default().insert(font_size_au);
        }
    }

    log_debug!(
        LogCategory::Rendering,
        "[collect_font_resource_updates] Found {} unique fonts with various sizes",
        font_hash_sizes.len()
    );

    // For each font hash, check if it's already registered
    for (&font_hash, font_sizes) in &font_hash_sizes {
        let font_key = font_key_from_hash(font_hash);

        // Check if font itself is already registered
        let font_needs_registration = !renderer_resources.font_hash_map.contains_key(&font_hash);

        if font_needs_registration {
            // First try to get embedded font (e.g. Material Icons)
            // Then fall back to parsed font (fontconfig-loaded)
            // THE font lookup — the same one the CPU renderer uses. Both pools
            // (loaded faces + `StyleFontFamily::Ref` embedded faces) can shape
            // text, so both must be searched, and by ONE function so the GPU and
            // CPU paths cannot drift apart.
            let font_ref = layout_window.font_manager.resolve_font_by_hash(font_hash);

            if let Some(font_ref) = font_ref {
                log_debug!(
                    LogCategory::Rendering,
                    "[collect_font_resource_updates] Font found, parsed ptr: {:?}",
                    font_ref.get_parsed()
                );

                resource_updates.push(ResourceUpdate::AddFont(AddFont {
                    key: font_key,
                    font: font_ref.clone(),
                }));

                log_debug!(
                    LogCategory::Rendering,
                    "[collect_font_resource_updates] ✓ Created AddFont for hash {} -> key {:?}",
                    font_hash,
                    font_key
                );
            } else {
                log_debug!(
                    LogCategory::Rendering,
                    "[collect_font_resource_updates] BUG: layout emitted font hash {} that its \
                     own FontManager cannot resolve — text using it cannot be drawn",
                    font_hash
                );
                continue;
            }
        }

        // Register font instances for each size
        for &font_size in font_sizes {
            // Check if this font instance already exists
            let instance_exists = renderer_resources
                .currently_registered_fonts
                .get(&font_key)
                .and_then(|(_, instances)| instances.get(&(font_size, dpi_factor)))
                .is_some();

            if !instance_exists {
                let font_instance_key =
                    FontInstanceKey::unique(IdNamespace((font_hash >> 32) as u32));

                #[cfg(target_os = "macos")]
                let platform_options = FontInstancePlatformOptions::default();

                #[cfg(target_os = "windows")]
                let platform_options = FontInstancePlatformOptions {
                    gamma: 300,
                    contrast: 100,
                    cleartype_level: 100,
                };

                #[cfg(target_os = "linux")]
                let platform_options = FontInstancePlatformOptions {
                    lcd_filter: azul_core::resources::FontLCDFilter::Default,
                    hinting: azul_core::resources::FontHinting::Normal,
                };

                #[cfg(any(target_os = "android", target_os = "ios"))]
                let platform_options = FontInstancePlatformOptions::default();

                let options = FontInstanceOptions {
                    render_mode: FontRenderMode::Subpixel,
                    flags: FONT_INSTANCE_FLAG_NO_AUTOHINT,
                    ..Default::default()
                };

                resource_updates.push(ResourceUpdate::AddFontInstance(AddFontInstance {
                    key: font_instance_key,
                    font_key,
                    glyph_size: (font_size, dpi_factor),
                    options: Some(options),
                    platform_options: Some(platform_options),
                    variations: Vec::new(),
                }));

                log_debug!(
                    LogCategory::Rendering,
                    "[collect_font_resource_updates] ✓ Created AddFontInstance for size {:?} @ \
                     dpi {:?}",
                    font_size,
                    dpi_factor
                );
            }
        }
    }

    log_debug!(
        LogCategory::Rendering,
        "[collect_font_resource_updates] Generated {} resource updates",
        resource_updates.len()
    );
    let live = font_hash_sizes.keys().copied().collect();
    (resource_updates, live)
}

/// Translate azul-core ResourceUpdate to WebRender ResourceUpdate
fn translate_resource_update(
    update: ResourceUpdate,
) -> Option<webrender::ResourceUpdate> {
    use azul_core::resources::ResourceUpdate as AzResourceUpdate;
    use webrender::ResourceUpdate as WrResourceUpdate;

    match update {
        AzResourceUpdate::AddImage(add_image) => {
            Some(WrResourceUpdate::AddImage(translate_add_image(add_image)?))
        }
        AzResourceUpdate::UpdateImage(update_image) => Some(WrResourceUpdate::UpdateImage(
            translate_update_image(update_image)?,
        )),
        AzResourceUpdate::DeleteImage(key) => {
            Some(WrResourceUpdate::DeleteImage(translate_image_key(key)))
        }
        AzResourceUpdate::AddFont(add_font) => {
            Some(WrResourceUpdate::AddFont(translate_add_font(add_font)?))
        }
        AzResourceUpdate::DeleteFont(key) => {
            Some(WrResourceUpdate::DeleteFont(translate_font_key(key)))
        }
        AzResourceUpdate::AddFontInstance(add_instance) => Some(WrResourceUpdate::AddFontInstance(
            translate_add_font_instance(add_instance)?,
        )),
        AzResourceUpdate::DeleteFontInstance(key) => Some(WrResourceUpdate::DeleteFontInstance(
            wr_translate_font_instance_key(key),
        )),
    }
}

/// Convert azul-core RawImageFormat to WebRender ImageFormat
fn translate_image_format(
    format: azul_core::resources::RawImageFormat,
) -> webrender::api::ImageFormat {
    use azul_core::resources::RawImageFormat;
    use webrender::api::ImageFormat;

    match format {
        RawImageFormat::R8 => ImageFormat::R8,
        RawImageFormat::R16 => ImageFormat::R16,
        RawImageFormat::RG8 => ImageFormat::RG8,
        RawImageFormat::RG16 => ImageFormat::RG16,
        RawImageFormat::RGBA8 => ImageFormat::RGBA8,
        RawImageFormat::BGRA8 => ImageFormat::BGRA8,
        RawImageFormat::RGBAF32 => ImageFormat::RGBAF32,

        // Formats not supported by WebRender - convert to closest equivalent
        RawImageFormat::RGB8 => ImageFormat::RGBA8, // Add alpha channel
        RawImageFormat::RGB16 => ImageFormat::RGBA8, // Convert to 8-bit with alpha
        RawImageFormat::RGBA16 => ImageFormat::RGBA8, // Convert to 8-bit
        RawImageFormat::BGR8 => ImageFormat::BGRA8, // Add alpha channel
        RawImageFormat::RGBF32 => ImageFormat::RGBAF32, // Add alpha channel

        // NV12 is TWO WebRender images (the R8 Y plane and the RG8 Cb,Cr
        // plane, see `nv12_image_updates`); a single image of it is its Y
        // plane.
        RawImageFormat::NV12Rec601Video
        | RawImageFormat::NV12Rec601Full
        | RawImageFormat::NV12Rec709Video
        | RawImageFormat::NV12Rec709Full => ImageFormat::R8,
    }
}

/// Translate AddImage from azul-core to WebRender
fn translate_add_image(add_image: AddImage) -> Option<WrAddImage> {
    let mut flags = WrImageDescriptorFlags::empty();
    if add_image.descriptor.flags.is_opaque {
        flags |= WrImageDescriptorFlags::IS_OPAQUE;
    }
    if add_image.descriptor.flags.allow_mipmaps {
        flags |= WrImageDescriptorFlags::ALLOW_MIPMAPS;
    }

    Some(webrender::AddImage {
        key: translate_image_key(add_image.key),
        descriptor: WrImageDescriptor {
            format: translate_image_format(add_image.descriptor.format),
            size: DeviceIntSize::new(
                add_image.descriptor.width as i32,
                add_image.descriptor.height as i32,
            ),
            stride: add_image.descriptor.stride.into_option(),
            offset: add_image.descriptor.offset,
            flags,
        },
        data: translate_image_data(add_image.data),
        tiling: add_image.tiling,
    })
}

/// Translate UpdateImage from azul-core to WebRender
fn translate_update_image(update_image: UpdateImage) -> Option<WrUpdateImage> {
    let mut flags = WrImageDescriptorFlags::empty();
    if update_image.descriptor.flags.is_opaque {
        flags |= WrImageDescriptorFlags::IS_OPAQUE;
    }
    if update_image.descriptor.flags.allow_mipmaps {
        flags |= WrImageDescriptorFlags::ALLOW_MIPMAPS;
    }

    let dirty_rect = translate_dirty_rect(&update_image.dirty_rect);

    Some(WrUpdateImage {
        key: translate_image_key(update_image.key),
        descriptor: WrImageDescriptor {
            format: translate_image_format(update_image.descriptor.format),
            size: DeviceIntSize::new(
                update_image.descriptor.width as i32,
                update_image.descriptor.height as i32,
            ),
            stride: update_image.descriptor.stride.into_option(),
            offset: update_image.descriptor.offset,
            flags,
        },
        data: translate_image_data(update_image.data),
        dirty_rect,
    })
}

/// Translate AddFont from azul-core to WebRender
fn translate_add_font(add_font: AddFont) -> Option<webrender::AddFont> {
    // WebRender's AddFont is an enum with Parsed variant
    // azul-core's AddFont already has both key and FontRef
    log_debug!(
        LogCategory::Rendering,
        "[translate_add_font] Translating FontKey {:?}, parsed ptr: {:?}",
        add_font.key,
        add_font.font.get_parsed()
    );

    Some(webrender::AddFont::Parsed(
        translate_font_key(add_font.key),
        add_font.font, // FontRef is Clone
    ))
}

/// Translate AddFontInstance from azul-core to WebRender  
fn translate_add_font_instance(add_instance: AddFontInstance) -> Option<WrAddFontInstance> {
    // Convert Au to f32 pixels: Au units are 1/60th of a pixel
    // glyph_size is (Au, DpiScaleFactor)
    let font_size_au = add_instance.glyph_size.0;
    let dpi_factor = add_instance.glyph_size.1.inner.get();

    // Convert Au to logical pixels (1 Au = 1/60 px), then multiply by DPI factor
    // to get the physical pixel size for rasterization.
    // NOTE: azul_layout outputs coordinates in CSS pixels (logical pixels).
    // WebRender handles HiDPI scaling via device_pixel_scale. However, font instances
    // need to be pre-scaled because they are rasterized at specific pixel sizes.
    let glyph_size_px = (font_size_au.0 as f32) / 60.0 * dpi_factor;

    log_debug!(
        LogCategory::Rendering,
        "[translate_add_font_instance] Converting Au({}) to {}px (physical), dpi={}",
        font_size_au.0,
        glyph_size_px,
        dpi_factor
    );

    Some(WrAddFontInstance {
        key: wr_translate_font_instance_key(add_instance.key),
        font_key: translate_font_key(add_instance.font_key),
        glyph_size: glyph_size_px,
        options: add_instance.options.map(|opts| WrFontInstanceOptions {
            flags: wr_translate_font_instance_flags(opts.flags),
            synthetic_italics: wr_translate_synthetic_italics(opts.synthetic_italics),
            render_mode: wr_translate_font_render_mode(opts.render_mode),
            _padding: 0,
        }),
        platform_options: add_instance.platform_options.map(|_opts| {
            // Platform options are platform-specific, for now use defaults
            WrFontInstancePlatformOptions::default()
        }),
        variations: add_instance
            .variations
            .iter()
            .map(|v| WrFontVariation {
                tag: v.tag,
                value: v.value,
            })
            .collect(),
    })
}

/// Translate ImageKey from azul-core to WebRender.
///
/// Azul's `ImageKey.key` is a `u64` so pointer-derived hashes round-trip
/// losslessly internally; webrender's slot is a `u32`, so we narrow at this
/// FFI boundary. Collision risk is bounded to a single frame's add/delete
/// pairing within webrender's own namespace+u32 ID space.
pub fn translate_image_key(key: ImageKey) -> WrImageKey {
    WrImageKey(wr_translate_id_namespace(key.namespace), key.key as u32)
}

/// Translate ImageDescriptor from azul-core to WebRender
fn wr_translate_image_descriptor(
    descriptor: &azul_core::resources::ImageDescriptor,
) -> WrImageDescriptor {
    let mut flags = WrImageDescriptorFlags::empty();
    if descriptor.flags.is_opaque {
        flags |= WrImageDescriptorFlags::IS_OPAQUE;
    }
    if descriptor.flags.allow_mipmaps {
        flags |= WrImageDescriptorFlags::ALLOW_MIPMAPS;
    }

    WrImageDescriptor {
        format: translate_image_format(descriptor.format),
        size: DeviceIntSize::new(descriptor.width as i32, descriptor.height as i32),
        stride: descriptor.stride.into_option(),
        offset: descriptor.offset,
        flags,
    }
}

/// Translate FontKey from azul-core to WebRender.
/// Narrows azul's u64 key to webrender's u32 slot (see `translate_image_key`).
fn translate_font_key(key: FontKey) -> WrFontKey {
    WrFontKey(wr_translate_id_namespace(key.namespace), key.key as u32)
}

/// Translate ImageData from azul-core to WebRender's ImageData
///
/// Note: Both types now use SharedRawImageData for the Raw variant,
/// so we only need to translate the External variant's structure.
fn translate_image_data(data: azul_core::resources::ImageData) -> webrender::api::ImageData {
    use azul_core::resources::ImageData as AzImageData;

    match data {
        AzImageData::Raw(shared_data) => {
            // SharedRawImageData can be passed directly
            webrender::api::ImageData::Raw(shared_data)
        }
        AzImageData::External(ext_data) => {
            // External images need structure translation
            webrender::api::ImageData::External(webrender::api::ExternalImageData {
                id: webrender::api::ExternalImageId(ext_data.id.inner),
                channel_index: ext_data.channel_index,
                image_type: match ext_data.image_type {
                    azul_core::resources::ExternalImageType::TextureHandle(kind) => {
                        webrender::api::ExternalImageType::TextureHandle(match kind {
                            azul_core::resources::ImageBufferKind::Texture2D => {
                                webrender::api::ImageBufferKind::Texture2D
                            }
                            azul_core::resources::ImageBufferKind::TextureRect => {
                                webrender::api::ImageBufferKind::TextureRect
                            }
                            azul_core::resources::ImageBufferKind::TextureExternal => {
                                webrender::api::ImageBufferKind::TextureExternal
                            }
                        })
                    }
                    azul_core::resources::ExternalImageType::Buffer => {
                        webrender::api::ExternalImageType::Buffer
                    }
                },
                normalized_uvs: false, // azul-core doesn't track this, default to false
            })
        }
    }
}

/// Translate SyntheticItalics from azul-core to WebRender
fn wr_translate_synthetic_italics(italics: SyntheticItalics) -> WrSyntheticItalics {
    WrSyntheticItalics {
        angle: italics.angle,
    }
}

/// Collect font + image resources referenced by the current display lists,
/// register them into `layout_window.renderer_resources` (so `push_text` /
/// image rendering can resolve `FontKey` / `FontInstanceKey` / `ImageKey` from
/// hashes), and add the translated WebRender resources to `txn`.
///
/// Shared by [`generate_frame`] and [`build_webrender_transaction`], which both
/// previously inlined this identical font+image registration block.
pub(crate) fn register_frame_resources(
    layout_window: &mut LayoutWindow,
    txn: &mut WrTransaction,
    dpi: DpiScaleFactor,
) -> bool {
    let _timed = gpu_time(GpuTime::Resources);
    // --- Fonts ---
    let (font_updates, live_font_hashes) =
        collect_font_resource_updates(layout_window, &layout_window.renderer_resources, dpi);

    // --- Font GC: delete the fonts gone from every display list ---
    // BEFORE the adds: a delete never targets a font this frame draws.
    let font_deletes = collect_stale_font_deletes(layout_window, &live_font_hashes);
    if !font_deletes.is_empty() {
        let wr_font_deletes: Vec<webrender::ResourceUpdate> = font_deletes
            .into_iter()
            .filter_map(translate_resource_update)
            .collect();
        if !wr_font_deletes.is_empty() {
            txn.update_resources(wr_font_deletes);
        }
    }

    // Update font_hash_map + currently_registered_fonts as we process resources.
    // This is CRITICAL for push_text() to look up FontKey / FontInstanceKey from
    // the font hash + glyph size.
    for resource in &font_updates {
        match resource {
            ResourceUpdate::AddFont(ref add_font) => {
                layout_window
                    .renderer_resources
                    .font_hash_map
                    .insert(add_font.font.get_hash(), add_font.key);
                layout_window
                    .renderer_resources
                    .currently_registered_fonts
                    .entry(add_font.key)
                    .or_insert_with(|| (add_font.font.clone(), BTreeMap::default()));
            }
            ResourceUpdate::AddFontInstance(ref add_font_instance) => {
                if let Some((_, instances)) = layout_window
                    .renderer_resources
                    .currently_registered_fonts
                    .get_mut(&add_font_instance.font_key)
                {
                    instances.insert(add_font_instance.glyph_size, add_font_instance.key);
                }
            }
            _ => {}
        }
    }

    if !font_updates.is_empty() {
        let wr_resources: Vec<webrender::ResourceUpdate> = font_updates
            .into_iter()
            .filter_map(translate_resource_update)
            .collect();
        if !wr_resources.is_empty() {
            txn.update_resources(wr_resources);
        }
    }

    // --- Video tiles: in-place-replaced node images, under their stable keys ---
    // BEFORE the display-list scan, so their frames are found registered and
    // are not added again under a per-frame key. A slot keyed for the first
    // time is named by no list WebRender holds: the caller sends the lists
    // again (the return value).
    let overlay_new_slot = upload_overlay_images(layout_window, txn).new_slot;

    register_image_resources(layout_window, txn);

    overlay_new_slot
}

/// The images of the display lists: register them with the renderer (a
/// text halo drawn here included, see [`collect_image_resource_updates`]), add
/// the ones it does not hold and delete the ones no list names any more.
/// Records in `GpuStateManager::halos_owed` whether this scan left halos for a
/// later frame. A scan that starts while halos are owed is a follow-up, and the
/// chain of them ends after [`HALO_FOLLOW_UPS_MAX`] at the latest. (Lists that
/// name more halos than can be kept do not keep the window awake: a scan drops
/// none of the halos it uses, and one it has no room for is not left for a
/// later frame.)
fn register_image_resources(layout_window: &mut LayoutWindow, txn: &mut WrTransaction) {
    let follow_up = layout_window.gpu_state_manager.halos_owed;
    // --- Images ---
    let (image_updates, live_image_hashes, halos) =
        collect_image_resource_updates_and_halos(layout_window, &layout_window.renderer_resources);
    {
        let state = &mut layout_window.gpu_state_manager;
        state.halo_follow_ups = if follow_up { state.halo_follow_ups + 1 } else { 0 };
        state.halos_owed = halos.left > 0 && state.halo_follow_ups < HALO_FOLLOW_UPS_MAX;
    }

    // --- Text halos: each slot's picture under the slot's key ---
    // BEFORE the image GC below: a slot that shows a new picture forgets the
    // old one here (`release_frame`), so the GC does not delete the slot's key
    // for a picture the key no longer shows.
    register_text_halos(layout_window, txn, &halos);

    // --- The places of pictures to come (nodes that show a null image) ---
    reserve_image_places(layout_window, txn, halos.null_images);

    // Update currently_registered_images + the reverse image_key_map lookup so
    // the display-list builder can resolve an ImageRefHash to its ImageKey.
    for (image_ref_hash, add_image_msg) in &image_updates {
        use azul_core::resources::ResolvedImage;

        let resolved_image = ResolvedImage {
            key: add_image_msg.0.key,
            descriptor: add_image_msg.0.descriptor,
        };
        layout_window
            .renderer_resources
            .currently_registered_images
            .insert(*image_ref_hash, resolved_image);
        layout_window
            .renderer_resources
            .image_key_map
            .insert(add_image_msg.0.key, *image_ref_hash);
    }

    // --- Image GC: DeleteImage for images gone from every display list ---
    // Without this the texture cache grows unboundedly for any window that
    // swaps images each frame (the ~1 GB video leak, audit §3.4). We keep an
    // image for IMAGE_GC_KEEP_EPOCHS frames after it was last referenced (so a
    // one-frame blip during re-layout doesn't churn a delete+re-upload), then
    // DeleteImage it and drop both registry entries. Aliasing-safe: ImageRefHash
    // is a never-reused id, so a freed key can't collide with a future image.
    let image_deletes = collect_stale_image_deletes(layout_window, &live_image_hashes);
    if !image_deletes.is_empty() {
        forget_deleted_halo_slots(&mut layout_window.renderer_resources, &image_deletes);
        let wr_image_deletes: Vec<webrender::ResourceUpdate> = image_deletes
            .into_iter()
            .filter_map(|del| translate_resource_update(del))
            .collect();
        if !wr_image_deletes.is_empty() {
            txn.update_resources(wr_image_deletes);
        }
    }

    if !image_updates.is_empty() {
        let namespace = layout_window.id_namespace;
        let mut wr_image_resources: Vec<webrender::ResourceUpdate> = Vec::new();
        for (_, add_image_msg) in image_updates {
            let add = add_image_msg.0;
            if add.descriptor.format.is_nv12() {
                // Two renderer images (luma + chroma), one buffer.
                wr_image_resources.extend(nv12_add_images(
                    &mut layout_window.renderer_resources,
                    namespace,
                    add,
                ));
            } else if let Some(update) = translate_resource_update(ResourceUpdate::AddImage(add)) {
                wr_image_resources.push(update);
            }
        }
        if !wr_image_resources.is_empty() {
            txn.update_resources(wr_image_resources);
        }
    }
}

/// The text halos of a scan, registered with the renderer: every SLOT the lists
/// paint a halo from (`TextHalo::slot`) has one image key for as long as it is
/// painted, and the key shows the slot's picture - `add_image` the first time,
/// `update_image` when the slot shows another picture (the label's own, drawn
/// into the place of the one that stood in for it, `cpurender::text_halo`).
/// The picture's hash resolves to the slot's key, which is how the translation
/// finds it (`compositor2`, `PushTextShadow`).
///
/// Then what each DOM's list paints of halos - per shadow the key, the rect,
/// the colour - is compared, as a digest, with what the scan before found
/// (`GpuStateManager::text_halo_digests`), and a DOM for which it differs is
/// marked: its list goes again (`stale_display_lists`, [`submit_display_lists`]).
/// So a list is sent again exactly when the translation of its halos would
/// come out different. A scan that only replaced pictures under their keys
/// marks nothing, every list stays where it is, and its transaction carries
/// image updates and nothing else (audit A4, the frames after a zoom settles:
/// each one translated every list and built a scene, 17,495 items a frame on a
/// municipal map, to say "this label's halo is that picture now").
///
/// The order of a slot's updates across transactions - its `add_image` before
/// every `update_image` - holds because a transaction with resource updates
/// never skips the scene builder ([`skip_scene_builder_unless_it_carries_resources`]).
fn register_text_halos(layout_window: &mut LayoutWindow, txn: &mut WrTransaction, halos: &HaloScan) {
    use azul_core::resources::ImageSlot;

    let namespace = layout_window.id_namespace;
    // Each slot once, with the picture it shows. (Two shadows of one slot show
    // one picture: a slot is a kept picture's, and the scan asked what the
    // lists paint only after it had drawn.)
    let mut seen: HashSet<u64> = HashSet::new();
    let frames = halos
        .painted
        .iter()
        .flat_map(|(_, list)| list.iter())
        .filter(|halo| seen.insert(halo.slot))
        .map(|halo| (ImageSlot::TextHalo(halo.slot), &halo.image, ImageDirtyRect::All));
    let uploads =
        plan_overlay_image_uploads(frames, &mut layout_window.renderer_resources, namespace);
    for upload in uploads {
        let key = translate_image_key(upload.key);
        let descriptor = wr_translate_image_descriptor(&upload.descriptor);
        let data = translate_image_data(upload.data);
        if upload.update {
            // Another picture, and as a rule another size: the whole of it.
            txn.update_image(key, descriptor, data, &DirtyRect::All);
        } else {
            txn.add_image(key, descriptor, data, None);
        }
    }

    // What each DOM's list paints of halos now, against what the last scan
    // found: a DOM for which that differs has a list the renderer holds
    // translated otherwise.
    let mut found: BTreeMap<DomId, u64> = BTreeMap::new();
    for (dom_id, list) in &halos.painted {
        let mut digest = FrameDigest::new();
        digest.push_u64(list.len() as u64);
        for halo in list {
            let key = layout_window
                .renderer_resources
                .currently_registered_images
                .get(&halo.image.get_hash())
                .map_or(u64::MAX, |resolved| resolved.key.key);
            digest.push_u64(halo.push as u64);
            digest.push_u64(key);
            digest.push_f32(halo.origin.0);
            digest.push_f32(halo.origin.1);
            digest.push_f32(halo.size.0);
            digest.push_f32(halo.size.1);
            digest.push_u64(u64::from(u32::from_be_bytes([
                halo.color.r,
                halo.color.g,
                halo.color.b,
                halo.color.a,
            ])));
        }
        found.insert(*dom_id, digest.finish());
    }
    let LayoutWindow {
        ref layout_results,
        gpu_state_manager: ref mut state,
        ..
    } = *layout_window;
    // (A DOM that is gone has no list to send.)
    state
        .stale_display_lists
        .retain(|dom_id| layout_results.contains_key(dom_id));
    for dom_id in layout_results.keys() {
        if state.text_halo_digests.get(dom_id) != found.get(dom_id) {
            state.stale_display_lists.insert(*dom_id);
        }
    }
    state.text_halo_digests = found;
}

/// A node that shows a null image (`ImageRef::null_image`: a size and no
/// pixels - the place of a picture to come, a map tile that is being drawn) is
/// given the key of its slot NOW, showing one clear pixel
/// (`ImageRefHash::PLACE`), and the translation paints the node from that key
/// (`compositor2`, `Image`). The node's first picture (`change_node_image`) is
/// then what every later one already was: `update_image` under the key the
/// display list names (`plan_overlay_image_uploads` finds the slot keyed), with
/// no list sent and no scene built. Before, the first picture gave the slot
/// its key (`new_slot`), and every display list of the window was translated
/// and sent again for it: per arriving tile of a map, its 17,495 items and a
/// scene build (audit A4-09).
///
/// A place whose node shows no null image any more is deleted - unless the
/// node's first picture took it, which makes it the node's slot like any
/// other. A DOM in which a place is new is marked: its list is translated
/// with the place from now on (`stale_display_lists`).
fn reserve_image_places(
    layout_window: &mut LayoutWindow,
    txn: &mut WrTransaction,
    null_images: bool,
) {
    use azul_core::resources::{
        DecodedImage, ImageRefHash, ImageSlot, RawImage, RawImageData, RawImageFormat,
    };
    use azul_layout::solver3::display_list::DisplayListItem;

    // The nodes that show a null image now: looked for only when the scan of
    // the lists' images met one (it has seen every image item already).
    let mut shown: std::collections::BTreeSet<(DomId, NodeId)> = std::collections::BTreeSet::new();
    let lists = if null_images {
        Some(&layout_window.layout_results)
    } else {
        None
    };
    for (dom_id, layout_result) in lists.into_iter().flatten() {
        let list = &layout_result.display_list;
        // Only the items that have an image are looked at (`list_names`).
        for &index in &list_names(list).image_items {
            let Some(DisplayListItem::Image { image, .. }) = list.items.get(index) else {
                continue;
            };
            if !image.is_null_image() {
                continue;
            }
            let Some(node) = list.node_mapping.get(index).copied().flatten() else {
                continue;
            };
            // A node that has its picture already (the list is the one from
            // before it arrived: a layout is pending) keeps the slot it has.
            if layout_window
                .content_overlay
                .image_for_node(*dom_id, node)
                .is_some()
            {
                continue;
            }
            shown.insert((*dom_id, node));
        }
    }

    let namespace = layout_window.id_namespace;
    let LayoutWindow {
        renderer_resources: ref mut rr,
        gpu_state_manager: ref mut state,
        ..
    } = *layout_window;

    let gone: Vec<(ImageSlot, ImageKey)> = rr
        .image_slots
        .iter()
        .filter(|(slot, key)| {
            rr.image_key_map.get(*key) == Some(&ImageRefHash::PLACE)
                && !matches!(slot, ImageSlot::Node(dom, node) if shown.contains(&(*dom, *node)))
        })
        .map(|(slot, key)| (*slot, *key))
        .collect();
    for (slot, key) in gone {
        rr.image_slots.remove(&slot);
        rr.image_key_map.remove(&key);
        txn.delete_image(translate_image_key(key));
    }

    // One clear pixel, made when the first place needs it.
    let mut pixel: Option<ImageRef> = None;
    for (dom_id, node) in shown {
        let slot = ImageSlot::Node(dom_id, node);
        let has_its_place = rr
            .image_slots
            .get(&slot)
            .is_some_and(|key| rr.image_key_map.get(key) == Some(&ImageRefHash::PLACE));
        if has_its_place {
            continue;
        }
        if pixel.is_none() {
            pixel = ImageRef::new_rawimage(RawImage {
                pixels: RawImageData::U8(vec![0u8; 4].into()),
                width: 1,
                height: 1,
                premultiplied_alpha: true,
                data_format: RawImageFormat::RGBA8,
                tag: Vec::new().into(),
            });
        }
        let Some(DecodedImage::Raw((descriptor, data))) = pixel.as_ref().map(ImageRef::get_data)
        else {
            return;
        };
        // A slot the node had before (the picture of an earlier view, which
        // the image GC lets go by its own clock) is not the place: a new key.
        let key = ImageKey::unique_image_slot(namespace);
        rr.image_slots.insert(slot, key);
        rr.image_key_map.insert(key, ImageRefHash::PLACE);
        txn.add_image(
            translate_image_key(key),
            wr_translate_image_descriptor(descriptor),
            translate_image_data(data.clone()),
            None,
        );
        state.stale_display_lists.insert(dom_id);
    }
}

/// The image GC deleted these keys: a text halo's slot whose key went is
/// forgotten with it (the slots are numbered without end; kept, the map grew by
/// one entry for every label a window ever showed).
fn forget_deleted_halo_slots(
    rr: &mut azul_core::resources::RendererResources,
    deletes: &[ResourceUpdate],
) {
    use azul_core::resources::ImageSlot;

    let deleted: HashSet<ImageKey> = deletes
        .iter()
        .filter_map(|update| match update {
            ResourceUpdate::DeleteImage(key) => Some(*key),
            _ => None,
        })
        .collect();
    if deleted.is_empty() {
        return;
    }
    rr.image_slots
        .retain(|slot, key| !(matches!(slot, ImageSlot::TextHalo(_)) && deleted.contains(&*key)));
}

// ==== Video tiles: one renderer image per tile, updated in place ====

/// The Cb,Cr plane of one [`OverlayImageUpload`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChromaPlane {
    /// The plane's stable key (see `RendererResources::nv12_chroma_keys`).
    pub key: ImageKey,
    /// `update_image` (the key already holds a plane) rather than `add_image`.
    pub update: bool,
}

/// One new frame of an in-place-replaced image (a video tile, a background
/// layer's callback frame), to hand to `WebRender` under its slot's stable key.
#[derive(Debug, Clone)]
pub struct OverlayImageUpload {
    /// The slot's stable key (`RendererResources::image_slots`); the Y
    /// plane of an NV12 frame.
    pub key: ImageKey,
    /// `update_image` (the key already holds a frame) rather than
    /// `add_image`.
    pub update: bool,
    /// The frame's descriptor.
    pub descriptor: azul_core::resources::ImageDescriptor,
    /// The frame's bytes (shared, not copied).
    pub data: AzImageData,
    /// The chroma plane of an NV12 frame.
    pub chroma: Option<ChromaPlane>,
    /// A chroma plane the key no longer needs (the tile turned from NV12 to
    /// a packed format): `delete_image` it.
    pub drop_chroma: Option<ImageKey>,
    /// The part of the frame to upload: `All` for a new key or a frame of
    /// another pixel shape, else the region the app said it repainted
    /// (`CallbackInfo::change_node_image_rect`), clipped to the frame.
    pub dirty: ImageDirtyRect,
    /// The key holds a frame of another colour model than this one (an NV12
    /// frame of another matrix or range): the display list says how a YUV
    /// image is converted, so the lists that name the key go again.
    pub relist: bool,
}

/// Plan the uploads of this frame's in-place-replaced images.
///
/// `frames` are the content overlay's images by slot (a node's image, a
/// background layer's callback frame), each with the region of it the
/// renderer has not uploaded yet (`ContentOverlay::image_dirty`). Every raw
/// frame that is not registered yet goes under its slot's stable key: the
/// slot's first frame mints the key (an `add_image`), every later one UPDATES
/// it (pixels only - no new key, so neither the display list nor the scene is
/// rebuilt), and only its dirty region when the key already holds a frame of
/// the same pixel shape. The frame's hash resolves to the key, and the frame
/// the key showed before is forgotten (never `DeleteImage`d: the key lives
/// on). An NV12 frame gets a second stable key for its chroma plane.
pub fn plan_overlay_image_uploads<'a, I>(
    frames: I,
    rr: &mut azul_core::resources::RendererResources,
    namespace: azul_core::resources::IdNamespace,
) -> Vec<OverlayImageUpload>
where
    I: Iterator<Item = (azul_core::resources::ImageSlot, &'a ImageRef, ImageDirtyRect)>,
{
    use azul_core::resources::{DecodedImage, ResolvedImage};

    let mut uploads = Vec::new();
    for (slot, image, pending) in frames {
        let DecodedImage::Raw((descriptor, data)) = image.get_data() else {
            continue;
        };
        let hash = image.get_hash();
        // Only this slot's own key counts: a frame another slot shows as well
        // goes under this slot's key too, so it outlives the other's change.
        let shown_here = rr
            .image_slots
            .get(&slot)
            .is_some_and(|key| rr.image_key_map.get(key) == Some(&hash));
        if shown_here {
            continue;
        }
        // A text halo's picture that the key of ANOTHER halo slot shows, in a
        // slot that has no key yet: the keeper of the pictures gave the picture
        // a new slot (another window on this thread drew a picture into its
        // place; the kept pictures are a thread's, the keys a window's). The key
        // goes with the picture: the list that names it paints what it painted,
        // nothing is uploaded, and the old slot's key is not left behind showing
        // a picture that is registered elsewhere, which no GC reaches (audit
        // A5-16). One window alone never comes here: there a slot that lost its
        // picture was given another in the same scan.
        if matches!(slot, azul_core::resources::ImageSlot::TextHalo(_))
            && !rr.image_slots.contains_key(&slot)
        {
            let shows = rr
                .currently_registered_images
                .get(&hash)
                .map(|resolved| resolved.key)
                .filter(|key| rr.image_key_map.get(key) == Some(&hash));
            let from = shows.and_then(|key| {
                rr.image_slots
                    .iter()
                    .find(|(other, shown)| {
                        **shown == key
                            && matches!(other, azul_core::resources::ImageSlot::TextHalo(_))
                    })
                    .map(|(other, _)| *other)
            });
            if let (Some(key), Some(from)) = (shows, from) {
                rr.image_slots.remove(&from);
                rr.image_slots.insert(slot, key);
                continue;
            }
        }
        let (key, update) = match rr.image_slots.get(&slot) {
            Some(&key) if rr.image_key_map.contains_key(&key) => (key, true),
            _ => {
                let key = ImageKey::unique_image_slot(namespace);
                rr.image_slots.insert(slot, key);
                (key, false)
            }
        };
        let (chroma, drop_chroma) = if descriptor.format.is_nv12() {
            let plane = match rr.nv12_chroma_keys.get(&key) {
                Some(&existing) if update => ChromaPlane {
                    key: existing,
                    update: true,
                },
                _ => {
                    let fresh = ImageKey::unique_image_slot(namespace);
                    rr.nv12_chroma_keys.insert(key, fresh);
                    ChromaPlane {
                        key: fresh,
                        update: false,
                    }
                }
            };
            (Some(plane), None)
        } else {
            (None, rr.nv12_chroma_keys.remove(&key))
        };
        // The frame the key holds now: a partial upload needs the same pixel
        // shape (size, format, row layout), or WebRender must take it whole.
        let same_shape = update
            && !descriptor.format.is_nv12()
            && rr
                .image_key_map
                .get(&key)
                .and_then(|previous| rr.currently_registered_images.get(previous))
                .is_some_and(|previous| {
                    let p = previous.descriptor;
                    (p.format, p.width, p.height, p.stride, p.offset)
                        == (
                            descriptor.format,
                            descriptor.width,
                            descriptor.height,
                            descriptor.stride,
                            descriptor.offset,
                        )
                });
        let dirty = if same_shape {
            pending.clipped_to(descriptor.width, descriptor.height)
        } else {
            ImageDirtyRect::All
        };
        let relist = update
            && descriptor.format.is_nv12()
            && rr
                .image_key_map
                .get(&key)
                .and_then(|previous| rr.currently_registered_images.get(previous))
                .is_some_and(|previous| {
                    previous.descriptor.format.is_nv12()
                        && previous.descriptor.format != descriptor.format
                });
        if let Some(old) = rr.image_key_map.get(&key).copied() {
            if old != hash {
                if matches!(slot, azul_core::resources::ImageSlot::TextHalo(_)) {
                    // A halo's picture is one slot's (`TextHalo::slot`): no other
                    // key shows it, and `release_frame` would look at every key
                    // of the window to find that out - for each of the pictures
                    // a frame replaces (2,200 keys x 40 pictures a frame on a
                    // municipal map).
                    // Unless the picture is painted from another slot by now and
                    // that slot was planned before this one (two windows on one
                    // thread share the kept pictures: the other's scan put a new
                    // picture into this slot and the old one got a new slot at
                    // its next use): its registration is then that slot's, and
                    // removing it left the label that paints it without a halo
                    // for good (audit A5-15).
                    if rr
                        .currently_registered_images
                        .get(&old)
                        .is_some_and(|resolved| resolved.key == key)
                    {
                        rr.currently_registered_images.remove(&old);
                        rr.image_last_seen_epoch.remove(&old);
                    }
                } else {
                    rr.release_frame(key, old);
                }
            }
        }
        rr.image_key_map.insert(key, hash);
        rr.currently_registered_images.insert(
            hash,
            ResolvedImage {
                key,
                descriptor: *descriptor,
            },
        );
        uploads.push(OverlayImageUpload {
            key,
            update,
            descriptor: *descriptor,
            data: data.clone(),
            chroma,
            drop_chroma,
            dirty,
            relist,
        });
    }
    uploads
}

/// An azul dirty region as WebRender's: origin + size becomes the min / max
/// device corners. The one translation for every image update.
pub(crate) fn translate_dirty_rect(
    dirty: &ImageDirtyRect,
) -> DirtyRect<i32, webrender::api::units::DevicePixel> {
    use webrender::euclid::{Box2D, Point2D};

    match dirty {
        ImageDirtyRect::All => DirtyRect::All,
        ImageDirtyRect::Partial(rect) => DirtyRect::Partial(Box2D::new(
            Point2D::new(rect.origin.x as i32, rect.origin.y as i32),
            Point2D::new(
                (rect.origin.x + rect.size.width) as i32,
                (rect.origin.y + rect.size.height) as i32,
            ),
        )),
    }
}

/// The two WebRender descriptors of an NV12 image: the R8 luma plane and the
/// RG8 chroma plane, both views into the one buffer (the chroma plane starts
/// at `offset = y_len`), so neither plane is copied out. `None` for a format
/// that is not NV12 or a size WebRender cannot take.
pub fn nv12_plane_descriptors(
    descriptor: &azul_core::resources::ImageDescriptor,
) -> Option<(WrImageDescriptor, WrImageDescriptor)> {
    use azul_core::resources::Nv12Layout;
    use webrender::api::ImageFormat;

    if !descriptor.format.is_nv12() {
        return None;
    }
    let layout = Nv12Layout::new(descriptor.width, descriptor.height);
    let w = i32::try_from(layout.width).ok()?;
    let h = i32::try_from(layout.height).ok()?;
    let cw = i32::try_from(layout.chroma_width).ok()?;
    let ch = i32::try_from(layout.chroma_height).ok()?;
    let offset = i32::try_from(layout.y_len()).ok()?;
    let flags = WrImageDescriptorFlags::IS_OPAQUE;
    Some((
        WrImageDescriptor {
            format: ImageFormat::R8,
            size: DeviceIntSize::new(w, h),
            stride: Some(w),
            offset: 0,
            flags,
        },
        WrImageDescriptor {
            format: ImageFormat::RG8,
            size: DeviceIntSize::new(cw, ch),
            stride: Some(cw.checked_mul(2)?),
            offset,
            flags,
        },
    ))
}

/// An NV12 image of a display list (not a video tile's frame): its luma
/// plane under `add.key`, its chroma plane under a key of its own, recorded
/// in `nv12_chroma_keys` so the display list and the image GC find it.
fn nv12_add_images(
    rr: &mut azul_core::resources::RendererResources,
    namespace: azul_core::resources::IdNamespace,
    add: AddImage,
) -> Vec<webrender::ResourceUpdate> {
    let Some((y_desc, uv_desc)) = nv12_plane_descriptors(&add.descriptor) else {
        return Vec::new();
    };
    let chroma = *rr
        .nv12_chroma_keys
        .entry(add.key)
        .or_insert_with(|| ImageKey::unique_image_slot(namespace));
    let data = translate_image_data(add.data);
    vec![
        webrender::ResourceUpdate::AddImage(WrAddImage {
            key: translate_image_key(add.key),
            descriptor: y_desc,
            data: data.clone(),
            tiling: None,
        }),
        webrender::ResourceUpdate::AddImage(WrAddImage {
            key: translate_image_key(chroma),
            descriptor: uv_desc,
            data,
            tiling: None,
        }),
    ]
}

/// What [`upload_overlay_images`] put into the transaction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OverlayUploads {
    /// A frame was uploaded: the transaction changes what is on screen.
    pub changed: bool,
    /// A tile got its key in THIS transaction, so the display list
    /// WebRender holds does not reference it yet and must be resent once.
    pub new_slot: bool,
}

/// Upload the content overlay's new raw frames (video tiles, canvases, a
/// background layer's callback frames drawn on the CPU) into their slots'
/// stable keys: `add_image` the first time, `update_image` after - of the
/// dirty rect only, when the app named one.
fn upload_overlay_images(layout_window: &mut LayoutWindow, txn: &mut WrTransaction) -> OverlayUploads {
    let namespace = layout_window.id_namespace;
    let LayoutWindow {
        ref mut content_overlay,
        ref mut renderer_resources,
        ..
    } = *layout_window;
    let uploads = plan_overlay_image_uploads(
        content_overlay
            .iter_images()
            .map(|(&(dom, node), image)| {
                let pending = content_overlay
                    .image_dirty(dom, node)
                    .unwrap_or(ImageDirtyRect::All);
                (azul_core::resources::ImageSlot::Node(dom, node), image, pending)
            })
            // A background layer's new frame is new as a whole.
            .chain(content_overlay.iter_backgrounds().map(|(slot, image)| {
                (
                    azul_core::resources::ImageSlot::Background(*slot),
                    image,
                    ImageDirtyRect::All,
                )
            })),
        renderer_resources,
        namespace,
    );
    // Every overlay image is now either in this transaction or already up.
    content_overlay.clear_image_dirty();
    let mut result = OverlayUploads::default();
    for upload in uploads {
        result.changed = true;
        result.new_slot |= !upload.update || upload.relist;
        let key = translate_image_key(upload.key);
        let data = translate_image_data(upload.data);
        match (upload.chroma, nv12_plane_descriptors(&upload.descriptor)) {
            (Some(chroma), Some((y_desc, uv_desc))) => {
                let chroma_key = translate_image_key(chroma.key);
                if upload.update {
                    txn.update_image(key, y_desc, data.clone(), &DirtyRect::All);
                } else {
                    txn.add_image(key, y_desc, data.clone(), None);
                }
                if chroma.update {
                    txn.update_image(chroma_key, uv_desc, data, &DirtyRect::All);
                } else {
                    txn.add_image(chroma_key, uv_desc, data, None);
                    // A tile whose frames turned into NV12 keeps its key, but
                    // the display list has to name the new chroma key.
                    result.new_slot = true;
                }
            }
            _ => {
                let desc = wr_translate_image_descriptor(&upload.descriptor);
                if upload.update {
                    txn.update_image(key, desc, data, &translate_dirty_rect(&upload.dirty));
                } else {
                    txn.add_image(key, desc, data, None);
                }
            }
        }
        if let Some(stale) = upload.drop_chroma {
            txn.delete_image(translate_image_key(stale));
            // The display list still pushes this tile as a YUV image.
            result.new_slot = true;
        }
    }
    result
}

/// Number of frames an image may be absent from every display list before it
/// is `DeleteImage`d. ≥2 mirrors the GL texture cache's "current + previous"
/// double-buffer retention (`gl_texture_cache::remove_old_epochs`) so an image
/// that flickers out for a frame isn't needlessly re-uploaded.
const IMAGE_GC_KEEP_EPOCHS: u32 = 2;

/// Mark this frame's live images, then return `DeleteImage`s for every
/// registered image not seen for more than [`IMAGE_GC_KEEP_EPOCHS`] frames,
/// evicting them from `currently_registered_images` / `image_key_map` /
/// `image_last_seen_epoch` as it goes. `pub` so the image-lifecycle test can
/// drive the GC without standing up a WebRender transaction.
pub fn collect_stale_image_deletes(
    layout_window: &mut LayoutWindow,
    live_image_hashes: &azul_core::FastBTreeSet<azul_core::resources::ImageRefHash>,
) -> Vec<ResourceUpdate> {
    use azul_core::resources::ResourceUpdate;

    let now = layout_window.epoch.into_u32();
    let rr = &mut layout_window.renderer_resources;

    // Refresh the last-seen stamp for everything referenced this frame.
    for hash in live_image_hashes.iter() {
        rr.image_last_seen_epoch.insert(*hash, now);
    }

    // A registered image with no (or a too-old) stamp is stale. `now` wraps at
    // ~u32::MAX (Epoch::increment), a ~2-year runtime at 60 fps; treat a
    // last-seen that is *ahead* of `now` (only possible right after a wrap) as
    // "just seen" so a wrap can't mass-delete live images.
    let mut to_delete: Vec<azul_core::resources::ImageRefHash> = Vec::new();
    for hash in rr.currently_registered_images.keys() {
        if live_image_hashes.contains(hash) {
            continue;
        }
        let stale = match rr.image_last_seen_epoch.get(hash) {
            Some(&last) => now >= last && now.saturating_sub(last) > IMAGE_GC_KEEP_EPOCHS,
            None => true, // registered but never stamped → from before GC existed
        };
        if stale {
            to_delete.push(*hash);
        }
    }

    let mut deletes = Vec::with_capacity(to_delete.len());
    for hash in to_delete {
        if let Some(resolved) = rr.currently_registered_images.remove(&hash) {
            rr.image_key_map.remove(&resolved.key);
            rr.image_last_seen_epoch.remove(&hash);
            deletes.push(ResourceUpdate::DeleteImage(resolved.key));
            // An NV12 image is two renderer images: its chroma plane goes too.
            if let Some(chroma) = rr.nv12_chroma_keys.remove(&resolved.key) {
                deletes.push(ResourceUpdate::DeleteImage(chroma));
            }
        }
    }
    if !deletes.is_empty() {
        log_debug!(
            LogCategory::Rendering,
            "[image-gc] DeleteImage x{} (registered now {})",
            deletes.len(),
            layout_window
                .renderer_resources
                .currently_registered_images
                .len()
        );
    }
    deletes
}

/// Number of frames a font may be absent from every display list before it is
/// deleted from the renderer: [`IMAGE_GC_KEEP_EPOCHS`]' rule, for fonts.
const FONT_GC_KEEP_EPOCHS: u32 = 2;

/// Mark this frame's live fonts (by hash), then delete every registered font
/// not drawn for more than [`FONT_GC_KEEP_EPOCHS`] frames: its instances and
/// the font itself ([`azul_core::resources::RendererResources::delete_font`]).
/// Before, a window kept every font it had ever drawn - and a document that
/// brings its own fonts (the pages of a PDF, each with its subset fonts)
/// added new ones on every page. `pub` so a test can drive the GC without a
/// WebRender transaction.
pub fn collect_stale_font_deletes(
    layout_window: &mut LayoutWindow,
    live_font_hashes: &azul_core::FastBTreeSet<u64>,
) -> Vec<ResourceUpdate> {
    let now = layout_window.epoch.into_u32();
    let rr = &mut layout_window.renderer_resources;
    for hash in live_font_hashes.iter() {
        rr.font_last_seen_epoch.insert(*hash, now);
    }
    // As for images: a last-seen AHEAD of `now` (only right after the epoch
    // wrapped) counts as just seen.
    let stale: Vec<u64> = rr
        .font_hash_map
        .keys()
        .filter(|hash| !live_font_hashes.contains(hash))
        .filter(|hash| match rr.font_last_seen_epoch.get(hash) {
            Some(&last) => now >= last && now.saturating_sub(last) > FONT_GC_KEEP_EPOCHS,
            None => true,
        })
        .copied()
        .collect();
    let deletes: Vec<ResourceUpdate> = stale
        .into_iter()
        .flat_map(|hash| rr.delete_font(hash))
        .collect();
    if !deletes.is_empty() {
        log_debug!(
            LogCategory::Rendering,
            "[font-gc] {} delete(s) (registered now {})",
            deletes.len(),
            rr.currently_registered_fonts.len()
        );
    }
    deletes
}

/// Generate a new WebRender frame
///
/// This function sets up the scene and tells WebRender to render.
/// Uses DomId-based pipeline management for virtualized view support.
pub fn generate_frame(
    txn: &mut WrTransaction,
    layout_window: &mut LayoutWindow,
    render_api: &mut WrRenderApi,
    display_list_was_rebuilt: bool,
    gl_context: &azul_core::gl::OptionGlContextPtr,
) {
    publish_stale_gpu_frame(layout_window);
    let _timed = gpu_time(GpuTime::Transaction);
    let physical_size = layout_window.current_window_state.size.get_physical_size();
    let framebuffer_size =
        DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

    // Don't render if window is minimized (width/height = 0)
    if framebuffer_size.width == 0 || framebuffer_size.height == 0 {
        // Nothing is drawn, so no frame is owed for the halos either: the
        // pass that follows the window's return scans the lists again.
        layout_window.gpu_state_manager.halos_owed = false;
        return;
    }

    // A full frame rebuilds the scene, so "identical to what I sent last time"
    // is the wrong question for the property values, the images and the scroll
    // offsets: they go across again regardless. (The display lists are another
    // matter: WebRender keeps the ones it has, `submit_display_lists`.)
    layout_window
        .gpu_state_manager
        .invalidate_submitted_digests();

    // The frame's content first - every callback whose inputs changed draws
    // its frame with this frame's GL context, into the display lists - and
    // its GL textures under their slots' keys, before anything below
    // registers the lists' images or translates them: a frame produced after
    // the translation would show one frame late. A slot keyed for the first
    // time is named by no list WebRender holds: the lists go again.
    let lists_rebuilt = prepare_frame_content_gl(layout_window, gl_context);
    let textures = register_overlay_textures(layout_window, txn);
    // Text halos a scan left for a later frame are drawn by the scan of this one
    // (`GpuStateManager::halos_owed`): its lists are translated again with them.
    let display_list_was_rebuilt = display_list_was_rebuilt
        || lists_rebuilt
        || textures.new_slot
        || layout_window.gpu_state_manager.halos_owed;

    // CRITICAL: Build display list FIRST, then set root pipeline (matching upstream WebRender
    // order)
    let root_pipeline_id = wr_translate_pipeline_id(PipelineId(0, layout_window.document_id.id));

    // If display list was rebuilt, add resources and display lists to this transaction FIRST
    if display_list_was_rebuilt {
        log_debug!(
            LogCategory::Rendering,
            "[generate_frame] Display list was rebuilt - adding resources and display lists to \
             transaction"
        );

        // Collect + register font and image resources, then add them to the
        // transaction. Shared with build_webrender_transaction.
        let dpi = layout_window.current_window_state.size.get_hidpi_factor();
        let overlay_new_slot = register_frame_resources(layout_window, txn, dpi);

        // Translate the DOMs' display lists the renderer does not hold yet and
        // add them to the transaction (a DOM whose list it holds is neither
        // translated nor sent: see `submit_display_lists`). A slot keyed for the
        // first time names an image the held lists do not: they go again.
        let submission = submit_display_lists(
            txn,
            layout_window,
            textures.new_slot || overlay_new_slot,
        );
        log_debug!(
            LogCategory::Rendering,
            "[generate_frame] Display lists: {} translated, {} held by the renderer, {} failed",
            submission.translated.len(),
            submission.held.len(),
            submission.failed.len()
        );
        if submission.failed.is_empty() {
            note_display_lists_submitted(layout_window);
        }

        // Increment epoch after using it
        layout_window.epoch.increment();
    } else {
        log_debug!(
            LogCategory::Rendering,
            "[generate_frame] Display list unchanged - skipping scene builder"
        );
        let _ = skip_scene_builder_unless_it_carries_resources(txn);
    }

    // CRITICAL: Set root pipeline AFTER display list (matching upstream WebRender order)
    log_debug!(
        LogCategory::Rendering,
        "[generate_frame] Setting root pipeline to {:?}",
        root_pipeline_id
    );
    txn.set_root_pipeline(root_pipeline_id);

    // Update document view size (in case window was resized)
    let view_rect =
        DeviceIntRect::from_origin_and_size(DeviceIntPoint::new(0, 0), framebuffer_size);
    let hidpi_factor = layout_window.current_window_state.size.get_hidpi_factor();
    log_debug!(
        LogCategory::Rendering,
        "[generate_frame] Setting document view: {:?}, hidpi: {}",
        view_rect,
        hidpi_factor.inner.get()
    );
    // NOTE: azul_layout outputs coordinates in CSS pixels (logical pixels), like a HTML engine.
    // WebRender's device_pixel_scale handles the conversion to device pixels.
    txn.set_document_view(view_rect, DevicePixelScale::new(hidpi_factor.inner.get()));

    // Process VirtualView updates (if any callbacks requested re-rendering)
    process_virtual_view_updates(layout_window, txn);

    // Scroll all nodes to their current positions
    let _ = scroll_all_nodes(layout_window, txn);

    // Synchronize GPU values (transforms, opacities, etc.)
    let _ = synchronize_gpu_values(layout_window, txn);

    log_debug!(
        LogCategory::Rendering,
        "[generate_frame] Calling generate_frame on transaction"
    );
    txn.generate_frame(0, WrRenderReasons::empty());
    gpu_frame_requested();
    layout_window.note_frame_presented();

    log_debug!(
        LogCategory::Rendering,
        "[generate_frame] Sending unified transaction (root_pipeline + document_view + resources \
         + display_lists) to document {:?}",
        layout_window.document_id
    );
}

/// Synchronize scroll positions from ScrollManager to WebRender.
///
/// Returns `true` if the offsets DIFFER from the ones last pushed. Unchanged
/// offsets are still written into the transaction (cheap, and keeps WebRender's
/// view authoritative) but are not, by themselves, a reason to build a frame.
pub fn scroll_all_nodes(layout_window: &mut LayoutWindow, txn: &mut WrTransaction) -> bool {
    use webrender::api::{units::LayoutVector2D as WrLayoutVector2D, SampledScrollOffset};

    // Get HiDPI factor for scaling scroll offsets
    // Display list coordinates are in physical pixels (scaled by DPI), so scroll
    // offsets must also be scaled to match.
    let hidpi_factor = layout_window
        .current_window_state
        .size
        .get_hidpi_factor()
        .inner
        .get();

    // Digest of everything pushed below, so an identical set of offsets can be
    // recognised as "not a reason to draw".
    let mut digest = FrameDigest::new();

    // Iterate through all DOMs
    for (dom_id, layout_result) in &layout_window.layout_results {
        let pipeline_id = PipelineId(dom_id.inner as u32, layout_window.document_id.id);

        // Get scroll states for this DOM
        let scroll_states = layout_window
            .scroll_manager
            .get_scroll_states_for_dom(*dom_id);

        // Forward node -> id map, built once: the per-node reverse `find`
        // hid the scroll-id collision as a silent `continue` (a scroller's
        // offsets simply never reached WebRender).
        let node_to_scroll_id: std::collections::HashMap<_, _> = layout_result
            .scroll_id_to_node_id
            .iter()
            .map(|(&sid, &nid)| (nid, sid))
            .collect();

        // Update each scrollable node
        for (node_id, scroll_position) in scroll_states {
            let Some(&scroll_id) = node_to_scroll_id.get(&node_id) else {
                continue;
            };

            let external_scroll_id = wr_translate_external_scroll_id(
                azul_core::hit_test::ExternalScrollId(scroll_id, pipeline_id),
            );

            // Calculate scroll offset (origin of children_rect within parent_rect)
            // Scale by HiDPI factor to match physical pixel coordinates in display list
            let scroll_offset = WrLayoutVector2D::new(
                scroll_position.children_rect.origin.x * hidpi_factor,
                scroll_position.children_rect.origin.y * hidpi_factor,
            );

            digest.push_u64(scroll_id);
            digest.push_u64(u64::from(pipeline_id.0));
            digest.push_f32(scroll_offset.x);
            digest.push_f32(scroll_offset.y);

            // WebRender expects scroll offsets as sampled offsets
            txn.set_scroll_offsets(
                external_scroll_id,
                vec![SampledScrollOffset {
                    offset: scroll_offset,
                    generation: 0, // Generation counter for APZ
                }],
            );
        }
    }

    // A `VirtualView` has no scroll frame of its own in the parent list: its
    // child dom is an iframe pipeline pushed at the host item's bounds
    // (`compositor2`, `DisplayListItem::VirtualView`). WebRender gives every
    // iframe pipeline a root scroll frame, `ExternalScrollId(0, pipeline)`,
    // and moving that frame here, with the other offsets, slides the rows
    // to the pixel without a new display list.
    for (child_dom_id, offset) in layout_window.virtual_view_frame_scroll_offsets() {
        let pipeline_id = PipelineId(child_dom_id.inner as u32, layout_window.document_id.id);
        let scroll_offset =
            WrLayoutVector2D::new(offset.x * hidpi_factor, offset.y * hidpi_factor);
        digest.push_u64(0);
        digest.push_u64(u64::from(pipeline_id.0));
        digest.push_f32(scroll_offset.x);
        digest.push_f32(scroll_offset.y);
        txn.set_scroll_offsets(
            wr_translate_external_scroll_id(azul_core::hit_test::ExternalScrollId(
                0,
                pipeline_id,
            )),
            vec![SampledScrollOffset {
                offset: scroll_offset,
                generation: 0,
            }],
        );
    }

    layout_window
        .gpu_state_manager
        .submitted_digest_changed(SubmittedDigest::ScrollOffsets, digest.finish())
}

/// `AZ_WALK_GPU_VALUES=1`: [`synchronize_gpu_values`] walks and hashes the bound
/// values on every frame, as it did before it could tell from the write
/// generation that none was written (A6-26), and sends them when the walk finds
/// another set than the one sent. The switch for the real window: a value that
/// is shown late or not at all without it and at once with it was written
/// around its stamp. Read once per process.
fn walk_gpu_values_every_frame() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("AZ_WALK_GPU_VALUES").is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// Synchronize GPU-animated values (transforms, opacities) to WebRender.
///
/// Returns `true` if anything was actually written into `txn` — that is, if the
/// property set DIFFERS from the one last handed to the renderer. Re-sending an
/// identical set is not a reason to draw, and treating it as one is what made
/// an idle window repaint at vsync forever: the lightweight transaction pushed
/// the same two floats, asked for a frame, WebRender signalled `new_frame_ready`,
/// the backend presented, and the present ran the lightweight path again.
/// See `GpuStateManager::gpu_values_changed`.
#[must_use]
pub fn synchronize_gpu_values(layout_window: &mut LayoutWindow, txn: &mut WrTransaction) -> bool {
    use webrender::api::{DynamicProperties, PropertyValue};

    let _timed = gpu_time(GpuTime::Values);

    // Get DPI scale factor to match display list coordinate space.
    // Display list items are in logical CSS pixels scaled by DPI in compositor2.
    // Transform values must use the same scaling.
    let dpi_scale = layout_window
        .current_window_state
        .size
        .get_hidpi_factor()
        .inner
        .get();

    // (A6-26) Nothing the digest is made of is another thing than when it was taken: no
    // value or key of any DOM was written (every bound value stamps itself, whoever writes
    // it: `GpuStateManager::value_generation`), the scale is the same, every DOM shows the
    // very list it showed, and the digest kept is still that one (a full frame forgets it).
    // The walk below would give that digest and answer "not news": a release build does not
    // make it. A debug build makes it anyway and holds the answer against it (two asserts
    // below): a wrong skip would leave WebRender painting the old values.
    let values_as_digested = layout_window
        .gpu_state_manager
        .gpu_values_are_the_digested_ones(&layout_window.layout_results, dpi_scale);
    if values_as_digested && !cfg!(debug_assertions) && !walk_gpu_values_every_frame() {
        return false;
    }

    // FIRST WALK: is the set the one last sent? Nothing is allocated for the
    // answer - a pan, a tile, a hover and every frame of a fade's hold ask it
    // and get "yes". Every bound value, from THE shared source the CPU
    // renderer reads too (`GpuValueCache::for_each_bound_value`): scrollbar
    // thumbs and fades, CSS transforms and opacities, the animation channel,
    // and the backgrounds whose colour fades (`DisplayList::color_bindings`).
    //
    // The digest is a SUM of one hash per (kind, key, value): the values come
    // out of `HashMap`s, whose order is not a promise, and a sum does not
    // depend on it (the lists used to be collected, copied and sorted for
    // that, 320 KB for a map view's 2 000 transforms on every frame). Floats go
    // in bitwise: a NaN opacity must compare equal to itself, or the idle loop
    // this exists to break is back.
    let (mut transform_sum, mut transform_count) = (0u64, 0usize);
    let (mut float_sum, mut float_count) = (0u64, 0usize);
    let (mut color_sum, mut color_count) = (0u64, 0usize);
    for dom_id in layout_window.layout_results.keys() {
        let Some(gpu_cache) = layout_window.gpu_state_manager.get_cache(*dom_id) else {
            continue;
        };
        gpu_cache.for_each_bound_value(
            *dom_id,
            |key, transform| {
                let mut h = mix_value(0x7472_616e, key as u64);
                for row in &transform.m {
                    h = mix_value(
                        h,
                        (u64::from(row[0].to_bits()) << 32) | u64::from(row[1].to_bits()),
                    );
                    h = mix_value(
                        h,
                        (u64::from(row[2].to_bits()) << 32) | u64::from(row[3].to_bits()),
                    );
                }
                transform_sum = transform_sum.wrapping_add(h);
                transform_count += 1;
            },
            |key, opacity| {
                let h = mix_value(
                    mix_value(0x666c_6f61, key as u64),
                    u64::from(opacity.to_bits()),
                );
                float_sum = float_sum.wrapping_add(h);
                float_count += 1;
            },
        );
        gpu_cache.for_each_bound_color(|key, color| {
            let h = mix_value(
                mix_value(0x636f_6c6f, key as u64),
                u64::from(u32::from_be_bytes([color.r, color.g, color.b, color.a])),
            );
            color_sum = color_sum.wrapping_add(h);
            color_count += 1;
        });
    }

    // Nothing collected at all: nothing to send, nothing changed.
    if transform_count == 0 && float_count == 0 && color_count == 0 {
        debug_assert!(
            !values_as_digested,
            "a digest was taken of bound values, none is left, and the write generation did not move"
        );
        return false;
    }

    let mut digest = mix_value(transform_sum, transform_count as u64);
    digest = mix_value(mix_value(digest, float_sum), float_count as u64);
    digest = mix_value(mix_value(digest, color_sum), color_count as u64);
    // The values are sent in device px: another scale is another set.
    digest = mix_value(digest, u64::from(dpi_scale.to_bits()));
    debug_assert!(
        !values_as_digested
            || layout_window.gpu_state_manager.last_gpu_value_digest == Some(digest),
        "the bound values changed and their write generation did not"
    );
    if !values_as_digested {
        #[cfg(test)]
        BOUND_VALUES_WALKED.with(|n| n.set(n.get() + transform_count + float_count + color_count));
        // What this digest was taken of, for the frames that follow.
        let LayoutWindow {
            ref layout_results,
            ref mut gpu_state_manager,
            ..
        } = *layout_window;
        gpu_state_manager.note_gpu_value_digest(layout_results, dpi_scale, digest);
    }

    // The same values as last time are not news.
    if layout_window.gpu_state_manager.last_gpu_value_digest == Some(digest) {
        log_debug!(
            LogCategory::Rendering,
            "[synchronize_gpu_values] {} floats / {} transforms unchanged - not submitting",
            float_count,
            transform_count
        );
        return false;
    }
    layout_window.gpu_state_manager.last_gpu_value_digest = Some(digest);

    // SECOND WALK, only for a set that differs: the values as WebRender takes
    // them, in lists of the size the first walk counted.
    let mut properties = DynamicProperties {
        transforms: Vec::with_capacity(transform_count),
        floats: Vec::with_capacity(float_count),
        colors: Vec::with_capacity(color_count),
    };
    for dom_id in layout_window.layout_results.keys() {
        let Some(gpu_cache) = layout_window.gpu_state_manager.get_cache(*dom_id) else {
            continue;
        };
        gpu_cache.for_each_bound_value(
            *dom_id,
            |key, transform| {
                use webrender::api::units::LayoutTransform;
                // Translation scaled by DPI: compositor2 lays items out in
                // logical px x dpi_scale, and a transform acts in that space.
                // The perspective column is divided by it, as in the list's
                // own value (`compositor2`, `PushReferenceFrame`; A2-gpu-01).
                let wr_transform = LayoutTransform::new(
                    transform.m[0][0],
                    transform.m[0][1],
                    transform.m[0][2],
                    transform.m[0][3] / dpi_scale,
                    transform.m[1][0],
                    transform.m[1][1],
                    transform.m[1][2],
                    transform.m[1][3] / dpi_scale,
                    transform.m[2][0],
                    transform.m[2][1],
                    transform.m[2][2],
                    transform.m[2][3] / dpi_scale,
                    transform.m[3][0] * dpi_scale,
                    transform.m[3][1] * dpi_scale,
                    transform.m[3][2] * dpi_scale,
                    transform.m[3][3],
                );
                properties.transforms.push(PropertyValue {
                    key: webrender::api::PropertyBindingKey::new(key as u64),
                    value: wr_transform,
                });
            },
            |key, opacity| {
                properties.floats.push(PropertyValue {
                    key: webrender::api::PropertyBindingKey::new(key as u64),
                    value: opacity,
                });
            },
        );
        gpu_cache.for_each_bound_color(|key, color| {
            properties.colors.push(PropertyValue {
                key: webrender::api::PropertyBindingKey::new(key as u64),
                value: webrender::api::ColorF::new(
                    f32::from(color.r) / 255.0,
                    f32::from(color.g) / 255.0,
                    f32::from(color.b) / 255.0,
                    f32::from(color.a) / 255.0,
                ),
            });
        });
    }

    // The set REPLACES the one WebRender holds: `append_dynamic_properties`
    // alone only ever adds to its pending list (`SceneProperties::add_properties`
    // extends it, and nothing but a reset empties it), so every changed frame
    // left the whole set behind once more - a map view's 2 000 label
    // transforms per zoom step, kept twice, compared, hashed and cloned again
    // on every later frame (audit A4-01).
    txn.reset_dynamic_properties();
    txn.append_dynamic_properties(properties);
    gpu_count(|c| {
        c.dynamic_properties += (float_count + transform_count + color_count) as u64;
    });

    log_debug!(
        LogCategory::Rendering,
        "[synchronize_gpu_values] Updated {} float properties, {} transforms, {} colors",
        float_count,
        transform_count,
        color_count
    );

    true
}

/// One step of the digest of the bound values ([`synchronize_gpu_values`]):
/// `word` mixed into `h` (a multiply by the 64-bit golden ratio and a fold,
/// one per word where the byte-wise FNV took eight).
#[inline]
const fn mix_value(h: u64, word: u64) -> u64 {
    let x = (h ^ word).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^ (x >> 29)
}

// Additional Translation Functions

use azul_core::{
    geom::LogicalSize,
    resources::{FontInstanceKey, GlyphOptions},
    ui_solver::GlyphInstance,
};
use azul_css::props::{
    basic::{
        color::{ColorF as CssColorF, ColorU as CssColorU},
        pixel::DEFAULT_FONT_SIZE,
    },
    style::border_radius::StyleBorderRadius,
};
use webrender::api::{
    units::LayoutSize as WrLayoutSize, BorderRadius as WrBorderRadius, ColorF as WrColorF,
    ColorU as WrColorU, GlyphInstance as WrGlyphInstance, GlyphOptions as WrGlyphOptions,
};

#[inline(always)]
const fn wr_translate_color_u(input: CssColorU) -> WrColorU {
    WrColorU {
        r: input.r,
        g: input.g,
        b: input.b,
        a: input.a,
    }
}

#[inline(always)]
pub const fn wr_translate_color_f(input: CssColorF) -> WrColorF {
    WrColorF {
        r: input.r,
        g: input.g,
        b: input.b,
        a: input.a,
    }
}

#[inline]
pub fn wr_translate_logical_size(size: LogicalSize) -> WrLayoutSize {
    WrLayoutSize::new(size.width, size.height)
}

#[inline]
pub fn wr_translate_border_radius(
    border_radius: StyleBorderRadius,
    rect_size: LogicalSize,
) -> WrBorderRadius {
    let StyleBorderRadius {
        top_left,
        top_right,
        bottom_left,
        bottom_right,
    } = border_radius;

    let w = rect_size.width;
    let h = rect_size.height;

    // The "w / h" is necessary to convert percentage-based values into pixels, for example
    // "border-radius: 50%;"

    let top_left_px_h = top_left.to_pixels_internal(w, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);
    let top_left_px_v = top_left.to_pixels_internal(h, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);

    let top_right_px_h = top_right.to_pixels_internal(w, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);
    let top_right_px_v = top_right.to_pixels_internal(h, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);

    let bottom_left_px_h = bottom_left.to_pixels_internal(w, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);
    let bottom_left_px_v = bottom_left.to_pixels_internal(h, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);

    let bottom_right_px_h =
        bottom_right.to_pixels_internal(w, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);
    let bottom_right_px_v =
        bottom_right.to_pixels_internal(h, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE);

    WrBorderRadius {
        top_left: WrLayoutSize::new(top_left_px_h, top_left_px_v),
        top_right: WrLayoutSize::new(top_right_px_h, top_right_px_v),
        bottom_left: WrLayoutSize::new(bottom_left_px_h, bottom_left_px_v),
        bottom_right: WrLayoutSize::new(bottom_right_px_h, bottom_right_px_v),
    }
}

#[inline]
const fn wr_translate_id_namespace(
    ns: azul_core::resources::IdNamespace,
) -> webrender::api::IdNamespace {
    webrender::api::IdNamespace(ns.0)
}

#[inline]
pub fn wr_translate_font_instance_key(key: FontInstanceKey) -> WrFontInstanceKey {
    WrFontInstanceKey(wr_translate_id_namespace(key.namespace), key.key as u32)
}

#[inline]
fn wr_translate_glyph_options(opts: GlyphOptions) -> WrGlyphOptions {
    WrGlyphOptions {
        render_mode: wr_translate_font_render_mode(opts.render_mode),
        flags: wr_translate_font_instance_flags(opts.flags),
    }
}

#[inline]
fn wr_translate_font_render_mode(
    mode: azul_core::resources::FontRenderMode,
) -> webrender::api::FontRenderMode {
    use azul_core::resources::FontRenderMode::*;
    match mode {
        Mono => webrender::api::FontRenderMode::Mono,
        Alpha => webrender::api::FontRenderMode::Alpha,
        Subpixel => webrender::api::FontRenderMode::Subpixel,
    }
}

#[inline]
fn wr_translate_font_instance_flags(
    flags: azul_core::resources::FontInstanceFlags,
) -> webrender::api::FontInstanceFlags {
    use azul_core::resources::*;

    let mut wr_flags = webrender::api::FontInstanceFlags::empty();

    if flags & FONT_INSTANCE_FLAG_SYNTHETIC_BOLD != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::SYNTHETIC_BOLD;
    }
    if flags & FONT_INSTANCE_FLAG_EMBEDDED_BITMAPS != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::EMBEDDED_BITMAPS;
    }
    if flags & FONT_INSTANCE_FLAG_SUBPIXEL_BGR != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::SUBPIXEL_BGR;
    }
    if flags & FONT_INSTANCE_FLAG_TRANSPOSE != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::TRANSPOSE;
    }
    if flags & FONT_INSTANCE_FLAG_FLIP_X != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::FLIP_X;
    }
    if flags & FONT_INSTANCE_FLAG_FLIP_Y != 0 {
        wr_flags |= webrender::api::FontInstanceFlags::FLIP_Y;
    }

    wr_flags
}

#[inline]
fn wr_translate_layouted_glyphs(glyphs: &[GlyphInstance]) -> Vec<WrGlyphInstance> {
    glyphs
        .iter()
        .map(|g| WrGlyphInstance {
            index: g.index,
            point: webrender::api::units::LayoutPoint::new(g.point.x, g.point.y),
        })
        .collect()
}

/// Translate border radius from azul-css to WebRender

/// Translate border style from azul-css to WebRender
#[inline]
fn wr_translate_border_style(
    style: azul_css::props::style::border::BorderStyle,
) -> webrender::api::BorderStyle {
    use azul_css::props::style::border::BorderStyle::*;
    match style {
        None => webrender::api::BorderStyle::None,
        Solid => webrender::api::BorderStyle::Solid,
        Double => webrender::api::BorderStyle::Double,
        Dotted => webrender::api::BorderStyle::Dotted,
        Dashed => webrender::api::BorderStyle::Dashed,
        Hidden => webrender::api::BorderStyle::Hidden,
        Groove => webrender::api::BorderStyle::Groove,
        Ridge => webrender::api::BorderStyle::Ridge,
        Inset => webrender::api::BorderStyle::Inset,
        Outset => webrender::api::BorderStyle::Outset,
    }
}

/// Get WebRender border from Azul border properties
/// Returns None if no border should be rendered
pub fn get_webrender_border(
    rect_size: LogicalSize,
    radii: StyleBorderRadius,
    widths: azul_layout::solver3::display_list::StyleBorderWidths,
    colors: azul_layout::solver3::display_list::StyleBorderColors,
    styles: azul_layout::solver3::display_list::StyleBorderStyles,
    hidpi: f32,
) -> Option<(
    webrender::api::units::LayoutSideOffsets,
    webrender::api::BorderDetails,
)> {
    use azul_css::{css::CssPropertyValue, props::basic::color::ColorU};
    use webrender::api::{
        units::LayoutSideOffsets as WrLayoutSideOffsets, BorderDetails as WrBorderDetails,
        BorderRadius as WrBorderRadius, BorderSide as WrBorderSide, NormalBorder as WrNormalBorder,
    };

    let (width_top, width_right, width_bottom, width_left) = (
        widths
            .top
            .and_then(|w| w.get_property().cloned())
            .map(|w| w.inner),
        widths
            .right
            .and_then(|w| w.get_property().cloned())
            .map(|w| w.inner),
        widths
            .bottom
            .and_then(|w| w.get_property().cloned())
            .map(|w| w.inner),
        widths
            .left
            .and_then(|w| w.get_property().cloned())
            .map(|w| w.inner),
    );

    let (style_top, style_right, style_bottom, style_left) = (
        styles
            .top
            .and_then(|s| s.get_property().cloned())
            .map(|s| s.inner),
        styles
            .right
            .and_then(|s| s.get_property().cloned())
            .map(|s| s.inner),
        styles
            .bottom
            .and_then(|s| s.get_property().cloned())
            .map(|s| s.inner),
        styles
            .left
            .and_then(|s| s.get_property().cloned())
            .map(|s| s.inner),
    );

    let no_border_style = style_top.is_none()
        && style_right.is_none()
        && style_bottom.is_none()
        && style_left.is_none();

    let no_border_width = width_top.is_none()
        && width_right.is_none()
        && width_bottom.is_none()
        && width_left.is_none();

    // border has all borders set to border: none; or all border-widths set to none
    if no_border_style || no_border_width {
        return None;
    }

    let has_no_border_radius =
        radii
            .top_left
            .to_pixels_internal(rect_size.width, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE)
            == 0.0
            && radii.top_right.to_pixels_internal(
                rect_size.width,
                DEFAULT_FONT_SIZE,
                DEFAULT_FONT_SIZE,
            ) == 0.0
            && radii.bottom_left.to_pixels_internal(
                rect_size.width,
                DEFAULT_FONT_SIZE,
                DEFAULT_FONT_SIZE,
            ) == 0.0
            && radii.bottom_right.to_pixels_internal(
                rect_size.width,
                DEFAULT_FONT_SIZE,
                DEFAULT_FONT_SIZE,
            ) == 0.0;

    let (color_top, color_right, color_bottom, color_left) = (
        colors
            .top
            .and_then(|ct| ct.get_property().cloned())
            .map(|c| c.inner)
            .unwrap_or(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
        colors
            .right
            .and_then(|cr| cr.get_property().cloned())
            .map(|c| c.inner)
            .unwrap_or(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
        colors
            .bottom
            .and_then(|cb| cb.get_property().cloned())
            .map(|c| c.inner)
            .unwrap_or(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
        colors
            .left
            .and_then(|cl| cl.get_property().cloned())
            .map(|c| c.inner)
            .unwrap_or(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
    );

    // `rect_size` is the border box in device px, and WebRender takes the
    // widths and radii in device px too. A width is snapped to whole device
    // pixels (rounded down, but never below one for a visible line), as
    // Chrome snaps it, so a 1px hairline stays one crisp line at any scale.
    let hidpi = if hidpi.is_finite() && hidpi > 0.0 {
        hidpi
    } else {
        1.0
    };
    let css_size = LogicalSize::new(rect_size.width / hidpi, rect_size.height / hidpi);
    let device_width = |css: f32| {
        let device = css * hidpi;
        if device > 0.0 && device < 1.0 {
            1.0
        } else {
            device.floor()
        }
    };
    let border_widths = WrLayoutSideOffsets::new(
        width_top
            .map(|v| {
                device_width(v.to_pixels_internal(
                    css_size.height,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0),
        width_right
            .map(|v| {
                device_width(v.to_pixels_internal(
                    css_size.width,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0),
        width_bottom
            .map(|v| {
                device_width(v.to_pixels_internal(
                    css_size.height,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0),
        width_left
            .map(|v| {
                device_width(v.to_pixels_internal(
                    css_size.width,
                    DEFAULT_FONT_SIZE,
                    DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0),
    );

    let border_details = WrBorderDetails::Normal(WrNormalBorder {
        top: WrBorderSide {
            color: wr_translate_color_u(color_top).into(),
            style: style_top
                .map(wr_translate_border_style)
                .unwrap_or(webrender::api::BorderStyle::None),
        },
        left: WrBorderSide {
            color: wr_translate_color_u(color_left).into(),
            style: style_left
                .map(wr_translate_border_style)
                .unwrap_or(webrender::api::BorderStyle::None),
        },
        right: WrBorderSide {
            color: wr_translate_color_u(color_right).into(),
            style: style_right
                .map(wr_translate_border_style)
                .unwrap_or(webrender::api::BorderStyle::None),
        },
        bottom: WrBorderSide {
            color: wr_translate_color_u(color_bottom).into(),
            style: style_bottom
                .map(wr_translate_border_style)
                .unwrap_or(webrender::api::BorderStyle::None),
        },
        radius: if has_no_border_radius {
            WrBorderRadius::zero()
        } else {
            // A radius in px is in CSS px and one in % is of the box: both
            // are resolved against the box in CSS px, then scaled.
            let r = wr_translate_border_radius(radii, css_size);
            WrBorderRadius {
                top_left: r.top_left * hidpi,
                top_right: r.top_right * hidpi,
                bottom_left: r.bottom_left * hidpi,
                bottom_right: r.bottom_right * hidpi,
            }
        },
        do_aa: true,
    });

    Some((border_widths, border_details))
}

/// Build a complete atomic WebRender transaction (matching upstream WebRender pattern)
/// This creates ONE transaction with: resources + display lists + root_pipeline + document_view
pub fn build_webrender_transaction(
    txn: &mut WrTransaction,
    layout_window: &mut LayoutWindow,
    render_api: &mut WrRenderApi,
    gl_context: &azul_core::gl::OptionGlContextPtr,
) -> Result<(), &'static str> {
    let _ = render_api;
    build_full_transaction(txn, layout_window, gl_context)
}

/// [`build_webrender_transaction`] itself. It asks nothing of the renderer's
/// handle, so a test without a renderer builds a frame's transaction with it.
pub(crate) fn build_full_transaction(
    txn: &mut WrTransaction,
    layout_window: &mut LayoutWindow,
    gl_context: &azul_core::gl::OptionGlContextPtr,
) -> Result<(), &'static str> {
    publish_stale_gpu_frame(layout_window);
    let _timed = gpu_time(GpuTime::Transaction);
    log_debug!(
        LogCategory::Rendering,
        "[build_atomic_txn] Building atomic transaction"
    );

    // Get sizes
    let physical_size = layout_window.current_window_state.size.get_physical_size();
    let framebuffer_size =
        DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
    let dpi = layout_window.current_window_state.size.get_hidpi_factor();

    // Get root pipeline ID
    let root_pipeline_id = wr_translate_pipeline_id(PipelineId(0, layout_window.document_id.id));

    // Step 0: the frame's content - every callback whose inputs changed
    // draws its frame with this frame's GL context - before anything below
    // registers the display lists' images or translates them (which this
    // transaction does in any case).
    let _ = prepare_frame_content_gl(layout_window, gl_context);

    // Step 0.5: the overlay's GL textures (callback frames of image nodes and
    // background layers) under their slots' keys, BEFORE the display-list
    // scan below, so their frames are found registered and not added again
    // under a per-frame key (the raw ones go the same way, first thing in
    // `register_frame_resources`).
    log_debug!(
        LogCategory::Rendering,
        "[build_atomic_txn] Step 0.5: Registering the overlay's GL textures"
    );
    layout_window
        .gpu_state_manager
        .invalidate_submitted_digests();
    let textures = register_overlay_textures(layout_window, txn);

    // Step 1: Collect + register font and image resources, then add them to the
    // transaction. Shared with generate_frame.
    log_debug!(
        LogCategory::Rendering,
        "[build_atomic_txn] Step 1: Collecting + registering font/image resources"
    );
    let overlay_new_slot = register_frame_resources(layout_window, txn, dpi);

    // Step 1.7: Pre-populate scrollbar opacity keys in GPU cache BEFORE building
    // display lists. The display list generator reads opacity_key from the GPU cache
    // to embed PropertyBinding references into ScrollBarStyled items. If we don't
    // populate these keys first, the display list will have opacity_key=None and
    // WebRender won't be able to animate the scrollbar opacity via DynamicProperties.
    {
        let system_callbacks = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
        for (dom_id, layout_result) in &layout_window.layout_results {
            LayoutWindow::synchronize_scrollbar_opacity(
                &mut layout_window.gpu_state_manager,
                &layout_window.scroll_manager,
                *dom_id,
                &layout_result.layout_tree,
                &layout_result.styled_dom,
                &system_callbacks,
                azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                    500,
                )),
                azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                    200,
                )),
            );
        }
    }

    // Step 1.8: Pre-populate scrollbar transform keys in GPU cache BEFORE building
    // display lists, so the display list can embed the correct PropertyBinding keys
    // for GPU-animated thumb positioning.
    {
        let LayoutWindow {
            ref layout_results,
            ref scroll_manager,
            ref mut gpu_state_manager,
            ..
        } = *layout_window;

        for (dom_id, layout_result) in layout_results {
            gpu_state_manager.refresh_scroll_driven_transforms(
                *dom_id,
                scroll_manager,
                &layout_result.layout_tree,
            );
        }
    }

    // Step 2: Build and add the display lists the renderer does not hold to the
    // transaction. A slot keyed for the first time names an image the held
    // lists do not: they go again.
    set_display_lists(txn, layout_window, textures.new_slot || overlay_new_slot)?;

    // Step 3: Set root pipeline
    log_debug!(
        LogCategory::Rendering,
        "[build_atomic_txn] Step 3: Setting root pipeline {:?}",
        root_pipeline_id
    );
    txn.set_root_pipeline(root_pipeline_id);

    // Step 4: Set document view
    let view_rect =
        DeviceIntRect::from_origin_and_size(DeviceIntPoint::new(0, 0), framebuffer_size);
    let hidpi_factor = layout_window.current_window_state.size.get_hidpi_factor();
    log_debug!(
        LogCategory::Rendering,
        "[build_atomic_txn] Step 4: Setting document view {:?}, hidpi: {}",
        view_rect,
        hidpi_factor.inner.get()
    );
    // NOTE: azul_layout outputs coordinates in CSS pixels (logical pixels).
    txn.set_document_view(view_rect, DevicePixelScale::new(hidpi_factor.inner.get()));

    build_webrender_transaction_tail(txn, layout_window)
}

/// The display lists of a transaction, and what became of each DOM's.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct DisplayListSubmission {
    /// The DOMs whose list was translated and put into the transaction.
    pub(crate) translated: Vec<DomId>,
    /// The DOMs whose list the renderer already holds: neither translated nor
    /// sent.
    pub(crate) held: Vec<DomId>,
    /// The DOMs whose list could not be translated.
    pub(crate) failed: Vec<DomId>,
}

/// What every display list is translated against besides the list itself: the
/// renderer's scale (every coordinate is scaled by it). A held list is held
/// only while this is what it was translated against.
///
/// The text halos were part of it, as the NUMBER of halos drawn so far (a
/// shadow is translated to its picture once it has one): every halo a frame
/// drew sent every list of the window again. What a list paints of halos is
/// now followed per DOM ([`register_text_halos`], `stale_display_lists`): a picture
/// that takes the place of its stand-in - the same key, the same rect - sends
/// no list, and one that does not sends the list of its DOM.
pub(crate) fn translation_env(layout_window: &LayoutWindow) -> u64 {
    let scale = u64::from(
        layout_window
            .current_window_state
            .size
            .get_hidpi_factor()
            .inner
            .get()
            .to_bits(),
    );
    scale << 32
}

/// Record the lists as the renderer now holds them (see
/// `GpuStateManager::note_display_lists_submitted`).
fn note_display_lists_submitted(layout_window: &mut LayoutWindow) {
    let env = translation_env(layout_window);
    let LayoutWindow {
        ref layout_results,
        ref mut gpu_state_manager,
        ..
    } = *layout_window;
    gpu_state_manager.note_display_lists_submitted(layout_results, env);
    // Every list the renderer holds is translated against the halos of the
    // last scan now.
    gpu_state_manager.stale_display_lists.clear();
}

/// Translate the DOMs' display lists and put them into `txn` at the current
/// epoch, EACH DOM'S LIST ONCE and only if the renderer does not hold it.
///
/// `WebRender` keeps every pipeline's list until another replaces it
/// (`Scene::set_display_list`), and a `VirtualView` is an iframe of the
/// child DOM's own pipeline. So the list a DOM had when it was last sent is
/// still what `WebRender` draws, and a frame that changed only the host's list
/// (a hover, the caret) sends the host's list and leaves the child's where it
/// is: a map's 5 000-item child was translated TWICE per frame before (once
/// inline by the host's translation, once as a DOM of its own), and again for a
/// restyle that did not touch it. `force_all` sends everything (an image a held
/// list names was keyed anew).
///
/// The caller records what was sent ([`note_display_lists_submitted`]) once
/// the transaction is complete.
fn submit_display_lists(
    txn: &mut WrTransaction,
    layout_window: &LayoutWindow,
    force_all: bool,
) -> DisplayListSubmission {
    let physical_size = layout_window.current_window_state.size.get_physical_size();
    let viewport_size =
        DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
    let dpi = layout_window.current_window_state.size.get_hidpi_factor();
    let env = translation_env(layout_window);
    let epoch = webrender::api::Epoch(layout_window.epoch.into_u32());

    log_debug!(
        LogCategory::Rendering,
        "[submit_display_lists] {} DOMs, epoch {:?}, force_all={}",
        layout_window.layout_results.len(),
        epoch,
        force_all
    );

    let mut submission = DisplayListSubmission::default();
    for (dom_id, layout_result) in &layout_window.layout_results {
        if !force_all
            && !layout_window
                .gpu_state_manager
                .stale_display_lists
                .contains(dom_id)
            && layout_window.gpu_state_manager.display_list_is_held(
                *dom_id,
                &layout_result.display_list,
                env,
            )
        {
            submission.held.push(*dom_id);
            continue;
        }

        let pipeline_id = wr_translate_pipeline_id(PipelineId(
            dom_id.inner as u32,
            layout_window.document_id.id,
        ));
        let translation = {
            let _timed = gpu_time(GpuTime::Translate);
            crate::desktop::compositor2::translate_displaylist_to_wr(
                &layout_result.display_list,
                pipeline_id,
                viewport_size,
                &layout_window.renderer_resources,
                dpi,
                Vec::new(), // Resources already added above
                &layout_window.layout_results,
                layout_window.document_id.id,
            )
        };
        match translation {
            Ok((_, built_display_list)) => {
                let (items, bytes) = (
                    layout_result.display_list.items.len() as u64,
                    built_display_list.size_in_bytes() as u64,
                );
                gpu_count(|c| {
                    c.lists_translated += 1;
                    c.items_translated += items;
                    c.list_bytes += bytes;
                });
                log_debug!(
                    LogCategory::Rendering,
                    "[submit_display_lists] Adding display list for DOM {} (pipeline {:?}, \
                     epoch {:?}), display_list_size_in_bytes={}",
                    dom_id.inner,
                    pipeline_id,
                    epoch,
                    built_display_list.size_in_bytes(),
                );
                txn.set_display_list(epoch, (pipeline_id, built_display_list));
                submission.translated.push(*dom_id);
            }
            Err(e) => {
                log_debug!(
                    LogCategory::Rendering,
                    "[submit_display_lists] Error building display list for DOM {}: {}",
                    dom_id.inner,
                    e
                );
                submission.failed.push(*dom_id);
            }
        }
    }
    submission
}

/// Put the display lists the renderer does not hold into `txn` (see
/// [`submit_display_lists`]) and record what it now holds. The display-list
/// half of a full transaction, and what the lightweight one sends when a video
/// tile got its stable image key and WebRender's list does not name it yet.
fn set_display_lists(
    txn: &mut WrTransaction,
    layout_window: &mut LayoutWindow,
    force_all: bool,
) -> Result<DisplayListSubmission, &'static str> {
    let submission = submit_display_lists(txn, layout_window, force_all);
    if !submission.failed.is_empty() {
        return Err("Failed to build display list");
    }
    note_display_lists_submitted(layout_window);
    Ok(submission)
}

/// Steps 5-7 of a full transaction: scroll offsets, scrollbar fades, GPU
/// values, the frame request, and the next epoch.
fn build_webrender_transaction_tail(
    txn: &mut WrTransaction,
    layout_window: &mut LayoutWindow,
) -> Result<(), &'static str> {
    // Step 5: Add scroll offsets
    log_debug!(
        LogCategory::Rendering,
        "[build_atomic_txn] Step 5: Adding scroll offsets"
    );
    let _ = scroll_all_nodes(layout_window, txn);

    // Step 5.5: Update scrollbar opacity for overlay fade-in/fade-out
    {
        let system_callbacks = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
        for (dom_id, layout_result) in &layout_window.layout_results {
            LayoutWindow::synchronize_scrollbar_opacity(
                &mut layout_window.gpu_state_manager,
                &layout_window.scroll_manager,
                *dom_id,
                &layout_result.layout_tree,
                &layout_result.styled_dom,
                &system_callbacks,
                azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                    500,
                )),
                azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                    200,
                )),
            );
        }
    }

    // Step 6: Synchronize GPU values
    log_debug!(
        LogCategory::Rendering,
        "[build_atomic_txn] Step 6: Synchronizing GPU values"
    );
    let _ = synchronize_gpu_values(layout_window, txn);

    // Step 7: Generate frame
    log_debug!(
        LogCategory::Rendering,
        "[build_atomic_txn] Step 7: Calling generate_frame"
    );
    txn.generate_frame(0, webrender::api::RenderReasons::empty());
    gpu_frame_requested();
    layout_window.note_frame_presented();

    // Increment epoch for next frame
    layout_window.epoch.increment();

    log_debug!(
        LogCategory::Rendering,
        "[build_atomic_txn] Transaction ready to send"
    );
    Ok(())
}

/// Build a lightweight WebRender transaction that ONLY re-invokes image callbacks,
/// updates scroll offsets, and generates a frame — WITHOUT rebuilding display lists.
///
/// This is used when the DOM structure hasn't changed (detected by `is_layout_equivalent`),
/// so the display lists from the previous frame are still valid. Only GL texture content
/// (from image callbacks) may have changed.
///
/// Compared to `build_webrender_transaction`, this skips:
/// - Font resource collection (fonts haven't changed)
/// - Image resource collection (static images haven't changed)
/// - Display list translation (layout positions haven't changed)
/// - Root pipeline setup (already set from previous frame)
///
/// This reduces frame time from ~5-15ms to ~0.1-0.5ms for unchanged DOMs.
/// What a lightweight ("nothing was rebuilt") transaction turned out to be.
///
/// `changed == false` means the transaction carries nothing the renderer does
/// not already have: no frame was requested, and the caller should drop it
/// rather than send it. See `submit_lightweight_frame`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LightweightFrame {
    /// Does this transaction change what is on screen?
    pub changed: bool,
}

pub fn build_image_only_transaction(
    txn: &mut WrTransaction,
    layout_window: &mut LayoutWindow,
    _render_api: &mut WrRenderApi,
    gl_context: &azul_core::gl::OptionGlContextPtr,
) -> Result<LightweightFrame, &'static str> {
    build_lightweight_transaction(txn, layout_window, gl_context)
}

/// [`build_image_only_transaction`] itself. It asks nothing of the renderer's
/// handle, so a test without a renderer makes the pass of a frame that laid
/// nothing out with it.
pub(crate) fn build_lightweight_transaction(
    txn: &mut WrTransaction,
    layout_window: &mut LayoutWindow,
    gl_context: &azul_core::gl::OptionGlContextPtr,
) -> Result<LightweightFrame, &'static str> {
    publish_stale_gpu_frame(layout_window);
    let _timed = gpu_time(GpuTime::Transaction);
    log_debug!(
        LogCategory::Rendering,
        "[build_image_only_txn] Building lightweight transaction (layout unchanged)"
    );

    // Step 1: the frame's content - every callback whose inputs changed
    // draws its frame with this frame's GL context - and the GL textures
    // under their slots' keys (a new frame of a slot is pixels only).
    let lists_rebuilt = prepare_frame_content_gl(layout_window, gl_context);
    let textures = register_overlay_textures(layout_window, txn);

    // Step 1.5: Video tiles. A new frame of an in-place-replaced node image
    // (`change_node_image`, the content overlay) goes into the node's stable
    // key: `update_image`, pixels only. After the callbacks, so a callback's
    // raw frame is picked up in the same transaction.
    let overlay = upload_overlay_images(layout_window, txn);
    let images_changed = textures.changed || overlay.changed;

    // Step 1.6: Text halos. A scan that ran out of its budget left halos for the
    // frames that follow (`GpuStateManager::halos_owed`, which keeps the platform
    // loop asking for them): this one scans again and draws the next ones. A
    // picture drawn into the place of its stand-in is an image update under the
    // key the lists name, and no list goes; a DOM whose list would be translated
    // differently now is marked and its list goes below (`register_text_halos`).
    let resources_before_the_scan = txn.resource_updates.len();
    if layout_window.gpu_state_manager.halos_owed {
        let _timed = gpu_time(GpuTime::Resources);
        register_image_resources(layout_window, txn);
    }
    // What that scan put into the transaction is already recorded as
    // registered (`currently_registered_images`): the transaction has to
    // reach the renderer, whatever else it carries.
    let scan_registered_images = txn.resource_updates.len() != resources_before_the_scan;

    // Step 2: Skip scene builder (display lists haven't changed) - unless a
    // slot got its key in this transaction (the display list WebRender holds
    // names the slot's previous image) or the content step built a list
    // again: then the lists go once more, at a new epoch.
    // A list patched in place since the last submission (a colour or a
    // gradient fade, a content patch) goes too: nothing else tells the
    // renderer, which kept drawing the list as it was.
    let env_changed = layout_window.gpu_state_manager.submitted_translation_env
        != translation_env(layout_window);
    let lists_unsubmitted = env_changed
        || !layout_window.gpu_state_manager.stale_display_lists.is_empty()
        || layout_window
            .gpu_state_manager
            .display_lists_unsubmitted(&layout_window.layout_results);
    let lists_sent = overlay.new_slot || textures.new_slot || lists_rebuilt || lists_unsubmitted;
    if lists_sent {
        // The lists go at a new epoch, and the texture cache frees a GL
        // texture two epochs after the last list that drew it was SENT
        // (`gl_texture_cache::remove_old_epochs`, after every render). The
        // full transaction renews the stamps while it registers the images;
        // this one registers nothing, so a GL image of a list lost its
        // texture at the first list patched in place (a caret's blink, a
        // colour fade) and was drawn from texture 0, black (audit A4-22).
        renew_gl_textures_of_the_lists(layout_window);
        set_display_lists(txn, layout_window, overlay.new_slot || textures.new_slot)?;
        layout_window.epoch.increment();
    } else {
        let _ = skip_scene_builder_unless_it_carries_resources(txn);
    }

    // Step 3: Add scroll offsets (scroll position may have changed)
    let scroll_changed = scroll_all_nodes(layout_window, txn);

    // Step 3.5: Update scrollbar thumb transforms based on current scroll offsets.
    // This must happen AFTER scroll_all_nodes (which updates WebRender scroll frames)
    // and BEFORE synchronize_gpu_values (which pushes transforms to WebRender).
    // We destructure to satisfy the borrow checker (disjoint field borrows).
    {
        let LayoutWindow {
            ref layout_results,
            ref scroll_manager,
            ref mut gpu_state_manager,
            ..
        } = *layout_window;

        for (dom_id, layout_result) in layout_results {
            gpu_state_manager.refresh_scroll_driven_transforms(
                *dom_id,
                scroll_manager,
                &layout_result.layout_tree,
            );
        }
    }

    // Step 3.6: Update scrollbar opacity for overlay fade-in/fade-out
    {
        let system_callbacks = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
        for (dom_id, layout_result) in &layout_window.layout_results {
            LayoutWindow::synchronize_scrollbar_opacity(
                &mut layout_window.gpu_state_manager,
                &layout_window.scroll_manager,
                *dom_id,
                &layout_result.layout_tree,
                &layout_result.styled_dom,
                &system_callbacks,
                azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                    500,
                )),
                azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                    200,
                )),
            );
        }
    }

    // Step 4: Synchronize GPU values (opacity/transform animations)
    let gpu_values_changed = synchronize_gpu_values(layout_window, txn);

    // Step 5: Generate a frame ONLY if this transaction actually changes what
    // is on screen.
    //
    // Asking for one unconditionally is what made an idle GPU window repaint
    // forever: WebRender built the frame, `new_frame_ready` woke the backend,
    // the backend presented, and the present ran this path again — measured at
    // 1087 identical submissions and 58.6 fps on an X11 window nobody was
    // touching. Nothing here is a reason to draw unless the images, the scroll
    // offsets or the animated GPU values differ from what the renderer already
    // has.
    //
    // A transaction that carries display lists IS a change: a list patched in place
    // (the caret's blink, a halo that was drawn) alters no property value, scroll
    // offset or image, and sent without a frame it was dropped with the rest of the
    // idle pass - the renderer never drew it, while the lists were recorded as sent.
    let changed = images_changed
        || scroll_changed
        || gpu_values_changed
        || lists_sent
        || scan_registered_images;
    if changed {
        txn.generate_frame(0, webrender::api::RenderReasons::empty());
        gpu_frame_requested();
        layout_window.note_frame_presented();
    }

    log_debug!(
        LogCategory::Rendering,
        "[build_image_only_txn] Lightweight transaction ready (images={} scroll={} gpu={} -> {})",
        images_changed,
        scroll_changed,
        gpu_values_changed,
        if changed {
            "frame requested"
        } else {
            "idle, no frame"
        }
    );

    Ok(LightweightFrame { changed })
}

/// A transaction without a display list may skip the scene builder thread
/// and go straight to the render backend - unless it carries resource
/// updates. Returns whether it skips.
///
/// A transaction that skips "can race ahead of transactions that don't"
/// (`Transaction::skip_scene_builder`): an `update_image` sent directly could
/// reach the render backend before the `add_image` of the same key that is
/// still on its way through the scene builder, and the backend panics on an
/// update of an image it does not hold (`ResourceCache::update_image_template`).
/// Today every shell waits for both threads after every send
/// (`flush_scene_builder`), which hides the race; with this rule every
/// transaction that adds, updates or deletes a resource takes the same queue,
/// so add-before-update holds by construction, flush or no flush (audit
/// A4-24, A5's H8). A transaction with resources and no list costs the scene
/// builder nothing but the hop: it builds a scene only for a display list.
fn skip_scene_builder_unless_it_carries_resources(txn: &mut WrTransaction) -> bool {
    if txn.resource_updates.is_empty() {
        txn.skip_scene_builder();
        true
    } else {
        false
    }
}

/// Every GL texture a display list of the window draws is stamped with the
/// epoch the lists are about to be sent at: it lives while a list that draws
/// it is sent (see `collect_image_resource_updates`, which does the same for
/// a full transaction).
fn renew_gl_textures_of_the_lists(layout_window: &LayoutWindow) {
    for image in layout_window
        .images_in_display_lists()
        .filter(|image| image.is_gl_texture())
    {
        let _ = crate::desktop::gl_texture_cache::keep_alive(
            &layout_window.document_id,
            &azul_core::resources::image_ref_hash_to_external_image_id(image.get_hash()),
            layout_window.epoch,
        );
    }
}

/// The frame's content on the GL path: every render callback whose inputs
/// changed draws its frame - image nodes and background layers, with this
/// frame's GL context - and the frames go through the content chokepoint
/// into the display lists (`LayoutWindow::prepare_frame_gl`, the twin of the
/// CPU hosts' `prepare_frame_cpu`). One call per submitted frame, before the
/// frame's images are registered or its lists translated. Returns whether a
/// display list was built again (a material's sheen layer appeared or went):
/// the lists must go to `WebRender` anew.
#[must_use]
fn prepare_frame_content_gl(
    layout_window: &mut LayoutWindow,
    gl_context: &azul_core::gl::OptionGlContextPtr,
) -> bool {
    let lists_rebuilt = layout_window.prepare_frame_gl(gl_context);

    // Callbacks may have left arbitrary GL state behind: flush the pending
    // commands BEFORE WebRender samples the produced textures (without it
    // some drivers show incomplete frames), and reset the bindings WebRender
    // assumes clean.
    #[cfg(feature = "gl_context_loader")]
    if let Some(gl) = gl_context.as_ref() {
        use gl_context_loader::gl;
        gl.flush();
        gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
        gl.bind_texture(gl::TEXTURE_2D, 0);
        gl.bind_renderbuffer(gl::RENDERBUFFER, 0);
        gl.disable(gl::FRAMEBUFFER_SRGB);
        gl.disable(gl::MULTISAMPLE);
        gl.use_program(0);
    }
    lists_rebuilt
}

/// Register the content overlay's GL textures - the callback frames of image
/// nodes and of background layers - each under its slot's STABLE key: the
/// slot's first frame is an `add_image`, every later one an `update_image`
/// (pixels only, so neither the display list nor the scene is rebuilt). A
/// background frame the key already holds is not sent again: a background
/// layer's frame never changes under its `ImageRef`. A node's is - a canvas
/// may draw into the texture it handed over before. The frame's hash
/// resolves to the key, and the frame the key showed before is forgotten -
/// never `DeleteImage`d (the image GC would delete the shared key), the key
/// lives on. Every texture is stamped with this epoch in the texture cache:
/// it lives while its slot shows it.
///
/// `changed`: the set of textures handed to `WebRender` differs from the one
/// handed over last time. `new_slot`: a key was added, so the display lists
/// `WebRender` holds do not name it yet and must go again.
fn register_overlay_textures(
    layout_window: &mut LayoutWindow,
    txn: &mut WrTransaction,
) -> OverlayUploads {
    use azul_core::resources::{DecodedImage, ResolvedImage};

    use crate::desktop::gl_texture_cache;

    // Collected first: the iteration borrows the window, the registration
    // mutates it. With each texture: whether it is sent again while the key
    // holds it already (a node's canvas, see above).
    let mut to_register: Vec<(
        azul_core::resources::ImageSlot,
        azul_core::resources::ExternalImageId,
        azul_core::resources::ImageRefHash,
        azul_core::gl::Texture,
        bool,
    )> = Vec::new();
    for ((dom_id, node_id), image_ref) in layout_window.content_overlay.iter_images() {
        if let DecodedImage::Gl(texture) = image_ref.get_data() {
            let id = gl_texture_cache::TextureSlotKey::new(*dom_id, *node_id).to_external_image_id();
            to_register.push((
                azul_core::resources::ImageSlot::Node(*dom_id, *node_id),
                id,
                image_ref.get_hash(),
                texture.clone(),
                true,
            ));
        }
    }
    for (slot, image_ref) in layout_window.content_overlay.iter_backgrounds() {
        if let DecodedImage::Gl(texture) = image_ref.get_data() {
            let id = gl_texture_cache::background_texture_id(slot);
            to_register.push((
                azul_core::resources::ImageSlot::Background(*slot),
                id,
                image_ref.get_hash(),
                texture.clone(),
                false,
            ));
        }
    }

    let namespace = layout_window.id_namespace;
    let mut result = OverlayUploads::default();
    let mut digest = FrameDigest::new();
    for (slot, external_image_id, produced_hash, texture, sent_again) in to_register {
        digest.push_u64(external_image_id.inner);
        digest.push_u64(produced_hash.inner);

        gl_texture_cache::insert_texture_by_id(
            layout_window.document_id,
            external_image_id,
            layout_window.epoch,
            texture.clone(),
        );

        let rr = &mut layout_window.renderer_resources;
        // The slot's image key is its own, minted from the counter the images
        // take their keys from (`RendererResources::image_slots`, as for the
        // frames in memory, which may share the slot: a node shows one or the
        // other). It was the slot's external image id - the DOM in the high
        // and the node in the low 32 bits, or a hash - and the renderer takes
        // the low 32 bits of a key: the texture of node 7 had the key of the
        // seventh image ever made, and of node 7 of every other DOM (audit
        // A4-11). The external image id stays what it was: the texture cache's.
        let image_key = match rr.image_slots.get(&slot) {
            Some(&key) if rr.image_key_map.contains_key(&key) => key,
            _ => {
                let key = ImageKey::unique_image_slot(namespace);
                rr.image_slots.insert(slot, key);
                key
            }
        };
        let shown = rr.image_key_map.get(&image_key).copied();
        let new_frame = shown != Some(produced_hash);
        if !new_frame && !sent_again {
            continue;
        }
        let descriptor = texture.get_descriptor();
        let wr_key = translate_image_key(image_key);
        let wr_descriptor = wr_translate_image_descriptor(&descriptor);
        let wr_data = WrImageData::External(webrender::api::ExternalImageData {
            id: webrender::api::ExternalImageId(external_image_id.inner),
            channel_index: 0,
            image_type: webrender::api::ExternalImageType::TextureHandle(
                webrender::api::ImageBufferKind::Texture2D,
            ),
            normalized_uvs: false,
        });
        match shown {
            Some(previous) => {
                txn.update_image(wr_key, wr_descriptor, wr_data, &DirtyRect::All);
                // The key shows this frame now: the previous one resolves only
                // through another slot's key that still shows it, and the image
                // GC must not delete this key for the old hash.
                if previous != produced_hash {
                    rr.release_frame(image_key, previous);
                }
            }
            None => {
                txn.add_image(wr_key, wr_descriptor, wr_data, None);
                result.new_slot = true;
            }
        }
        // Pixels sent again under an unchanged frame are no reason for a
        // frame of their own.
        result.changed |= new_frame;
        // The hash the (chokepoint-patched) display list carries resolves to
        // the stable key, so a later scene rebuild finds it.
        rr.currently_registered_images.insert(
            produced_hash,
            ResolvedImage {
                key: image_key,
                descriptor,
            },
        );
        rr.image_key_map.insert(image_key, produced_hash);
    }

    result.changed |= layout_window
        .gpu_state_manager
        .submitted_digest_changed(SubmittedDigest::Images, digest.finish());
    result
}

/// Process VirtualView updates requested by callbacks
///
/// This function handles manual VirtualView re-rendering triggered by
/// `trigger_virtual_view_rerender()`. It rebuilds display lists for VirtualViews that were already
/// re-rendered during layout, then submits only those pipelines to WebRender without rebuilding the
/// entire scene.
///
/// # Architecture
///
/// Each VirtualView gets its own WebRender pipeline with a stable PipelineId based on
/// (dom_id, node_id). When a VirtualView needs updating:
///
/// 1. The VirtualView callback was already re-invoked during the layout phase
/// 2. The layout result for that VirtualView's DOM exists in layout_results
/// 3. We just need to rebuild and submit that specific VirtualView's display list
/// 4. Other pipelines remain untouched
///
/// This allows efficient updates without full scene rebuilds.
///
/// # Arguments
///
/// * `layout_window` - The layout window with VirtualView state
/// * `txn` - The WebRender transaction to add updates to
fn process_virtual_view_updates(layout_window: &mut LayoutWindow, txn: &mut WrTransaction) {
    // Check if there are any pending VirtualView updates
    if layout_window.pending_virtual_view_updates.is_empty() {
        return;
    }

    log_debug!(
        LogCategory::Rendering,
        "[process_virtual_view_updates] Processing {} pending VirtualView updates",
        layout_window.pending_virtual_view_updates.len()
    );

    use webrender::api::Epoch as WrEpoch;

    let dpi = layout_window.current_window_state.size.get_hidpi_factor();
    let physical_size = layout_window.current_window_state.size.get_physical_size();
    let viewport_size = DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);

    // Collect all child DOM IDs that need their display lists rebuilt
    let mut child_dom_ids = Vec::new();

    for (parent_dom_id, node_ids) in &layout_window.pending_virtual_view_updates {
        // MWA-C-virtual_view: the pending map now carries per-node reasons;
        // this GPU display-list rebuild only needs the node ids.
        for node_id in node_ids.keys() {
            if let Some(child_dom_id) = layout_window
                .virtual_view_manager
                .get_nested_dom_id(*parent_dom_id, *node_id)
            {
                child_dom_ids.push((child_dom_id, *parent_dom_id, *node_id));
            }
        }
    }

    // Clear pending updates
    layout_window.pending_virtual_view_updates.clear();

    // For each VirtualView, rebuild and submit its display list
    let mut sent: Vec<(DomId, Arc<azul_layout::solver3::display_list::DisplayList>)> = Vec::new();
    for (child_dom_id, parent_dom_id, node_id) in child_dom_ids {
        // Get the layout result for the VirtualView's content
        let layout_result = match layout_window.layout_results.get(&child_dom_id) {
            Some(lr) => lr,
            None => {
                log_debug!(
                    LogCategory::Rendering,
                    "[process_virtual_view_updates] No layout result for child DOM {:?} (parent \
                     {:?}, node {:?})",
                    child_dom_id,
                    parent_dom_id,
                    node_id
                );
                continue;
            }
        };

        // Build the pipeline ID for this VirtualView
        let pipeline_id = wr_translate_pipeline_id(PipelineId(
            child_dom_id.inner as u32,
            layout_window.document_id.id,
        ));

        // Translate display list to WebRender
        let translation = {
            let _timed = gpu_time(GpuTime::Translate);
            crate::desktop::compositor2::translate_displaylist_to_wr(
                &layout_result.display_list,
                pipeline_id,
                viewport_size,
                &layout_window.renderer_resources,
                dpi,
                Vec::new(), // Resources should already be registered
                &layout_window.layout_results,
                layout_window.document_id.id,
            )
        };
        match translation {
            Ok((_, built_display_list)) => {
                let (items, bytes) = (
                    layout_result.display_list.items.len() as u64,
                    built_display_list.size_in_bytes() as u64,
                );
                gpu_count(|c| {
                    c.lists_translated += 1;
                    c.items_translated += items;
                    c.list_bytes += bytes;
                });
                log_debug!(
                    LogCategory::Rendering,
                    "[process_virtual_view_updates] Submitting display list for VirtualView DOM \
                     {} (pipeline {:?})",
                    child_dom_id.inner,
                    pipeline_id
                );

                // Submit the updated display list. A VirtualView inside it is
                // a DOM of its own in `layout_results`, which the caller
                // submits (or holds) like this one.
                txn.set_display_list(
                    WrEpoch(layout_window.epoch.into_u32()),
                    (pipeline_id, built_display_list),
                );
                sent.push((child_dom_id, Arc::clone(&layout_result.display_list)));
            }
            Err(e) => {
                log_debug!(
                    LogCategory::Rendering,
                    "[process_virtual_view_updates] Error building display list for VirtualView \
                     DOM {}: {}",
                    child_dom_id.inner,
                    e
                );
            }
        }
    }

    // The renderer holds these lists now (`GpuStateManager::display_list_is_held`).
    for (dom_id, list) in sent {
        layout_window
            .gpu_state_manager
            .note_one_display_list_submitted(dom_id, &list);
        layout_window.gpu_state_manager.stale_display_lists.remove(&dom_id);
    }
}

/// A video tile's frames on the GPU path: one renderer image per tile for its
/// whole life, whose pixels each new frame replaces (`update_image`), instead
/// of a new image key, a new texture and a display-list + scene rebuild per
/// frame.
#[cfg(test)]
mod overlay_upload_tests {
    use azul_core::{
        dom::{DomId, NodeId},
        resources::{
            IdNamespace, ImageDirtyRect, ImageRef, Nv12Layout, RawImage, RawImageData,
            RawImageFormat, RendererResources,
        },
    };
    use azul_css::props::basic::{LayoutPoint, LayoutRect, LayoutSize};
    use webrender::api::{DirtyRect, ImageFormat};

    use azul_core::resources::ImageSlot;

    use super::{nv12_plane_descriptors, plan_overlay_image_uploads, translate_dirty_rect};

    fn frame(format: RawImageFormat, w: usize, h: usize, fill: u8) -> ImageRef {
        let len = if format.is_nv12() {
            Nv12Layout::new(w, h).checked_total_len().expect("small")
        } else {
            w * h * 4
        };
        ImageRef::new_rawimage(RawImage {
            pixels: RawImageData::U8(vec![fill; len].into()),
            width: w,
            height: h,
            premultiplied_alpha: true,
            data_format: format,
            tag: Vec::new().into(),
        })
        .expect("a well-formed frame")
    }

    const TILE: ImageSlot = ImageSlot::Node(DomId { inner: 0 }, NodeId::new(3));
    const NS: IdNamespace = IdNamespace(7);

    #[test]
    fn a_tiles_frames_share_one_renderer_key_and_the_old_frame_is_forgotten() {
        let mut rr = RendererResources::default();
        let f1 = frame(RawImageFormat::BGRA8, 4, 2, 1);
        let first = plan_overlay_image_uploads([(TILE, &f1, ImageDirtyRect::All)].into_iter(), &mut rr, NS);
        assert_eq!(first.len(), 1);
        assert!(!first[0].update, "the tile's first frame adds its key");
        let key = first[0].key;
        assert_eq!(rr.image_slots.get(&TILE), Some(&key));
        assert_eq!(
            rr.currently_registered_images
                .get(&f1.get_hash())
                .map(|r| r.key),
            Some(key),
            "the display list's frame resolves to the tile's key"
        );

        let f2 = frame(RawImageFormat::BGRA8, 4, 2, 2);
        let second = plan_overlay_image_uploads([(TILE, &f2, ImageDirtyRect::All)].into_iter(), &mut rr, NS);
        assert_eq!(second.len(), 1);
        assert!(second[0].update, "a later frame UPDATES the same key: pixels only");
        assert_eq!(second[0].key, key);
        assert!(
            !rr.currently_registered_images.contains_key(&f1.get_hash()),
            "the previous frame is forgotten, not deleted: the key lives on"
        );
        assert_eq!(
            rr.currently_registered_images
                .get(&f2.get_hash())
                .map(|r| r.key),
            Some(key)
        );
        assert_eq!(rr.image_key_map.get(&key), Some(&f2.get_hash()));

        let again = plan_overlay_image_uploads([(TILE, &f2, ImageDirtyRect::All)].into_iter(), &mut rr, NS);
        assert!(again.is_empty(), "a frame already up uploads nothing");
    }

    /// The CPU render cache hands identical surfaces (one material, size and light) one frame,
    /// so two slots may show one ImageRef. Each holds it under its own key: when one slot's
    /// frame changes, the other's items still resolve to a key that shows the frame (they lost
    /// it when the frame lived only under the first slot's key, whose pixels the change
    /// replaced).
    #[test]
    fn two_slots_showing_one_frame_keep_it_when_one_of_them_moves_on() {
        const OTHER: ImageSlot = ImageSlot::Node(DomId { inner: 0 }, NodeId::new(4));
        let mut rr = RendererResources::default();
        let shared = frame(RawImageFormat::BGRA8, 4, 2, 1);
        let up = plan_overlay_image_uploads(
            [
                (TILE, &shared, ImageDirtyRect::All),
                (OTHER, &shared, ImageDirtyRect::All),
            ]
            .into_iter(),
            &mut rr,
            NS,
        );
        assert_eq!(up.len(), 2, "each slot holds the frame under its own key");
        let tile_key = rr.image_slots[&TILE];
        let other_key = rr.image_slots[&OTHER];
        assert_ne!(tile_key, other_key);

        let moved = frame(RawImageFormat::BGRA8, 4, 2, 2);
        let up = plan_overlay_image_uploads(
            [
                (TILE, &moved, ImageDirtyRect::All),
                (OTHER, &shared, ImageDirtyRect::All),
            ]
            .into_iter(),
            &mut rr,
            NS,
        );
        assert_eq!(up.len(), 1, "only the tile sends its new frame");
        assert_eq!(up[0].key, tile_key);
        assert_eq!(
            rr.currently_registered_images
                .get(&shared.get_hash())
                .map(|r| r.key),
            Some(other_key),
            "the other slot's items still find the frame"
        );
        assert_eq!(
            rr.currently_registered_images
                .get(&moved.get_hash())
                .map(|r| r.key),
            Some(tile_key)
        );
    }

    #[test]
    fn an_nv12_frame_is_two_planes_under_two_stable_keys_sharing_one_buffer() {
        let mut rr = RendererResources::default();
        let f1 = frame(RawImageFormat::NV12Rec709Video, 6, 4, 9);
        let up = plan_overlay_image_uploads([(TILE, &f1, ImageDirtyRect::All)].into_iter(), &mut rr, NS);
        let chroma = up[0].chroma.expect("NV12 has a chroma plane");
        assert!(!chroma.update);
        assert_ne!(chroma.key, up[0].key);
        assert_eq!(rr.nv12_chroma_keys.get(&up[0].key), Some(&chroma.key));

        let f2 = frame(RawImageFormat::NV12Rec709Video, 6, 4, 10);
        let up2 = plan_overlay_image_uploads([(TILE, &f2, ImageDirtyRect::All)].into_iter(), &mut rr, NS);
        assert!(up2[0].update);
        assert_eq!(
            up2[0].chroma.map(|c| (c.key, c.update)),
            Some((chroma.key, true)),
            "the chroma plane is updated in place too"
        );

        // The two planes are views into ONE buffer: no split copy.
        let (y, uv) = nv12_plane_descriptors(&up2[0].descriptor).expect("NV12 planes");
        assert_eq!(y.format, ImageFormat::R8);
        assert_eq!((y.size.width, y.size.height), (6, 4));
        assert_eq!((y.stride, y.offset), (Some(6), 0));
        assert_eq!(uv.format, ImageFormat::RG8);
        assert_eq!((uv.size.width, uv.size.height), (3, 2));
        assert_eq!((uv.stride, uv.offset), (Some(6), 24));
    }

    #[test]
    fn a_tile_that_turns_from_nv12_to_bgra_drops_its_chroma_plane() {
        let mut rr = RendererResources::default();
        let f1 = frame(RawImageFormat::NV12Rec601Video, 4, 4, 9);
        let up = plan_overlay_image_uploads([(TILE, &f1, ImageDirtyRect::All)].into_iter(), &mut rr, NS);
        let chroma = up[0].chroma.expect("NV12").key;
        let f2 = frame(RawImageFormat::BGRA8, 4, 4, 1);
        let up2 = plan_overlay_image_uploads([(TILE, &f2, ImageDirtyRect::All)].into_iter(), &mut rr, NS);
        assert!(up2[0].chroma.is_none());
        assert_eq!(up2[0].drop_chroma, Some(chroma));
        assert!(rr.nv12_chroma_keys.get(&up2[0].key).is_none());
    }

    // ---- A canvas: one image node whose app repaints part of it per pointer move ----

    fn partial(x: isize, y: isize, w: isize, h: isize) -> ImageDirtyRect {
        ImageDirtyRect::Partial(LayoutRect::new(
            LayoutPoint::new(x, y),
            LayoutSize::new(w, h),
        ))
    }

    #[test]
    fn a_later_frame_of_the_same_size_uploads_only_its_dirty_rect() {
        let mut rr = RendererResources::default();
        let f1 = frame(RawImageFormat::BGRA8, 64, 32, 1);
        let first = plan_overlay_image_uploads(
            [(TILE, &f1, partial(0, 0, 4, 4))].into_iter(),
            &mut rr,
            NS,
        );
        assert_eq!(
            first[0].dirty,
            ImageDirtyRect::All,
            "the first frame adds the key: the renderer has none of it yet"
        );
        let f2 = frame(RawImageFormat::BGRA8, 64, 32, 2);
        let second = plan_overlay_image_uploads(
            [(TILE, &f2, partial(8, 4, 16, 8))].into_iter(),
            &mut rr,
            NS,
        );
        assert!(second[0].update);
        assert_eq!(
            second[0].dirty,
            partial(8, 4, 16, 8),
            "only the rect the app repainted goes up again"
        );
    }

    #[test]
    fn a_frame_of_another_size_or_format_is_uploaded_whole() {
        let mut rr = RendererResources::default();
        let f1 = frame(RawImageFormat::BGRA8, 64, 32, 1);
        let _ = plan_overlay_image_uploads([(TILE, &f1, ImageDirtyRect::All)].into_iter(), &mut rr, NS);
        let smaller = frame(RawImageFormat::BGRA8, 32, 32, 2);
        let up = plan_overlay_image_uploads(
            [(TILE, &smaller, partial(0, 0, 2, 2))].into_iter(),
            &mut rr,
            NS,
        );
        assert!(up[0].update, "the tile keeps its key");
        assert_eq!(up[0].dirty, ImageDirtyRect::All, "a reshaped frame goes up whole");
        let nv12 = frame(RawImageFormat::NV12Rec709Video, 32, 32, 3);
        let up = plan_overlay_image_uploads(
            [(TILE, &nv12, partial(0, 0, 2, 2))].into_iter(),
            &mut rr,
            NS,
        );
        assert_eq!(up[0].dirty, ImageDirtyRect::All, "so does a frame in another format");
    }

    #[test]
    fn a_dirty_rect_reaching_past_the_frame_is_clipped_to_it() {
        let mut rr = RendererResources::default();
        let f1 = frame(RawImageFormat::BGRA8, 64, 32, 1);
        let _ = plan_overlay_image_uploads([(TILE, &f1, ImageDirtyRect::All)].into_iter(), &mut rr, NS);
        let f2 = frame(RawImageFormat::BGRA8, 64, 32, 2);
        let up = plan_overlay_image_uploads(
            [(TILE, &f2, partial(60, 30, 20, 20))].into_iter(),
            &mut rr,
            NS,
        );
        assert_eq!(up[0].dirty, partial(60, 30, 4, 2));
    }

    #[test]
    fn the_renderer_gets_the_same_rect_in_device_pixels() {
        match translate_dirty_rect(&partial(8, 4, 16, 8)) {
            DirtyRect::Partial(b) => assert_eq!(
                (b.min.x, b.min.y, b.max.x, b.max.y),
                (8, 4, 24, 12),
                "origin + size becomes min / max corners"
            ),
            DirtyRect::All => panic!("a partial rect must stay partial"),
        }
        assert!(matches!(
            translate_dirty_rect(&ImageDirtyRect::All),
            DirtyRect::All
        ));
    }
}

#[cfg(test)]
mod gl_texture_lifetime_tests {
    //! A GL texture WebRender draws lives as long as a display list draws it.
    //! WebRender looks an external image up at every render, and an image
    //! the texture cache no longer holds is drawn from texture 0: black.

    use std::rc::Rc;

    use azul_core::{
        dom::Dom,
        geom::{LogicalSize, PhysicalSize},
        gl::{GenericGlContext, GlContextPtr, Texture, TextureFlags},
        resources::{image_ref_hash_to_external_image_id, ImageRef, RawImageFormat, RendererResources},
        styled_dom::StyledDom,
        window::RendererType,
    };
    use azul_css::props::basic::ColorU;
    use azul_layout::{
        callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
    };
    use rust_fontconfig::FcFontCache;

    use super::{register_frame_resources, WrTransaction};
    use crate::desktop::{gl_texture_cache, gl_texture_integration};

    /// An 8 x 8 GL texture as an image. The context is a null one: every GL
    /// call is a no-op, which is all the cache's bookkeeping needs.
    fn gl_image() -> ImageRef {
        // SAFETY: `GenericGlContext` is `repr(C)` and holds only function
        // pointers, for which all-zero (null) is valid; the loader checks each
        // for null before it calls it.
        let null_gl = Rc::new(unsafe { core::mem::zeroed::<GenericGlContext>() });
        ImageRef::new_gltexture(Texture::create(
            42,
            TextureFlags {
                is_opaque: false,
                is_video_texture: false,
            },
            PhysicalSize { width: 8, height: 8 },
            ColorU::TRANSPARENT,
            GlContextPtr::new(RendererType::Software, null_gl),
            RawImageFormat::BGRA8,
        ))
    }

    fn lay_out(lw: &mut LayoutWindow, dom: Dom) {
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(200.0, 100.0);
        lw.current_window_state = ws.clone();
        lw.layout_and_generate_display_list(
            StyledDom::create_from_dom(dom),
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the window lays out");
    }

    /// One frame of the GPU path as far as the texture cache sees it: the
    /// display list's images are registered, the list goes out at a new
    /// epoch, WebRender renders it and the cache drops what is older.
    fn send_frame(lw: &mut LayoutWindow) {
        let dpi = lw.current_window_state.size.get_hidpi_factor();
        let mut txn = WrTransaction::new();
        register_frame_resources(lw, &mut txn, dpi);
        lw.epoch.increment();
        gl_texture_integration::remove_old_gl_textures(&lw.document_id, lw.epoch);
    }

    #[test]
    fn a_gl_image_a_display_list_draws_keeps_its_texture_over_every_frame() {
        let image = gl_image();
        let id = image_ref_hash_to_external_image_id(image.get_hash());
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        lay_out(
            &mut lw,
            Dom::create_body().with_child(Dom::create_image(image).with_css("width: 8px; height: 8px;")),
        );
        // A hover restyle, a resize, a theme switch: each sends the display
        // list again, and the image is in every one of them.
        for frame in 0..6 {
            send_frame(&mut lw);
            assert!(
                gl_texture_cache::get_texture(&id).is_some(),
                "frame {frame}: the display list draws the image, but its texture left the cache \
                 (WebRender would draw it black)"
            );
        }
    }

    /// A4-22. A list patched in place (a caret's blink, a colour fade) is sent
    /// by the lightweight transaction, at a new epoch, without the image
    /// registration of a full one. The GL image the list draws keeps its
    /// texture over any number of such sends. Mutation: without
    /// `renew_gl_textures_of_the_lists` the texture is gone after the first.
    #[test]
    fn a_gl_image_keeps_its_texture_while_its_list_is_only_sent_again() {
        let image = gl_image();
        let id = image_ref_hash_to_external_image_id(image.get_hash());
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        lay_out(
            &mut lw,
            Dom::create_body().with_child(Dom::create_image(image).with_css("width: 8px; height: 8px;")),
        );
        send_frame(&mut lw);
        assert!(gl_texture_cache::get_texture(&id).is_some(), "drawn, so kept");
        // What `build_image_only_transaction` does when a list is sent again:
        // the stamps, the lists at this epoch, the next epoch; then the render
        // and the cache's clean-up.
        for send in 0..6 {
            super::renew_gl_textures_of_the_lists(&lw);
            lw.epoch.increment();
            gl_texture_integration::remove_old_gl_textures(&lw.document_id, lw.epoch);
            assert!(
                gl_texture_cache::get_texture(&id).is_some(),
                "send {send}: the list still draws the image, but its texture left the cache"
            );
        }
    }

    /// A4-11. The image key of a canvas' slot is its own. It was the slot's
    /// external image id - the DOM in the high and the node in the low 32 bits -
    /// and the renderer's key is the low 32 bits: the canvas at node n of the
    /// window's DOM had key n, which is the key of the image with id n (an
    /// image's key is its id, a counter from 1). Here the canvas IS the node
    /// whose index is the id of a picture the same window shows: one renderer
    /// image for two pictures, and whichever was added last was drawn for both.
    ///
    /// Mutation: `key: external_image_id.inner` back in
    /// `register_overlay_textures` gives both the picture's key (when the
    /// picture's id is small enough to make a DOM of: under 4,096).
    #[test]
    fn a_canvas_slot_has_an_image_key_no_image_has() {
        use azul_core::{
            dom::{DomId, NodeId},
            resources::{ImageSlot, RawImage, RawImageData},
        };
        use azul_layout::overlay::ContentChange;

        let picture = |fill: u8| {
            ImageRef::new_rawimage(RawImage {
                pixels: RawImageData::U8(vec![fill; 8 * 8 * 4].into()),
                width: 8,
                height: 8,
                premultiplied_alpha: true,
                data_format: RawImageFormat::RGBA8,
                tag: Vec::new().into(),
            })
            .expect("a picture in memory")
        };
        let shown = picture(9);
        // The body is node 0, its children 1, 2, ..: the canvas is child
        // number `id` (as far as a DOM of that many nodes is worth making).
        let id = usize::try_from(shown.get_hash().inner).expect("an id");
        let fillers = id.saturating_sub(1).min(4096);
        let canvas = NodeId::new(fillers + 1);
        let mut body = Dom::create_body();
        for _ in 0..fillers {
            body = body.with_child(Dom::create_div());
        }
        body = body
            .with_child(Dom::create_image(picture(1)).with_css("width: 8px; height: 8px;"))
            .with_child(Dom::create_image(shown.clone()).with_css("width: 8px; height: 8px;"));
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        lay_out(&mut lw, body);

        // The canvas draws its first frame.
        let frame = gl_image();
        let _ = lw.apply_content_change(ContentChange::Image {
            dom_id: DomId::ROOT_ID,
            node_id: canvas,
            image: frame.clone(),
            dirty_rect: None,
        });
        let dpi = lw.current_window_state.size.get_hidpi_factor();
        let mut txn = WrTransaction::new();
        let textures = super::register_overlay_textures(&mut lw, &mut txn);
        assert!(textures.new_slot, "the slot's first frame: a new key");
        register_frame_resources(&mut lw, &mut txn, dpi);

        let rr = &lw.renderer_resources;
        let canvas_key = rr.currently_registered_images[&frame.get_hash()].key;
        let picture_key = rr.currently_registered_images[&shown.get_hash()].key;
        assert_eq!(
            rr.image_slots.get(&ImageSlot::Node(DomId::ROOT_ID, canvas)),
            Some(&canvas_key),
            "the slot keeps its key"
        );
        assert_ne!(
            super::translate_image_key(canvas_key),
            super::translate_image_key(picture_key),
            "the canvas at node {} and the picture with id {id} are two renderer images",
            canvas.index()
        );

        // The next frame of the canvas is new pixels under that key.
        let next = gl_image();
        let _ = lw.apply_content_change(ContentChange::Image {
            dom_id: DomId::ROOT_ID,
            node_id: canvas,
            image: next.clone(),
            dirty_rect: None,
        });
        let mut txn = WrTransaction::new();
        let textures = super::register_overlay_textures(&mut lw, &mut txn);
        assert!(!textures.new_slot && textures.changed);
        assert_eq!(
            lw.renderer_resources.currently_registered_images[&next.get_hash()].key,
            canvas_key
        );
    }

    #[test]
    fn a_gl_image_no_display_list_draws_any_more_lets_its_texture_go() {
        let image = gl_image();
        let id = image_ref_hash_to_external_image_id(image.get_hash());
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        lay_out(
            &mut lw,
            Dom::create_body().with_child(Dom::create_image(image).with_css("width: 8px; height: 8px;")),
        );
        send_frame(&mut lw);
        assert!(gl_texture_cache::get_texture(&id).is_some(), "drawn, so kept");
        lay_out(&mut lw, Dom::create_body());
        for _ in 0..6 {
            send_frame(&mut lw);
        }
        assert!(
            gl_texture_cache::get_texture(&id).is_none(),
            "a texture no display list has drawn for several frames is freed"
        );
    }
}

#[cfg(test)]
mod display_list_submission_tests {
    //! Which display lists a transaction translates and sends.
    //!
    //! A `VirtualView` is an iframe of its child DOM's own pipeline, and
    //! `WebRender` keeps every pipeline's list until another replaces it. A map
    //! window is a host list (the sidebars, the toolbar) and a child (the tiles,
    //! 5 000 items): the child was translated TWICE per frame (inline by the
    //! host's translation, and as a DOM of its own), and again for every
    //! restyle of the host that did not touch it (a hover, the caret blink:
    //! 2.3 s of a 38 000-item window, 0.27 s of a map frame in logging alone).

    use azul_core::{
        callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
        dom::{Dom, DomId},
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        refany::RefAny,
        resources::RendererResources,
        styled_dom::StyledDom,
    };
    use azul_layout::{
        callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
    };
    use rust_fontconfig::FcFontCache;

    use super::{note_display_lists_submitted, set_display_lists, submit_display_lists, WrTransaction};
    use crate::desktop::compositor2::TRANSLATIONS;

    /// Sixty rows of painted boxes: the child's list holds a few hundred items.
    extern "C" fn rows(_data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
        let mut dom = Dom::create_div();
        for i in 0..60 {
            dom = dom.with_child(
                Dom::create_div().with_css(
                    if i % 2 == 0 {
                        "height: 20px; background: #3366cc;"
                    } else {
                        "height: 20px; background: #cc6633;"
                    },
                ),
            );
        }
        let rect = LogicalRect::new(LogicalPosition::zero(), info.bounds.logical_size);
        VirtualViewReturn::with_dom(dom, rect, rect)
    }

    /// A host page: a bar, and a view over the rest.
    fn window() -> LayoutWindow {
        let mut dom = Dom::create_body()
            .with_child(Dom::create_div().with_css("height: 30px; background: #444444;"))
            .with_child(
                Dom::create_virtual_view(RefAny::new(()), VirtualViewCallback::create(rows))
                    .with_css("width: 300px; height: 200px;"),
            );
        let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(400.0, 300.0);
        lw.current_window_state = ws.clone();
        lw.layout_new_generation(
            styled_dom,
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the host lays out");
        lw
    }

    fn child_of(lw: &LayoutWindow) -> DomId {
        *lw.layout_results
            .keys()
            .find(|dom| **dom != DomId::ROOT_ID)
            .expect("the view's child DOM is laid out")
    }

    /// The translations `f` made on this thread, by the counter of the compositor.
    fn translations_during(f: impl FnOnce()) -> usize {
        let before = TRANSLATIONS.with(core::cell::Cell::get);
        f();
        TRANSLATIONS.with(core::cell::Cell::get) - before
    }

    #[test]
    fn every_dom_of_a_window_with_a_view_is_translated_once() {
        let mut lw = window();
        let child = child_of(&lw);
        assert!(
            lw.layout_results[&child].display_list.items.len() > 20,
            "the child's list holds the rows"
        );
        let mut txn = WrTransaction::new();
        let mut submission = None;
        let translations = translations_during(|| {
            submission = Some(set_display_lists(&mut txn, &mut lw, false).expect("translates"));
        });
        let submission = submission.expect("a submission");
        assert_eq!(
            submission.translated,
            vec![DomId::ROOT_ID, child],
            "the host and its child, each once"
        );
        assert!(submission.held.is_empty());
        assert_eq!(
            translations, 2,
            "two lists, two translations: the host's translation named the child's pipeline and \
             translated the child's list again, inline"
        );
    }

    #[test]
    fn a_second_frame_of_the_same_lists_translates_nothing() {
        let mut lw = window();
        let child = child_of(&lw);
        let mut txn = WrTransaction::new();
        set_display_lists(&mut txn, &mut lw, false).expect("translates");
        let mut again = None;
        let translations = translations_during(|| {
            again = Some(set_display_lists(&mut WrTransaction::new(), &mut lw, false).expect("ok"));
        });
        let again = again.expect("a submission");
        assert_eq!(translations, 0, "WebRender holds both lists");
        assert!(again.translated.is_empty());
        assert_eq!(again.held, vec![DomId::ROOT_ID, child]);
    }

    /// REGRESSION: a hover or a caret blink rebuilds the HOST's list. The child
    /// is the same list: it is neither translated nor sent again.
    #[test]
    fn a_rebuilt_host_list_sends_the_host_and_holds_the_view() {
        let mut lw = window();
        let child = child_of(&lw);
        set_display_lists(&mut WrTransaction::new(), &mut lw, false).expect("translates");
        let child_list = std::sync::Arc::clone(&lw.layout_results[&child].display_list);

        lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
        assert!(
            std::sync::Arc::ptr_eq(&child_list, &lw.layout_results[&child].display_list),
            "the host's restyle did not touch the child's list"
        );
        let mut got = None;
        let translations = translations_during(|| {
            got = Some(set_display_lists(&mut WrTransaction::new(), &mut lw, false).expect("ok"));
        });
        let got = got.expect("a submission");
        assert_eq!(got.translated, vec![DomId::ROOT_ID], "only the host's list is new");
        assert_eq!(got.held, vec![child]);
        assert_eq!(translations, 1, "one list translated, not the child's 60 rows again");
    }

    #[test]
    fn a_changed_view_list_is_sent_alone() {
        let mut lw = window();
        let child = child_of(&lw);
        set_display_lists(&mut WrTransaction::new(), &mut lw, false).expect("translates");
        lw.regenerate_display_list_for_dom(child);
        let got = set_display_lists(&mut WrTransaction::new(), &mut lw, false).expect("ok");
        assert_eq!(got.translated, vec![child]);
        assert_eq!(got.held, vec![DomId::ROOT_ID]);
    }

    /// Everything the renderer holds was translated against its scale, and a new
    /// scale (a window moved to another screen) translates it all again - as a
    /// slot keyed for the first time does (`force_all`).
    #[test]
    fn a_new_scale_or_a_new_image_key_sends_every_list_again() {
        let mut lw = window();
        set_display_lists(&mut WrTransaction::new(), &mut lw, false).expect("translates");

        let forced = submit_display_lists(&mut WrTransaction::new(), &lw, true);
        assert_eq!(forced.translated.len(), 2, "force_all");
        assert!(forced.held.is_empty());

        lw.current_window_state.size.dpi = 192;
        let rescaled = submit_display_lists(&mut WrTransaction::new(), &lw, false);
        assert_eq!(rescaled.translated.len(), 2, "a window at 2x: every list translated anew");
        note_display_lists_submitted(&mut lw);
        let settled = submit_display_lists(&mut WrTransaction::new(), &lw, false);
        assert!(settled.translated.is_empty(), "and held again at the new scale");
    }
}

#[cfg(test)]
mod audit_a4_tests {
    //! The GPU path's counters of a frame (audit A4, `## Counters`): what a
    //! transaction translated and held is what the window reports for the
    //! frame, and a frame starts at zero.

    use azul_core::{
        dom::{Dom, DomId},
        geom::LogicalSize,
        resources::RendererResources,
        styled_dom::StyledDom,
    };
    use azul_layout::{
        callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
    };
    use rust_fontconfig::FcFontCache;

    use super::{count_transaction, publish_gpu_frame, set_display_lists, WrTransaction};

    /// A laid-out window of `boxes` painted boxes.
    fn window(boxes: usize) -> LayoutWindow {
        let mut dom = Dom::create_body();
        for _ in 0..boxes {
            dom = dom.with_child(
                Dom::create_div().with_css("width: 20px; height: 4px; background: #3366cc;"),
            );
        }
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(200.0, 400.0);
        lw.current_window_state = ws.clone();
        lw.layout_and_generate_display_list(
            StyledDom::create_from_dom(dom),
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the window lays out");
        lw
    }

    /// A4-24. Only a transaction without resource updates goes straight to the
    /// render backend: one that deletes (adds, updates) an image takes the scene
    /// builder's queue like the transaction that added it.
    /// Mutation: skipping unconditionally returns `true` for the second.
    #[test]
    fn a_transaction_with_resource_updates_does_not_skip_the_scene_builder() {
        let mut plain = WrTransaction::new();
        assert!(super::skip_scene_builder_unless_it_carries_resources(&mut plain));

        let mut with_resources = WrTransaction::new();
        with_resources.update_resources(vec![webrender::ResourceUpdate::DeleteImage(
            webrender::api::ImageKey(webrender::api::IdNamespace(1), 7),
        )]);
        assert!(!super::skip_scene_builder_unless_it_carries_resources(
            &mut with_resources
        ));
    }

    #[test]
    fn a_frame_counts_its_lists_items_and_transaction_and_the_next_starts_at_zero() {
        let mut lw = window(32);
        // Whatever this thread counted before this test is not this frame's.
        publish_gpu_frame(&lw, 0, 0, 0);
        let before = lw.frame_clock.gpu_total().items_translated;

        let mut txn = WrTransaction::new();
        let sent = set_display_lists(&mut txn, &mut lw, false).expect("translates");
        assert_eq!(sent.translated, vec![DomId::ROOT_ID]);
        count_transaction(&txn);
        publish_gpu_frame(&lw, 1_500, 2_500, 7);

        let items = lw.layout_results[&DomId::ROOT_ID].display_list.items.len() as u64;
        assert!(items >= 32, "a box each at the least: {items}");
        let frame = lw.frame_clock.gpu_last();
        assert_eq!(frame.transactions, 1);
        assert_eq!(frame.lists_translated, 1);
        assert_eq!(frame.items_translated, items);
        assert!(frame.list_bytes > 0);
        assert_eq!(
            (frame.blocked_us, frame.render_us, frame.draw_calls),
            (1_500, 2_500, 7)
        );
        assert_eq!(lw.frame_timings().gpu_items_translated, items, "what an app reads");

        // The renderer holds the list now: the next frame's transaction
        // translates nothing, and its counters do not carry the last frame's.
        let mut txn = WrTransaction::new();
        let again = set_display_lists(&mut txn, &mut lw, false).expect("ok");
        assert!(again.translated.is_empty());
        count_transaction(&txn);
        publish_gpu_frame(&lw, 0, 0, 0);
        let frame = lw.frame_clock.gpu_last();
        assert_eq!(
            (frame.lists_translated, frame.items_translated, frame.list_bytes),
            (0, 0, 0),
            "a held list costs the frame no translation"
        );
        assert_eq!(frame.transactions, 1);
        assert_eq!(
            lw.frame_clock.gpu_total().items_translated - before,
            items,
            "the sum of both frames"
        );
    }
}
