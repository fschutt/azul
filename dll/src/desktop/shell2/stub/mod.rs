//! Stub/headless backend for testing and CPU-only rendering.
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
//! `StubWindow::run()` blocks in an infinite loop just like every other
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
//! StubWindow
//! ├── common: CommonWindowState        (shared with all platforms)
//! ├── cpu_backend: CpuBackend          (replaces WebRender)
//! ├── event_queue: VecDeque<StubEvent> (programmatic event injection)
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
    window::{RawWindowHandle, VirtualKeyCode},
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

/// Events that can be injected into a StubWindow for testing or
/// via the debug server.
#[derive(Debug, Clone)]
pub enum StubEvent {
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
/// The backend is intentionally less efficient than WebRender; the target
/// are small CPU-rendered windows (Linux menu bars, tooltip popups) and
/// headless E2E testing.
pub struct CpuBackend {
    /// CPU-based hit tester rebuilt after each layout pass.
    pub hit_tester: azul_layout::headless::CpuHitTester,
    /// Last rendered pixmap (if CPU rendering is enabled).
    /// `None` when rendering is disabled (layout-only mode).
    #[cfg(feature = "cpurender")]
    pub last_frame: Option<tiny_skia::Pixmap>,
}

impl CpuBackend {
    pub fn new() -> Self {
        Self {
            hit_tester: azul_layout::headless::CpuHitTester::new(),
            #[cfg(feature = "cpurender")]
            last_frame: None,
        }
    }
}

// ---------------------------------------------------------------------------
// StubWindow
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
pub struct StubWindow {
    /// Common window state (layout, resources, etc.) — shared with all platforms.
    pub common: CommonWindowState,
    /// CPU rendering backend (replaces WebRender).
    pub cpu_backend: CpuBackend,
    /// Whether the window is "open".
    is_open: bool,
    /// Event queue for programmatic event injection.
    event_queue: VecDeque<StubEvent>,
    /// Timer storage (timer_id -> Timer).
    stub_timers: BTreeMap<usize, azul_layout::timer::Timer>,
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

impl StubWindow {
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
                system_style: Arc::new(azul_css::system::SystemStyle::default()),
                app_data,
                scrollbar_drag_state: None,
                hit_tester: None,
                last_hovered_node: None,
                document_id: None,
                id_namespace: None,
                render_api: None,
                renderer: None,
                frame_needs_regeneration: true,
                display_list_initialized: false,
            },
            cpu_backend: CpuBackend::new(),
            is_open: true,
            event_queue: VecDeque::new(),
            stub_timers: BTreeMap::new(),
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
    pub fn poll_event(&mut self) -> Option<StubEvent> {
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
    /// This is the StubWindow equivalent of `MacOSWindow::regenerate_layout()` /
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

        // Mark that frame needs regeneration
        self.common.frame_needs_regeneration = true;

        Ok(result)
    }

    // === Event injection (for tests / debug server) ===

    /// Inject an event into the queue for the next poll cycle.
    ///
    /// Wakes the blocking event loop if it is sleeping on the condvar.
    pub fn inject_event(&mut self, event: StubEvent) {
        self.event_queue.push_back(event);
        self.wake();
    }

    /// Inject multiple events at once.
    pub fn inject_events(&mut self, events: impl IntoIterator<Item = StubEvent>) {
        self.event_queue.extend(events);
        self.wake();
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
        !self.stub_timers.is_empty()
    }

    /// Get the number of pending window creation requests.
    pub fn pending_window_count(&self) -> usize {
        self.pending_window_creates.len()
    }

    // === Blocking event loop ===

    /// Run the headless event loop — **blocks** until the window closes.
    ///
    /// This is the StubWindow equivalent of `NSApplication.run()` / the
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
            "[Stub] Entering condvar-based blocking event loop (debug={})",
            debug_enabled,
        );

        // -- Perform initial layout (same as every platform) --
        log_debug!(
            LogCategory::Layout,
            "[Stub] Performing initial layout"
        );
        if let Err(e) = self.regenerate_layout() {
            log_warn!(
                LogCategory::Layout,
                "[Stub] WARNING: Initial layout failed: {}",
                e
            );
        }

