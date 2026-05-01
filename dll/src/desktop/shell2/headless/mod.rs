//! Headless backend for testing and CPU-only rendering (`AZ_BACKEND=headless`).
//!
//! This backend implements the full `PlatformWindow` trait without
//! GPU / OpenGL. It behaves like a real platform window — DOM is laid out,
//! callbacks fire, timers tick — but rendering goes through a **CpuBackend**
//! instead of WebRender.
//!
//! ## CpuBackend
//!
//! `CpuBackend` has a similar *purpose* to the WebRender pipeline
//! (render-api, renderer, hit-tester) but is fully CPU-based and much
//! simpler. It is intentionally less efficient — the target use-case is
//! small, ancillary windows (Linux menu bars, tooltip popups) and headless
//! E2E tests, not high-framerate rendering.
//!
//! ```text
//! WebRender path:   DisplayList → WrRenderApi → Renderer (GPU) → swapBuffers
//! CpuBackend path:  DisplayList → cpurender   → Pixmap  (CPU)  → (no-op / PNG)
//! ```
//!
//! ## Headless Event Loop
//!
//! `HeadlessWindow::run()` blocks in an infinite loop just like every other
//! platform's `run()`. Instead of busy-waiting or `thread::sleep`, it
//! blocks on a **`Condvar`** that is signalled when:
//!
//! * An event is injected (via `inject_event` / debug server)
//! * A timer fires (the earliest timer deadline is used as `wait_timeout`)
//! * A background thread completes
//!
//! This means the headless loop consumes **zero CPU** when idle, just
//! like the native `WaitMessage()` / `XNextEvent()` / `NSEvent` loops
//! on real platforms.
//!
//! If nothing can wake the loop (no timers, no threads, no debug server)
//! a warning is printed to stderr and the loop blocks indefinitely
//! (the programme hangs). This is intentional — it is the same behaviour
//! you would get from a real window that nobody interacts with.
//!
//! ## Architecture
//!
//! ```text
//! HeadlessWindow
//! ├── common: CommonWindowState        (shared with all platforms)
//! ├── cpu_backend: CpuBackend          (replaces WebRender)
//! ├── event_queue: VecDeque<HeadlessEvent> (programmatic event injection)
//! └── pending_window_creates: Vec      (popup/dialog queue)
//! ```

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::cell::RefCell;
use std::time::{Duration, Instant};

use azul_core::{
    geom::LogicalPosition,
    gl::OptionGlContextPtr,
    hit_test::DocumentId,
    icon::SharedIconProvider,
    refany::RefAny,
    resources::{AppConfig, AppTerminationBehavior, IdNamespace, ImageCache, RendererResources},
    window::{
        AcceleratorKey, FullScreenMode, RawWindowHandle, ScrollResult, TouchPoint, TouchPointVec,
        VirtualKeyCode, WindowFrame,
    },
};
use azul_layout::{
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{FullWindowState, WindowCreateOptions},
};
use rust_fontconfig::FcFontCache;
use rust_fontconfig::registry::FcFontRegistry;

use crate::desktop::wr_translate2::{AsyncHitTester, WrRenderApi};
use crate::desktop::shell2::common::event::HitTestNode;

use crate::desktop::shell2::common::{
    debug_server::{self, LogCategory},
    event::{self, CommonWindowState, PlatformWindow},
    WindowError,
};
use crate::{impl_platform_window_getters, log_debug, log_error, log_info, log_trace, log_warn};

/// Events that can be injected into a HeadlessWindow for testing or
/// via the debug server.
#[derive(Debug, Clone)]
pub enum HeadlessEvent {
    /// Simulate window close
    Close,
    /// Simulate mouse move to position
    MouseMove { x: f32, y: f32 },
    /// Simulate mouse button press
    MouseDown { button: azul_core::events::MouseButton },
    /// Simulate mouse button release
    MouseUp { button: azul_core::events::MouseButton },
    /// Simulate key press
    KeyDown { virtual_keycode: VirtualKeyCode },
    /// Simulate key release
    KeyUp { virtual_keycode: VirtualKeyCode },
    /// Simulate text input
    TextInput { text: String },
    /// Simulate window resize
    Resize { width: f32, height: f32 },
    /// Simulate scroll wheel
    Scroll { delta_x: f32, delta_y: f32 },
}

// ---------------------------------------------------------------------------
// CpuBackend — replaces WebRender in headless / CPU-only windows
// ---------------------------------------------------------------------------

