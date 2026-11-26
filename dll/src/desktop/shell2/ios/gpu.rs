//! The iOS window's GPU path: the executor of `shell2/common/gpu_lifecycle.rs` on OpenGL ES, and the frame it draws.
//!
//! The decisions are not here. WHAT happens to the context, the surface and the renderer at each event of the
//! window and the app is the pure reducer's ([`GpuLifecycle`]: tested against a fake platform that enforces
//! Apple's rule that a background app issues no GL command); WHICH renderer a window gets and the line the QA
//! harness reads is `gpu_select`'s. This file performs the reducer's actions on `EAGLContext` / `CAEAGLLayer`
//! (`ios/gl.rs`) and WebRender, and runs one frame. It is an `impl IOSWindow` in a child module of the shell, so
//! the window's fields stay private to the shell and the CPU path is left exactly as it is.
//!
//! Everything runs on the main thread. A window that does not ask for the GPU never gets an [`IosGpu`], and
//! every entry point here returns at once for it.

use std::{
    rc::Rc,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

use azul_core::{
    callbacks::RelayoutReason,
    gl::{GlContextPtr, OptionGlContextPtr},
    window::RendererType,
};
use azul_layout::window_state::WindowCreateOptions;
use gl_context_loader::GenericGlContext;
use objc2::{
    msg_send,
    runtime::AnyObject,
};

use super::{
    gl::{Drawable, EaglContext, EaglLibrary},
    set_display_link_paused, IOSWindow, RenderBackend,
};
use crate::{
    desktop::{
        gl_texture_integration::remove_old_gl_textures,
        shell2::common::{
            debug_server::LogCategory,
            event::PlatformWindow,
            gpu_lifecycle::{Action, Event, GpuLifecycle, Phase, Surface, TrimLevel},
            gpu_select::{
                decide, DeviceInfo, FailPlan, GpuFallback, GpuRequest, GpuStage,
            },
            layout::{generate_frame, submit_lightweight_frame},
        },
        wr_translate2::{
            create_program_cache, default_renderer_options, elapsed_us, publish_gpu_frame,
            translate_document_id_wr, translate_id_namespace_wr, Compositor, Notifier,
        },
    },
    log_debug, log_error, log_info, log_warn,
};

/// What a window says about its own presents: the frame times of a scroll, measured where the frame is handed to
/// the system (`common::frame_stats`: the lines the phone QA reads). The CPU path notes its presents too, so the
/// two renderers are measured by the same clock.
pub(super) struct PresentLog {
    started: Instant,
    stats: crate::desktop::shell2::common::frame_stats::FrameStats,
}

impl PresentLog {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            stats: crate::desktop::shell2::common::frame_stats::FrameStats::new(),
        }
    }

    /// A frame was handed to the system; `draw_us` is how long its draw took, when known.
    pub(super) fn note(&mut self, draw_us: Option<u64>) {
        let at_us = u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX);
        if let Some(line) = self.stats.record(at_us, draw_us) {
            log_info!(LogCategory::Rendering, "{}", line);
        }
    }
}

/// What an iOS window keeps while it runs on the GPU. `None` in [`IOSWindow`] for a window that draws on the CPU.
pub(super) struct IosGpu {
    /// The lifecycle machine: the one that says what exists and what may be done.
    lifecycle: GpuLifecycle,
    /// What the window asked for.
    request: GpuRequest,
    /// The failures a test run asks for (empty in a shipped app).
    plan: FailPlan,
    /// `OpenGLES.framework`, opened at the first context.
    library: Option<EaglLibrary>,
    /// The engine's GL table over it.
    gl: Option<Rc<GenericGlContext>>,
    context: Option<EaglContext>,
    drawable: Option<Drawable>,
    /// What the context says about itself.
    device: Option<DeviceInfo>,
    /// WebRender's notifier sets this from its backend thread when a frame is built; the display tick draws it.
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,
    /// The display list must be sent again (a renderer was made after a layout already ran).
    resend: bool,
    /// The line of the QA harness is written (once the GPU path is up, or after it fell back).
    reported: bool,
    /// When the GPU path began: the first frame says how long after it came.
    started: Instant,
    /// The first frame of this renderer has been reported (once).
    first_frame_reported: bool,
}

