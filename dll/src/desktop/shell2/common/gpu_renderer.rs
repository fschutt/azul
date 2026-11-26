//! WebRender on a GL context the shell made: the one bring-up the phone shells' GPU paths share (Android over EGL, iOS
//! over EAGL) and the GLES host test runs, and the steps of a GPU frame. It is the block the X11 shell writes inline
//! (`x11/mod.rs`, "GPU rendering initialized"), as functions that return what failed and at which stage instead of
//! falling back in the middle of a window's construction, so that the shell decides what a failure means.
//!
//! The stages are the names of the GPU path's diagnostics and of the `AZ_GPU_FAIL` hook: `probe` (the context is OpenGL
//! ES 3 with a texture large enough), `renderer` (`create_webrender_instance`), `document` (the render API), `shaders` (the
//! engine's own GL programs compile), `frame` (a frame that did not render).

use std::{panic::AssertUnwindSafe, path::Path, rc::Rc};

use azul_core::{
    gl::{GlContextPtr, OptionGlContextPtr},
    hit_test::DocumentId,
    resources::{IdNamespace, RendererResources},
    window::RendererType,
};
use azul_layout::window_state::WindowCreateOptions;
use gl_context_loader::GenericGlContext;
use webrender::api::units::DeviceIntSize;

use super::{
    compositor::{query_gpu_info, GpuCheckResult, GpuInfo},
    event::CommonWindowState,
    gles_probe::{self, GlesVersion, RendererClass},
};
use crate::desktop::{
    shader_cache::ProgramCacheStatus,
    wr_translate2::{self, AsyncHitTester, Notifier, PartialPresentDamage, WrRenderApi},
};

/// A step of the bring-up that failed: the stage, and what the driver or WebRender said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuInitFailure {
    /// `probe`, `renderer`, `document` or `shaders`.
    pub stage: &'static str,
    /// The cause, in one line.
    pub cause: String,
}

impl GpuInitFailure {
    fn new(stage: &'static str, cause: impl Into<String>) -> Self {
        Self {
            stage,
            cause: cause.into(),
        }
    }
}

impl std::fmt::Display for GpuInitFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.stage, self.cause)
    }
}

/// What a successful bring-up hands to the window: the renderer and its API, the document, the hit tester, the engine's
/// GL context and what the driver is.
pub struct GpuParts {
    /// The renderer; the window keeps it in `CommonWindowState::renderer`.
    pub renderer: webrender::Renderer,
    /// The API that sends it transactions (`CommonWindowState::render_api`).
    pub render_api: WrRenderApi,
    /// The window's document, in the engine's type.
    pub document_id: DocumentId,
    /// The namespace the document's keys are minted in.
    pub id_namespace: IdNamespace,
    /// The hit tester WebRender builds for the document (the CPU one answers the engine's hit tests, see
    /// `CommonWindowState::cpu_hit_tester`; the window keeps this one as the desktop shells do).
    pub hit_tester: AsyncHitTester,
    /// The engine's GL context (the SVG, FXAA and brush programs the app's callbacks draw with).
    pub gl_context_ptr: OptionGlContextPtr,
    /// The driver's strings.
    pub info: GpuInfo,
    /// The OpenGL ES version of the context.
    pub gles: GlesVersion,
    /// Software, emulated or hardware.
    pub class: RendererClass,
    /// What the program cache did (cold or warm start, and where).
    pub shader_cache: ProgramCacheStatus,
}

impl std::fmt::Debug for GpuParts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuParts")
            .field("info", &self.info)
            .field("gles", &self.gles)
            .field("class", &self.class)
            .finish()
    }
}

/// Reads `GL_MAX_TEXTURE_SIZE` of the current context.
fn max_texture_size(gl: &GenericGlContext) -> i32 {
    let mut value = [0_i32];
    // SAFETY: one query that writes one int into a one-element slice.
    unsafe { gl.get_integer_v(azul_core::gl::MAX_TEXTURE_SIZE, &mut value) };
    value[0]
}