/// CPU-based rendering backend that replaces the WebRender pipeline.
///
/// In the GPU path every window holds a `WrRenderApi` (for submitting
/// display-lists, registering fonts/images), a `webrender::Renderer`
/// (for rasterising on the GPU) and an `AsyncHitTester` (for spatial
/// queries).  `CpuBackend` fills the same role with a much simpler,
/// fully CPU-based implementation:
///
/// | GPU path               | CpuBackend equivalent                       |
/// |------------------------|---------------------------------------------|
/// | `WrRenderApi`          | not needed – fonts/images stay in LayoutWindow |
/// | `webrender::Renderer`  | `cpurender::render()` (behind feature gate) |
/// | `AsyncHitTester`       | `CpuHitTester` (layout-based)               |
/// | `swapBuffers`          | no-op (or write PNG for screenshots)        |
///
/// The backend holds a retained-mode `CompositorState` for efficient
/// incremental re-rendering.  On resize, only the root layer pixbuf is
/// reallocated; scroll and damage use pixel-shift / partial re-render.
pub struct CpuBackend {
    /// CPU-based hit tester rebuilt after each layout pass.
    pub hit_tester: azul_layout::headless::CpuHitTester,
    /// Last rendered pixmap (if CPU rendering is enabled).
    /// `None` when rendering is disabled (layout-only mode).
    #[cfg(feature = "cpurender")]
    pub last_frame: Option<azul_layout::cpurender::AzulPixmap>,
    /// Retained compositor state with per-layer pixbufs.
    #[cfg(feature = "cpurender")]
    pub compositor: Option<azul_layout::cpurender::CompositorState>,
    /// Glyph cache — persists across frames for text rendering.
    #[cfg(feature = "cpurender")]
    pub glyph_cache: azul_layout::glyph_cache::GlyphCache,
    /// Previous display list for damage rect computation.
    #[cfg(feature = "cpurender")]
    pub previous_display_list: Option<azul_layout::solver3::display_list::DisplayList>,
}

impl Default for CpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CpuBackend {
    pub fn new() -> Self {
        Self {
            hit_tester: azul_layout::headless::CpuHitTester::new(),
            #[cfg(feature = "cpurender")]
            last_frame: None,
            #[cfg(feature = "cpurender")]
            compositor: None,
            #[cfg(feature = "cpurender")]
            glyph_cache: azul_layout::glyph_cache::GlyphCache::new(),
            #[cfg(feature = "cpurender")]
            previous_display_list: None,
        }
    }