impl IosGpu {
    fn new(request: GpuRequest, plan: FailPlan) -> Self {
        Self {
            lifecycle: GpuLifecycle::new(),
            request,
            plan,
            library: None,
            gl: None,
            context: None,
            drawable: None,
            device: None,
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            resend: false,
            reported: false,
            started: Instant::now(),
            first_frame_reported: false,
        }
    }
}

impl IOSWindow {
    // ----- start -------------------------------------------------------------------------------

    /// Choose the renderer of this window and, if it is the GPU, bring it up. Called once, at the end of the
    /// window's construction (its view exists and has its size). Writes the one line the QA harness reads, and the
    /// one diagnostic of a request for the GPU that fell back.
    pub(super) fn gpu_start(&mut self) {
        // `AZ_BACKEND` first; an app that is not started from a shell with an environment (an iPhone) asks
        // in its Info.plist (`AZBackend`: `gpu`, `auto` or `cpu`) instead
        let backend = self
            .common
            .app
            .settings()
            .shell
            .backend
            .clone()
            .or_else(plist_backend);
        let request = GpuRequest::resolve(
            backend.as_deref(),
            Some(self.common.current_window_state().renderer_options.hw_accel),
        );
        // a test run's failure plan; a shipped build has no hooks and an empty plan
        let plan = FailPlan::parse(azul_core::run_mode::test_hooks().gpu_fail.as_deref());
        for note in plan.notes() {
            log_warn!(LogCategory::Rendering, "[render] {}", note);
        }
        if request == GpuRequest::Cpu {
            self.gpu_report(&decide(request, None));
            return;
        }
        self.gpu = Some(IosGpu::new(request, plan));
        // the bring-up runs with the window already on the GPU: `FallBackToCpu` turns it back
        self.backend = RenderBackend::Gpu;
        let surface = self.gpu_surface();
        self.gpu_event(Event::SurfaceCreated(surface));
        // up now, or waiting for a view that has no size yet (the line is written when it is up)
        self.gpu_report_when_up();
    }

