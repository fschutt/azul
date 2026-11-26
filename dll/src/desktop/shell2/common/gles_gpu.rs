//! The GPU path of a phone window that draws with OpenGL ES over EGL (Android), and of its host test (Linux, Mesa): the
//! executor of the lifecycle reducer the iOS shell follows too (`gpu_lifecycle`). An EGL surface on the window, an
//! OpenGL ES 3 context, and the engine's WebRender on it, exactly as the desktop GL shells use it.
//!
//! # Who owns what
//!
//! The window owns one [`GlesGpu`]. It owns the reducer, the EGL session (display, config, context, window surface), the
//! GL function table and the cell WebRender reports the damage of a frame to; the renderer, its API and the document
//! stay where every GL window keeps them, in `CommonWindowState` (`gpu_renderer::install`). Nothing is global. Every call
//! is made on the thread that runs the window's loop: an EGL context is current on one thread at a time (on Android the
//! `android_main` thread, never the Java UI thread).
//!
//! # The loop
//!
//! The shell turns what Android says into events and hands them to [`GlesGpu::feed`] (or one of its named wrappers); the
//! reducer answers with actions, which [`GlesGpu::perform`] performs in order. A failed action is reported back as
//! `Event::ActionFailed`; the reducer answers with the teardown and `FallBackToCpu`, which ends the GPU path with ONE
//! diagnostic line. The window surface is destroyed before the native window is let go and before the CPU path may lock
//! the window (an `ANativeWindow` has one producer at a time).
//!
//! - `InitWindow` is `Event::SurfaceCreated`, before any CPU frame: a window the CPU path locked can never get an EGL
//!   surface.
//! - `TerminateWindow` is `Event::SurfaceDestroyed`: the EGL surface goes, the CONTEXT AND THE RENDERER STAY (current with
//!   no surface, `EGL_KHR_surfaceless_context`, or on a 1 x 1 pbuffer): a rotation or a home press costs one new window
//!   surface, not the glyph atlas and the compiled programs.
//! - `onTrimMemory` and `onLowMemory` are `Event::TrimMemory`.
//! - The destroying actions make the context current themselves: an answer that tears down holds no `MakeCurrent`.