    /// Render the current display list into `last_frame`.
    ///
    /// Uses damage-rect-based incremental rendering when possible:
    /// - Compares current display list against `previous_display_list`
    /// - If items match structurally, only repaints changed regions
    /// - On resize, uses grow-only buffer reuse for window expansion
    ///
    /// Returns the damage rects that were rendered (empty = full repaint).
    #[cfg(feature = "cpurender")]
    pub fn render_frame(
        &mut self,
        layout_window: &azul_layout::window::LayoutWindow,
        renderer_resources: &azul_core::resources::RendererResources,
        width: f32,
        height: f32,
        dpi_factor: f32,
    ) -> Vec<azul_core::geom::LogicalRect> {
        use azul_core::dom::DomId;
        use azul_layout::cpurender;

        // Get the display list from layout results
        let dom_id = DomId { inner: 0 };
        let display_list = match layout_window.layout_results.get(&dom_id) {
            Some(result) => &result.display_list,
            None => return Vec::new(),
        };

        let pixel_w = (width * dpi_factor).ceil() as u32;
        let pixel_h = (height * dpi_factor).ceil() as u32;
        if pixel_w == 0 || pixel_h == 0 {
            return Vec::new();
        }

        // Allocate or resize compositor
        let compositor = self.compositor.get_or_insert_with(|| {
            cpurender::CompositorState::new(pixel_w, pixel_h)
        });

        // Check if we need to resize the root layer
        let root = compositor.layers.get(&compositor.root_layer);
        let (old_pw, old_ph) = match root {
            Some(layer) => (layer.pixbuf.width(), layer.pixbuf.height()),
            None => (0, 0),
        };
        let needs_resize = old_pw != pixel_w || old_ph != pixel_h;

        let mut resize_damage = Vec::new();
        if needs_resize {
            let is_grow = pixel_w >= old_pw && pixel_h >= old_ph && old_pw > 0 && old_ph > 0;
            if is_grow {
                // Grow-only: resize root layer pixbuf, keep old content
                if let Some(root_layer) = compositor.layers.get_mut(&compositor.root_layer) {
                    let _ = root_layer.pixbuf.resize_grow_only(pixel_w, pixel_h, 255, 255, 255, 255);
                    root_layer.bounds.size = azul_core::geom::LogicalSize {
                        width: pixel_w as f32, height: pixel_h as f32,
                    };
                }
                resize_damage = cpurender::compute_resize_damage(
                    old_pw as f32, old_ph as f32,
                    pixel_w as f32, pixel_h as f32,
                );
            } else {
                // Shrink or first allocation: full recreate
                *compositor = cpurender::CompositorState::new(pixel_w, pixel_h);
            }
        }

        // Compute display list damage (incremental path)
        let dl_damage = match &self.previous_display_list {
            Some(old_dl) if !needs_resize => {
                cpurender::compute_display_list_damage(old_dl, display_list)
            }
            _ => None, // first frame or resize → full repaint
        };

        // Determine render path
        let all_damage: Vec<azul_core::geom::LogicalRect>;
        let is_incremental;

        match dl_damage {
            Some(rects) if rects.is_empty() && resize_damage.is_empty() => {
                // Nothing changed — skip rendering entirely
                self.previous_display_list = Some(display_list.clone());
                return Vec::new();
            }
            Some(mut rects) if !needs_resize => {
                // Incremental: only repaint changed items
                rects.extend(resize_damage);
                all_damage = rects;
                is_incremental = true;
            }
            _ => {
                // Full repaint (first frame, structural change, resize)
                all_damage = resize_damage;
                is_incremental = false;
            }
        }

        // Acquire output pixmap — reuse buffer for both grow and shrink
        let mut output = match self.last_frame.take() {
            Some(p) if p.width() == pixel_w && p.height() == pixel_h => p,
            Some(mut p) => {
                p.resize_reuse(pixel_w, pixel_h, 255, 255, 255, 255);
                p
            }
            None => match cpurender::AzulPixmap::new(pixel_w, pixel_h) {
                Some(mut p) => { p.fill(255, 255, 255, 255); p }
                None => return Vec::new(),
            },
        };

        let render_state = cpurender::CpuRenderState::new(
            cpurender::ScrollOffsetMap::new()
        )
        .with_system_style(layout_window.system_style.clone());

        if is_incremental && !all_damage.is_empty() {
            // Incremental: render only damaged regions
            let _ = cpurender::render_display_list_damaged(
                display_list, &mut output, dpi_factor,
                renderer_resources, Some(&layout_window.font_manager),
                &mut self.glyph_cache, &render_state, &all_damage,
            );
        } else {
            // Full render
            output.fill(255, 255, 255, 255);
            compositor.allocate_layers_from_display_list(display_list, dpi_factor);
            if let Err(e) = compositor.render_layers(
                display_list, dpi_factor, renderer_resources,
                Some(&layout_window.font_manager), &mut self.glyph_cache,
            ) {
                log_error!(
                    LogCategory::Rendering,
                    "[CpuBackend] render_layers error: {}",
                    e
                );
            }
            compositor.composite_frame(&mut output, dpi_factor);
        }

        self.previous_display_list = Some(display_list.clone());
        self.last_frame = Some(output);
        all_damage
    }
}

// ---------------------------------------------------------------------------
// HeadlessWindow
// ---------------------------------------------------------------------------

/// Shared wake-up state for the condvar-based event loop.
///
/// The `Condvar` is signalled whenever new work is available (event
/// injected, timer registered, thread completed).  This lets the
/// blocking loop sleep with zero CPU usage when idle.
struct WakeState {
    /// `true` when the loop should re-check for work.
    woken: bool,
}

/// Headless / CPU-only window implementing the full `PlatformWindow` trait.
///
/// Behaves identically to platform windows for layout, callbacks, and state
/// management.  Instead of a GPU context it holds a [`CpuBackend`] for
/// hit-testing and optional CPU rendering.
pub struct HeadlessWindow {
    /// Common window state (layout, resources, etc.) — shared with all platforms.
    pub common: CommonWindowState,
    /// CPU rendering backend (replaces WebRender).
    pub cpu_backend: CpuBackend,
    /// Whether the window is "open".
    is_open: bool,
    /// Event queue for programmatic event injection.
    event_queue: VecDeque<HeadlessEvent>,
    /// Thread poll timer running flag.
    thread_poll_timer_running: bool,
    /// Pending window creation requests (for popup menus, dialogs, etc.).
    pub pending_window_creates: Vec<WindowCreateOptions>,
    /// Config snapshot (needed for spawning sub-windows).
    config: AppConfig,
    /// Icon provider (shared across all windows).
    icon_provider: SharedIconProvider,
    /// Font registry (needed for spawning sub-windows).
    font_registry: Option<Arc<FcFontRegistry>>,
    /// Condvar + mutex used to block the event loop until work arrives.
    wake_condvar: Arc<Condvar>,
    wake_mutex: Arc<Mutex<WakeState>>,
}