    /// The line of a GPU window that came up: once, the first time the machine is ready. (A window that fell
    /// back wrote its lines at the fall back.)
    fn gpu_report_when_up(&mut self) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        if gpu.reported || gpu.lifecycle.phase() != Phase::Ready {
            return;
        }
        gpu.reported = true;
        let request = gpu.request;
        let device = gpu.device.clone();
        self.gpu_report(&decide(request, device.map(Ok)));
    }

    /// The lines about the renderer this window got: the fallback's diagnostic, then the contract line.
    fn gpu_report(&self, decision: &crate::desktop::shell2::common::gpu_select::Decision) {
        if let Some(line) = decision.fallback_line() {
            log_warn!(LogCategory::Rendering, "{}", line);
            // an iPhone has no stderr to read: the unified log (Console.app, `idevicesyslog`) does
            super::system_log(&line);
        }
        let line = decision.render_line();
        log_info!(LogCategory::Rendering, "{}", line);
        super::system_log(&line);
    }

    /// The drawable the window asks for: its size in device pixels and its scale.
    fn gpu_surface(&self) -> Surface {
        let size = &self.common.current_window_state().size;
        let physical = size.get_physical_size();
        Surface::new(physical.width, physical.height, size.dpi as f32 / 96.0)
    }

    /// The layer of the render view: the CAEAGLLayer goes under it.
    fn gpu_view_layer(&self) -> *mut AnyObject {
        let view = (&*self.ui_view as *const objc::runtime::Object) as *mut AnyObject;
        // SAFETY: `layer` of a live UIView, on the main thread.
        unsafe { msg_send![view, layer] }
    }

    // ----- events and the executor -------------------------------------------------------------

    /// Tell the lifecycle machine what happened and perform its answer. A no-op for a CPU window.
    pub(super) fn gpu_event(&mut self, event: Event) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        let was_allowed = gpu.lifecycle.gl_allowed();
        // (a frame's events come sixty times a second: only the others are named)
        let name = (!matches!(event, Event::Frame | Event::FramePresented)).then(|| format!("{event:?}"));
        let actions = gpu.lifecycle.step(event);
        let is_allowed = gpu.lifecycle.gl_allowed();
        // The two lines the QA harness reads to prove that no GL command went out in the background: the line that
        // ends them comes after the last command of its own answer, the line that allows them before the first.
        if !was_allowed && is_allowed {
            log_info!(
                LogCategory::Rendering,
                "[iOS GPU] GL commands allowed again ({})",
                name.as_deref().unwrap_or("-")
            );
        }
        self.gpu_perform_all(actions);
        if was_allowed && !is_allowed {
            log_info!(
                LogCategory::Rendering,
                "[iOS GPU] GL commands stopped ({})",
                name.as_deref().unwrap_or("-")
            );
        }
    }

    /// May a GL command be issued now? `false` in the background; a window on the CPU never asks.
    pub(super) fn gpu_gl_allowed(&self) -> bool {
        self.gpu.as_ref().map_or(true, |gpu| gpu.lifecycle.gl_allowed())
    }

    /// Perform the actions in order, stopping at the first that fails: the failure goes back to the machine,
    /// which answers with the teardown and the fall back. An answer that ends the machine is performed to its
    /// end whatever fails in it.
    fn gpu_perform_all(&mut self, actions: Vec<Action>) {
        let ended = self.gpu.as_ref().map_or(true, |gpu| {
            matches!(gpu.lifecycle.phase(), Phase::Fallen | Phase::Closed)
        });
        for action in actions {
            match self.gpu_perform(&action) {
                Ok(()) => {}
                Err(fallback) if ended => {
                    log_debug!(
                        LogCategory::Rendering,
                        "[iOS GPU] {:?} failed while closing down ({}): {}",
                        action,
                        fallback.stage.name(),
                        fallback.cause
                    );
                }
                Err(fallback) => {
                    log_warn!(
                        LogCategory::Rendering,
                        "[iOS GPU] {:?} failed at {}: {}",
                        action,
                        fallback.stage.name(),
                        fallback.cause
                    );
                    let more = self
                        .gpu
                        .as_mut()
                        .map(|gpu| gpu.lifecycle.step(Event::ActionFailed(fallback)))
                        .unwrap_or_default();
                    self.gpu_perform_all(more);
                    return;
                }
            }
        }
    }

    fn gpu_perform(&mut self, action: &Action) -> Result<(), GpuFallback> {
        match action {
            Action::CreateContext => self.gpu_create_context(),
            Action::MakeCurrent => {
                let gpu = self.gpu.as_mut().ok_or_else(no_gpu)?;
                if gpu.plan.fails(GpuStage::MakeCurrent) {
                    return Err(GpuFallback::injected(GpuStage::MakeCurrent));
                }
                gpu.context.as_ref().ok_or_else(no_gpu)?.make_current()
            }
            Action::CreateSurface(surface) => {
                let parent = self.gpu_view_layer();
                let gpu = self.gpu.as_mut().ok_or_else(no_gpu)?;
                if gpu.plan.fails(GpuStage::Surface) {
                    return Err(GpuFallback::injected(GpuStage::Surface));
                }
                let (Some(library), Some(context), Some(gl)) =
                    (gpu.library.as_ref(), gpu.context.as_ref(), gpu.gl.as_ref())
                else {
                    return Err(no_gpu());
                };
                context.make_current()?;
                gpu.drawable = Some(Drawable::create(library, context, gl, parent, *surface)?);
                Ok(())
            }
            Action::ResizeSurface(surface) => {
                let parent = self.gpu_view_layer();
                let gpu = self.gpu.as_mut().ok_or_else(no_gpu)?;
                if gpu.plan.fails(GpuStage::Surface) {
                    return Err(GpuFallback::injected(GpuStage::Surface));
                }
                let (Some(context), Some(drawable)) = (gpu.context.as_ref(), gpu.drawable.as_mut())
                else {
                    return Err(no_gpu());
                };
                context.make_current()?;
                drawable.resize(context, parent, *surface)
            }
            Action::DestroySurface => {
                if let Some(gpu) = self.gpu.as_mut() {
                    if let Some(context) = gpu.context.as_ref() {
                        let _ = context.make_current();
                    }
                    if let Some(mut drawable) = gpu.drawable.take() {
                        drawable.destroy();
                    }
                }
                Ok(())
            }
            Action::BuildRenderer => self.gpu_build_renderer(),
            Action::DestroyRenderer => {
                if let Some(gpu) = self.gpu.as_ref() {
                    if let Some(context) = gpu.context.as_ref() {
                        let _ = context.make_current();
                    }
                }
                // WebRender's renderer must be deinit'd inside a frame, with its context current; the
                // engine's own GL objects (SVG shaders) go with the pointer
                self.common.deinit_renderer();
                self.common.render_api = None;
                self.common.document_id = None;
                self.common.id_namespace = None;
                self.common.gl_context_ptr = OptionGlContextPtr::None;
                Ok(())
            }
            Action::DestroyContext => {
                if let Some(gpu) = self.gpu.as_mut() {
                    gpu.context = None;
                    EaglContext::clear_current();
                }
                Ok(())
            }
            Action::ReleaseCaches => {
                if let Some(api) = self.common.render_api.as_ref() {
                    api.notify_memory_pressure();
                    log_info!(
                        LogCategory::Rendering,
                        "[iOS GPU] memory pressure: the renderer's caches are released"
                    );
                }
                Ok(())
            }
            Action::StopFrames => {
                // SAFETY: the display link is the shell's, on the main thread.
                unsafe { set_display_link_paused(true) };
                Ok(())
            }
            Action::StartFrames => {
                // SAFETY: as above.
                unsafe { set_display_link_paused(false) };
                Ok(())
            }
            Action::Finish => {
                if let Some(gl) = self.gpu.as_ref().and_then(|gpu| gpu.gl.as_ref()) {
                    gl.finish();
                }
                Ok(())
            }
            Action::Present => self.gpu_draw_frame(),
            Action::AbandonGl => {
                // no GL command and no call that flushes one: what must not be released is leaked
                if let Some(renderer) = self.common.renderer.take() {
                    std::mem::forget(renderer);
                }
                self.common.render_api = None;
                self.common.document_id = None;
                self.common.id_namespace = None;
                // (the pointer owns the engine's GL programs: dropping it would delete them)
                if let OptionGlContextPtr::Some(pointer) = std::mem::replace(
                    &mut self.common.gl_context_ptr,
                    OptionGlContextPtr::None,
                ) {
                    std::mem::forget(pointer);
                }
                if let Some(gpu) = self.gpu.as_mut() {
                    if let Some(mut drawable) = gpu.drawable.take() {
                        drawable.abandon();
                    }
                    if let Some(context) = gpu.context.take() {
                        context.leak();
                    }
                }
                Ok(())
            }
            Action::FallBackToCpu(fallback) => {
                self.gpu_fall_back(fallback);
                Ok(())
            }
        }
    }

    // ----- the pieces of the bring-up ----------------------------------------------------------

    /// Open the framework, fill the GL table, make the context and make it current.
    fn gpu_create_context(&mut self) -> Result<(), GpuFallback> {
        let gpu = self.gpu.as_mut().ok_or_else(no_gpu)?;
        if gpu.library.is_none() {
            if gpu.plan.fails(GpuStage::Library) {
                return Err(GpuFallback::injected(GpuStage::Library));
            }
            gpu.library = Some(EaglLibrary::open()?);
        }
        if gpu.gl.is_none() {
            if gpu.plan.fails(GpuStage::Functions) {
                return Err(GpuFallback::injected(GpuStage::Functions));
            }
            gpu.gl = Some(gpu.library.as_ref().ok_or_else(no_gpu)?.functions()?);
        }
        if gpu.plan.fails(GpuStage::Context) {
            return Err(GpuFallback::injected(GpuStage::Context));
        }
        let context = EaglContext::new()?;
        context.make_current()?;
        gpu.context = Some(context);
        Ok(())
    }

    /// WebRender on the context: probe what it is, build the renderer and its document, prove the engine's own
    /// shaders compile, and hand all of it to the window.
    fn gpu_build_renderer(&mut self) -> Result<(), GpuFallback> {
        let gpu = self.gpu.as_mut().ok_or_else(no_gpu)?;
        let (Some(gl), Some(context), Some(drawable)) =
            (gpu.gl.clone(), gpu.context.as_ref(), gpu.drawable.as_ref())
        else {
            return Err(no_gpu());
        };
        context.make_current()?;

        // what the context is: ES 3.0 is required, and `auto` refuses a software renderer
        if gpu.plan.fails(GpuStage::Probe) {
            return Err(GpuFallback::injected(GpuStage::Probe));
        }
        let info = EaglContext::device_info(&gl);
        log_info!(
            LogCategory::Rendering,
            "[iOS GPU] GL_VENDOR={:?} GL_RENDERER={:?} GL_VERSION={:?} GLSL={:?}",
            info.vendor,
            info.renderer,
            info.version,
            info.glsl
        );
        if let Some(fallback) = decide(gpu.request, Some(Ok(info.clone()))).fallback {
            return Err(fallback);
        }
        gpu.device = Some(info);

        // the renderer: it takes the framebuffer that is bound when a frame begins as its default one
        let (width, height) = drawable.size();
        let framebuffer_size = webrender::api::units::DeviceIntSize::new(width.max(1), height.max(1));
        drawable.bind();
        if gpu.plan.fails(GpuStage::Renderer) {
            return Err(GpuFallback::injected(GpuStage::Renderer));
        }
        let notifier = Notifier {
            new_frame_ready: gpu.new_frame_ready.clone(),
            // the display tick looks at the flag every frame; nothing sleeps that has to be woken
            wake: None,
        };
        let renderer_started = Instant::now();
        let options = WindowCreateOptions {
            window_state: self.common.current_window_state().clone(),
            ..WindowCreateOptions::default()
        };
        let (mut renderer, sender) = webrender::create_webrender_instance(
            gl.clone(),
            Box::new(notifier),
            default_renderer_options(&options, create_program_cache(&gl), None),
            None,
        )
        .map_err(|e| GpuFallback::new(GpuStage::Renderer, &format!("create_webrender_instance: {e:?}")))?;
        renderer.set_external_image_handler(Box::new(Compositor::default()));
        let mut render_api = sender.create_api();
        log_info!(
            LogCategory::Rendering,
            "[iOS GPU] WebRender renderer created in {} ms",
            renderer_started.elapsed().as_millis()
        );

        if gpu.plan.fails(GpuStage::Document) {
            renderer.deinit();
            return Err(GpuFallback::injected(GpuStage::Document));
        }
        let document_id = translate_document_id_wr(render_api.add_document(framebuffer_size));
        let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());

        // the engine's own SVG / FXAA shaders must compile at some GLSL version, or the context is no use
        let gl_pointer = GlContextPtr::new(RendererType::Hardware, gl.clone());
        if gpu.plan.fails(GpuStage::Shaders) || !gl_pointer.is_gl_usable() {
            renderer.deinit();
            return Err(GpuFallback::new(
                GpuStage::Shaders,
                "the engine's shaders compile at no GLSL version on this context",
            ));
        }

        self.common.renderer = Some(renderer);
        self.common.render_api = Some(render_api);
        self.common.document_id = Some(document_id);
        self.common.id_namespace = Some(id_namespace);
        self.common.gl_context_ptr = OptionGlContextPtr::Some(gl_pointer);
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.document_id = document_id;
            layout_window.id_namespace = id_namespace;
            layout_window.renderer_type = Some(RendererType::Hardware);
        }
        // a document that is new has no display list: the next frame sends the layout that already ran
        self.common.display_list_initialized = false;
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.resend = true;
        }
        Ok(())
    }

    /// The GPU path ended: the CPU renderer draws from now on, and the whole window is drawn again by it.
    fn gpu_fall_back(&mut self, fallback: &GpuFallback) {
        let request = self.gpu.as_ref().map_or(GpuRequest::Gpu, |gpu| gpu.request);
        self.gpu_report(&decide(request, Some(Err(fallback.clone()))));
        self.backend = RenderBackend::Cpu;
        self.gpu = None;
        self.common.gl_context_ptr = OptionGlContextPtr::None;
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.renderer_type = Some(RendererType::Software);
        }
        // the CPU renderer has drawn nothing yet: a full layout, a full raster, a present
        self.common.request_regeneration(RelayoutReason::RefreshDom);
        self.needs_rerender = true;
        let _ = self.present();
    }

    // ----- the frame ---------------------------------------------------------------------------

    /// The display tick's GPU work: do what is owed (a layout, a repaint, a frame WebRender finished) and, if
    /// something was, draw and present one frame. Nothing is done while the app is not active.
    pub(super) fn gpu_tick(&mut self) {
        self.gpu_report_when_up();
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        if gpu.lifecycle.phase() != Phase::Ready {
            return;
        }
        let built = gpu
            .new_frame_ready
            .0
            .lock()
            .map(|flag| *flag)
            .unwrap_or(false);
        let animating = self.common.layout_window.as_ref().is_some_and(|lw| {
            lw.scroll_manager.has_active_animations()
                || lw.needs_animation_frame()
                || lw.gpu_state_manager.frames_owed()
                || !lw.pending_virtual_view_updates.is_empty()
        });
        let owed = self.common.regeneration_pending()
            || self.needs_rerender
            || built
            || gpu.resend
            || animating;
        if !owed {
            return;
        }

        // `<webview>`s first: their reports may ask for a rebuild or a repaint, which this pass then makes
        self.pump_webviews_if_any();

        // the same order `displayLayer:` follows on the CPU path: a relayout-only request first (it has already
        // laid the existing DOM out), then a rebuild, then a repaint
        let mut sent = false;
        if self.common.take_relayout_only() {
            let epoch = self.common.regen_epoch();
            self.common.clear_regeneration_unless_reraised(epoch);
            self.gpu_send_frame();
            sent = true;
        }
        if self.common.regeneration_pending() {
            self.needs_rerender = false;
            // `regenerate_layout_inner` sends the frame of a GPU window
            if let Err(e) = self.regenerate_layout() {
                log_error!(LogCategory::Layout, "[iOS] regenerate_layout: {}", e);
            }
            sent = true;
        }
        if !sent {
            self.needs_rerender = false;
            if self.gpu.as_ref().is_some_and(|gpu| gpu.resend) {
                self.gpu_send_frame();
            } else {
                self.gpu_send_lightweight();
            }
        }
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.resend = false;
        }
        let _ = crate::desktop::shell2::common::webview::sync(self);

        // The first frame of a renderer waits until WebRender has built one (its notifier sets the flag the
        // tick reads): a render before that draws nothing. Later frames draw what WebRender has, at once.
        let built = self.gpu.as_ref().is_some_and(|gpu| {
            gpu.new_frame_ready
                .0
                .lock()
                .map(|flag| *flag)
                .unwrap_or(false)
        });
        if built || self.common.display_list_initialized {
            self.gpu_event(Event::Frame);
        }
    }

    /// Send the display list of the layout that exists to WebRender (a full transaction).
    pub(super) fn gpu_send_frame(&mut self) {
        let (Some(layout_window), Some(render_api), Some(document_id)) = (
            self.common.layout_window.as_mut(),
            self.common.render_api.as_mut(),
            self.common.document_id,
        ) else {
            return;
        };
        generate_frame(layout_window, render_api, document_id, &self.common.gl_context_ptr);
        render_api.flush_scene_builder();
    }

    /// A pass that laid nothing out: scroll offsets, GPU values and image callbacks go as the light
    /// transaction, unless the display list itself changed (a caret, a selection, an undo, a re-rendered virtual
    /// view): then it is the full one.
    fn gpu_send_lightweight(&mut self) {
        let display_list_dirty = std::mem::take(&mut self.common.display_list_dirty);
        let (Some(layout_window), Some(render_api), Some(document_id)) = (
            self.common.layout_window.as_mut(),
            self.common.render_api.as_mut(),
            self.common.document_id,
        ) else {
            return;
        };
        // easing-based scroll animations move one step
        let now = azul_core::task::Instant::System(std::time::Instant::now().into());
        let tick = layout_window.scroll_manager.tick(now);
        if tick.needs_repaint {
            layout_window.scroll_manager.calculate_scrollbar_states();
        }
        if display_list_dirty || !layout_window.pending_virtual_view_updates.is_empty() {
            generate_frame(layout_window, render_api, document_id, &self.common.gl_context_ptr);
            render_api.flush_scene_builder();
        } else {
            let _sent = submit_lightweight_frame(
                layout_window,
                render_api,
                document_id,
                &self.common.gl_context_ptr,
                true,
                "iOS",
            );
        }
    }

    /// `Action::Present`: WebRender draws the frame it has into the framebuffer of the layer, and the layer shows
    /// it. A failure is the machine's (one rebuild, then the CPU).
    fn gpu_draw_frame(&mut self) -> Result<(), GpuFallback> {
        let gpu = self.gpu.as_mut().ok_or_else(no_gpu)?;
        if gpu.plan.fails(GpuStage::Frame) {
            return Err(GpuFallback::injected(GpuStage::Frame));
        }
        let (Some(context), Some(drawable)) = (gpu.context.as_ref(), gpu.drawable.as_ref()) else {
            return Err(no_gpu());
        };
        context.make_current()?;
        // every frame WebRender finished up to now is taken in
        if let Ok(mut flag) = gpu.new_frame_ready.0.lock() {
            *flag = false;
        }
        self.common.sync_renderer_clear_color();
        let Some(renderer) = self.common.renderer.as_mut() else {
            return Err(GpuFallback::new(GpuStage::Frame, "no renderer"));
        };
        renderer.update();
        drawable.bind();
        let (width, height) = drawable.size();
        let size = webrender::api::units::DeviceIntSize::new(width.max(1), height.max(1));
        let started = Instant::now();
        let results = renderer
            .render(size, 0)
            .map_err(|errors| GpuFallback::new(GpuStage::Frame, &format!("render: {errors:?}")))?;
        let draw_calls = results.stats.total_draw_calls as u64;
        drawable.present(context)?;
        let render_us = elapsed_us(started);
        self.present_log.note(Some(render_us));
        if let Some(gpu) = self.gpu.as_mut() {
            if !gpu.first_frame_reported {
                gpu.first_frame_reported = true;
                log_info!(
                    LogCategory::Rendering,
                    "[iOS GPU] first frame {}x{}: {} draw calls, drawn in {} ms, {} ms after the GPU path began",
                    width,
                    height,
                    draw_calls,
                    render_us / 1000,
                    gpu.started.elapsed().as_millis()
                );
            }
        }

        if let Some(layout_window) = self.common.layout_window.as_ref() {
            // the frame report and `get_frame_timings` know what the GPU path did
            publish_gpu_frame(layout_window, 0, render_us, draw_calls);
            remove_old_gl_textures(&layout_window.document_id, layout_window.epoch);
        }
        self.common.display_list_initialized = true;
        // a scrollbar that is fading owes more frames
        let owes_frames = self
            .common
            .layout_window
            .as_ref()
            .is_some_and(|lw| lw.gpu_state_manager.frames_owed());
        if owes_frames {
            self.needs_rerender = true;
        }
        // a simulated loss of the context at the swap (a test run's `AZ_GPU_FAIL=lost`)
        let lost = self
            .gpu
            .as_mut()
            .is_some_and(|gpu| gpu.plan.fails(GpuStage::Lost));
        if lost {
            return Err(GpuFallback::injected(GpuStage::Lost));
        }
        if let Some(gpu) = self.gpu.as_mut() {
            let _ = gpu.lifecycle.step(Event::FramePresented);
        }
        log_debug!(
            LogCategory::Rendering,
            "[iOS GPU] frame {}x{}: {} draw calls, {} us",
            width,
            height,
            draw_calls,
            render_us
        );
        Ok(())
    }

    // ----- what UIKit tells the window ---------------------------------------------------------

    /// The view's size or scale changed (`layoutSubviews`): the drawable follows.
    pub(super) fn gpu_resized(&mut self) {
        if self.gpu.is_none() {
            return;
        }
        let surface = self.gpu_surface();
        self.gpu_event(Event::Resized(surface));
    }

    /// The system wants memory back (`applicationDidReceiveMemoryWarning:`).
    pub(super) fn gpu_memory_warning(&mut self) {
        self.gpu_event(Event::TrimMemory(TrimLevel::Light));
    }
}