/// Makes the renderer, its API and the document for the GL context that is current on this thread.
///
/// `framebuffer_px` is the size of the surface the frames will be drawn to; `damage` is the cell WebRender reports
/// the region of each frame to when the shell swaps with damage (EGL buffer age); `shader_cache_dir` is where the
/// compiled programs are kept between runs (`None`: not kept).
///
/// Nothing is left behind on a failure: a renderer that was made is `deinit`ed before the failure returns.
///
/// # Errors
///
/// The stage that failed (see the module's documentation) and its cause.
pub fn init_webrender(
    gl: &Rc<GenericGlContext>,
    options: &WindowCreateOptions,
    notifier: Notifier,
    damage: Option<PartialPresentDamage>,
    framebuffer_px: (u32, u32),
    shader_cache_dir: Option<&Path>,
) -> Result<GpuParts, GpuInitFailure> {
    // The probe: the strings (which also publish the GpuStatus the GPU-check dialog shows), then WebRender's minimum.
    let (info, renderer_type) = match query_gpu_info(gl) {
        GpuCheckResult::Ok(info) => (info, RendererType::Hardware),
        // A software GL stack (llvmpipe, swrast): WebRender composites, the engine's own GL programs are skipped.
        GpuCheckResult::Blacklisted { info, .. } => (info, RendererType::Software),
        GpuCheckResult::QueryFailed(reason) => return Err(GpuInitFailure::new("probe", reason)),
    };
    let probe = gles_probe::probe(&info.version, &info.renderer, max_texture_size(gl))
        .map_err(|cause| GpuInitFailure::new("probe", cause))?;

    // The programs compiled by an earlier run, when the shell has a directory to keep them in.
    let (program_cache, shader_cache) =
        wr_translate2::create_program_cache_with_status(gl, shader_cache_dir);

    // The renderer. Inside a catch: a driver that makes the device layer panic must be a fallback, not a dead process
    // (where panics abort, there is nothing to catch and this is the same call).
    let created = std::panic::catch_unwind(AssertUnwindSafe(|| {
        webrender::create_webrender_instance(
            gl.clone(),
            Box::new(notifier),
            wr_translate2::default_renderer_options(options, program_cache, damage),
            None,
        )
    }));
    let (mut renderer, sender) = match created {
        Ok(Ok(made)) => made,
        Ok(Err(e)) => return Err(GpuInitFailure::new("renderer", format!("{e:?}"))),
        Err(panic) => {
            return Err(GpuInitFailure::new(
                "renderer",
                format!("WebRender panicked: {}", panic_message(panic.as_ref())),
            ))
        }
    };
    // External images (the app's GL textures, the paint canvas) need a handler or WebRender panics on the first one.
    renderer.set_external_image_handler(Box::new(wr_translate2::Compositor::default()));

    let render_api = sender.create_api();
    let size = DeviceIntSize::new(
        i32::try_from(framebuffer_px.0).unwrap_or(i32::MAX).max(1),
        i32::try_from(framebuffer_px.1).unwrap_or(i32::MAX).max(1),
    );
    let wr_document = render_api.add_document(size);
    let document_id = wr_translate2::translate_document_id_wr(wr_document);
    let id_namespace = wr_translate2::translate_id_namespace_wr(render_api.get_namespace_id());
    let hit_tester = AsyncHitTester::Requested(render_api.request_hit_tester(wr_document));

    // The engine's own GL context: compiles its programs at once, so a driver that rejects them is found here and not in
    // the middle of the first frame. A software stack skips them by construction.
    let gl_ptr = GlContextPtr::new(renderer_type, gl.clone());
    if matches!(renderer_type, RendererType::Hardware) && !gl_ptr.is_gl_usable() {
        drop(gl_ptr);
        drop(render_api);
        renderer.deinit();
        return Err(GpuInitFailure::new(
            "shaders",
            "the engine's GL programs did not compile at any GLSL version",
        ));
    }

    Ok(GpuParts {
        renderer,
        render_api,
        document_id,
        id_namespace,
        hit_tester,
        gl_context_ptr: OptionGlContextPtr::Some(gl_ptr),
        info,
        gles: probe.version,
        class: probe.class,
        shader_cache,
    })
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_owned())
}

/// Hands the parts to the window: the renderer, its API and document, the hit tester and the GL context are what
/// `CommonWindowState` keeps for a GPU window.
pub fn install(common: &mut CommonWindowState, parts: GpuParts) -> GpuDevice {
    common.renderer = Some(parts.renderer);
    common.render_api = Some(parts.render_api);
    common.hit_tester = Some(parts.hit_tester);
    common.document_id = Some(parts.document_id);
    common.id_namespace = Some(parts.id_namespace);
    common.gl_context_ptr = parts.gl_context_ptr;
    // The first frame of a renderer sends the whole display list.
    common.display_list_initialized = false;
    GpuDevice {
        info: parts.info,
        gles: parts.gles,
        class: parts.class,
        shader_cache: parts.shader_cache,
    }
}

/// What the driver is, as the shell keeps it after the parts are installed: for the log line, the startup timeline and
/// the GPU-check dialog.
#[derive(Debug, Clone)]
pub struct GpuDevice {
    /// The driver's strings (`GL_VENDOR`, `GL_RENDERER`, `GL_VERSION`, the GLSL version).
    pub info: GpuInfo,
    /// The OpenGL ES version of the context.
    pub gles: GlesVersion,
    /// Software, emulated or hardware.
    pub class: RendererClass,
    /// What the program cache did.
    pub shader_cache: ProgramCacheStatus,
}