/// Timer poll interval — how often the loop re-checks when timers are
/// active.  16 ms = 60 Hz, matches the Linux select() timeout used
/// by the X11 backend.
const TIMER_POLL_MS: u64 = 16;

impl HeadlessWindow {
    /// Create a new headless window with the given options.
    ///
    /// This constructor mirrors the real platform window constructors:
    /// 1. Creates `LayoutWindow` with font cache
    /// 2. Initialises `CommonWindowState`
    /// 3. Sets up the `CpuBackend`
    ///
    /// No system resources (window handle, GL context) are allocated.
    pub fn new(
        options: WindowCreateOptions,
        app_data: Arc<RefCell<RefAny>>,
        config: AppConfig,
        icon_provider: SharedIconProvider,
        fc_cache: Arc<FcFontCache>,
        font_registry: Option<Arc<FcFontRegistry>>,
    ) -> Result<Self, WindowError> {
        let full_window_state = options.window_state;

        // Create layout window — same as real platforms
        let mut layout_window = LayoutWindow::new(fc_cache.as_ref().clone())
            .map_err(|e| WindowError::PlatformError(format!("Layout init failed: {:?}", e)))?;
        layout_window.current_window_state = full_window_state.clone();
        layout_window.routes = config.routes.clone();

        let wake_condvar = Arc::new(Condvar::new());
        let wake_mutex = Arc::new(Mutex::new(WakeState { woken: false }));

        Ok(Self {
            common: CommonWindowState {
                layout_window: Some(layout_window),
                current_window_state: full_window_state,
                previous_window_state: None,
                image_cache: ImageCache::default(),
                renderer_resources: RendererResources::default(),
                fc_cache,
                gl_context_ptr: OptionGlContextPtr::None,
                system_style: Arc::new(crate::desktop::app::discover_system_style()),
                app_data,
                scrollbar_drag_state: None,
                hit_tester: None,
                cpu_hit_tester: Some(azul_layout::headless::CpuHitTester::new()),
                last_hovered_node: None,
                document_id: None,
                id_namespace: None,
                render_api: None,
                renderer: None,
                frame_needs_regeneration: true,
                display_list_initialized: false,
                display_list_dirty: false,
                a11y_dirty: true,
            },
            cpu_backend: CpuBackend::new(),
            is_open: true,
            event_queue: VecDeque::new(),
            thread_poll_timer_running: false,
            pending_window_creates: Vec::new(),
            config,
            icon_provider,
            font_registry,
            wake_condvar,
            wake_mutex,
        })
    }

    // === Lifecycle ===

    /// Poll the next event from the queue.
    pub fn poll_event(&mut self) -> Option<HeadlessEvent> {
        self.event_queue.pop_front()
    }

    /// Check if the window is still "open".
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Close the window.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    // === Layout ===

    /// Regenerate layout and rebuild CPU hit-tester.
    ///
    /// This is the HeadlessWindow equivalent of `MacOSWindow::regenerate_layout()` /
    /// `WinWindow::regenerate_layout()` etc. It calls the shared
    /// `common::layout::regenerate_layout()` (which no longer requires WebRender
    /// types) and then rebuilds the `CpuHitTester` from the new layout results.
    pub fn regenerate_layout(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        let layout_window = self.common.layout_window.as_mut().ok_or("No layout window")?;

        // Collect debug messages if debug server is enabled
        let debug_enabled = crate::desktop::shell2::common::debug_server::is_debug_enabled();
        let mut debug_messages = if debug_enabled {
            Some(Vec::new())
        } else {
            None
        };

        // Call unified regenerate_layout from common module
        let result = crate::desktop::shell2::common::layout::regenerate_layout(
            layout_window,
            &self.common.app_data,
            &self.common.current_window_state,
            &mut self.common.renderer_resources,
            &self.common.image_cache,
            &self.common.gl_context_ptr,
            &self.common.fc_cache,
            &self.font_registry,
            &self.common.system_style,
            &self.icon_provider,
            &mut debug_messages,
        )?;

        // Forward layout debug messages to the debug server's log queue
        if let Some(msgs) = debug_messages {
            for msg in msgs {
                crate::desktop::shell2::common::debug_server::log(
                    crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                    crate::desktop::shell2::common::debug_server::LogCategory::Layout,
                    msg.message.as_str().to_string(),
                    None,
                );
            }
        }

        // Rebuild CPU hit-tester from new layout results
        if let Some(lw) = self.common.layout_window.as_ref() {
            self.cpu_backend.hit_tester.rebuild_from_layout(&lw.layout_results);
        }

        // Drain any lifecycle events produced by reconciliation (Mount/Unmount/
        // Update/Resize) and dispatch them through the normal callback pipeline.
        // Doing this inside regenerate_layout keeps the headless test harness
        // self-contained: callers do not have to remember to pump lifecycle
        // events separately to see `.with_callback(EventFilter::Component(_))`
        // fire.
        self.dispatch_pending_lifecycle_events();

        // CPU-render the frame (retained compositor handles efficient resize)
        #[cfg(feature = "cpurender")]
        {
            let ws = &self.common.current_window_state;
            let width = ws.size.dimensions.width;
            let height = ws.size.dimensions.height;
            let dpi = ws.size.dpi as f32 / 96.0;
            if let Some(lw) = self.common.layout_window.as_ref() {
                self.cpu_backend.render_frame(
                    lw,
                    &self.common.renderer_resources,
                    width,
                    height,
                    dpi,
                );
            }
        }

        // Mark that frame needs regeneration
        self.common.frame_needs_regeneration = true;

        Ok(result)
    }