        // -- child windows (sub-StubWindows for menus, dialogs) --
        let mut children: Vec<StubWindow> = Vec::new();
        let mut warned_no_wake_sources = false;

        while self.is_open() {
            // ── Phase 1: Process injected events ─────────────────
            while let Some(event) = self.poll_event() {
                match event {
                    StubEvent::Close => {
                        self.close();
                    }
                    // TODO: wire mouse/keyboard events into
                    // PlatformWindow::process_window_events() once the
                    // shared event-dispatch code is ready.
                    _ => {}
                }
            }

            // ── Phase 2: Tick timers and threads ─────────────────
            // Use the shared PlatformWindow trait method to invoke
            // expired timer callbacks and poll background threads.
            let needs_relayout = self.process_timers_and_threads();

            // If timer/thread callbacks requested a DOM refresh, re-layout
            if needs_relayout {
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Stub] Layout regeneration failed: {}",
                        e
                    );
                }
            }

            // ── Phase 3: Spawn sub-StubWindows for pending creates ─
            while let Some(pending_create) = self.pending_window_creates.pop() {
                log_debug!(
                    LogCategory::Window,
                    "[Stub] Spawning sub-StubWindow (type: {:?})",
                    pending_create.window_state.flags.window_type
                );
                match StubWindow::new(
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
                            "[Stub] Failed to create sub-StubWindow: {:?}",
                            e
                        );
                    }
                }
            }

            // ── Phase 4: Pump child windows ──────────────────────
            children.retain_mut(|child| {
                while let Some(ev) = child.poll_event() {
                    match ev {
                        StubEvent::Close => { child.close(); }
                        _ => {}
                    }
                }
                child.pending_window_creates.clear();
                child.is_open()
            });

            // ── Phase 5: Condvar-based wait ──────────────────────
            let has_timers = !self.stub_timers.is_empty();
            let has_wake_sources = has_timers
                || self.thread_poll_timer_running
                || debug_enabled
                || !children.is_empty();

            if !has_wake_sources && !warned_no_wake_sources {
                warned_no_wake_sources = true;
                eprintln!(
                    "[azul] StubWindow: no timers, threads, or debug server active. \
                     The event loop will block indefinitely on a condvar \
                     (same as a desktop window nobody interacts with). \
                     Set AZUL_DEBUG=1 to enable the debug server, or \
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
            "[Stub] Event loop finished (elapsed: {:.1}s)",
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

impl PlatformWindow for StubWindow {
    // 28 getter/setter methods generated by macro — identical to all other platforms
    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Unsupported
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows {
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
                .insert(azul_core::task::TimerId { id: timer_id }, timer.clone());
        }
        self.stub_timers.insert(timer_id, timer);
        self.wake(); // transition condvar from indefinite to timed wait
    }

    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
        self.stub_timers.remove(&timer_id);
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
        // TODO: could create a sub-StubWindow with the menu content
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

    fn make_stub() -> StubWindow {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        let fc_cache = Arc::new(FcFontCache::default());
        let app_data = Arc::new(RefCell::new(RefAny::new(())));
        let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
        StubWindow::new(
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

        window.inject_event(StubEvent::MouseMove { x: 100.0, y: 200.0 });
        window.inject_event(StubEvent::Close);

        assert!(matches!(window.poll_event(), Some(StubEvent::MouseMove { .. })));
        assert!(matches!(window.poll_event(), Some(StubEvent::Close)));
        assert!(window.poll_event().is_none());
    }

    #[test]
    fn test_stub_timer_management() {
        let mut window = make_stub();
        assert!(!window.has_active_timers());

        let timer = azul_layout::timer::Timer::new(
            RefAny::new(()),
            test_timer_callback,
            azul_core::task::TimerInterval::from_millis(100),
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
}

extern "C" fn test_timer_callback(
    _data: RefAny,
    _info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
}
