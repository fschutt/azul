//! WebRender on a GL context the shell made: the one bring-up the phone shells' GPU paths share (Android over EGL, iOS
//! over EAGL) and the GLES host test runs, and the steps of a GPU frame. It is the block the X11 shell writes inline
//! (`x11/mod.rs`, "GPU rendering initialized"), as functions that return what failed and at which stage instead of
//! falling back in the middle of a window's construction, so that the shell decides what a failure means.
//!
//! The stages are those of `gpu_select::GpuStage`, the names of the GPU path's diagnostics and of the `AZ_GPU_FAIL` hook:
//! `probe` (the context is OpenGL ES 3 with a texture large enough), `renderer` (`create_webrender_instance`),
//! `document` (the render API), `shaders` (the engine's own GL programs compile), `frame` (a frame that did not render).

use std::{panic::AssertUnwindSafe, path::Path, rc::Rc};

use azul_core::{
    gl::{GlContextPtr, OptionGlContextPtr},
    hit_test::DocumentId,
    resources::{IdNamespace, RendererResources, TextAntialiasMode},
    window::RendererType,
};
use azul_layout::window_state::WindowCreateOptions;
use gl_context_loader::GenericGlContext;
use webrender::api::units::DeviceIntSize;

use super::{
    compositor::{query_gpu_info, GpuCheckResult, GpuInfo},
    event::CommonWindowState,
    gles_probe::{self, GlesVersion, RendererClass},
    gpu_select::{DeviceInfo, FailPlan, GpuFallback, GpuStage},
};
use crate::desktop::{
    shader_cache::ProgramCacheStatus,
    wr_translate2::{self, AsyncHitTester, Notifier, PartialPresentDamage, WrRenderApi},
};

/// Whether this target is a phone or a tablet: the platform default of text antialiasing is grayscale there (its panel turns and
/// often has no RGB stripes), as `azul_layout::cpurender::TEXT_AA_DEFAULT` says for the CPU renderer.
pub const PHONE: bool = cfg!(any(target_os = "android", target_os = "ios"));

/// How WebRender draws text: what `AppConfig::text_antialiasing` asks for, decided as the CPU renderer decides it
/// ([`webrender_text_aa`]), so that the two renderers show one kind of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRendering {
    /// Antialiased text. Off: aliased text, coverage cut at one half.
    pub aa: bool,
    /// LCD (subpixel) text where the device can blend it. Off: grayscale text everywhere.
    pub subpixel: bool,
}

impl Default for TextRendering {
    /// What `TextAntialiasMode::PlatformDefault` is on this target: LCD text on a desktop, grayscale on a phone.
    fn default() -> Self {
        webrender_text_aa(TextAntialiasMode::PlatformDefault, PHONE)
    }
}

/// The text WebRender draws for an app's `TextAntialiasMode`. `PlatformDefault` is what the platform's own text does, LCD on
/// a desktop and grayscale on a `phone`; the other modes mean the same on every platform. Without this the renderer drew LCD
/// text wherever the device had dual-source blending (a phone's GPU has it) whatever the app asked for, colour fringes on a
/// panel that turns, while the CPU renderer drew grayscale text next to it.
#[must_use]
pub const fn webrender_text_aa(mode: TextAntialiasMode, phone: bool) -> TextRendering {
    match mode {
        TextAntialiasMode::PlatformDefault => TextRendering {
            aa: true,
            subpixel: !phone,
        },
        TextAntialiasMode::Lcd | TextAntialiasMode::LcdLegacy => TextRendering {
            aa: true,
            subpixel: true,
        },
        TextAntialiasMode::Grayscale => TextRendering {
            aa: true,
            subpixel: false,
        },
        TextAntialiasMode::Aliased => TextRendering {
            aa: false,
            subpixel: false,
        },
    }
}

/// What a shell gives the bring-up besides the context and the window's options.
pub struct InitParams<'a> {
    /// The notifier WebRender calls from its backend thread when a frame is built (its `wake` is what wakes the loop).
    pub notifier: Notifier,
    /// The cell WebRender reports the damaged region of each frame to, when the shell swaps with damage (EGL buffer
    /// age); `None` draws every frame in full.
    pub damage: Option<PartialPresentDamage>,
    /// The size of the surface the frames will be drawn to, in device pixels.
    pub framebuffer_px: (u32, u32),
    /// Where the compiled programs are kept between runs (`None`: the platform's own directory, or none).
    pub shader_cache_dir: Option<&'a Path>,
    /// How text is drawn (`AppConfig::text_antialiasing`).
    pub text: TextRendering,
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

impl GpuDevice {
    /// The strings in the form `gpu_select::decide` reads.
    #[must_use]
    pub fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            vendor: self.info.vendor.clone(),
            renderer: self.info.renderer.clone(),
            version: self.info.version.clone(),
            glsl: self.info.glsl_version.clone(),
        }
    }
}

/// Reads `GL_MAX_TEXTURE_SIZE` of the current context.
fn max_texture_size(gl: &GenericGlContext) -> i32 {
    let mut value = [0_i32];
    // SAFETY: one query that writes one int into a one-element slice.
    unsafe { gl.get_integer_v(azul_core::gl::MAX_TEXTURE_SIZE, &mut value) };
    value[0]
}

fn device_size(px: (u32, u32)) -> DeviceIntSize {
    DeviceIntSize::new(
        i32::try_from(px.0).unwrap_or(i32::MAX).max(1),
        i32::try_from(px.1).unwrap_or(i32::MAX).max(1),
    )
}