use std::{
    cell::Cell,
    ffi::c_void,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

use azul_core::gl::OptionGlContextPtr;
use azul_layout::window_state::WindowCreateOptions;

use super::{
    debug_server::LogCategory,
    egl_gles::{DisplayKind, EglApi, EglFailure, EglSession, GlesFunctions},
    egl_pure::{egl_error_name, EglInt, SwapError, EGL_PBUFFER_BIT, EGL_WINDOW_BIT},
    event::CommonWindowState,
    frame_stats::FrameStats,
    gpu_lifecycle::{Action, Event, GpuLifecycle, Phase, Surface, TrimLevel},
    gpu_renderer::{self, GpuDevice, InitParams},
    gpu_select::{decide, Decision, FailPlan, GpuFallback, GpuRequest, GpuStage, Ran},
};
use crate::{
    desktop::wr_translate2::{device_rects_to_present_rects, Notifier, PartialPresentDamage},
    log_info, log_warn,
};

#[cfg(target_os = "android")]
#[link(name = "android")]
extern "C" {
    /// `ANativeWindow_setBuffersGeometry` (`libandroid`): with `0, 0` and the config's
    /// `EGL_NATIVE_VISUAL_ID` the window keeps its own size and takes the pixel format its EGL surface draws in.
    fn ANativeWindow_setBuffersGeometry(
        window: *mut c_void,
        width: i32,
        height: i32,
        format: i32,
    ) -> i32;
}

/// Gives the native window the pixel format of the EGL config; the status of the platform call, 0 for success. Off
/// Android there is no native window.
#[allow(unused_variables)]
fn set_window_format(window: *mut c_void, format: EglInt) -> i32 {
    #[cfg(target_os = "android")]
    {
        // SAFETY: `window` is the live `ANativeWindow*` the shell lent for the length of the call.
        unsafe { ANativeWindow_setBuffersGeometry(window, 0, 0, format) }
    }
    #[cfg(not(target_os = "android"))]
    {
        0
    }
}

/// Where the path finds its libraries and what it makes its surfaces of.
#[derive(Debug, Clone)]
pub struct GlesGpuConfig {
    /// The EGL libraries, first that loads.
    pub egl_libraries: Vec<&'static str>,
    /// The OpenGL ES libraries whose symbols are the core functions, first that loads.
    pub gles_libraries: Vec<&'static str>,
    /// Which display.
    pub display: DisplayKind,
    /// The `EGL_SURFACE_TYPE` masks to try for the config, best first.
    pub surface_types: Vec<EglInt>,
    /// The window's surface is a pbuffer of the window's size (a host with no window system: the test), not a native
    /// window's.
    pub offscreen: bool,
}

impl GlesGpuConfig {
    /// A phone: `libEGL.so`, `libGLESv3.so` (all of ES 2 as well), the platform's display, a config that can make a
    /// window surface and a pbuffer (so the context can run without the window), else a window only.
    #[must_use]
    pub fn android() -> Self {
        Self {
            egl_libraries: vec!["libEGL.so"],
            gles_libraries: vec!["libGLESv3.so", "libGLESv2.so"],
            display: DisplayKind::Default,
            surface_types: vec![EGL_WINDOW_BIT | EGL_PBUFFER_BIT, EGL_WINDOW_BIT],
            offscreen: false,
        }
    }

    /// A Linux host with Mesa: no window system, a pbuffer where the window's surface would be.
    #[must_use]
    pub fn host_offscreen() -> Self {
        Self {
            egl_libraries: vec!["libEGL.so.1", "libEGL.so"],
            gles_libraries: vec!["libGLESv2.so.2", "libGLESv2.so"],
            display: DisplayKind::SurfacelessMesa,
            surface_types: vec![EGL_PBUFFER_BIT],
            offscreen: true,
        }
    }
}

/// What a window asks of the renderer: `AZ_BACKEND` as the app's environment typed it, then the window's `hw_accel`
/// (its own renderer options, else the window state's), then the CPU renderer (`GpuRequest::resolve`).
#[must_use]
pub fn request_for(
    shell: &crate::desktop::shell_settings::ShellSettings,
    options: &WindowCreateOptions,
) -> GpuRequest {
    let hw_accel = options
        .renderer
        .as_option()
        .map(|r| r.hw_accel)
        .or(Some(options.window_state.renderer_options.hw_accel));
    GpuRequest::resolve(shell.backend.as_deref(), hw_accel)
}

/// What the GPU path does for an Android `onTrimMemory` level (`ComponentCallbacks2`): nothing below
/// `TRIM_MEMORY_RUNNING_LOW` (10: `RUNNING_MODERATE` is 5), the renderer's caches from there (`RUNNING_LOW` 10,
/// `RUNNING_CRITICAL` 15), and from `TRIM_MEMORY_UI_HIDDEN` (20: the app's UI is no longer visible; `BACKGROUND` 40,
/// `MODERATE` 60, `COMPLETE` 80) the renderer and the context too when no surface needs them (the reducer makes a heavy
/// trim with a surface a light one).
#[must_use]
pub const fn trim_level_for_android(level: i32) -> Option<TrimLevel> {
    if level >= 20 {
        Some(TrimLevel::Heavy)
    } else if level >= 10 {
        Some(TrimLevel::Light)
    } else {
        None
    }
}

/// What the shell lends the GPU path for the length of one call.
pub struct GpuCtx<'a> {
    /// The window's common state: where the renderer, its API, the document and the GL context live.
    pub common: &'a mut CommonWindowState,
    /// The `ANativeWindow*` of the surface, while the window has one (`None` on a host with an offscreen surface).
    pub native_window: Option<*mut c_void>,
}

/// What a call did to the choice of renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The GPU path runs (or nothing changed about it).
    Gpu,
    /// The GPU path ended in this call: the CPU renderer draws from now on, and why. The window's surface is destroyed;
    /// the shell re-arms its CPU present path and asks for a full frame.
    FellBack(GpuFallback),
}

/// What one action did.
enum Performed {
    Done,
    FellBack(GpuFallback),
}