    // === Event injection (for tests / debug server) ===

    /// Inject an event into the queue for the next poll cycle.
    ///
    /// Wakes the blocking event loop if it is sleeping on the condvar.
    pub fn inject_event(&mut self, event: HeadlessEvent) {
        self.event_queue.push_back(event);
        self.wake();
    }

    /// Inject multiple events at once.
    pub fn inject_events(&mut self, events: impl IntoIterator<Item = HeadlessEvent>) {
        self.event_queue.extend(events);
        self.wake();
    }

    /// Convert a `KeyDown` virtual keycode into the locale-independent character
    /// fallback (delegating to [`VirtualKeyCode::get_lowercase`]) and, if a
    /// character is available, queue a synthetic `TextInput` event for the next
    /// poll cycle.
    ///
    /// This mirrors what platform IME paths do when no locale-specific composer
    /// is active: latin keys still produce a typed character without going
    /// through a full input-method round-trip.
    pub fn synthesize_character_input(&mut self, vk: VirtualKeyCode) -> Option<char> {
        let c = vk.get_lowercase()?;
        self.inject_event(HeadlessEvent::TextInput { text: c.to_string() });
        Some(c)
    }

    /// Replace the active touch point list. Updates `num_touches` to match.
    pub fn inject_touch_points(&mut self, points: impl IntoIterator<Item = TouchPoint>) {
        let vec: TouchPointVec = points.into_iter().collect::<Vec<_>>().into();
        let touch_state = &mut self.common.current_window_state.touch_state;
        touch_state.num_touches = vec.len();
        touch_state.touch_points = vec;
        self.wake();
    }

    /// Set the desired fullscreen-transition style on the current window state
    /// flags. The next request to enter or leave fullscreen will honor this
    /// value (slow vs. fast on macOS).
    ///
    /// On platforms that do not distinguish slow/fast transitions this is a
    /// no-op for animation purposes but still recorded on the window state for
    /// observation.
    pub fn set_fullscreen_mode(&mut self, mode: FullScreenMode) {
        let flags = &mut self.common.current_window_state.flags;
        flags.fullscreen_mode = mode;
        // Fold the request into the current frame state so headless callers
        // can observe the transition without a real OS event loop.
        flags.frame = match mode {
            FullScreenMode::SlowFullScreen | FullScreenMode::FastFullScreen => {
                WindowFrame::Fullscreen
            }
            FullScreenMode::SlowWindowed | FullScreenMode::FastWindowed => WindowFrame::Normal,
        };
    }

    /// Returns `true` if every entry of `chord` is currently active in the
    /// window's keyboard state. Use to evaluate registered accelerator
    /// shortcuts (e.g. `[Ctrl, Key(VirtualKeyCode::S)]`) on each key event.
    pub fn matches_accelerator(&self, chord: &[AcceleratorKey]) -> bool {
        self.common
            .current_window_state
            .keyboard_state
            .matches_accelerator(chord)
    }

    /// Drive a synthetic scroll delta through [`process_system_scroll`] and
    /// return the [`ScrollResult`] for assertion in tests.
    pub fn process_system_scroll(
        &mut self,
        delta: LogicalPosition,
        hit_scrollbar: bool,
    ) -> ScrollResult {
        azul_core::window::process_system_scroll(delta, hit_scrollbar)
    }

