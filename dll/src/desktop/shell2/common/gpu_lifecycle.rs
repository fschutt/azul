//! The surface and context lifecycle of a phone's GPU path, as a pure reducer.
//!
//! Phone GL breaks at the lifecycle, not at the drawing: the surface goes away under a live context
//! (Android's `TerminateWindow`), the context is lost, the app is pushed into the background where a single GL
//! command ends the process (iOS: "an OpenGL ES app is terminated if it attempts to execute OpenGL ES commands
//! on the graphics hardware" in the background), the screen turns, the density changes, the system asks for
//! memory back. This module is the ONE place that decides what happens to the context, the surface and the
//! renderer on each of those events, for both phone shells, and it decides without touching GL: the shell gives
//! it [`Event`]s, it answers with [`Action`]s, and the shell's executor (EGL on Android, EAGL on iOS) performs
//! them. The same reducer therefore runs on the host, against a fake platform that enforces the platforms'
//! rules (`gpu_lifecycle_test.rs`).
//!
//! # The contract between the machine and its executor
//!
//! - The executor performs the actions of one answer in order and stops at the first that fails; it reports the
//!   failure as [`Event::ActionFailed`] (the stage and the platform's cause). The machine had assumed the
//!   actions would work; it corrects what it believes exists and answers with the teardown and
//!   [`Action::FallBackToCpu`].
//! - The teardown actions ([`Action::StopFrames`], [`Action::DestroySurface`], [`Action::DestroyRenderer`],
//!   [`Action::DestroyContext`], [`Action::AbandonGl`]) never fail the machine: the executor logs a failure and
//!   goes on, and each is a no-op when there is nothing to destroy. They make the context current themselves
//!   (a renderer is `deinit`ed on a current context, on Android surfaceless), which is why no answer that
//!   destroys has a `MakeCurrent` in it: an answer that destroys cannot fail half way. An answer that ends the
//!   machine ([`Action::FallBackToCpu`], [`Event::Shutdown`]) is performed to its end whatever fails in it.
//! - Every other action makes its own preconditions true too (a `CreateSurface` on iOS makes the context current
//!   first); [`Action::MakeCurrent`] in a sequence is the "this thread draws now" whose failure is a stage of
//!   its own (`make_current`), and it is a no-op when the context already is current.
//! - After [`Action::FallBackToCpu`] the machine owns nothing and answers every event with nothing.
//! - [`Action::FallBackToCpu`] always comes after [`Action::DestroySurface`]: Android's `ANativeWindow` has one
//!   producer, a window the GPU path connected cannot be locked by the CPU path until `eglDestroySurface` has
//!   disconnected it, and the other way round (so the CPU path must never draw on a window before the GPU
//!   attempt has ended).
//!
//! # iOS and Android
//!
//! iOS sends [`Event::Paused`] (`applicationWillResignActive:`), [`Event::Backgrounded`]
//! (`applicationDidEnterBackground:`) and [`Event::Resumed`] (`applicationDidBecomeActive:`); its surface lives
//! as long as the view. Android sends [`Event::SurfaceCreated`] and [`Event::SurfaceDestroyed`] (`InitWindow`,
//! `TerminateWindow`) and keeps its context through a missing surface. Both send [`Event::Resized`] (rotation,
//! a new density), [`Event::TrimMemory`], [`Event::ContextLost`], [`Event::Frame`] (a frame is wanted),
//! [`Event::FramePresented`] (the frame went out) and [`Event::Shutdown`].

use super::gpu_select::{GpuFallback, GpuStage};

/// The longest edge of a surface the machine passes on; a larger one is cut to it (the platform then fails the
/// surface at `GL_MAX_RENDERBUFFER_SIZE` with its own cause).
pub const SURFACE_EDGE_MAX: u32 = 16384;

/// The size of the drawable, in device pixels, and the scale of the screen it is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Surface {
    /// Width in device pixels.
    pub width: u32,
    /// Height in device pixels.
    pub height: u32,
    /// Device pixels per point (iOS `contentScaleFactor`) or per dp (Android density / 160).
    pub scale: f32,
}