/// Reports a named point of the start (`egl_ready`, `gl_context_ready`, ...) with a detail: the shell's startup timeline.
pub type Marker = Box<dyn Fn(&'static str, &str)>;

/// The GPU path of one window.
pub struct GlesGpu {
    config: GlesGpuConfig,
    lifecycle: GpuLifecycle,
    request: GpuRequest,
    plan: FailPlan,
    options: WindowCreateOptions,
    /// The cache directory of compiled programs (`None`: not kept).
    shader_cache_dir: Option<PathBuf>,
    /// Wakes the loop from WebRender's backend thread when a frame is built.
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
    mark: Option<Marker>,
    /// Set by WebRender's notifier (on its backend thread) when it has built a frame; the loop presents when it finds it set.
    frame_ready: Arc<(Mutex<bool>, Condvar)>,
    /// The current renderer has built at least one frame (what [`Self::has_frame`] says): a present before that would swap an
    /// empty buffer (WebRender renders nothing when it holds no document), a black flash before the first picture.
    frame_seen: Cell<bool>,
    session: Option<EglSession>,
    functions: Option<GlesFunctions>,
    /// The cell WebRender reports the damaged region of a frame to (buffer age).
    damage: PartialPresentDamage,
    /// The driver, once a renderer was made on it.
    device: Option<GpuDevice>,
    /// The frame size of the surface in device pixels.
    size_px: (u32, u32),
    /// The next swap is a full one: a new surface, a new size, a renderer that was just made.
    full_swap_owed: bool,
    /// How many renderers were made: from the second on, the registries of the first one's keys are cleared first.
    renderers_made: u32,
    /// The first frame of the current renderer is timed (WebRender compiles its programs lazily, in that frame).
    first_frame_pending: bool,
    /// What the window decided about its renderer.
    decision: Option<Decision>,
    /// The frames this path presents, measured inside the engine: one `[render] frames n=..` line a second while they come
    /// one after another (`common::frame_stats`), which is how the phone QA tells the frame times of the two renderers.
    frame_stats: FrameStats,
    /// The clock of `frame_stats`: microseconds since the path was made.
    clock: Instant,
}

impl std::fmt::Debug for GlesGpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlesGpu")
            .field("phase", &self.lifecycle.phase())
            .field("request", &self.request)
            .finish()
    }
}

fn fallback_of_egl(failure: &EglFailure, default: GpuStage) -> GpuFallback {
    let stage = GpuStage::parse(failure.stage).unwrap_or(default);
    GpuFallback::new(stage, &failure.to_string())
}

impl GlesGpu {
    /// The GPU path of a window that asked for `request` (never [`GpuRequest::Cpu`]: the shell makes none then).
    /// `plan` is what a test run made fail on purpose (`AZ_GPU_FAIL`), empty in a shipped app. `options` are the window's
    /// creation options (the renderer's clear colour and debug flags come from them). `shader_cache_dir` is where the
    /// compiled programs are kept between runs. `wake` wakes the loop from WebRender's backend thread.
    #[must_use]
    pub fn new(
        config: GlesGpuConfig,
        request: GpuRequest,
        plan: FailPlan,
        options: WindowCreateOptions,
        shader_cache_dir: Option<PathBuf>,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        Self {
            config,
            lifecycle: GpuLifecycle::new(),
            request,
            plan,
            options,
            shader_cache_dir,
            wake,
            mark: None,
            frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            frame_seen: Cell::new(false),
            session: None,
            functions: None,
            damage: PartialPresentDamage::default(),
            device: None,
            size_px: (0, 0),
            full_swap_owed: true,
            renderers_made: 0,
            first_frame_pending: false,
            decision: None,
            frame_stats: FrameStats::new(),
            clock: Instant::now(),
        }
    }

    /// Reports the named points of the start to `mark` (the startup timeline).
    pub fn set_marker(&mut self, mark: Marker) {
        self.mark = Some(mark);
    }

    /// Where the compiled programs are kept between runs; takes effect when the next renderer is made (the app's directory is known
    /// once the activity is, which is after the window).
    pub fn set_shader_cache_dir(&mut self, dir: Option<PathBuf>) {
        self.shader_cache_dir = dir;
    }