/// What the app's own `Info.plist` says about the renderer (`AZBackend`), when the environment does not: a phone
/// app is started by the home screen, not by a shell that can set `AZ_BACKEND`. `None` when the key is not there
/// or is not text.
fn plist_backend() -> Option<String> {
    use std::ffi::{CStr, CString};

    let bundle_class = objc2::runtime::AnyClass::get(c"NSBundle")?;
    let string_class = objc2::runtime::AnyClass::get(c"NSString")?;
    let key = CString::new("AZBackend").ok()?;
    // SAFETY: Foundation on the main thread; every object is autoreleased and read at once.
    unsafe {
        let bundle: *mut AnyObject = msg_send![bundle_class, mainBundle];
        if bundle.is_null() {
            return None;
        }
        let name: *mut AnyObject = msg_send![string_class, stringWithUTF8String: key.as_ptr()];
        let value: *mut AnyObject = msg_send![bundle, objectForInfoDictionaryKey: name];
        if value.is_null() {
            return None;
        }
        let is_text: objc2::runtime::Bool = msg_send![value, isKindOfClass: string_class];
        if !is_text.as_bool() {
            return None;
        }
        let bytes: *const std::ffi::c_char = msg_send![value, UTF8String];
        if bytes.is_null() {
            return None;
        }
        Some(CStr::from_ptr(bytes).to_string_lossy().into_owned())
    }
}

fn no_gpu() -> GpuFallback {
    GpuFallback::new(GpuStage::Context, "the GPU path has no such object (a bug in the lifecycle)")
}