/// Makes the renderer, its API and the document for the GL context that is current on this thread.
///
/// `plan` is what a test run made fail on purpose (`AZ_GPU_FAIL`); the stages it is asked about are `probe`, `renderer`,
/// `document` and `shaders`, in that order. An empty plan fails nothing.
///
/// Nothing is left behind on a failure: a renderer that was made is `deinit`ed before the failure returns.
///
/// # Errors
///
/// The stage that failed and the cause.
pub fn init_webrender(
    gl: &Rc<GenericGlContext>,
    options: &WindowCreateOptions,
    params: InitParams<'_>,
    plan: &mut FailPlan,
) -> Result<GpuParts, GpuFallback> {
    // The probe: the strings (which also publish the GpuStatus the GPU-check dialog shows), then WebRender's minimum.
    if plan.fails(GpuStage::Probe) {
        return Err(GpuFallback::injected(GpuStage::Probe));
    }
    let (info, renderer_type) = match query_gpu_info(gl) {
        GpuCheckResult::Ok(info) => (info, RendererType::Hardware),
        // A software GL stack (llvmpipe, swrast): WebRender composites, the engine's own GL programs are skipped.
        GpuCheckResult::Blacklisted { info, .. } => (info, RendererType::Software),
        GpuCheckResult::QueryFailed(reason) => {
            return Err(GpuFallback::new(GpuStage::Probe, &reason))
        }
    };
    let probe = gles_probe::probe(&info.version, &info.renderer, max_texture_size(gl))
        .map_err(|cause| GpuFallback::new(GpuStage::Probe, &cause))?;

    // The programs compiled by an earlier run, when the shell has a directory to keep them in.
    if plan.fails(GpuStage::Renderer) {
        return Err(GpuFallback::injected(GpuStage::Renderer));
    }
    let (program_cache, shader_cache) =
        wr_translate2::create_program_cache_with_status(gl, params.shader_cache_dir);

    // The renderer. Inside a catch: a driver that makes the device layer panic must be a fallback, not a dead process
    // (where panics abort, there is nothing to catch and this is the same call).
    let created = std::panic::catch_unwind(AssertUnwindSafe(|| {
        webrender::create_webrender_instance(
            gl.clone(),
            Box::new(params.notifier),
            wr_translate2::default_renderer_options_with_text(
                options,
                program_cache,
                params.damage,
                params.text.aa,
                params.text.subpixel,
            ),
            None,
        )
    }));
    let (mut renderer, sender) = match created {
        Ok(Ok(made)) => made,
        Ok(Err(e)) => return Err(GpuFallback::new(GpuStage::Renderer, &format!("{e:?}"))),
        Err(panic) => {
            return Err(GpuFallback::new(
                GpuStage::Renderer,
                &format!("WebRender panicked: {}", panic_message(panic.as_ref())),
            ))
        }
    };
    // External images (the app's GL textures, the paint canvas) need a handler or WebRender panics on the first one.
    renderer.set_external_image_handler(Box::new(wr_translate2::Compositor::default()));

    if plan.fails(GpuStage::Document) {
        renderer.deinit();
        return Err(GpuFallback::injected(GpuStage::Document));
    }
    let render_api = sender.create_api();
    let wr_document = render_api.add_document(device_size(params.framebuffer_px));
    let document_id = wr_translate2::translate_document_id_wr(wr_document);
    let id_namespace = wr_translate2::translate_id_namespace_wr(render_api.get_namespace_id());

    // The engine's own GL context: compiles its programs at once, so a driver that rejects them is found here and not in
    // the middle of the first frame. A software stack skips them by construction. (Before the hit tester is asked for: a
    // bring-up that ends here has asked nothing of WebRender's backend thread that nobody would be left to take.)
    let gl_ptr = GlContextPtr::new(renderer_type, gl.clone());
    let shaders_fail = plan.fails(GpuStage::Shaders);
    if shaders_fail
        || (matches!(renderer_type, RendererType::Hardware) && !gl_ptr.is_gl_usable())
    {
        drop(gl_ptr);
        drop(render_api);
        renderer.deinit();
        return Err(if shaders_fail {
            GpuFallback::injected(GpuStage::Shaders)
        } else {
            GpuFallback::new(
                GpuStage::Shaders,
                "the engine's GL programs did not compile at any GLSL version",
            )
        });
    }

    let hit_tester = AsyncHitTester::Requested(render_api.request_hit_tester(wr_document));

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
    // The layout window mints its keys in the renderer's namespace and names its document.
    if let Some(window) = common.layout_window.as_mut() {
        window.document_id = parts.document_id;
        window.id_namespace = parts.id_namespace;
        window.renderer_type = Some(if parts.class.software {
            RendererType::Software
        } else {
            RendererType::Hardware
        });
    }
    // The first frame of a renderer sends the whole display list.
    common.display_list_initialized = false;
    GpuDevice {
        info: parts.info,
        gles: parts.gles,
        class: parts.class,
        shader_cache: parts.shader_cache,
    }
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
) -> Result<(), GpuFallback> {
    // The canvas follows the mode the window shows.
    common.sync_renderer_clear_color();
    let Some(renderer) = common.renderer.as_mut() else {
        return Err(GpuFallback::new(GpuStage::Frame, "no renderer"));
    };
    renderer.update();
    match renderer.render(device_size(framebuffer_px), buffer_age) {
        Ok(_) => {
            common.display_list_initialized = true;
            Ok(())
        }
        Err(errors) => Err(GpuFallback::new(GpuStage::Frame, &format!("{errors:?}"))),
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

#[cfg(test)]
#[path = "gpu_renderer_test.rs"]
mod tests;