    /// Signal the condvar so the blocking loop wakes up.
    fn wake(&self) {
        if let Ok(mut guard) = self.wake_mutex.lock() {
            guard.woken = true;
            self.wake_condvar.notify_one();
        }
    }

    /// Check if any timers are currently active.
    pub fn has_active_timers(&self) -> bool {
        self.common.layout_window.as_ref()
            .map_or(false, |lw| !lw.timers.is_empty())
    }

    /// Get the number of pending window creation requests.
    pub fn pending_window_count(&self) -> usize {
        self.pending_window_creates.len()
    }

    // === Blocking event loop ===

    /// Run the headless event loop — **blocks** until the window closes.
    ///
    /// This is the HeadlessWindow equivalent of `NSApplication.run()` / the
    /// Win32 `GetMessage` loop / the X11 `XNextEvent` loop.
    ///
    /// The loop uses a `Condvar` for zero-CPU blocking:
    /// * When timers are active it uses `wait_timeout` (16 ms / 60 Hz)
    ///   so timers get ticked even without external events.
    /// * When no timers are active it calls `wait` (indefinite) — the
    ///   thread is parked until `inject_event()`, `start_timer()`, or
    ///   another caller invokes `wake()`.
    /// * If nothing can ever wake the loop (no timers, no threads, no
    ///   debug server) a one-time warning is printed to stderr and the
    ///   loop blocks forever — identical to a desktop window nobody
    ///   interacts with.
    pub fn run(mut self) -> Result<(), WindowError> {
        let debug_enabled = debug_server::is_debug_enabled();
        let start = Instant::now();

        log_info!(
            LogCategory::EventLoop,
            "[Headless] Entering condvar-based blocking event loop (debug={})",
            debug_enabled,
        );

        // -- Perform initial layout (same as every platform) --
        log_debug!(
            LogCategory::Layout,
            "[Headless] Performing initial layout"
        );
        if let Err(e) = self.regenerate_layout() {
            log_warn!(
                LogCategory::Layout,
                "[Headless] WARNING: Initial layout failed: {}",
                e
            );
        }

        // -- child windows (sub-HeadlessWindows for menus, dialogs) --
        let mut children: Vec<HeadlessWindow> = Vec::new();
        let mut warned_no_wake_sources = false;

        while self.is_open() {
            // ── Phase 1: Process injected events ─────────────────
            let mut events_need_redraw = false;
            while let Some(event) = self.poll_event() {
                match event {
                    HeadlessEvent::Close => {
                        self.close();
                    }
                    HeadlessEvent::MouseMove { x, y } => {
                        use azul_core::window::CursorPosition;
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        let pos = LogicalPosition { x, y };
                        self.common.current_window_state.mouse_state.cursor_position =
                            CursorPosition::InWindow(pos);
                        self.update_hit_test_at(pos);
                        let r = self.process_window_events(0);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::MouseDown { button } => {
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        match button {
                            azul_core::events::MouseButton::Left => {
                                self.common.current_window_state.mouse_state.left_down = true;
                            }
                            azul_core::events::MouseButton::Right => {
                                self.common.current_window_state.mouse_state.right_down = true;
                            }
                            azul_core::events::MouseButton::Middle => {
                                self.common.current_window_state.mouse_state.middle_down = true;
                            }
                            _ => {}
                        }
                        let r = self.process_window_events(0);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::MouseUp { button } => {
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        match button {
                            azul_core::events::MouseButton::Left => {
                                self.common.current_window_state.mouse_state.left_down = false;
                            }
                            azul_core::events::MouseButton::Right => {
                                self.common.current_window_state.mouse_state.right_down = false;
                            }
                            azul_core::events::MouseButton::Middle => {
                                self.common.current_window_state.mouse_state.middle_down = false;
                            }
                            _ => {}
                        }
                        let r = self.process_window_events(0);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::KeyDown { virtual_keycode } => {
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        self.common.current_window_state.keyboard_state.current_virtual_keycode =
                            azul_core::window::OptionVirtualKeyCode::Some(virtual_keycode);
                        self.common.current_window_state.keyboard_state
                            .pressed_virtual_keycodes.insert_hm_item(virtual_keycode);
                        let r = self.process_window_events(0);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::KeyUp { virtual_keycode } => {
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        self.common.current_window_state.keyboard_state.current_virtual_keycode =
                            azul_core::window::OptionVirtualKeyCode::None;
                        self.common.current_window_state.keyboard_state
                            .pressed_virtual_keycodes.remove_hm_item(&virtual_keycode);
                        let r = self.process_window_events(0);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                    HeadlessEvent::TextInput { .. } => {
                        // Text input requires IME / text composition pipeline;
                        // state-diff picks up keyboard events automatically.
                    }
                    HeadlessEvent::Resize { width, height } => {
                        self.common.previous_window_state =
                            Some(self.common.current_window_state.clone());
                        self.common.current_window_state.size.dimensions.width = width;
                        self.common.current_window_state.size.dimensions.height = height;
                        events_need_redraw = true;
                    }
                    HeadlessEvent::Scroll { .. } => {
                        // Scroll requires the physics-based scroll momentum
                        // timer system (ScrollInputQueue + ScrollPhysicsState).
                        // Not yet wired for headless.
                    }
                }
            }
            if events_need_redraw {
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Headless] Layout regeneration after event failed: {}",
                        e
                    );
                }
            }

            // ── Phase 2: Tick timers and threads ─────────────────
            // Use the shared PlatformWindow trait method to invoke
            // expired timer callbacks and poll background threads.
            let needs_redraw = self.process_timers_and_threads();

            // In the CPU-only path there is no GPU compositor that can
            // handle scroll-offset-only or repaint-only updates.  Every
            // visual change (including scroll) requires a full display
            // list rebuild, so we regenerate layout on any redraw signal.
            if needs_redraw {
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Headless] Layout regeneration failed: {}",
                        e
                    );
                }
            }

            // ── Phase 3: Spawn sub-HeadlessWindows for pending creates ─
            while let Some(pending_create) = self.pending_window_creates.pop() {
                log_debug!(
                    LogCategory::Window,
                    "[Headless] Spawning sub-HeadlessWindow (type: {:?})",
                    pending_create.window_state.flags.window_type
                );
                match HeadlessWindow::new(
                    pending_create,
                    self.common.app_data.clone(),
                    self.config.clone(),
                    self.icon_provider.clone(),
                    self.common.fc_cache.clone(),
                    self.font_registry.clone(),
                ) {
                    Ok(child) => children.push(child),
                    Err(e) => {
                        log_error!(
                            LogCategory::Window,
                            "[Headless] Failed to create sub-HeadlessWindow: {:?}",
                            e
                        );
                    }
                }
            }

            // ── Phase 4: Pump child windows ──────────────────────
            children.retain_mut(|child| {
                while let Some(ev) = child.poll_event() {
                    if let HeadlessEvent::Close = ev { child.close(); }
                }
                child.pending_window_creates.clear();
                child.is_open()
            });

            // ── Phase 5: Condvar-based wait ──────────────────────
            let has_timers = self.common.layout_window.as_ref()
                .map_or(false, |lw| !lw.timers.is_empty());
            let has_wake_sources = has_timers
                || self.thread_poll_timer_running
                || debug_enabled
                || !children.is_empty();

            if !has_wake_sources && !warned_no_wake_sources {
                warned_no_wake_sources = true;
                eprintln!(
                    "[azul] HeadlessWindow: no timers, threads, or debug server active. \
                     The event loop will block indefinitely on a condvar \
                     (same as a desktop window nobody interacts with). \
                     Set AZ_DEBUG=1 to enable the debug server, or \
                     inject events via inject_event()."
                );
            }

            // Lock, clear `woken`, then wait.
            let mut guard = self.wake_mutex.lock().unwrap();
            guard.woken = false;

            if has_timers {
                // Timers active → poll at 60 Hz
                let _r = self.wake_condvar.wait_timeout_while(
                    guard,
                    Duration::from_millis(TIMER_POLL_MS),
                    |ws| !ws.woken,
                );
            } else {
                // No timers → block indefinitely until woken
                let _r = self.wake_condvar.wait_while(
                    guard,
                    |ws| !ws.woken,
                );
            }
        }