    fn note(&self, name: &'static str, detail: &str) {
        if let Some(mark) = &self.mark {
            mark(name, detail);
        }
    }

    /// What the window decided about its renderer, once it did (the lines of the QA contract were written for it).
    #[must_use]
    pub const fn decision(&self) -> Option<&Decision> {
        self.decision.as_ref()
    }

    /// The driver the renderer runs on, while one does.
    #[must_use]
    pub const fn device(&self) -> Option<&GpuDevice> {
        self.device.as_ref()
    }

    /// Whether WebRender has built a frame since the last call, and clears the flag. WebRender builds a frame on its own
    /// thread after a transaction (`submit_full_frame`) and wakes the loop when it is done: the loop presents then, not
    /// at once (a render right after the transaction draws the frame before it).
    #[must_use]
    pub fn take_frame_ready(&self) -> bool {
        let ready = match self.frame_ready.0.lock() {
            Ok(mut ready) => std::mem::take(&mut *ready),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        };
        if ready {
            self.frame_seen.set(true);
        }
        ready
    }

    /// Whether the current renderer has built a frame (and so has something to show): the shell presents a surface's content
    /// again (`RedrawNeeded`, a new surface) only then, and the first picture is the frame WebRender announces.
    #[must_use]
    pub fn has_frame(&self) -> bool {
        self.frame_seen.get()
    }

    /// Waits up to `timeout` for WebRender to build a frame, and clears the flag: `true` when it did. For a host test; a
    /// shell sleeps in its own loop and calls [`Self::take_frame_ready`] when it wakes.
    #[must_use]
    pub fn wait_frame_ready(&self, timeout: std::time::Duration) -> bool {
        let (lock, signal) = &*self.frame_ready;
        let Ok(mut ready) = lock.lock() else {
            return false;
        };
        let deadline = Instant::now() + timeout;
        while !*ready {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            match signal.wait_timeout(ready, left) {
                Ok((next, _)) => ready = next,
                Err(_) => return false,
            }
        }
        *ready = false;
        self.frame_seen.set(true);
        true
    }

    /// The GL function table of the context, while there is one (a host test reads the picture back through it).
    #[must_use]
    pub fn gl(&self) -> Option<std::rc::Rc<gl_context_loader::GenericGlContext>> {
        self.functions.as_ref().map(|f| f.context.clone())
    }

    /// How many renderers this window made (a lost context makes a second one).
    #[must_use]
    pub const fn renderers_made(&self) -> u32 {
        self.renderers_made
    }

