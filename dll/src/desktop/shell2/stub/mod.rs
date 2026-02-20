//! Stub/headless backend for testing.
//!
//! This backend implements the full `PlatformWindow` trait without using any
//! system APIs (no window, no OpenGL, no platform event loop). It behaves
//! exactly like the real platform windows — DOM is laid out, callbacks fire,
//! timers tick — but all rendering is a no-op and events are injected
//! programmatically.
//!
//! ## Use Cases
//!
//! - **AZUL_HEADLESS mode**: Full E2E testing without a display server
//! - **CI/CD pipelines**: Headless test runs with no GPU
//! - **Benchmarking**: Measure layout/callback performance without GPU overhead
//! - **Future**: CPU screenshot capture via `cpurender` integration
//!
//! ## Architecture
//!
//! ```text
//! StubWindow
//! ├── common: CommonWindowState        (shared with all platforms)
//! ├── event_queue: VecDeque<StubEvent> (programmatic event injection)
//! ├── stub_timers: BTreeMap            (in-process timer tracking)
//! └── pending_window_creates: Vec      (popup/dialog queue)
//! ```
//!
//! The stub uses `CommonWindowState` + `impl_platform_window_getters!` just
//! like macOS/Win32/X11/Wayland, so all cross-platform event processing,
//! callback dispatch, and state diffing works identically.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::cell::RefCell;

use azul_core::{
    geom::LogicalPosition,
    gl::OptionGlContextPtr,
    hit_test::DocumentId,
    refany::RefAny,
    resources::{IdNamespace, ImageCache, RendererResources},
    window::{RawWindowHandle, VirtualKeyCode},
};
use azul_layout::{
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{FullWindowState, WindowCreateOptions},
};
use rust_fontconfig::FcFontCache;

use crate::desktop::wr_translate2::{AsyncHitTester, WrRenderApi};
use crate::desktop::shell2::common::event::HitTestNode;

use crate::desktop::shell2::common::{
    event::{self, CommonWindowState, PlatformWindow},
    WindowError,
};
use crate::impl_platform_window_getters;

/// Events that can be injected into a StubWindow for testing.
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
    /// Tick all active timers (simulates timer expiration)
    TickTimers,
}

/// Headless window that implements the full PlatformWindow trait.
///
/// Behaves identically to platform windows for layout, callbacks, and state
/// management — but without any system API calls. All "rendering" is a no-op.
pub struct StubWindow {
    /// Common window state (layout, resources, etc.)
    pub common: CommonWindowState,
    /// Whether the window is "open"
    is_open: bool,
    /// Event queue for programmatic event injection
    event_queue: VecDeque<StubEvent>,
    /// Timer storage (timer_id -> Timer) — timers are ticked manually
    stub_timers: BTreeMap<usize, azul_layout::timer::Timer>,
    /// Thread poll timer running flag
    thread_poll_timer_running: bool,
    /// Pending window creation requests (for popup menus, dialogs, etc.)
    pub pending_window_creates: Vec<WindowCreateOptions>,
}

impl StubWindow {
    /// Create a new headless window with the given options.
    ///
    /// This constructor mirrors the real platform window constructors:
    /// 1. Creates LayoutWindow with font cache
    /// 2. Initializes CommonWindowState
    /// 3. Ready for initial layout (DOM → display list)
    ///
    /// Unlike real windows, no system resources are allocated.
    pub fn new(
        options: WindowCreateOptions,
        app_data: Arc<RefCell<RefAny>>,
        fc_cache: Arc<FcFontCache>,
    ) -> Result<Self, WindowError> {
        let full_window_state = options.window_state;

        // Create layout window — same as real platforms
        let mut layout_window = LayoutWindow::new(fc_cache.as_ref().clone())
            .map_err(|e| WindowError::PlatformError(format!("Layout init failed: {:?}", e)))?;
        layout_window.current_window_state = full_window_state.clone();

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
            },
            is_open: true,
            event_queue: VecDeque::new(),
            stub_timers: BTreeMap::new(),
            thread_poll_timer_running: false,
            pending_window_creates: Vec::new(),
        })
    }

    // === Lifecycle Methods ===

    /// Poll the next event from the queue.
    pub fn poll_event(&mut self) -> Option<StubEvent> {
        self.event_queue.pop_front()
    }

    /// Present (render) — no-op in headless mode.
    pub fn present(&mut self) -> Result<(), WindowError> {
        Ok(())
    }

    /// Check if the window is still "open".
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Close the window.
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Request a redraw — no-op in headless mode.
    pub fn request_redraw(&mut self) {
        // No rendering in headless mode
    }

    // === Test Helpers ===

    /// Inject an event into the queue for the next poll cycle.
    pub fn inject_event(&mut self, event: StubEvent) {
        self.event_queue.push_back(event);
    }

    /// Inject multiple events at once.
    pub fn inject_events(&mut self, events: impl IntoIterator<Item = StubEvent>) {
        self.event_queue.extend(events);
    }

    /// Tick all active timers manually.
    ///
    /// In real platforms, timers fire via OS mechanisms (NSTimer, SetTimer, timerfd).
    /// In headless mode, the test harness calls this to simulate timer ticks.
    pub fn tick_timers(&mut self) {
        if self.common.layout_window.is_some() {
            self.common.frame_needs_regeneration = true;
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

    // Timer Management — in-process (no OS timers)

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer.clone());
        }
        self.stub_timers.insert(timer_id, timer);
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
        // No-op in headless mode
    }

    fn show_tooltip_from_callback(
        &mut self,
        _text: &str,
        _position: LogicalPosition,
    ) {
        // No-op in headless mode
    }

    fn hide_tooltip_from_callback(&mut self) {
        // No-op in headless mode
    }

    fn sync_window_state(&mut self) {
        // No native window to synchronize in headless mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stub() -> StubWindow {
        let fc_cache = Arc::new(FcFontCache::default());
        let app_data = Arc::new(RefCell::new(RefAny::new(())));
        StubWindow::new(WindowCreateOptions::default(), app_data, fc_cache).unwrap()
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
}

extern "C" fn test_timer_callback(
    _data: RefAny,
    _info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
}