impl Surface {
    /// A surface of this size.
    #[must_use]
    pub const fn new(width: u32, height: u32, scale: f32) -> Self {
        Self {
            width,
            height,
            scale,
        }
    }

    /// A surface with no area (the view is not laid out yet) is waited for, not made.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The edges cut to [`SURFACE_EDGE_MAX`] and a scale that is no number, or not positive, made 1.
    fn clamped(self) -> Self {
        let scale = if self.scale.is_finite() && self.scale > 0.0 {
            self.scale.min(16.0)
        } else {
            1.0
        };
        Self {
            width: self.width.min(SURFACE_EDGE_MAX),
            height: self.height.min(SURFACE_EDGE_MAX),
            scale,
        }
    }
}

/// How much the system wants back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrimLevel {
    /// Drop the renderer's caches, keep the context (iOS memory warning; Android `RUNNING_LOW`).
    Light,
    /// Drop the renderer and the context too, when no surface needs them (Android `UI_HIDDEN` and above, the
    /// app in the background). With a surface on screen it is [`TrimLevel::Light`].
    Heavy,
}

/// What happened to the window or the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// There is a drawable (iOS: the view has its layer and a size; Android: `InitWindow`). A second one of the
    /// same size is the same surface; of another size it is a resize.
    SurfaceCreated(Surface),
    /// The drawable is going away (Android `TerminateWindow`): the surface goes before the native window does.
    SurfaceDestroyed,
    /// The drawable has another size or scale (a rotation, `layoutSubviews`, a density change).
    Resized(Surface),
    /// The app is going inactive (iOS `applicationWillResignActive:`): stop the frames, finish.
    Paused,
    /// The app is in the background (iOS `applicationDidEnterBackground:`): the last GL command the platform
    /// allows until [`Event::Resumed`] belongs to this answer.
    Backgrounded,
    /// The app is active again (iOS `applicationDidBecomeActive:`).
    Resumed,
    /// The context is gone.
    ContextLost,
    /// The system wants memory back.
    TrimMemory(TrimLevel),
    /// A frame is wanted: the answer is [`Action::Present`], or nothing when no frame can be drawn.
    Frame,
    /// A frame went out: the context is good, past losses are forgiven.
    FramePresented,
    /// The window is closing.
    Shutdown,
    /// The executor could not perform an action.
    ActionFailed(GpuFallback),
}

/// What the platform is to do. Performed in order; see the module's contract.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Make the GL context (and the display or config it needs).
    CreateContext,
    /// Make the context current on the drawing thread (with the surface, or surfaceless).
    MakeCurrent,
    /// Make the drawable: EGL window surface, or the `CAEAGLLayer`'s renderbuffer and framebuffer.
    CreateSurface(Surface),
    /// The drawable has another size: reallocate its storage (the renderer's frame size follows the window).
    ResizeSurface(Surface),
    /// Destroy the drawable, before anything lets the native window go. A no-op without one.
    DestroySurface,
    /// Build WebRender on the current context: the renderer, its document, the engine's shaders.
    BuildRenderer,
    /// `Renderer::deinit` on the current context. A no-op without a renderer.
    DestroyRenderer,
    /// Destroy the context. A no-op without one. Never terminates a display the process shares.
    DestroyContext,
    /// Release the renderer's caches (texture cache, glyph and image budgets) on the current context.
    ReleaseCaches,
    /// Stop the platform's frame clock (iOS `CADisplayLink` paused, Android's Choreographer callback dropped).
    StopFrames,
    /// Start it again.
    StartFrames,
    /// `glFinish`: every command the app has submitted has run.
    Finish,
    /// Render one frame and present it.
    Present,
    /// Drop every GL object the machine knew of (the renderer, the context, a surface that is left) WITHOUT any
    /// GL command or context call: the context is lost, or the platform forbids GL (iOS background). What must
    /// not be released is leaked.
    AbandonGl,
    /// The GPU path ends: draw with the CPU renderer from now on. The last action of its answer.
    FallBackToCpu(GpuFallback),
}