    /// Where the reducer stands.
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.lifecycle.phase()
    }

    /// Whether a context exists (it outlives the surface until a heavy trim).
    #[must_use]
    pub const fn has_context(&self) -> bool {
        self.session.is_some()
    }

    /// Whether the GPU path still runs: it has not ended with a fallback and was not shut down.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.lifecycle.fallback().is_none() && self.lifecycle.phase() != Phase::Closed
    }

    /// The window has a surface: `InitWindow`.
    pub fn surface_created(&mut self, ctx: &mut GpuCtx<'_>, surface: Surface) -> Outcome {
        let outcome = self.feed(ctx, Event::SurfaceCreated(surface));
        // The contract line again for the surface that came (the first one wrote it with the renderer): the log of a long
        // run has rolled the first one away, and "which renderer draws this window now" must stay answerable.
        if outcome == Outcome::Gpu {
            if let Some(decision) = self.decision.as_ref().filter(|d| d.ran == Ran::Gpu) {
                log_info!(LogCategory::Rendering, "{}", decision.render_line());
            }
        }
        outcome
    }

    /// The window's surface is going: `TerminateWindow`. Call it BEFORE the native window is released.
    pub fn surface_destroyed(&mut self, ctx: &mut GpuCtx<'_>) -> Outcome {
        self.feed(ctx, Event::SurfaceDestroyed)
    }

    /// The window has another size or density.
    pub fn resized(&mut self, ctx: &mut GpuCtx<'_>, surface: Surface) -> Outcome {
        self.feed(ctx, Event::Resized(surface))
    }

    /// The system wants memory back.
    pub fn trim(&mut self, ctx: &mut GpuCtx<'_>, level: TrimLevel) -> Outcome {
        self.feed(ctx, Event::TrimMemory(level))
    }

    /// A frame is wanted: renders what WebRender has built and swaps it. The frame may be dropped (no surface), or the
    /// path may end (a context that cannot be rebuilt).
    pub fn present(&mut self, ctx: &mut GpuCtx<'_>) -> Outcome {
        self.feed(ctx, Event::Frame)
    }

    /// The context was reported lost.
    pub fn context_lost(&mut self, ctx: &mut GpuCtx<'_>) -> Outcome {
        self.feed(ctx, Event::ContextLost)
    }

    /// The window is closing.
    pub fn shutdown(&mut self, ctx: &mut GpuCtx<'_>) -> Outcome {
        self.feed(ctx, Event::Shutdown)
    }

    /// One event in; its actions performed; what a failed action asks for performed too. Returns the choice of renderer
    /// that stands at the end.
    pub fn feed(&mut self, ctx: &mut GpuCtx<'_>, event: Event) -> Outcome {
        let mut outcome = Outcome::Gpu;
        let mut actions = self.lifecycle.step(event);
        // The answer to a failure holds teardown actions and the fall-back, which cannot fail again; a rebuild that fails
        // answers with the fall-back at the second round. The bound is a guard, not a rule.
        for _ in 0..4 {
            let mut failure = None;
            for action in actions.drain(..) {
                match self.perform(action, ctx) {
                    Ok(Performed::Done) => {}
                    Ok(Performed::FellBack(fallback)) => outcome = Outcome::FellBack(fallback),
                    Err(fallback) => {
                        failure = Some(fallback);
                        break;
                    }
                }
            }
            match failure {
                Some(fallback) => actions = self.lifecycle.step(Event::ActionFailed(fallback)),
                None => break,
            }
        }
        outcome
    }

    /// Performs one action on this thread's EGL. `Err` is the failure the reducer is told about.
    fn perform(&mut self, action: Action, ctx: &mut GpuCtx<'_>) -> Result<Performed, GpuFallback> {
        match action {
            Action::CreateContext => self.create_context().map(|()| Performed::Done),
            Action::CreateSurface(surface) => {
                self.create_surface(surface, ctx).map(|()| Performed::Done)
            }
            Action::ResizeSurface(surface) => {
                self.size_px = (surface.width, surface.height);
                // The old damage says nothing about a surface of another size.
                self.full_swap_owed = true;
                let _ = self.damage.take();
                Ok(Performed::Done)
            }
            Action::MakeCurrent => self.make_current().map(|()| Performed::Done),
            Action::BuildRenderer => self.build_renderer(ctx).map(|()| Performed::Done),
            Action::Present => self.render_and_swap(ctx).map(|()| Performed::Done),
            Action::ReleaseCaches => {
                if let Some(api) = ctx.common.render_api.as_ref() {
                    api.notify_memory_pressure();
                }
                Ok(Performed::Done)
            }
            Action::Finish => {
                if let Some(functions) = &self.functions {
                    functions.context.finish();
                }
                Ok(Performed::Done)
            }
            // The loop polls at the frame interval while there is a surface and the swap waits for the display's vertical
            // blank (`eglSwapInterval(1)`): no frame clock of this path's own to start or stop.
            Action::StartFrames | Action::StopFrames => Ok(Performed::Done),
            Action::DestroySurface => {
                if let Some(session) = self.session.as_mut() {
                    session.destroy_window_surface();
                }
                self.full_swap_owed = true;
                Ok(Performed::Done)
            }
            Action::DestroyRenderer => {
                self.destroy_renderer(ctx);
                Ok(Performed::Done)
            }
            Action::DestroyContext => {
                // The renderer's objects are gone (`DestroyRenderer` came first); dropping the session unbinds the context
                // and destroys it. The display is the process's and stays.
                self.functions = None;
                self.session = None;
                self.device = None;
                Ok(Performed::Done)
            }
            Action::AbandonGl => {
                self.abandon(ctx);
                Ok(Performed::Done)
            }
            Action::FallBackToCpu(fallback) => {
                self.ended(ctx, &fallback);
                Ok(Performed::FellBack(fallback))
            }
        }
    }

    /// `eglGetDisplay`, `eglInitialize`, a config, an OpenGL ES 3 context.
    fn create_context(&mut self) -> Result<(), GpuFallback> {
        if self.plan.fails(GpuStage::Library) {
            return Err(GpuFallback::injected(GpuStage::Library));
        }
        let api = EglApi::load(&self.config.egl_libraries)
            .map_err(|e| fallback_of_egl(&e, GpuStage::Library))?;
        if self.plan.fails(GpuStage::Context) {
            return Err(GpuFallback::injected(GpuStage::Context));
        }
        let session = EglSession::open(api, self.config.display, &self.config.surface_types)
            .map_err(|e| fallback_of_egl(&e, GpuStage::Context))?;
        let info = session.info();
        let summary = format!(
            "EGL {}.{} {}: surfaceless context {}, buffer age {}, swap with damage {}, pbuffer {}, depth {} stencil {}",
            info.version.0,
            info.version.1,
            info.vendor,
            info.caps.surfaceless_context,
            info.caps.buffer_age,
            info.caps.swap_with_damage_symbol().is_some(),
            info.pbuffer_capable,
            info.depth_stencil.0,
            info.depth_stencil.1
        );
        log_info!(LogCategory::Rendering, "[render] {}", summary);
        self.note("egl_ready", &summary);
        self.session = Some(session);
        Ok(())
    }

    /// The EGL surface of the native window (or, off a phone, an offscreen one of the window's size).
    fn create_surface(&mut self, surface: Surface, ctx: &GpuCtx<'_>) -> Result<(), GpuFallback> {
        if self.plan.fails(GpuStage::Surface) {
            return Err(GpuFallback::injected(GpuStage::Surface));
        }
        let offscreen = self.config.offscreen;
        let Some(session) = self.session.as_mut() else {
            return Err(GpuFallback::new(GpuStage::Surface, "there is no EGL session"));
        };
        if offscreen {
            session
                .create_offscreen_surface(surface.width, surface.height)
                .map_err(|e| fallback_of_egl(&e, GpuStage::Surface))?;
        } else {
            let Some(window) = ctx.native_window else {
                return Err(GpuFallback::new(GpuStage::Surface, "the window has no native window"));
            };
            // The window takes the pixel format of the config, at its own size, before the surface is made.
            let status = set_window_format(window, session.info().native_visual_id);
            if status != 0 {
                return Err(GpuFallback::new(
                    GpuStage::Surface,
                    &format!("ANativeWindow_setBuffersGeometry returned {status}"),
                ));
            }
            session
                .create_window_surface(window)
                .map_err(|e| fallback_of_egl(&e, GpuStage::Surface))?;
        }
        self.size_px = (surface.width, surface.height);
        self.full_swap_owed = true;
        let _ = self.damage.take();
        Ok(())
    }

    /// The context current on this thread, with the surface it has.
    fn make_current(&mut self) -> Result<(), GpuFallback> {
        if self.plan.fails(GpuStage::MakeCurrent) {
            return Err(GpuFallback::injected(GpuStage::MakeCurrent));
        }
        let Some(session) = self.session.as_mut() else {
            return Err(GpuFallback::new(GpuStage::MakeCurrent, "there is no EGL session"));
        };
        session
            .bind()
            .map_err(|e| fallback_of_egl(&e, GpuStage::MakeCurrent))?;
        if session.has_window_surface() {
            // Paces the frame loop: a swap waits for the display's vertical blank.
            let _ = session.set_swap_interval(1);
        }
        Ok(())
    }

    /// The function table, the probe, WebRender and its document on the current context.
    fn build_renderer(&mut self, ctx: &mut GpuCtx<'_>) -> Result<(), GpuFallback> {
        let Some(session) = self.session.as_ref() else {
            return Err(GpuFallback::new(GpuStage::Renderer, "there is no EGL session"));
        };
        if self.functions.is_none() {
            if self.plan.fails(GpuStage::Functions) {
                return Err(GpuFallback::injected(GpuStage::Functions));
            }
            let functions = GlesFunctions::load(session, &self.config.gles_libraries)
                .map_err(|cause| GpuFallback::new(GpuStage::Functions, &cause))?;
            self.functions = Some(functions);
        }
        let Some(gl) = self.functions.as_ref().map(|f| f.context.clone()) else {
            return Err(GpuFallback::new(GpuStage::Functions, "no function table"));
        };
        // A renderer made before had keys of its own: the window's registries are cleared for the new one.
        if self.renderers_made > 0 {
            gpu_renderer::forget_renderer_state(ctx.common);
        }

        // A frame the renderer before announced is no frame of this one.
        let _ = self.take_frame_ready();
        self.frame_seen.set(false);
        let notifier = Notifier {
            new_frame_ready: self.frame_ready.clone(),
            wake: self.wake.clone(),
        };
        let damage = session.info().caps.buffer_age.then(|| self.damage.clone());
        let parts = gpu_renderer::init_webrender(
            &gl,
            &self.options,
            InitParams {
                notifier,
                damage,
                framebuffer_px: self.size_px,
                shader_cache_dir: self.shader_cache_dir.as_deref(),
            },
            &mut self.plan,
        )?;
        let device = gpu_renderer::install(ctx.common, parts);
        self.renderers_made += 1;

        // The decision, with what the context says about itself: OpenGL ES 3 whatever was asked, and `auto` does not take
        // a software renderer. A veto takes the renderer back before it reports, so the teardown has nothing left of it.
        let decision = decide(self.request, Some(Ok(device.device_info())));
        if decision.ran == Ran::Cpu {
            gpu_renderer::release(ctx.common);
            return Err(decision.fallback.clone().unwrap_or_else(|| {
                GpuFallback::new(GpuStage::Probe, "the probe refused the context")
            }));
        }
        self.full_swap_owed = true;
        self.first_frame_pending = true;
        self.note(
            "gl_context_ready",
            &format!("device={} gles={}.{}", device.info.renderer, device.gles.major, device.gles.minor),
        );
        self.note(
            "webrender_ready",
            &format!(
                "shaders=lazy cached_programs={} cache_dir={:?}",
                device.shader_cache.loaded, device.shader_cache.dir
            ),
        );
        self.note("render_backend", &decision.timeline_detail());
        log_info!(LogCategory::Rendering, "{}", decision.render_line());
        log_info!(
            LogCategory::Rendering,
            "[render] OpenGL ES {}.{} {} / {} (software: {}, emulated: {}); {} programs read from {:?}{}",
            device.gles.major,
            device.gles.minor,
            device.info.vendor,
            device.info.version,
            device.class.software,
            device.class.emulated,
            device.shader_cache.loaded,
            device.shader_cache.dir,
            device
                .shader_cache
                .problem
                .as_ref()
                .map(|p| format!(" (no program cache: {p})"))
                .unwrap_or_default()
        );
        self.decision = Some(decision);
        self.device = Some(device);
        Ok(())
    }

    /// Renders the frame WebRender built and swaps it to the window.
    fn render_and_swap(&mut self, ctx: &mut GpuCtx<'_>) -> Result<(), GpuFallback> {
        if self.plan.fails(GpuStage::Frame) {
            return Err(GpuFallback::injected(GpuStage::Frame));
        }
        // A simulated loss of the context at the swap.
        if self.plan.fails(GpuStage::Lost) {
            return Err(GpuFallback::injected(GpuStage::Lost));
        }
        let Some(session) = self.session.as_ref() else {
            return Err(GpuFallback::new(GpuStage::Frame, "there is no EGL session"));
        };
        let started = Instant::now();
        // What the window surface is, as EGL says it (it follows the native window at the next swap).
        let size = session.surface_size().unwrap_or(self.size_px);
        let age = session.buffer_age();
        gpu_renderer::render_frame(ctx.common, size, age)?;
        // What the draw cost the loop's thread: WebRender's commands for the frame, before the swap that may wait for the display.
        let draw_us = u64::try_from(started.elapsed().as_micros()).ok();

        // The damage of the frame, from WebRender, unless the swap must be a full one.
        let damage = if self.full_swap_owed {
            let _ = self.damage.take();
            None
        } else {
            self.damage
                .take()
                .map(|rects| device_rects_to_present_rects(&rects, size.0, size.1))
        };
        match session.swap(damage.as_deref(), size.1) {
            Ok(()) => {
                self.full_swap_owed = false;
                gpu_renderer::finish_frame(ctx.common);
                if self.first_frame_pending {
                    self.first_frame_pending = false;
                    let ms = started.elapsed().as_secs_f64() * 1000.0;
                    log_info!(
                        LogCategory::Rendering,
                        "[render] first GPU frame: {:.1} ms (WebRender compiles its programs in it)",
                        ms
                    );
                    self.note("first_gpu_frame_done", &format!("ms={ms:.1}"));
                }
                // A good frame forgives the losses before it.
                let _ = self.lifecycle.step(Event::FramePresented);
                let at_us = u64::try_from(self.clock.elapsed().as_micros()).unwrap_or(u64::MAX);
                if let Some(line) = self.frame_stats.record(at_us, draw_us) {
                    log_info!(LogCategory::Rendering, "{}", line);
                }
                Ok(())
            }
            // The system took the window; `TerminateWindow` is on its way and the next surface gets the next frame.
            Err(SwapError::SurfaceLost) => Ok(()),
            Err(SwapError::ContextLost) => Err(GpuFallback::new(
                GpuStage::Lost,
                "eglSwapBuffers: the context was lost",
            )),
            Err(SwapError::Failed(code)) => Err(GpuFallback::new(
                GpuStage::Frame,
                &format!("eglSwapBuffers failed ({} 0x{code:04x})", egl_error_name(code)),
            )),
        }
    }

    /// `Renderer::deinit` and what only the renderer knew, with the context made current first (the reducer's destroying
    /// answers make no `MakeCurrent`).
    fn destroy_renderer(&mut self, ctx: &mut GpuCtx<'_>) {
        if let Some(session) = self.session.as_mut() {
            if let Err(e) = session.bind() {
                // A context that cannot be made current (a config with no pbuffer and no surfaceless support, after the
                // window's surface went) cannot run the renderer's deletions: they are not attempted, what the renderer holds
                // goes with the context.
                log_warn!(
                    LogCategory::Rendering,
                    "[render] the renderer cannot be released in a current context ({}): left to the context's destruction",
                    e
                );
                self.abandon_renderer(ctx);
                return;
            }
        }
        gpu_renderer::release(ctx.common);
        self.device = None;
    }

    /// The renderer and what talks to it are dropped without any GL command (`mem::forget` for what would run one).
    fn abandon_renderer(&mut self, ctx: &mut GpuCtx<'_>) {
        if let Some(renderer) = ctx.common.renderer.take() {
            std::mem::forget(renderer);
        }
        std::mem::forget(std::mem::replace(
            &mut ctx.common.gl_context_ptr,
            OptionGlContextPtr::None,
        ));
        ctx.common.hit_tester = None;
        ctx.common.render_api = None;
        ctx.common.document_id = None;
        ctx.common.id_namespace = None;
        self.device = None;
    }

    /// The context is lost, or GL is not allowed: nothing is deleted with a GL command; what cannot be released is leaked.
    fn abandon(&mut self, ctx: &mut GpuCtx<'_>) {
        self.abandon_renderer(ctx);
        // The EGL objects of a lost context are destroyed with EGL calls, which a lost context allows; the table of
        // functions pointed into it.
        self.functions = None;
        self.session = None;
    }

    /// The GPU path ended: what is still there goes, the one diagnostic is written, and the CPU renderer takes over.
    fn ended(&mut self, ctx: &mut GpuCtx<'_>, fallback: &GpuFallback) {
        if ctx.common.renderer.is_some() {
            // The reducer's teardown released it; a renderer still here is released now so that nothing outlives the path.
            self.destroy_renderer(ctx);
        }
        let decision = decide(self.request, Some(Err(fallback.clone())));
        if let Some(line) = decision.fallback_line() {
            log_warn!(LogCategory::Rendering, "{}", line);
        }
        log_info!(LogCategory::Rendering, "{}", decision.render_line());
        self.note("render_backend", &decision.timeline_detail());
        self.decision = Some(decision);
    }
}

#[cfg(test)]
#[path = "gles_gpu_test.rs"]
mod tests;