/// Takes the renderer away from the window the way WebRender requires (`Renderer::deinit` in a current context) and
/// forgets what only that renderer knew: the keys, the display lists it holds, the GL textures of the app's callbacks.
///
/// The GL context must be current on the calling thread when the renderer is still there. Calling this twice is
/// harmless.
pub fn release(common: &mut CommonWindowState) {
    if let Some(document) = common.document_id {
        crate::desktop::gl_texture_integration::remove_document_textures(&document);
    }
    // The renderer first (its textures and programs go in its own frame), then what talks to it.
    common.deinit_renderer();
    common.hit_tester = None;
    common.render_api = None;
    common.document_id = None;
    common.id_namespace = None;
    // The engine's own GL programs are deleted by this drop, in the context that is still current.
    common.gl_context_ptr = OptionGlContextPtr::None;
}

/// Makes the window ready for a NEW renderer after the old one is gone (a context that was lost, a renderer that was
/// released to give the GPU memory back): the registries of fonts and images name keys of the old renderer, and the
/// retained-frame bookkeeping says what the old one holds, so they are cleared, and the next pass sends everything.
pub fn forget_renderer_state(common: &mut CommonWindowState) {
    common.renderer_resources = RendererResources::default();
    if let Some(window) = common.layout_window.as_mut() {
        window.renderer_resources = RendererResources::default();
        let gpu = &mut window.gpu_state_manager;
        gpu.invalidate_submitted_digests();
        gpu.submitted_display_lists.clear();
        gpu.submitted_translation_env = 0;
        gpu.text_halo_digests.clear();
        gpu.stale_display_lists.clear();
        gpu.zoom_frames.clear();
        gpu.zoom_gesture_running = false;
    }
    common.display_list_initialized = false;
    common.display_list_dirty = false;
    common.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
}

/// A full frame to WebRender: the display lists of the layout, translated and sent, and the scene built (the same call
/// every desktop shell makes after a layout pass).
pub fn submit_full_frame(common: &mut CommonWindowState) {
    if let (Some(window), Some(api), Some(document)) = (
        common.layout_window.as_mut(),
        common.render_api.as_mut(),
        common.document_id,
    ) {
        super::layout::generate_frame(window, api, document, &common.gl_context_ptr);
        api.flush_scene_builder();
    }
}

/// The frame of a pass that did not lay out: scroll offsets, property values and images only. `true` when something
/// changed and a frame was asked for (see `submit_lightweight_frame`).
pub fn submit_lightweight(common: &mut CommonWindowState, who: &str) -> bool {
    if let (Some(window), Some(api), Some(document)) = (
        common.layout_window.as_mut(),
        common.render_api.as_mut(),
        common.document_id,
    ) {
        super::layout::submit_lightweight_frame(
            window,
            api,
            document,
            &common.gl_context_ptr,
            true,
            who,
        )
    } else {
        false
    }
}

/// Draws the frame WebRender has built into the framebuffer that is bound (the window's surface, or the offscreen one
/// of a test): takes the update in, draws, and says what went wrong. `buffer_age` is the age of the back buffer
/// (`EglSession::buffer_age`), 0 when unknown.
///
/// # Errors
///
/// No renderer, or the errors WebRender reported for the frame (stage `frame`).
pub fn render_frame(
    common: &mut CommonWindowState,
    framebuffer_px: (u32, u32),
    buffer_age: usize,
) -> Result<(), GpuInitFailure> {
    // The canvas follows the mode the window shows.
    common.sync_renderer_clear_color();
    let Some(renderer) = common.renderer.as_mut() else {
        return Err(GpuInitFailure::new("frame", "no renderer"));
    };
    renderer.update();
    let size = DeviceIntSize::new(
        i32::try_from(framebuffer_px.0).unwrap_or(i32::MAX).max(1),
        i32::try_from(framebuffer_px.1).unwrap_or(i32::MAX).max(1),
    );
    match renderer.render(size, buffer_age) {
        Ok(_) => {
            common.display_list_initialized = true;
            Ok(())
        }
        Err(errors) => Err(GpuInitFailure::new("frame", format!("{errors:?}"))),
    }
}

/// After the swap: the GL textures of old epochs may go (WebRender no longer needs them).
pub fn finish_frame(common: &CommonWindowState) {
    if let Some(window) = common.layout_window.as_ref() {
        crate::desktop::gl_texture_integration::remove_old_gl_textures(
            &window.document_id,
            window.epoch,
        );
    }
}