/// Whether the app is allowed to use GL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Suspend {
    /// Active.
    Running,
    /// Inactive (iOS `WillResignActive`): the frames are stopped, nothing is drawn; teardown is still allowed.
    Inactive,
    /// In the background: NO GL command, and no call that flushes one, until [`Event::Resumed`].
    Background,
}

/// Where the machine stands, for the shell's logs and for the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// No context yet, or none left.
    NoContext,
    /// A context, but not everything a frame needs (no surface, or no renderer).
    Incomplete,
    /// Context, surface and renderer: frames can be drawn.
    Ready,
    /// The app is inactive or in the background; whatever exists is kept.
    Suspended,
    /// The GPU path ended; the CPU renderer draws.
    Fallen,
    /// Shut down.
    Closed,
}

#[derive(Debug, Clone, PartialEq)]
enum End {
    Live,
    Fallen(GpuFallback),
    Closed,
}

/// What the answer to the last event was making, so that a failure of one of its actions can take back exactly
/// what the answer had assumed and the platform did not do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Made {
    context: bool,
    surface: bool,
    renderer: bool,
}

impl Made {
    const NOTHING: Self = Self {
        context: false,
        surface: false,
        renderer: false,
    };
}

/// The lifecycle machine of ONE window's GPU path. Owned by the window that follows it.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuLifecycle {
    /// A context exists (as far as the machine knows: it believes its actions worked).
    context: bool,
    /// A WebRender renderer exists on it.
    renderer: bool,
    /// The drawable that exists, with the size it was made at.
    surface: Option<Surface>,
    /// The drawable the platform last asked for (made when the app may use GL).
    wanted: Option<Surface>,
    suspend: Suspend,
    /// The platform's frame clock runs.
    frames: bool,
    /// Context losses since the last frame that went out.
    losses: u8,
    /// A memory request that came while GL was not allowed; answered at the next [`Event::Resumed`].
    trim_owed: Option<TrimLevel>,
    /// A loss that was reported while GL was not allowed; answered at the next [`Event::Resumed`].
    rebuild_owed: bool,
    /// What the answer to the last event (other than a failure report) was making.
    made: Made,
    end: End,
}