        log_info!(
            LogCategory::EventLoop,
            "[Headless] Event loop finished (elapsed: {:.1}s)",
            start.elapsed().as_secs_f64()
        );

        // Handle termination behaviour (same as every platform run())
        match self.config.termination_behavior {
            AppTerminationBehavior::EndProcess => {
                std::process::exit(0);
            }
            AppTerminationBehavior::ReturnToMain => { /* return normally */ }
            AppTerminationBehavior::RunForever => { /* all windows closed */ }
        }

        Ok(())
    }
}

// === PlatformWindow Trait Implementation ===

impl PlatformWindow for HeadlessWindow {
    // 28 getter/setter methods generated by macro — identical to all other platforms
    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Unsupported
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows<'_> {
        let layout_window = self
            .common
            .layout_window
            .as_mut()
            .expect("Layout window must exist for callback invocation");

        event::InvokeSingleCallbackBorrows {
            layout_window,
            window_handle: RawWindowHandle::Unsupported,
            gl_context_ptr: &self.common.gl_context_ptr,
            image_cache: &mut self.common.image_cache,
            fc_cache_clone: (*self.common.fc_cache).clone(),
            system_style: self.common.system_style.clone(),
            previous_window_state: &self.common.previous_window_state,
            current_window_state: &self.common.current_window_state,
            renderer_resources: &mut self.common.renderer_resources,
        }
    }

    // Timer Management — condvar wakes the loop when timers change

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }
        self.wake(); // transition condvar from indefinite to timed wait
    }

    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
    }

    fn start_thread_poll_timer(&mut self) {
        self.thread_poll_timer_running = true;
    }

    fn stop_thread_poll_timer(&mut self) {
        self.thread_poll_timer_running = false;
    }

    fn add_threads(
        &mut self,
        threads: BTreeMap<azul_core::task::ThreadId, azul_layout::thread::Thread>,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for (id, thread) in threads {
                lw.threads.insert(id, thread);
            }
        }
        if !self.thread_poll_timer_running {
            self.start_thread_poll_timer();
        }
    }

    fn remove_threads(
        &mut self,
        thread_ids: &BTreeSet<azul_core::task::ThreadId>,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for id in thread_ids {
                lw.threads.remove(id);
            }
            if lw.threads.is_empty() {
                self.stop_thread_poll_timer();
            }
        }
    }

    fn queue_window_create(&mut self, options: WindowCreateOptions) {
        self.pending_window_creates.push(options);
    }

    fn show_menu_from_callback(
        &mut self,
        _menu: &azul_core::menu::Menu,
        _position: LogicalPosition,
    ) {
        // TODO: could create a sub-HeadlessWindow with the menu content
    }

    fn show_tooltip_from_callback(
        &mut self,
        _text: &str,
        _position: LogicalPosition,
    ) {
        // No-op — no visual surface to show a tooltip on
    }

    fn hide_tooltip_from_callback(&mut self) {
        // No-op
    }

    fn sync_window_state(&mut self) {
        // No native window to synchronise
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stub() -> HeadlessWindow {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        let fc_cache = Arc::new(FcFontCache::default());
        let app_data = Arc::new(RefCell::new(RefAny::new(())));
        let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
        HeadlessWindow::new(
            WindowCreateOptions::default(),
            app_data,
            AppConfig::default(),
            icon_provider,
            fc_cache,
            None,
        ).unwrap()
    }

    #[test]
    fn test_stub_window_creation() {
        let window = make_stub();
        assert!(window.is_open());
    }

    #[test]
    fn test_stub_window_close() {
        let mut window = make_stub();
        window.close();
        assert!(!window.is_open());
    }

    #[test]
    fn test_stub_event_injection() {
        let mut window = make_stub();

        assert!(window.poll_event().is_none());

        window.inject_event(HeadlessEvent::MouseMove { x: 100.0, y: 200.0 });
        window.inject_event(HeadlessEvent::Close);

        assert!(matches!(window.poll_event(), Some(HeadlessEvent::MouseMove { .. })));
        assert!(matches!(window.poll_event(), Some(HeadlessEvent::Close)));
        assert!(window.poll_event().is_none());
    }

    #[test]
    fn test_stub_timer_management() {
        let mut window = make_stub();
        assert!(!window.has_active_timers());

        let get_time = azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        };
        let timer = azul_layout::timer::Timer::create(
            RefAny::new(()),
            test_timer_callback as azul_layout::timer::TimerCallbackType,
            get_time,
        );
        window.start_timer(1, timer);
        assert!(window.has_active_timers());

        window.stop_timer(1);
        assert!(!window.has_active_timers());
    }

    #[test]
    fn test_stub_window_create_queue() {
        let mut window = make_stub();
        assert_eq!(window.pending_window_count(), 0);

        window.queue_window_create(WindowCreateOptions::default());
        assert_eq!(window.pending_window_count(), 1);
    }

    #[test]
    fn test_cpu_backend_creation() {
        let backend = CpuBackend::new();
        let results = backend.hit_tester.hit_test(
            azul_core::geom::LogicalPosition { x: 0.0, y: 0.0 },
        );
        assert!(results.is_empty());
    }

    extern "C" fn test_timer_callback(
        _data: RefAny,
        _info: azul_layout::timer::TimerCallbackInfo,
    ) -> azul_core::callbacks::TimerCallbackReturn {
        azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
    }
}