impl Default for GpuLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuLifecycle {
    /// A window that has asked for the GPU and has nothing yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            context: false,
            renderer: false,
            surface: None,
            wanted: None,
            suspend: Suspend::Running,
            frames: false,
            losses: 0,
            trim_owed: None,
            rebuild_owed: false,
            made: Made::NOTHING,
            end: End::Live,
        }
    }

    /// Where the machine stands.
    #[must_use]
    pub fn phase(&self) -> Phase {
        match self.end {
            End::Closed => Phase::Closed,
            End::Fallen(_) => Phase::Fallen,
            End::Live if self.suspend != Suspend::Running => Phase::Suspended,
            End::Live if !self.context => Phase::NoContext,
            End::Live if self.surface.is_none() || !self.renderer => Phase::Incomplete,
            End::Live => Phase::Ready,
        }
    }

    /// May the platform issue a GL command now? `false` in the background (a command there ends the
    /// process), which every GL-touching path of the shell asks before it acts (a deletion on drop, a trim).
    #[must_use]
    pub fn gl_allowed(&self) -> bool {
        self.suspend != Suspend::Background
    }

    /// The app's state as far as GL goes.
    #[must_use]
    pub fn suspend(&self) -> Suspend {
        self.suspend
    }

    /// The drawable that exists.
    #[must_use]
    pub fn surface(&self) -> Option<Surface> {
        self.surface
    }

    /// A context exists.
    #[must_use]
    pub fn has_context(&self) -> bool {
        self.context
    }

    /// A renderer exists.
    #[must_use]
    pub fn has_renderer(&self) -> bool {
        self.renderer
    }

    /// The platform's frame clock runs (as far as the machine told it to).
    #[must_use]
    pub fn frames_running(&self) -> bool {
        self.frames
    }

    /// The context losses since the last frame that went out.
    #[must_use]
    pub fn losses(&self) -> u8 {
        self.losses
    }

    /// Why the GPU path ended, once it did.
    #[must_use]
    pub fn fallback(&self) -> Option<&GpuFallback> {
        match &self.end {
            End::Fallen(fallback) => Some(fallback),
            _ => None,
        }
    }

    /// One event in, the actions it asks of the platform out.
    pub fn step(&mut self, event: Event) -> Vec<Action> {
        if !matches!(self.end, End::Live) {
            return Vec::new();
        }
        if !matches!(event, Event::ActionFailed(_)) {
            self.made = Made::NOTHING;
        }
        match event {
            Event::SurfaceCreated(surface) => self.surface_created(surface.clamped()),
            Event::SurfaceDestroyed => self.surface_destroyed(),
            Event::Resized(surface) => self.resized(surface.clamped()),
            Event::Paused => self.paused(),
            Event::Backgrounded => self.backgrounded(),
            Event::Resumed => self.resumed(),
            Event::ContextLost => self.context_lost(),
            Event::TrimMemory(level) => self.trim(level),
            Event::Frame => self.frame(),
            Event::FramePresented => {
                self.losses = 0;
                Vec::new()
            }
            Event::Shutdown => self.shutdown(),
            Event::ActionFailed(fallback) => self.action_failed(fallback),
        }
    }

    // ----- the surface -------------------------------------------------------------------------

    fn surface_created(&mut self, surface: Surface) -> Vec<Action> {
        self.wanted = Some(surface);
        if self.suspend != Suspend::Running {
            // made at Resumed, with whatever size the platform says by then
            return Vec::new();
        }
        match self.surface {
            Some(current) if current == surface => Vec::new(),
            Some(_) => self.resize_live(surface),
            None => self.bring_up(),
        }
    }

    fn surface_destroyed(&mut self) -> Vec<Action> {
        self.wanted = None;
        if self.suspend == Suspend::Background && self.surface.is_some() {
            // not an event of iOS, but if the drawable goes in the background the answer may not touch GL:
            // everything is dropped without a command and made again with the next surface
            let mut actions = vec![Action::AbandonGl];
            if self.frames {
                actions.insert(0, Action::StopFrames);
            }
            self.forget_everything();
            return actions;
        }
        let mut actions = Vec::new();
        if self.frames {
            actions.push(Action::StopFrames);
            self.frames = false;
        }
        if self.surface.take().is_some() {
            // the surface goes BEFORE the native window does; the context and the renderer stay
            actions.push(Action::DestroySurface);
        }
        actions
    }

    fn resized(&mut self, surface: Surface) -> Vec<Action> {
        let asked_for_a_surface = self.wanted.is_some();
        if surface.is_empty() && self.surface.is_some() {
            // a view that collapsed for a moment keeps the drawable it has
            return Vec::new();
        }
        if asked_for_a_surface {
            self.wanted = Some(surface);
        }
        if self.suspend != Suspend::Running {
            return Vec::new();
        }
        match self.surface {
            Some(current) if current == surface => Vec::new(),
            Some(_) => self.resize_live(surface),
            None if asked_for_a_surface => self.bring_up(),
            None => Vec::new(),
        }
    }

    fn resize_live(&mut self, surface: Surface) -> Vec<Action> {
        self.surface = Some(surface);
        vec![Action::MakeCurrent, Action::ResizeSurface(surface)]
    }

    /// What takes what exists to a context with a surface and a renderer, in an order every platform can follow.
    /// Nothing happens without a surface of some area to draw on (the view is not laid out yet).
    fn bring_up(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(wanted) = self.wanted.filter(|surface| !surface.is_empty()) else {
            return actions;
        };
        if !self.context {
            actions.push(Action::CreateContext);
            self.context = true;
            self.made.context = true;
        }
        if self.surface.is_none() {
            actions.push(Action::CreateSurface(wanted));
            self.surface = Some(wanted);
            self.made.surface = true;
        }
        if !self.renderer {
            actions.push(Action::MakeCurrent);
            actions.push(Action::BuildRenderer);
            self.renderer = true;
            self.made.renderer = true;
        } else if !actions.is_empty() {
            actions.push(Action::MakeCurrent);
        }
        if !actions.is_empty() && !self.frames {
            actions.push(Action::StartFrames);
            self.frames = true;
        }
        actions
    }

    // ----- the app's state ---------------------------------------------------------------------

    fn paused(&mut self) -> Vec<Action> {
        if self.suspend != Suspend::Running {
            return Vec::new();
        }
        self.suspend = Suspend::Inactive;
        let mut actions = Vec::new();
        if self.frames {
            actions.push(Action::StopFrames);
            self.frames = false;
        }
        if self.context {
            // "place itself into a known good state, and then call glFinish"
            actions.push(Action::MakeCurrent);
            actions.push(Action::Finish);
        }
        actions
    }

    fn backgrounded(&mut self) -> Vec<Action> {
        if self.suspend == Suspend::Background {
            return Vec::new();
        }
        self.suspend = Suspend::Background;
        let mut actions = Vec::new();
        if self.frames {
            actions.push(Action::StopFrames);
            self.frames = false;
        }
        if self.context {
            // the last GL the platform allows until the app is back: what was owed to the system first,
            // then everything submitted is made to finish. The framebuffer stays: the layer keeps the last
            // frame for the app switcher, and the next foreground needs no new storage.
            actions.push(Action::MakeCurrent);
            if self.trim_owed.take().is_some() && self.renderer {
                actions.push(Action::ReleaseCaches);
            }
            actions.push(Action::Finish);
        }
        actions
    }

    fn resumed(&mut self) -> Vec<Action> {
        if self.suspend == Suspend::Running {
            return Vec::new();
        }
        self.suspend = Suspend::Running;
        let mut actions = if self.rebuild_owed {
            self.rebuild_owed = false;
            self.rebuild()
        } else {
            self.bring_up()
        };
        if !(self.context && self.renderer && self.surface.is_some()) {
            // nothing to draw with yet (no surface was asked for): the frames start with the surface
            return actions;
        }
        if actions.is_empty() {
            actions.push(Action::MakeCurrent);
        }
        // a rotation, or a new scale, that came while the app was away
        if let (Some(current), Some(wanted)) = (self.surface, self.wanted) {
            if current != wanted && !wanted.is_empty() {
                actions.push(Action::ResizeSurface(wanted));
                self.surface = Some(wanted);
            }
        }
        if self.trim_owed.take().is_some() {
            actions.push(Action::ReleaseCaches);
        }
        if !self.frames {
            actions.push(Action::StartFrames);
            self.frames = true;
        }
        // the first frame after the app is back is a full one
        actions.push(Action::Present);
        actions
    }

    // ----- loss and memory ---------------------------------------------------------------------

    fn context_lost(&mut self) -> Vec<Action> {
        if !self.context {
            return Vec::new();
        }
        if self.suspend != Suspend::Running {
            // no GL until the app is back: the rebuild is owed
            self.rebuild_owed = true;
            return Vec::new();
        }
        self.losses = self.losses.saturating_add(1);
        if self.losses >= 2 {
            return self.fall_back(GpuFallback::new(
                GpuStage::Lost,
                "the context was lost twice without a frame in between",
            ));
        }
        self.rebuild()
    }

    /// A new context under the same window: the old one's objects are gone with it.
    fn rebuild(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.frames {
            actions.push(Action::StopFrames);
            self.frames = false;
        }
        if self.surface.take().is_some() {
            actions.push(Action::DestroySurface);
        }
        if self.context || self.renderer {
            actions.push(Action::AbandonGl);
        }
        self.context = false;
        self.renderer = false;
        actions.extend(self.bring_up());
        actions
    }

    fn trim(&mut self, level: TrimLevel) -> Vec<Action> {
        if !self.context {
            return Vec::new();
        }
        if self.suspend != Suspend::Running {
            self.trim_owed = Some(self.trim_owed.map_or(level, |owed| owed.max(level)));
            return Vec::new();
        }
        if level == TrimLevel::Heavy && self.surface.is_none() {
            // nothing on screen needs the renderer or the context: both go, and come back with a surface
            let mut actions = Vec::new();
            if self.renderer {
                actions.push(Action::DestroyRenderer);
            }
            actions.push(Action::DestroyContext);
            self.renderer = false;
            self.context = false;
            return actions;
        }
        if self.renderer {
            vec![Action::MakeCurrent, Action::ReleaseCaches]
        } else {
            Vec::new()
        }
    }

    // ----- frames and the end ------------------------------------------------------------------

    fn frame(&self) -> Vec<Action> {
        if self.suspend == Suspend::Running
            && self.context
            && self.renderer
            && self.surface.is_some()
        {
            vec![Action::Present]
        } else {
            Vec::new()
        }
    }

    fn shutdown(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.frames {
            actions.push(Action::StopFrames);
        }
        if self.suspend == Suspend::Background {
            // no GL command and no flush in the background, and the process is going: leak, do not release
            if self.context || self.renderer || self.surface.is_some() {
                actions.push(Action::AbandonGl);
            }
        } else {
            if self.surface.is_some() {
                actions.push(Action::DestroySurface);
            }
            if self.context {
                if self.renderer {
                    actions.push(Action::DestroyRenderer);
                }
                actions.push(Action::DestroyContext);
            }
        }
        self.forget_everything();
        self.end = End::Closed;
        actions
    }

    fn action_failed(&mut self, fallback: GpuFallback) -> Vec<Action> {
        match fallback.stage {
            // a frame that could not go out and a lost context are the same trouble: rebuild once
            GpuStage::Frame | GpuStage::Lost => self.context_lost(),
            stage => {
                // What the platform could not make does not exist, and nor does what came after it in the
                // answer (the order of `bring_up`: context, surface, current, renderer): of those, what THIS
                // answer was making is taken back. A resize that failed leaves the surface in the books: its
                // teardown removes the half of it that exists.
                let made = self.made;
                if matches!(stage, GpuStage::Library | GpuStage::Context) && made.context {
                    self.context = false;
                }
                if matches!(
                    stage,
                    GpuStage::Library | GpuStage::Context | GpuStage::Surface
                ) && made.surface
                {
                    self.surface = None;
                }
                if made.renderer {
                    self.renderer = false;
                }
                self.fall_back(fallback)
            }
        }
    }

    /// Everything that exists comes down, in the order the platforms need, and the CPU renderer takes over.
    fn fall_back(&mut self, fallback: GpuFallback) -> Vec<Action> {
        // whether the frame clock really runs depends on how far the answer that failed had come: stopping
        // a stopped clock is a no-op
        let mut actions = vec![Action::StopFrames];
        if self.suspend == Suspend::Background {
            // a failure of the last GL of the background handler: no command may follow it, the objects are
            // leaked (the CPU renderer takes the view when the app is back)
            if self.surface.is_some() || self.context {
                actions.push(Action::AbandonGl);
            }
        } else {
            if self.surface.is_some() {
                // before the CPU path may lock the window
                actions.push(Action::DestroySurface);
            }
            if self.context {
                if self.renderer {
                    actions.push(Action::DestroyRenderer);
                }
                actions.push(Action::DestroyContext);
            }
        }
        actions.push(Action::FallBackToCpu(fallback.clone()));
        self.forget_everything();
        self.end = End::Fallen(fallback);
        actions
    }

    fn forget_everything(&mut self) {
        self.context = false;
        self.renderer = false;
        self.surface = None;
        self.wanted = None;
        self.frames = false;
        self.trim_owed = None;
        self.rebuild_owed = false;
    }
}

#[cfg(test)]
#[path = "gpu_lifecycle_test.rs"]
mod gpu_lifecycle_test;
